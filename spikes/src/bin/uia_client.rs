// SPDX-License-Identifier: AGPL-3.0-or-later
//! UIA test client (prompt 019 §4, driver script `uia-tree`): a separate process that walks the
//! UI Automation tree of a YAMDV window and reads the Document's TextPattern.
//! Usage: `uia-client <hwnd> <out.txt> [steps]`; exit 0 = Document with non-empty text and ≥ 3
//! title-bar buttons, 1 = tree incomplete or a step failed, 2 = usage / COM error.
//! F-42 (prompt 020, driver script `uia-patterns`): `steps` is a `;`-separated list of
//! `invoke:<name>`, `toggle:<name>`, `expand:<name>`, `select:<name>`; each waits up to 5 s for
//! an element with that name and pattern, acts through the pattern and (toggle / expand /
//! select) waits up to 5 s for the new state.

#[cfg(windows)]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (Some(h), Some(out)) = (args.get(1), args.get(2)) else {
        std::process::exit(2);
    };
    let Ok(h) = h.parse::<usize>() else {
        std::process::exit(2);
    };
    let steps = args.get(3).cloned().unwrap_or_default();
    let (code, report) = match win::run(h, &steps) {
        Ok(r) => r,
        Err(e) => (2, format!("error: {e}\n")),
    };
    let _ = std::fs::write(out, report);
    std::process::exit(code);
}

#[cfg(windows)]
mod win {
    use std::fmt::Write as _;
    use std::mem::ManuallyDrop;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
    };
    use windows::Win32::System::Variant::VT_BSTR;
    use windows::Win32::System::Variant::{VARIANT, VT_I4};
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
        IUIAutomationTreeWalker, TreeScope_Descendants, UIA_ButtonControlTypeId,
        UIA_ControlTypePropertyId, UIA_DocumentControlTypeId, UIA_TextPatternId,
        UIA_TitleBarControlTypeId,
    };
    use windows::Win32::UI::Accessibility::{
        ExpandCollapseState_Expanded, IUIAutomationExpandCollapsePattern,
        IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern,
        UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_NamePropertyId, UIA_PATTERN_ID,
        UIA_SelectionItemPatternId, UIA_TogglePatternId,
    };
    use windows::core::{BSTR, Interface};

    struct Walk {
        lines: String,
        bar_buttons: usize,
        nodes: usize,
    }

    fn walk(
        w: &IUIAutomationTreeWalker,
        e: &IUIAutomationElement,
        depth: usize,
        in_bar: bool,
        s: &mut Walk,
    ) {
        if depth > 12 || s.nodes > 2000 {
            return;
        }
        let ct = unsafe { e.CurrentControlType() }.map(|c| c.0).unwrap_or(0);
        let name = unsafe { e.CurrentName() }
            .map(|b| b.to_string())
            .unwrap_or_default();
        s.nodes += 1;
        let _ = writeln!(s.lines, "{}{} {:?}", "  ".repeat(depth), ct, name);
        if in_bar && ct == UIA_ButtonControlTypeId.0 {
            s.bar_buttons += 1;
        }
        let bar = ct == UIA_TitleBarControlTypeId.0;
        let mut c = unsafe { w.GetFirstChildElement(e) }.ok();
        while let Some(k) = c {
            walk(w, &k, depth + 1, bar, s);
            c = unsafe { w.GetNextSiblingElement(&k) }.ok();
        }
    }

    fn by_name(
        ua: &IUIAutomation,
        root: &IUIAutomationElement,
        name: &str,
        pat: UIA_PATTERN_ID,
    ) -> Option<IUIAutomationElement> {
        unsafe {
            let mut v = VARIANT::default();
            (*v.Anonymous.Anonymous).vt = VT_BSTR;
            (*v.Anonymous.Anonymous).Anonymous.bstrVal = ManuallyDrop::new(BSTR::from(name));
            let cond = ua.CreatePropertyCondition(UIA_NamePropertyId, &v).ok()?;
            let all = root.FindAll(TreeScope_Descendants, &cond).ok()?;
            for i in 0..all.Length().unwrap_or(0) {
                if let Ok(e) = all.GetElement(i)
                    && e.GetCurrentPattern(pat)
                        .is_ok_and(|p| !p.as_raw().is_null())
                {
                    return Some(e);
                }
            }
            None
        }
    }

    /// Waits up to 5 s for `f` to return `Some`.
    fn wait<T>(mut f: impl FnMut() -> Option<T>) -> Option<T> {
        for _ in 0..50 {
            if let Some(x) = f() {
                return Some(x);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        None
    }

    /// One F-42 step; `Ok(detail)` on success.
    fn step(
        ua: &IUIAutomation,
        root: &IUIAutomationElement,
        verb: &str,
        name: &str,
    ) -> Result<String, String> {
        let pat = match verb {
            "invoke" => UIA_InvokePatternId,
            "toggle" => UIA_TogglePatternId,
            "expand" => UIA_ExpandCollapsePatternId,
            "select" => UIA_SelectionItemPatternId,
            _ => return Err(format!("unknown verb {verb}")),
        };
        let e = wait(|| by_name(ua, root, name, pat)).ok_or("element not found")?;
        unsafe {
            match verb {
                "invoke" => {
                    let p: IUIAutomationInvokePattern =
                        e.GetCurrentPatternAs(pat).map_err(|x| x.to_string())?;
                    p.Invoke().map_err(|x| x.to_string())?;
                    Ok("invoked".into())
                }
                "toggle" => {
                    let p: IUIAutomationTogglePattern =
                        e.GetCurrentPatternAs(pat).map_err(|x| x.to_string())?;
                    let before = p.CurrentToggleState().map_err(|x| x.to_string())?;
                    p.Toggle().map_err(|x| x.to_string())?;
                    let after = wait(|| {
                        let p: IUIAutomationTogglePattern = e.GetCurrentPatternAs(pat).ok()?;
                        p.CurrentToggleState().ok().filter(|s| *s != before)
                    })
                    .ok_or("state did not change")?;
                    Ok(format!("{} -> {}", before.0, after.0))
                }
                "expand" => {
                    let p: IUIAutomationExpandCollapsePattern =
                        e.GetCurrentPatternAs(pat).map_err(|x| x.to_string())?;
                    p.Expand().map_err(|x| x.to_string())?;
                    wait(|| {
                        let p: IUIAutomationExpandCollapsePattern =
                            e.GetCurrentPatternAs(pat).ok()?;
                        p.CurrentExpandCollapseState()
                            .ok()
                            .filter(|s| *s == ExpandCollapseState_Expanded)
                    })
                    .ok_or("not expanded")?;
                    Ok("expanded".into())
                }
                _ => {
                    let p: IUIAutomationSelectionItemPattern =
                        e.GetCurrentPatternAs(pat).map_err(|x| x.to_string())?;
                    p.Select().map_err(|x| x.to_string())?;
                    wait(|| {
                        let p: IUIAutomationSelectionItemPattern =
                            e.GetCurrentPatternAs(pat).ok()?;
                        p.CurrentIsSelected().ok().filter(|b| b.as_bool())
                    })
                    .map(|_| ())
                    .ok_or("not selected")?;
                    Ok("selected".into())
                }
            }
        }
    }

    pub fn run(h: usize, steps: &str) -> windows::core::Result<(i32, String)> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let ua: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = ua.ElementFromHandle(HWND(h as *mut _))?;
            let walker = ua.ControlViewWalker()?;
            let mut s = Walk {
                lines: String::new(),
                bar_buttons: 0,
                nodes: 0,
            };
            walk(&walker, &root, 0, false, &mut s);
            let mut v = VARIANT::default();
            (*v.Anonymous.Anonymous).vt = VT_I4;
            (*v.Anonymous.Anonymous).Anonymous.lVal = UIA_DocumentControlTypeId.0;
            let cond = ua.CreatePropertyCondition(UIA_ControlTypePropertyId, &v)?;
            let doc = root.FindFirst(TreeScope_Descendants, &cond).ok();
            let text = doc
                .as_ref()
                .and_then(|d| {
                    d.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                        .ok()
                })
                .and_then(|p| p.DocumentRange().ok())
                .and_then(|r| r.GetText(-1).ok())
                .map(|b| b.to_string())
                .unwrap_or_default();
            let mut ok = doc.is_some() && !text.trim().is_empty() && s.bar_buttons >= 3;
            let mut out = s.lines;
            for st in steps.split(';').map(str::trim).filter(|x| !x.is_empty()) {
                let (verb, name) = st.split_once(':').unwrap_or((st, ""));
                let r = step(&ua, &root, verb, name);
                if r.is_err() {
                    ok = false;
                }
                let _ = writeln!(out, "step {st}: {r:?}");
            }
            let _ = writeln!(
                out,
                "nodes={} bar_buttons={} document={} text_chars={}\n{}",
                s.nodes,
                s.bar_buttons,
                doc.is_some(),
                text.chars().count(),
                if ok { "UIA-OK" } else { "UIA-FAIL" }
            );
            let _ = writeln!(
                out,
                "text-head: {:?}",
                text.chars().take(80).collect::<String>()
            );
            Ok((if ok { 0 } else { 1 }, out))
        }
    }
}

#[cfg(not(windows))]
fn main() {
    std::process::exit(2);
}

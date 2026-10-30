// SPDX-License-Identifier: AGPL-3.0-or-later
//! In-window test driver (prompt 004 §4.5, dev-tools only): runs a `--script` file by calling
//! the window's own handlers (no `SendInput`), asserts over a JSON state dump, writes
//! `<script>.log` and exits with 0 (all assertions passed) or 3.

use std::cell::RefCell;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, PostQuitMessage, SetTimer};
use yamdv_core::devscript::{self, Cmd};
use yamdv_core::doc::detail::NO_LINK;

use super::app;
use super::{Content, Pending, Shared, TIMER_SCRIPT, View, WINDOWS, Win, focus_rects};
use crate::sys;
use serde_like::Get as _;

struct Script {
    path: String,
    cmds: Vec<Cmd>,
    pc: usize,
    log: Vec<String>,
    fails: u32,
    /// `sleep`: resume after this instant.
    resume: Option<std::time::Instant>,
    /// `type` in an edit mode: one character per step, so each edit is painted (T5).
    chars: std::collections::VecDeque<char>,
}

thread_local! {
    static SCRIPT: RefCell<Option<Script>> = const { RefCell::new(None) };
    /// The file written by the last `export-html` / `export-csv` (V1-D).
    static LAST_OUT: RefCell<Option<String>> = const { RefCell::new(None) };
    static BYTES_MARK: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Scroll anchor (block, offset) saved by `ui anchor-mark` (PERF-06, `anchor_same`).
    static ANCHOR_MARK: std::cell::Cell<Option<(usize, f64)>> = const { std::cell::Cell::new(None) };
    /// PWS baseline of `ui mem-mark` (`pws_delta_kib`).
    static MEM_MARK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// `ui uia-client` child process (prompt 019 §4) and its start time / exit code.
    static UIA_CHILD: RefCell<Option<(std::process::Child, std::time::Instant)>> = const { RefCell::new(None) };
    static UIA_EXIT: std::cell::Cell<i32> = const { std::cell::Cell::new(-1) };
    /// Pages seen by `ui job-wait` / `ui job-done` polls: (samples, last, monotonic).
    static JOB_SEQ: std::cell::Cell<(u32, u32, bool, u32)> = const { std::cell::Cell::new((0, 0, true, 0)) };
    static CPU_MARK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Document height at `ui height-mark` (V2-C folding).
    static HEIGHT_MARK: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
    /// Module names at `ui modules-mark` (`modules_new`, P-53).
    static MOD_MARK: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    /// `ui jumplist-flush` result: (started, exit code, dump text).
    static JUMP: RefCell<(bool, Option<u32>, String)> = const { RefCell::new((false, None, String::new())) };
    /// GDI / USER object counts at `ui mem-mark` (`gdi_delta`, `user_delta`).
    static GUI_MARK: std::cell::Cell<(u32, u32)> = const { std::cell::Cell::new((0, 0)) };
    /// Summary of the last written scroll trace (018a `scroll-fling`).
    static TRACE: RefCell<Option<yamdv_core::scroll::TraceStats>> = const { RefCell::new(None) };
    /// `ui near-heading`: the settle top (DIPs) the next slow notch should end near.
    static SNAP_BLOCK: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Re-runs the current command after `ms` (polling verbs; the message loop keeps running).
pub(super) fn again(ms: u64) -> String {
    SCRIPT.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.pc = s.pc.saturating_sub(1);
            s.resume = Some(std::time::Instant::now() + std::time::Duration::from_millis(ms));
        }
    });
    String::new()
}

/// Records a page count for the `job_monotonic` check (Printing phase only: Preparing reports
/// layout blocks, e.g. 482 for pages-30.md, before the page counter starts at 1). Returns the
/// current page (0 while preparing), so `ui job-wait N` waits for N printed pages.
fn job_sample() -> u32 {
    let (n, t) = super::jobui::state().map_or((0, 0), |(_, n, t, _)| (n, t));
    let pages = super::jobui::busy() && super::jobui::printing();
    JOB_SEQ.with(|q| {
        let (k, last, mono, total) = q.get();
        if pages {
            q.set((k + 1, n.max(last), mono && n >= last, t.max(total)));
        }
    });
    if pages { n } else { 0 }
}

/// Rust heap counters (measurement builds with `mem-stats`; zeros otherwise).
fn heap_stats() -> (usize, usize, usize) {
    #[cfg(feature = "mem-stats")]
    {
        crate::heapcount::stats()
    }
    #[cfg(not(feature = "mem-stats"))]
    {
        (0, 0, 0)
    }
}

/// Private working set now (no trim: what Task Manager shows).
fn pws_now() -> u64 {
    crate::perf::private_ws_bytes()
}

/// Private working set growth since `ui mem-mark`, KiB.
fn pws_delta_kib() -> f64 {
    let mark = MEM_MARK.with(std::cell::Cell::get);
    (pws_now() as f64 - mark as f64) / 1024.0
}

/// GDI and USER objects this process owns now (exact, unlike the working set).
fn gui_now() -> (u32, u32) {
    use windows::Win32::System::Threading::{
        GR_GDIOBJECTS, GR_USEROBJECTS, GetCurrentProcess, GetGuiResources,
    };
    // SAFETY: the pseudo handle of this process is always valid.
    unsafe {
        let p = GetCurrentProcess();
        (
            GetGuiResources(p, GR_GDIOBJECTS),
            GetGuiResources(p, GR_USEROBJECTS),
        )
    }
}

/// A script is running (popups then ignore the real mouse).
pub(super) fn active() -> bool {
    SCRIPT.with(|s| s.try_borrow().map_or(true, |s| s.is_some()))
}

/// Loads the script (once) and starts the step timer on `hwnd`.
pub(super) fn start(hwnd: HWND, path: &str) {
    if SCRIPT.with(|s| s.borrow().is_some()) {
        return;
    }
    sys::set_launch_dry_run();
    let src = std::fs::read_to_string(path).unwrap_or_default();
    let (cmds, log, fails) = match devscript::parse(&src) {
        Ok(c) => (c, Vec::new(), 0),
        Err(e) => (Vec::new(), vec![format!("FAIL parse: {e}")], 1),
    };
    SCRIPT.with(|s| {
        *s.borrow_mut() = Some(Script {
            path: path.to_string(),
            cmds,
            pc: 0,
            log,
            fails,
            resume: None,
            chars: std::collections::VecDeque::new(),
        });
    });
    // SAFETY: valid hwnd; killed when the script ends.
    unsafe { SetTimer(Some(hwnd), TIMER_SCRIPT, 60, None) };
}

/// One step: runs commands until one that needs the message loop (input events).
pub(super) fn tick(w: &mut Win, sh: &Shared) {
    let now = std::time::Instant::now();
    if SCRIPT.with(|s| {
        s.borrow()
            .as_ref()
            .and_then(|s| s.resume)
            .is_some_and(|r| now < r)
    }) {
        return;
    }
    let next = SCRIPT.with(|s| s.borrow_mut().as_mut().and_then(|s| s.chars.pop_front()));
    if let Some(c) = next {
        // As WM_CHAR: V2-E auto-pair first (prompt 015 §3.4).
        if !c.is_control() {
            w.xsel_input(sh);
        }
        if c.is_control() || !(w.linkpop_char(c) || w.plus_char(sh, c)) {
            w.edit_type(sh, &c.to_string());
        }
        return;
    }
    loop {
        let cmd = SCRIPT.with(|s| {
            let mut s = s.borrow_mut();
            let s = s.as_mut()?;
            let c = s.cmds.get(s.pc).cloned();
            s.pc += 1;
            c
        });
        let Some(cmd) = cmd else {
            finish(w);
            return;
        };
        let yield_after = !matches!(cmd, Cmd::Assert(..) | Cmd::Dump | Cmd::WaitIdle);
        let line = run(w, sh, &cmd);
        SCRIPT.with(|s| {
            if let Some(s) = s.borrow_mut().as_mut()
                && !line.is_empty()
            {
                if line.starts_with("FAIL") {
                    s.fails += 1;
                }
                s.log.push(line);
                // Partial log, so a script killed on timeout still shows how far it got.
                let _ = std::fs::write(format!("{}.log", s.path), s.log.join("\n"));
            }
        });
        if yield_after {
            return;
        }
    }
}

fn finish(w: &mut Win) {
    // SAFETY: valid hwnd.
    let _ = unsafe { KillTimer(Some(w.hwnd), TIMER_SCRIPT) };
    let code = SCRIPT.with(|s| {
        let s = s.borrow();
        let Some(s) = s.as_ref() else { return 3 };
        let mut out = s.log.join("\n");
        out.push_str(&format!(
            "\n{} ({} failed)\n",
            if s.fails == 0 { "PASS" } else { "FAIL" },
            s.fails
        ));
        let _ = std::fs::write(format!("{}.log", s.path), out);
        if s.fails == 0 { 0 } else { 3 }
    });
    // SAFETY: no preconditions; ends the message loop with the script result.
    unsafe { PostQuitMessage(code) };
}

fn run(w: &mut Win, sh: &Shared, cmd: &Cmd) -> String {
    match cmd {
        Cmd::Click(x, y) => {
            w.mouse_down(sh, *x, *y, false);
            w.mouse_up();
            format!("ok click {x} {y}")
        }
        Cmd::ClickLink(text, new) => match find_link(w, text) {
            None => format!("FAIL click-link \"{text}\": no such link"),
            Some((b, l)) => {
                w.scroll_into_view(sh, b, l);
                let (cx, _) = w.col(sh);
                let View::Doc(d) = &w.view else {
                    return "FAIL click-link: no document".into();
                };
                let top = (d.viewer.block_top(&d.content, b) - d.viewer.scroll) as f32;
                let Some(r) = focus_rects(&d.viewer, b, l).first().copied() else {
                    return format!("FAIL click-link \"{text}\": not laid out");
                };
                let (x, y) = (cx + r.x + r.w.min(4.0), top + r.y + r.h / 2.0);
                if *new {
                    w.middle_click(sh, x, y);
                } else {
                    w.mouse_down(sh, x, y, false);
                    w.mouse_up();
                }
                format!("ok click-link \"{text}\" at {x:.0},{y:.0}")
            }
        },
        Cmd::Key(c) => {
            // V2-D keys first, as in the window procedure.
            if w.v2d_key(sh, c.vk, c.ctrl, c.shift, c.alt) {
                w.nav_kbd(true);
            } else if let Some(a) = w.key_chord(sh, c.vk, c.ctrl, c.shift, c.alt) {
                w.actions.extend(a);
            } else if c.alt && (c.vk == 0x25 || c.vk == 0x27) {
                w.history_go(sh, if c.vk == 0x25 { -1 } else { 1 });
            } else if let Some(a) = w.key(sh, c.vk, c.ctrl, c.shift) {
                w.actions.push(a);
            }
            format!("ok key {c:?}")
        }
        Cmd::Wheel(d) => {
            w.wheel(sh, *d);
            format!("ok wheel {d}")
        }
        Cmd::WaitIdle => {
            if w.wm.active {
                let t = w.wm.target;
                w.wm.stop();
                w.set_scroll(t);
            }
            if let Some(a) = w.anim.take() {
                w.set_scroll(a.to);
            }
            w.prepare_now(sh);
            "ok wait-idle".into()
        }
        Cmd::Assert(f, op, v) => {
            // `math_errors` walks every block of the document: computed only when asserted, so
            // the other asserts stay cheap and do not grow the heap. `pws_delta_kib` is sampled
            // before anything else: building the full state on a 1 MB document commits ~200 KiB
            // of transient heap that the assert itself would measure (V2-G export-progress).
            let actual = if f == "math_errors" {
                format!("{}", math_errors(w))
            } else if f == "math_display_errors" {
                format!("{}", math_display_errors(w))
            } else if f == "pws_delta_kib" {
                format!("{}", pws_delta_kib())
            } else {
                state(w, sh)
                    .get(f)
                    .map(|x| match x {
                        serde_like::V::S(s) => s.clone(),
                        serde_like::V::N(n) => format!("{n}"),
                    })
                    .unwrap_or_else(|| "<missing>".into())
            };
            let pass = devscript::check(&actual, *op, v);
            format!(
                "{} assert {f} {op:?} {v} (actual {actual})",
                if pass { "ok" } else { "FAIL" }
            )
        }
        Cmd::Dump => format!("dump {}", serde_like::to_json(&state(w, sh))),
        Cmd::Command(name) => match command_id(name) {
            Some(id) => {
                w.actions.push(super::Action::Command(id));
                format!("ok command {name}")
            }
            None => format!("FAIL command {name}: unknown"),
        },
        Cmd::Type(t) => {
            if w.nav.palette.is_some() {
                for c in t.chars() {
                    w.palette_char(sh, c);
                }
            } else if w.settings.is_some() {
                for c in t.chars() {
                    w.panel_char(c);
                }
            } else if w.find.is_none() && (w.typing() || w.xsel().is_some() || w.mmd_active()) {
                SCRIPT.with(|s| {
                    if let Some(s) = s.borrow_mut().as_mut() {
                        s.chars.extend(t.chars());
                    }
                });
            } else {
                for c in t.chars() {
                    w.find_char(sh, c);
                }
            }
            format!("ok type \"{t}\"")
        }
        Cmd::ClickText(t) => click_text(w, sh, t, Click::Text),
        Cmd::ClickTask(t) => click_text(w, sh, t, Click::Task),
        Cmd::ClickRender(t) => click_text(w, sh, t, Click::Render),
        Cmd::SideClick(text) => {
            let row = w
                .side_rows(sh)
                .into_iter()
                .find(|r| r.text.contains(text.as_str()));
            match row {
                Some(r) => {
                    let (x, y) = (r.rect.x + r.rect.w / 2.0, r.rect.y + r.rect.h / 2.0);
                    w.mouse_down(sh, x, y, false);
                    w.mouse_up();
                    format!("ok side-click \"{text}\" at {x:.0},{y:.0}")
                }
                None => format!("FAIL side-click \"{text}\": no such row"),
            }
        }
        Cmd::WriteFile(f, t) => match std::fs::write(rel(f), t) {
            Ok(()) => format!("ok write-file {f}"),
            Err(e) => format!("FAIL write-file {f}: {e}"),
        },
        Cmd::Open(f) => {
            w.open(sh, &rel(f), None, false);
            format!("ok open {f}")
        }
        Cmd::Launch(f) => {
            if crate::shell::spawn_self(&[rel(f)]) {
                format!("ok launch {f}")
            } else {
                format!("FAIL launch {f}")
            }
        }
        Cmd::Snapshot(f) => snapshot(w, sh, f),
        Cmd::ExportHtml(f) => {
            let out = rel(f);
            let _ = std::fs::remove_file(&out);
            let ok = w
                .export_job(sh)
                .is_some_and(|j| crate::export::export_html(&sh.dw, &j, &out));
            LAST_OUT.with(|l| *l.borrow_mut() = Some(out));
            format!("{} export-html {f}", if ok { "ok" } else { "FAIL" })
        }
        Cmd::ExportCsv(f) => {
            let out = rel(f);
            let _ = std::fs::remove_file(&out);
            let csv = match &w.view {
                View::Doc(d) => match &d.content {
                    Content::Markdown(doc) => {
                        doc.index.blocks.iter().enumerate().find_map(|(bi, b)| {
                            matches!(b.kind, yamdv_core::doc::index::BlockKind::Table)
                                .then(|| crate::export::table_csv(doc, bi, b.range.start))
                                .flatten()
                        })
                    }
                    _ => None,
                },
                _ => None,
            };
            let ok = csv.is_some_and(|t| crate::shell::write_atomic(&out, &t));
            LAST_OUT.with(|l| *l.borrow_mut() = Some(out));
            format!("{} export-csv {f}", if ok { "ok" } else { "FAIL" })
        }
        Cmd::Resize(cw, ch) => resize(w, *cw, *ch),
        Cmd::Ui(verb, arg) => ui(w, sh, verb, arg),
        Cmd::Sleep(ms) => {
            let until =
                std::time::Instant::now() + std::time::Duration::from_millis(u64::from(*ms));
            SCRIPT.with(|s| {
                if let Some(s) = s.borrow_mut().as_mut() {
                    s.resume = Some(until);
                }
            });
            format!("ok sleep {ms}")
        }
    }
}

fn resize(w: &mut Win, cw: f32, ch: f32) -> String {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect};
    let (mut wr, mut cr) = (RECT::default(), RECT::default());
    // SAFETY: valid hwnd and out-structures.
    let ok =
        unsafe { GetWindowRect(w.hwnd, &mut wr).is_ok() && GetClientRect(w.hwnd, &mut cr).is_ok() };
    if !ok {
        return "FAIL resize".into();
    }
    let fw = (wr.right - wr.left) - (cr.right - cr.left);
    let fh = (wr.bottom - wr.top) - (cr.bottom - cr.top);
    let pw = (cw * w.scale).round() as i32 + fw;
    // `ch` is the content height; the title bar sits above it (B.4).
    let ph = ((ch + w.top) * w.scale).round() as i32 + fh;
    w.defer(super::Deferred::SetPos(wr.left, wr.top, pw, ph));
    format!("ok resize {cw} {ch}")
}

/// Renders the whole client area offscreen (same draw path as WM_PAINT) to a PNG.
fn snapshot(w: &mut Win, sh: &Shared, f: &str) -> String {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    w.prepare_now(sh);
    let mut cr = RECT::default();
    // SAFETY: valid hwnd and out-structure.
    let _ = unsafe { GetClientRect(w.hwnd, &mut cr) };
    let (cx, _) = w.col(sh);
    let geo = w.geometry(sh);
    let Some(r) = &w.renderer else {
        return "FAIL snapshot: no renderer".into();
    };
    let me = &*w;
    // Content only (the goldens of earlier phases); `ui shot-window` includes the title bar.
    let full = f.starts_with("top-");
    let h = if full {
        cr.bottom - cr.top
    } else {
        cr.bottom - cr.top - me.top_px()
    };
    let mut px = r.render_offscreen(h, me.pal.text, &mut |band| {
        if full {
            band.set_origin(me.top);
        }
        me.draw(sh, band, cx, geo.as_ref());
        me.draw_overlays(sh, band);
        me.tz_draw(sh, band);
        if full {
            band.set_origin(0.0);
            me.draw_chrome(sh, band);
            me.draw_toolbar(sh, band);
            me.tz_draw_client(sh, band);
        }
    });
    for p in px.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    let dir = std::env::var("YAMDV_SNAPSHOT_DIR").unwrap_or_default();
    let out = if dir.is_empty() {
        rel(f)
    } else {
        std::path::Path::new(&dir)
            .join(f)
            .to_string_lossy()
            .into_owned()
    };
    let (Ok(pw), Ok(ph)) = (u32::try_from(r.width()), u32::try_from(h)) else {
        return "FAIL snapshot: size".into();
    };
    match crate::devtools::write_png(&out, pw, ph, &px) {
        Ok(()) => format!("ok snapshot {f} {pw}x{ph}"),
        Err(e) => format!("FAIL snapshot {f}: {e}"),
    }
}

/// Path relative to the script's folder.
pub(super) fn rel(f: &str) -> String {
    let dir = SCRIPT.with(|s| {
        s.borrow().as_ref().and_then(|s| {
            std::path::Path::new(&s.path)
                .parent()
                .map(std::path::Path::to_path_buf)
        })
    });
    dir.unwrap_or_default()
        .join(f)
        .to_string_lossy()
        .into_owned()
}

/// Popup operations run outside the window borrow (`Action::PopupOp`).
#[derive(Debug, Clone)]
pub(super) enum PopupOp {
    Hover(Option<usize>),
    Click(usize),
    Key(u16),
    Shot(String),
}

/// Runs a popup operation (no window borrow is held).
pub(super) fn popup_op(_hwnd: HWND, op: PopupOp) {
    let line = match op {
        PopupOp::Hover(i) => {
            super::popup::drive_hover(i);
            format!("ok ui popup-hover {i:?}")
        }
        PopupOp::Click(i) => {
            super::popup::drive_click(i);
            format!("ok ui popup-click {i}")
        }
        PopupOp::Key(vk) => {
            let used = super::popup::key(
                windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                windows::Win32::Foundation::WPARAM(usize::from(vk)),
            );
            format!("{} ui popup-key {vk}", if used { "ok" } else { "FAIL" })
        }
        PopupOp::Shot(f) => {
            let dir = std::env::var("YAMDV_SNAPSHOT_DIR").unwrap_or_default();
            let out = if dir.is_empty() {
                rel(&f)
            } else {
                std::path::Path::new(&dir)
                    .join(&f)
                    .to_string_lossy()
                    .into_owned()
            };
            match super::popup::snapshot(&out) {
                Ok((pw, ph)) => format!("ok ui popup-shot {f} {pw}x{ph}"),
                Err(e) => format!("FAIL ui popup-shot {f}: {e}"),
            }
        }
    };
    SCRIPT.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            if line.starts_with("FAIL") {
                s.fails += 1;
            }
            s.log.push(line);
        }
    });
}

fn vk_of(name: &str) -> Option<u16> {
    Some(match name {
        "esc" => 0x1B,
        "enter" => 0x0D,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "space" => 0x20,
        s if s.len() == 1 => u16::from(s.as_bytes().first()?.to_ascii_uppercase()),
        _ => return None,
    })
}

/// `ui <verb> [arg]`: title bar, popup and chrome operations (V2-A).
fn ui(w: &mut Win, sh: &Shared, verb: &str, arg: &str) -> String {
    use yamdv_core::chrome::Part;
    let part = |a: &str| match a {
        "settings" => Some(Part::Settings),
        "more" => Some(Part::More),
        "min" => Some(Part::Min),
        "max" => Some(Part::Max),
        "close" => Some(Part::Close),
        _ => None,
    };
    let op = |w: &mut Win, op: PopupOp| {
        w.actions.push(super::Action::PopupOp(op));
        String::new()
    };
    match verb {
        "click" => match part(arg) {
            Some(p) => {
                w.chrome_click(p);
                format!("ok ui click {arg}")
            }
            None => format!("FAIL ui click {arg}: unknown button"),
        },
        "dblclick-caption" => {
            use windows::Win32::Foundation::{LPARAM, WPARAM};
            let (x, y) = w.button_anchor(Part::Settings);
            let (x, y) = (x - 200, y - 10);
            let lp = (((y & 0xFFFF) as isize) << 16) | ((x & 0xFFFF) as isize);
            // SAFETY: valid hwnd; DefWindowProc handles the caption double-click.
            let _ = unsafe {
                windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                    Some(w.hwnd),
                    0x00A3,
                    WPARAM(2),
                    LPARAM(lp),
                )
            };
            "ok ui dblclick-caption".into()
        }
        // IDEA-05: click / double-click node N (`eN`: edge N) of the first laid-out
        // flowchart, or drag node A onto node B (`A B`), through the real mouse path.
        "mmd-click" | "mmd-dblclick" | "mmd-drag" => {
            let pt = |w: &mut Win, a: &str| {
                let (edge, n) = a.strip_prefix('e').map_or((false, a), |n| (true, n));
                n.parse().ok().and_then(|i| w.mmd_point(sh, i, edge))
            };
            let mut it = arg.split_whitespace();
            let Some(a) = it.next().and_then(|a| pt(w, a)) else {
                return format!("FAIL ui {verb} {arg}: no such diagram element");
            };
            let b = if verb == "mmd-drag" {
                match it.next().and_then(|b| pt(w, b)) {
                    Some(b) => Some(b),
                    None => return format!("FAIL ui {verb} {arg}: no such node"),
                }
            } else {
                None
            };
            let times = if verb == "mmd-dblclick" { 2 } else { 1 };
            for _ in 0..times {
                if !w.edit_mouse_down(sh, a.0, a.1, false) {
                    return format!("FAIL ui {verb} {arg}: click not taken");
                }
                if let Some(b) = b {
                    w.mmd_move(f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
                    w.mmd_move(b.0, b.1);
                }
                w.edit_mouse_up(sh);
            }
            format!("ok ui {verb} {arg} -> {}", w.mmd_state())
        }
        "hover" => {
            let y: f32 = arg.parse().unwrap_or(0.0);
            let x = w.cw / 2.0;
            w.chrome_move(x, y);
            format!("ok ui hover {y}")
        }
        // V2-C transient UI (FUT-40/36/06/34/41).
        "hover-hot" | "hover-hot-now" => match w.tz_drive_hot(sh, arg) {
            Some((x, y)) => {
                w.tz_drive_hover(sh, x, y, verb == "hover-hot-now");
                format!("ok ui {verb} {arg} at {x:.0},{y:.0}")
            }
            None => format!("FAIL ui {verb} {arg}: not found"),
        },
        "hover-off" => {
            w.tz_tip_hide();
            "ok ui hover-off".into()
        }
        // Copy as PNG on the first diagram (the context-menu command).
        "diagram-copy-png" => match w.tz_drive_hot(sh, "diagram") {
            Some((x, y)) => {
                w.ctx_diagram = w.diagram_at(sh, x, y);
                w.diagram_copy(sh, true);
                "ok ui diagram-copy-png".into()
            }
            None => "FAIL ui diagram-copy-png: no diagram".into(),
        },
        // 018a §2-C: the next PNG/CSV export writes to `arg` instead of opening the save dialog
        // (the real Ctrl+S / menu path otherwise).
        "save-to" => {
            let out = rel(arg);
            let _ = std::fs::remove_file(&out);
            super::DRIVER_SAVE_TO.with(|t| *t.borrow_mut() = Some(out.clone()));
            LAST_OUT.with(|l| *l.borrow_mut() = Some(out));
            format!("ok ui save-to {arg}")
        }
        // Save as PNG… on the first diagram (the context-menu command).
        "diagram-save-png" => match w.tz_drive_hot(sh, "diagram") {
            Some((x, y)) => {
                w.ctx_diagram = w.diagram_at(sh, x, y);
                match w.diagram_save_png(sh) {
                    Some(png) => {
                        w.actions.push(super::Action::Export(super::ExportKind::Png(
                            png, "diagram",
                        )));
                        "ok ui diagram-save-png".into()
                    }
                    None => "FAIL ui diagram-save-png: no image".into(),
                }
            }
            None => "FAIL ui diagram-save-png: no diagram".into(),
        },
        // 018a §2-D editor detection: a fake install under %LOCALAPPDATA% (the driver's scratch
        // profile), e.g. `fake-editor Programs\Microsoft VS Code\Code.exe`.
        "fake-editor" => {
            let p = std::path::Path::new(&crate::known::local_app_data()).join(arg);
            let ok = p
                .parent()
                .is_some_and(|d| std::fs::create_dir_all(d).is_ok())
                && std::fs::write(&p, b"MZ").is_ok();
            if ok {
                format!("ok ui fake-editor {arg}")
            } else {
                format!("FAIL ui fake-editor {arg}")
            }
        }
        "open-editor" => {
            w.open_editor();
            "ok ui open-editor".into()
        }
        // PERF-06: `file-replace <file>|<old>|<new>` edits the file outside the app (first match).
        "file-replace" => {
            let mut it = arg.splitn(3, '|');
            let (f, a, b) = (
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
                it.next().unwrap_or(""),
            );
            let t = std::fs::read_to_string(rel(f)).unwrap_or_default();
            if a.is_empty() || !t.contains(a) {
                format!("FAIL ui file-replace {arg}: text not found")
            } else {
                match std::fs::write(rel(f), t.replacen(a, b, 1)) {
                    Ok(()) => format!("ok ui file-replace {arg}"),
                    Err(e) => format!("FAIL ui file-replace {arg}: {e}"),
                }
            }
        }
        "anchor-mark" => {
            let a = match &w.view {
                super::View::Doc(d) => Some(d.viewer.anchor()),
                _ => None,
            };
            ANCHOR_MARK.with(|m| m.set(a));
            format!("ok ui anchor-mark {a:?}")
        }
        "copy-file" => {
            let (a, b) = arg.split_once(' ').unwrap_or((arg, ""));
            match std::fs::copy(rel(a), rel(b)) {
                Ok(_) => format!("ok ui copy-file {arg}"),
                Err(e) => format!("FAIL ui copy-file {arg}: {e}"),
            }
        }
        // Byte comparison of the current document file with `arg` (`file_same`).
        "bytes-mark" => {
            let b = std::fs::read(rel(arg)).unwrap_or_default();
            BYTES_MARK.with(|m| *m.borrow_mut() = b);
            format!("ok ui bytes-mark {arg}")
        }
        "dblclick-media" => {
            if w.tz_drive_open(sh, arg) {
                format!("ok ui dblclick-media {arg}")
            } else {
                format!("FAIL ui dblclick-media {arg}: no viewer")
            }
        }
        "viewer-wheel" => {
            let notches: f32 = arg.parse().unwrap_or(1.0);
            let (x, y) = (w.cw * 0.3, w.ch * 0.4);
            match w.tz_drive_wheel(notches, x, y) {
                Some((a, b)) if (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.5 => {
                    format!("ok ui viewer-wheel {notches}: point kept")
                }
                Some((a, b)) => format!("FAIL ui viewer-wheel: {a:?} -> {b:?}"),
                None => "FAIL ui viewer-wheel: no viewer".into(),
            }
        }
        "viewer-key" => {
            if w.tz_drive_key(sh, arg) {
                format!("ok ui viewer-key {arg}")
            } else {
                format!("FAIL ui viewer-key {arg}")
            }
        }
        "fold" | "fold-alt" => match w.tz_heading(arg) {
            Some(h) => {
                w.tz_fold(h, verb == "fold-alt");
                w.tz_drive_settle();
                w.prepare_now(sh);
                format!("ok ui {verb} {arg}")
            }
            None => format!("FAIL ui {verb} {arg}: no heading"),
        },
        "popup-hover" => op(w, PopupOp::Hover(arg.parse().ok())),
        "popup-click" => match arg.parse() {
            Ok(i) => op(w, PopupOp::Click(i)),
            Err(_) => format!("FAIL ui popup-click {arg}"),
        },
        "popup-key" => match vk_of(arg) {
            Some(vk) => op(w, PopupOp::Key(vk)),
            None => format!("FAIL ui popup-key {arg}: unknown key"),
        },
        "popup-shot" => op(w, PopupOp::Shot(arg.to_string())),
        // 018a §1.3: select a menu entry by its label (not its position).
        "popup-select" => {
            if super::popup::drive_select(arg) {
                format!("ok ui popup-select {arg}")
            } else {
                format!("FAIL ui popup-select {arg}: no such entry")
            }
        }
        "shot-window" => snapshot(w, sh, &format!("top-{arg}")).replacen("top-", "", 1),
        // BUG-11: background export through the worker (`job*` state fields).
        "export-html-bg" | "export-pdf-bg" => {
            let out = rel(arg);
            let _ = std::fs::remove_file(&out);
            let k = if verb == "export-html-bg" {
                super::ExportKind::Html
            } else {
                super::ExportKind::Pdf
            };
            w.run_export(sh, &k, &out);
            LAST_OUT.with(|l| *l.borrow_mut() = Some(out));
            if super::jobui::busy() {
                format!("ok ui {verb} {arg}")
            } else {
                format!("FAIL ui {verb} {arg}: no job")
            }
        }
        // `path-setting key=file`: `file` relative to the script, made absolute.
        "path-setting" => {
            let (k, f) = arg.split_once('=').unwrap_or((arg, ""));
            let v = format!("{k}={}", rel(f));
            w.panel_drive(sh, "settings-set", &v)
                .unwrap_or_else(|| "FAIL ui path-setting".into())
        }
        // `pad-file src dst bytes`: a copy of `src` zero-padded to `bytes` (oversize inputs).
        "pad-file" => {
            let mut it = arg.split(' ');
            let (a, b, n) = (
                it.next(),
                it.next(),
                it.next().and_then(|n| n.parse::<usize>().ok()),
            );
            let (Some(a), Some(b), Some(n)) = (a, b, n) else {
                return format!("FAIL ui pad-file {arg}");
            };
            let mut d = std::fs::read(rel(a)).unwrap_or_default();
            d.resize(n.max(d.len()), 0);
            match std::fs::write(rel(b), d) {
                Ok(()) => format!("ok ui pad-file {arg}"),
                Err(e) => format!("FAIL ui pad-file {arg}: {e}"),
            }
        }
        "toolbar-click" => w.tb_drive(sh, arg),
        v if v.starts_with("settings-") => w
            .panel_drive(sh, verb, arg)
            .unwrap_or_else(|| format!("FAIL ui {verb}: unknown verb")),
        // BUG-11 export-progress: poll until `job_n >= N` (or the job ended) / until it ended.
        "job-wait" => {
            let want: u32 = arg.parse().unwrap_or(1);
            let n = job_sample();
            if super::jobui::busy() && n < want {
                again(40)
            } else {
                format!("ok ui job-wait {arg} (pages {n})")
            }
        }
        "job-done" => {
            job_sample();
            if super::jobui::busy() {
                again(100)
            } else {
                "ok ui job-done".into()
            }
        }
        // 018a §1.1: memory breakdown + module list with sizes, written as JSON to `arg`.
        "mem-report" => {
            let (live, peak, count) = heap_stats();
            let (g, u) = gui_now();
            let mods = crate::perf::modules();
            let list: Vec<String> = mods
                .iter()
                .map(|(n, sz)| format!("{{\"name\":\"{n}\",\"size\":{sz}}}"))
                .collect();
            let json = format!(
                "{{\"rust_heap_live\":{live},\"rust_heap_peak\":{peak},\"rust_allocs\":{count},\"process_heap_commit\":{},\"private_ws\":{},\"commit\":{},\"gdi\":{g},\"user\":{u},\"module_bytes\":{},\"modules\":[{}]}}",
                crate::perf::process_heap_commit(),
                pws_now(),
                crate::perf::commit_bytes(),
                mods.iter().map(|(_, sz)| u64::from(*sz)).sum::<u64>(),
                list.join(",")
            );
            match std::fs::write(rel(arg), json) {
                Ok(()) => format!("ok ui mem-report {arg} ({} modules)", mods.len()),
                Err(e) => format!("FAIL ui mem-report {arg}: {e}"),
            }
        }
        // P-53: remember the loaded modules (`modules_new` lists what loaded since).
        "modules-mark" => {
            let names = crate::perf::modules().into_iter().map(|(n, _)| n).collect();
            MOD_MARK.with(|m| *m.borrow_mut() = Some(names));
            "ok ui modules-mark".into()
        }
        // P-53: run any pending jump-list update now and wait for the host (≤ 10 s).
        "jumplist-flush" => {
            let dump = super::app::jump_dump_path();
            let _ = std::fs::remove_file(&dump);
            let (started, code) = crate::jumplist::flush_and_wait(10_000);
            let text = std::fs::read_to_string(&dump).unwrap_or_default();
            JUMP.with(|j| *j.borrow_mut() = (started, code, text));
            format!("ok ui jumplist-flush (started {started}, exit {code:?})")
        }
        "job-reset" => {
            JOB_SEQ.with(|q| q.set((0, 0, true, 0)));
            "ok ui job-reset".into()
        }
        // F-37 scroll trace: `ui scroll-trace on`, then `ui scroll-trace <file>` writes it.
        "scroll-trace" => {
            if arg == "on" {
                super::strace::on();
                return "ok ui scroll-trace on".into();
            }
            let lines = super::strace::take().unwrap_or_default();
            TRACE.with(|t| *t.borrow_mut() = Some(yamdv_core::scroll::trace_stats(&lines)));
            match std::fs::write(rel(arg), lines.join("\n")) {
                Ok(()) => format!("ok ui scroll-trace {arg} {} lines", lines.len()),
                Err(e) => format!("FAIL scroll-trace {arg}: {e}"),
            }
        }
        // F-37: N wheel notches posted at once (a fast burst; `-N` scrolls up).
        "wheel-burst" => {
            // `N` notches of 120, or `N:D` messages of delta D (free-spinning wheels send small
            // non-120 deltas at hundreds per second).
            let (n, step) = arg.split_once(':').unwrap_or((arg, "120"));
            let n: i32 = n.parse().unwrap_or(20);
            let step: i16 = step.parse().unwrap_or(120);
            let d: i16 = if n < 0 { step } else { -step };
            for _ in 0..n.abs() {
                // SAFETY: valid hwnd; posted message with plain integer parameters.
                let _ = unsafe {
                    windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                        Some(w.hwnd),
                        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL,
                        windows::Win32::Foundation::WPARAM((d as u16 as usize) << 16),
                        windows::Win32::Foundation::LPARAM(0),
                    )
                };
            }
            format!("ok ui wheel-burst {n}")
        }
        // F-37 free-spin wheel: `N:D:MS` = N messages of delta D, one every MS ms, posted from
        // a helper thread (hundreds per second over seconds; `-N` scrolls up).
        "wheel-stream" => {
            let mut it = arg.split(':').map(|v| v.parse::<i32>().unwrap_or(0));
            let (n, step, ms) = (
                it.next().unwrap_or(600),
                it.next().unwrap_or(15),
                it.next().unwrap_or(3),
            );
            let d = if n < 0 { step } else { -step } as i16;
            let h = w.hwnd.0 as isize;
            std::thread::spawn(move || {
                for _ in 0..n.abs() {
                    // SAFETY: posting to a window of this process; plain integer parameters.
                    let _ = unsafe {
                        windows::Win32::UI::WindowsAndMessaging::PostMessageW(
                            Some(windows::Win32::Foundation::HWND(h as *mut _)),
                            windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL,
                            windows::Win32::Foundation::WPARAM((d as u16 as usize) << 16),
                            windows::Win32::Foundation::LPARAM(0),
                        )
                    };
                    std::thread::sleep(std::time::Duration::from_millis(ms.max(1) as u64));
                }
            });
            format!("ok ui wheel-stream {arg}")
        }
        // 018a scroll-fling: wheel messages count as precision-touchpad input (direct scroll).
        "touchpad" => {
            sys::TOUCHPAD_FORCED.store(arg == "on", std::sync::atomic::Ordering::Relaxed);
            format!("ok ui touchpad {arg}")
        }
        // 018a scroll-fling: scroll directly so that one slow notch down ends `arg` DIPs above
        // the settle top of a heading below the first screen (`snap_dist` reads the distance).
        "near-heading" => {
            let dist: f64 = arg.parse().unwrap_or(10.0);
            match w.near_heading(dist) {
                Some((top, block)) => {
                    SNAP_BLOCK.with(|c| c.set(Some(block)));
                    format!("ok ui near-heading {arg} top={top:.1}")
                }
                None => format!("FAIL ui near-heading {arg}: no heading"),
            }
        }
        "motion-on" => {
            // The GIF check needs animations; CI runners run with reduced motion.
            sys::MOTION_FORCED.store(true, std::sync::atomic::Ordering::Relaxed);
            "ok ui motion-on".into()
        }
        "cpu-mark" => {
            CPU_MARK.with(|m| m.set(crate::perf::cpu_ms()));
            "ok ui cpu-mark".into()
        }
        "height-mark" => {
            HEIGHT_MARK.with(|m| m.set(doc_height(w)));
            "ok ui height-mark".into()
        }
        // FUT-47: simulated Windows High Contrast (black = dark scheme, white = light).
        "hc-on" => {
            let c = if arg == "white" {
                yamdv_core::hc::WHITE
            } else {
                yamdv_core::hc::BLACK
            };
            super::app::hc_force(Some(c));
            w.restyle(sh);
            format!("ok ui hc-on {arg}")
        }
        "hc-off" => {
            super::app::hc_force(None);
            w.restyle(sh);
            "ok ui hc-off".into()
        }
        // Prompt 019 §4: starts a separate UIA client process against this window (no wait:
        // the UI thread keeps pumping and answers its WM_GETOBJECT / provider calls).
        "uia-client" => {
            use std::os::windows::process::CommandExt;
            // `ui uia-client <exe> <out> [steps]` (F-42: steps go to the client unchanged).
            let (exe, rest) = arg.split_once(' ').unwrap_or((arg, "uia-tree.txt"));
            let (out, steps) = rest.split_once(' ').unwrap_or((rest, ""));
            let child = std::process::Command::new(rel(exe))
                .arg((w.hwnd.0 as usize).to_string())
                .arg(rel(out))
                .arg(steps)
                .creation_flags(0x0800_0000 /* CREATE_NO_WINDOW */)
                .spawn();
            match child {
                Ok(c) => {
                    UIA_EXIT.with(|e| e.set(-1));
                    UIA_CHILD.with(|u| *u.borrow_mut() = Some((c, std::time::Instant::now())));
                    "ok ui uia-client".into()
                }
                Err(e) => format!("FAIL ui uia-client: {e}"),
            }
        }
        "uia-wait" => {
            let done = UIA_CHILD.with(|u| {
                let mut u = u.borrow_mut();
                let (c, t0) = u.as_mut()?;
                match c.try_wait() {
                    Ok(Some(st)) => Some(Ok(st.code().unwrap_or(-2))),
                    Ok(None) if t0.elapsed().as_secs() < 20 => None,
                    Ok(None) => {
                        let _ = c.kill();
                        Some(Err("timeout"))
                    }
                    Err(_) => Some(Err("wait failed")),
                }
            });
            match done {
                None if UIA_CHILD.with(|u| u.borrow().is_some()) => again(50),
                None => "FAIL ui uia-wait: no client".into(),
                Some(r) => {
                    UIA_CHILD.with(|u| *u.borrow_mut() = None);
                    match r {
                        Ok(code) => {
                            UIA_EXIT.with(|e| e.set(code));
                            format!("ok ui uia-wait {code}")
                        }
                        Err(e) => format!("FAIL ui uia-wait: {e}"),
                    }
                }
            }
        }
        "mem-mark" => {
            MEM_MARK.with(|m| m.set(pws_now()));
            GUI_MARK.with(|m| m.set(gui_now()));
            "ok ui mem-mark".into()
        }
        "job-cancel" => {
            super::jobui::cancel_job();
            "ok ui job-cancel".to_string()
        }
        v if v.starts_with("nav-") => w.nav_ui(sh, v, arg),
        v if v.starts_with("v2e-") => w.v2e_ui(sh, v, arg),
        v if v.starts_with("v2f-") => w.v2f_ui(sh, v, arg),
        _ => format!("FAIL ui {verb}: unknown verb"),
    }
}

/// Context-menu command by script name.
fn command_id(name: &str) -> Option<u32> {
    let (verb, arg) = name.split_once(' ').unwrap_or((name, ""));
    let idx = |list: &[&str]| {
        list.iter()
            .position(|v| *v == arg)
            .and_then(|i| u32::try_from(i).ok())
    };
    Some(match verb {
        "mode" => app::MODE0 + idx(&["system", "light", "dark"])?,
        "insert" => {
            app::INS0
                + idx(&[
                    "table",
                    "code",
                    "note",
                    "tip",
                    "important",
                    "warning",
                    "caution",
                    "math",
                    "mermaid",
                    "rule",
                    "image",
                    "link",
                    "footnote",
                ])?
        }
        "scheme" => {
            let names: Vec<&str> = app::SCHEMES.iter().map(|s| s.0).collect();
            app::SCHEME0 + idx(&names)?
        }
        "favorite" => app::FAVORITE,
        "outline" => app::OUTLINE,
        "reload" => app::RELOAD,
        "select-all" => app::SELECT_ALL,
        "restore-position" => app::RESTORE_POS,
        "live" => app::LIVE_EDIT,
        "split" => app::SPLIT_EDIT,
        "swap" => app::SWAP_PANES,
        "autosave" => app::AUTOSAVE,
        "undo" => app::UNDO,
        "redo" => app::REDO,
        "line-numbers" => app::ELINES,
        "soft-wrap" => app::EWRAP,
        "fullscreen" => app::FULLSCREEN,
        "word-count" => app::WORD_COUNT,
        "scheme-cards" => app::SCHEME_CARDS,
        "settings-menu" => app::SETTINGS_MENU,
        "more-menu" => app::MORE_MENU,
        "font-big" => app::FONT_BIG,
        "font-reset" => app::FONT_RESET,
        "copy-md" => app::COPY_MD,
        "copy-rich" => app::COPY_RICH,
        // `reopen-encoding windows-1254` (R-OPEN-06, 018a §2-D).
        "reopen-encoding" => {
            let labels: Vec<&str> = yamdv_core::decode::REOPEN.iter().map(|r| r.1).collect();
            app::ENC0 + idx(&labels)?
        }
        _ => return None,
    })
}

/// Clicks the first rendered occurrence of `text` (or the checkbox of its task item).
/// What `click_text` aims at.
#[derive(Clone, Copy, PartialEq)]
enum Click {
    Text,
    Task,
    Render,
}

fn click_text(w: &mut Win, sh: &Shared, text: &str, what: Click) -> String {
    use yamdv_layout::block::HotKind;
    let found = match &w.view {
        View::Doc(d) => d
            .content
            .source()
            .find(text)
            .map(|p| (p, super::edit::block_of(&d.content, p))),
        _ => None,
    };
    let Some((pos, bi)) = found else {
        return format!("FAIL click-text \"{text}\": not in the document");
    };
    let top = match &w.view {
        View::Doc(d) => d.viewer.block_top(&d.content, bi),
        _ => 0.0,
    };
    let s = w.scroll_pos();
    if top < s + 40.0 || top > s + w.view_h() - 120.0 {
        w.set_scroll(top - 80.0);
    }
    if let Some(a) = w.anim.take() {
        w.set_scroll(a.to);
    }
    w.prepare_now(sh);
    let (cx, _) = w.col(sh);
    let pt = (|| {
        let View::Doc(d) = &w.view else { return None };
        let bl = d.viewer.layout_of(bi)?;
        let top = (d.viewer.block_top(&d.content, bi) - d.viewer.scroll) as f32;
        if what == Click::Render {
            let h = bl
                .hot
                .iter()
                .find(|h| matches!(h.kind, HotKind::RenderAnyway { .. }))?;
            return Some((
                cx + h.rect.x + h.rect.w / 2.0,
                top + h.rect.y + h.rect.h / 2.0,
            ));
        }
        if what == Click::Task {
            let h = bl
                .hot
                .iter()
                .rfind(|h| matches!(h.kind, HotKind::Task { item } if item as usize <= pos))?;
            return Some((
                cx + h.rect.x + h.rect.w / 2.0,
                top + h.rect.y + h.rect.h / 2.0,
            ));
        }
        let (a, b) = (
            u32::try_from(pos).ok()?,
            u32::try_from(pos + text.len()).ok()?,
        );
        bl.paras.iter().find_map(|p| {
            let r = *yamdv_layout::viewer::selection_rects(p, &(a..b)).first()?;
            Some((cx + p.x + r.x + r.w / 2.0, top + p.y + r.y + r.h / 2.0))
        })
    })();
    let Some((x, y)) = pt else {
        return format!("FAIL click-text \"{text}\": not laid out");
    };
    if !w.edit_mouse_down(sh, x, y, false) {
        w.mouse_down(sh, x, y, false);
    }
    if !w.edit_mouse_up(sh) {
        w.mouse_up();
    }
    format!(
        "ok click-{} \"{text}\" at {x:.0},{y:.0}",
        match what {
            Click::Text => "text",
            Click::Task => "task",
            Click::Render => "render",
        }
    )
}

/// Escapes CR/LF for one-line state values (scripts compare against `\n` / `\r`).
fn esc_text(s: &str) -> String {
    s.chars()
        .take(4000)
        .collect::<String>()
        .replace('\r', "\\r")
        .replace('\n', "\\n")
}

/// First link whose text contains `text` (document order).
fn find_link(w: &mut Win, text: &str) -> Option<(usize, u32)> {
    let View::Doc(d) = &mut w.view else {
        return None;
    };
    let Content::Markdown(doc) = &mut d.content else {
        return None;
    };
    let n = doc.index.blocks.len();
    for b in 0..n {
        let det = doc.detail(b);
        let src = doc.source.as_str();
        let mut acc: Vec<(u32, String)> = Vec::new();
        for r in det.runs.iter().filter(|r| r.link != NO_LINK) {
            match acc.iter_mut().find(|(l, _)| *l == r.link) {
                Some((_, s)) => s.push_str(r.text(src)),
                None => acc.push((r.link, r.text(src).to_string())),
            }
        }
        if let Some((l, _)) = acc.into_iter().find(|(_, s)| s.contains(text)) {
            return Some((b, l));
        }
    }
    None
}

fn link_text(w: &mut Win, b: usize, l: u32) -> String {
    let View::Doc(d) = &mut w.view else {
        return String::new();
    };
    let Content::Markdown(doc) = &mut d.content else {
        return String::new();
    };
    let det = doc.detail(b);
    let src = doc.source.as_str();
    det.runs
        .iter()
        .filter(|r| r.link == l)
        .map(|r| r.text(src))
        .collect()
}

fn doc_height(w: &Win) -> f64 {
    match &w.view {
        View::Doc(d) => d.viewer.total_height(),
        _ => 0.0,
    }
}

/// Formulas of the open Markdown document that fail to expand or parse (font-free probe).
fn math_errors(w: &Win) -> usize {
    match &w.view {
        View::Doc(d) => match &d.content {
            yamdv_layout::viewer::Content::Markdown(doc) => yamdv_layout::rich::math_errors(doc),
            _ => 0,
        },
        _ => 0,
    }
}

fn math_display_errors(w: &Win) -> usize {
    match &w.view {
        View::Doc(d) => match &d.content {
            yamdv_layout::viewer::Content::Markdown(doc) => {
                yamdv_layout::rich::math_display_errors(doc)
            }
            _ => 0,
        },
        _ => 0,
    }
}

/// BUG-01: the scroll position and document of the newest other window (0 / empty if none).
fn other_window(w: &Win) -> (f64, String) {
    let others: Vec<_> = WINDOWS.with(|m| {
        m.borrow()
            .iter()
            .filter(|(h, _)| **h != w.hwnd.0 as isize)
            .map(|(_, c)| c.clone())
            .collect()
    });
    others
        .iter()
        .find_map(|c| {
            let o = c.try_borrow().ok()?;
            let doc = match &o.view {
                View::Doc(d) => d.path.rsplit(['\\', '/']).next().unwrap_or("").to_string(),
                _ => String::new(),
            };
            Some((o.scroll_pos(), doc))
        })
        .unwrap_or_default()
}

fn state(w: &mut Win, sh: &Shared) -> Vec<(String, serde_like::V)> {
    use serde_like::V;
    let doc = match &w.view {
        View::Doc(d) => yamdv_core::path::file_name(&d.path).to_string(),
        View::Empty { .. } => "none".into(),
        View::Error { .. } => "error".into(),
    };
    let focus = match w.focus {
        Some((b, l)) => link_text(w, b, l),
        None => "none".into(),
    };
    let blocked = yamdv_core::i18n::tr(sh.lang, yamdv_core::i18n::Msg::BlockedSafety);
    let popover = match &w.dialog {
        None => w.tz_state().5.unwrap_or("none"),
        Some((_, Pending::Local { .. })) => "confirm-local",
        Some((d, Pending::Reveal { .. })) if d.title == blocked => "blocked",
        Some((_, Pending::Reveal { .. })) => "folder",
        Some((_, Pending::Edit(super::edit::Ask::Conflict))) => "conflict",
        Some((_, Pending::Edit(super::edit::Ask::Blocked(_)))) => "edit-blocked",
        Some((_, Pending::Edit(super::edit::Ask::Unmappable))) => "unmappable",
        Some((_, Pending::Edit(super::edit::Ask::Unsaved))) => "unsaved",
        Some(_) => "document-guard",
    };
    let windows = WINDOWS.with(|m| m.borrow().len());
    let other = other_window(w);
    let (find, find_count, find_index) = match &w.find {
        Some(f) => ("open", f.matches.len(), f.cur.map_or(0, |c| c + 1)),
        None => ("closed", 0, 0),
    };
    let (source_len, (math_cached, diagrams_cached)) = match &w.view {
        View::Doc(d) => (d.content.source().len(), d.rich.len()),
        _ => (0, (0, 0)),
    };
    let favorites = app::with(|a| a.state.favorites.len()).unwrap_or(0);
    let edit_mode = ["read", "live", "split"][usize::from(w.edit_mode_id())];
    let (dirty, active, caret, sel_len, buffer, undo, t5) = match w.edit() {
        Some(e) => {
            let mut l = e.lat.clone();
            l.sort_by(f64::total_cmp);
            let p99 = l
                .get(((l.len() as f64 * 0.99).ceil() as usize).saturating_sub(1))
                .copied()
                .unwrap_or(0.0);
            (
                u8::from(e.ed.is_dirty()),
                e.active.map_or(-1.0, |a| a.block as f64),
                e.ed.caret(),
                e.ed.selection().len(),
                esc_text(&e.ed.text()),
                e.ed.history().undo_len(),
                (p99, l.len()),
            )
        }
        None => (0, -1.0, 0, 0, String::new(), 0, (0.0, 0)),
    };
    let (fchg, fbom, fcrlf) = w.v2f_file_probe();
    let (preview, file) = match &w.view {
        View::Doc(d) if !d.virt => (
            esc_text(d.content.source()),
            std::fs::read(yamdv_core::path::long_form(&d.path))
                .map(|b| esc_text(&String::from_utf8_lossy(&b)))
                .unwrap_or_default(),
        ),
        _ => (String::new(), String::new()),
    };
    let title = {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
        let mut buf = [0u16; 512];
        // SAFETY: valid hwnd and buffer.
        let n = unsafe { GetWindowTextW(w.hwnd, &mut buf) };
        String::from_utf16_lossy(buf.get(..usize::try_from(n).unwrap_or(0)).unwrap_or(&[]))
    };
    let (menu_items, menu_checked) = app::menu_probe(&w.menu_ctx(None));
    let (p_open, p_levels, p_root, p_items, p_kind, p_first) = super::popup::probe();
    let disk = std::fs::read(app::settings_path()).unwrap_or_default();
    let disk_scheme = yamdv_core::settings::Settings::from_ini(&yamdv_core::ini::Ini::parse(&disk))
        .str("appearance.scheme")
        .to_string();
    let (badge, words, chars) = match w.badge(sh) {
        Some((_, t)) => {
            let n: Vec<String> = t
                .split(|c: char| !c.is_ascii_digit() && c != '/')
                .filter(|x| !x.is_empty() && x.chars().any(|c| c.is_ascii_digit()))
                .map(str::to_string)
                .collect();
            (
                esc_text(&t),
                n.first().cloned().unwrap_or_default(),
                n.get(1).cloned().unwrap_or_default(),
            )
        }
        None => ("none".to_string(), String::new(), String::new()),
    };
    let (job, job_n, job_total) = match super::jobui::state() {
        Some((_, n, t, true)) => ("cancelling", n, t),
        Some((_, n, t, false)) => ("running", n, t),
        None => ("none", 0, 0),
    };
    let out = LAST_OUT
        .with(|l| l.borrow().clone())
        .and_then(|p| std::fs::read(p).ok())
        .unwrap_or_default();
    let out_text = String::from_utf8_lossy(&out).into_owned();
    let lower = out_text.to_ascii_lowercase();
    let srcs: Vec<&str> = lower.split(" src=\"").skip(1).collect();
    let launch = sys::last_launch();
    let (doc_path, doc_enc) = match &w.view {
        View::Doc(d) => (d.path.clone(), d.encoding.to_string()),
        _ => (String::new(), String::new()),
    };
    let cf = sys::clip_bytes(w.hwnd, sys::html_format(), 8 << 20)
        .map(|b| {
            String::from_utf8_lossy(&b)
                .trim_end_matches('\0')
                .to_string()
        })
        .unwrap_or_default();
    let tz = w.tz_state();
    let or_none = |s: &str| {
        if s.is_empty() {
            "none".to_string()
        } else {
            s.to_string()
        }
    };
    let mut out = vec![
        ("mmd".into(), V::S(w.mmd_state())),
        ("document".into(), V::S(doc)),
        ("scroll_y".into(), V::N(w.scroll_pos())),
        (
            // PERF-06: the scroll anchor equals the one saved by `ui anchor-mark`.
            "anchor_same".into(),
            V::N(f64::from(u8::from(
                match (&w.view, ANCHOR_MARK.with(std::cell::Cell::get)) {
                    (super::View::Doc(d), Some((b, off))) => {
                        let (b2, off2) = d.viewer.anchor();
                        b2 == b && (off2 - off).abs() < 0.5
                    }
                    _ => false,
                },
            ))),
        ),
        (
            "wheel_active".into(),
            V::N(f64::from(u8::from(w.wm.active || w.anim.is_some()))),
        ),
        (
            "snap_dist".into(),
            // The heading top is read now: estimated heights above it are corrected while scrolling.
            V::N(
                SNAP_BLOCK
                    .with(std::cell::Cell::get)
                    .and_then(|b| w.heading_top(b))
                    .map_or(-1.0, |t| (t - w.scroll_pos()).abs()),
            ),
        ),
        ("focus".into(), V::S(focus)),
        ("popover".into(), V::S(popover.into())),
        ("banner".into(), V::S(or_none(&tz.1))),
        ("tooltip".into(), V::S(or_none(&tz.0))),
        ("viewer".into(), V::S(or_none(&tz.2))),
        ("viewer_zoom".into(), V::N(tz.3)),
        ("folds".into(), V::N(tz.4 as f64)),
        ("doc_height".into(), V::N(doc_height(w))),
        (
            "doc_height_delta".into(),
            V::N(doc_height(w) - HEIGHT_MARK.with(std::cell::Cell::get)),
        ),
        ("job".into(), V::S(job.into())),
        ("job_n".into(), V::N(f64::from(job_n))),
        ("job_total".into(), V::N(f64::from(job_total))),
        ("job_last".into(), V::S(super::jobui::last_outcome().into())),
        (
            "out_exists".into(),
            V::N(f64::from(u8::from(
                LAST_OUT
                    .with(|l| l.borrow().clone())
                    .is_some_and(|p| std::path::Path::new(&p).exists()),
            ))),
        ),
        ("nav_depth".into(), V::N(w.history.len() as f64)),
        ("nav_pos".into(), V::N(w.hist_pos as f64)),
        ("windows".into(), V::N(windows as f64)),
        ("other_scroll_y".into(), V::N(other.0)),
        ("other_document".into(), V::S(other.1)),
        ("launched".into(), V::N(f64::from(sys::launches()))),
        ("find".into(), V::S(find.into())),
        ("find_count".into(), V::N(find_count as f64)),
        ("find_index".into(), V::N(find_index as f64)),
        ("scheme".into(), V::S(sh.scheme.clone())),
        ("mode".into(), V::S(sh.mode.clone())),
        ("dark".into(), V::N(f64::from(u8::from(w.dark)))),
        (
            "sidebar".into(),
            V::S(if sh.side_visible { "visible" } else { "hidden" }.into()),
        ),
        ("favorites".into(), V::N(favorites as f64)),
        ("source_len".into(), V::N(source_len as f64)),
        (
            "reload_ms".into(),
            V::N(super::app::LAST_RELOAD.with(std::cell::Cell::get).0),
        ),
        (
            // PERF-06: blocks re-parsed by the last live reload (-1 = full reload).
            "reload_blocks".into(),
            V::N(match super::app::LAST_RELOAD.with(std::cell::Cell::get).1 {
                usize::MAX => -1.0,
                n => n as f64,
            }),
        ),
        ("math_cached".into(), V::N(math_cached as f64)),
        ("diagrams_cached".into(), V::N(diagrams_cached as f64)),
        (
            "img_decoded".into(),
            V::N(match &w.view {
                View::Doc(d) => d.images.summary().0 as f64,
                _ => 0.0,
            }),
        ),
        (
            "img_fail".into(),
            V::S(match &w.view {
                View::Doc(d) => d.images.summary().1,
                _ => String::new(),
            }),
        ),
        (
            "gif_playing".into(),
            V::N(match &w.view {
                View::Doc(d) => d.images.anims_playing() as f64,
                _ => 0.0,
            }),
        ),
        (
            "window_active".into(),
            V::N(f64::from(u8::from(!w.cap.inactive))),
        ),
        (
            "gif_due_ms".into(),
            V::N(match &w.view {
                View::Doc(d) => d
                    .images
                    .anim_due()
                    .map_or(-1.0, |t| t as f64 - crate::images::gif::now_ms() as f64),
                _ => -1.0,
            }),
        ),
        (
            "reduced_motion".into(),
            V::N(f64::from(u8::from(!crate::sys::animations_enabled()))),
        ),
        (
            "gif_frames".into(),
            V::N(crate::images::gif::FRAMES.load(std::sync::atomic::Ordering::Relaxed) as f64),
        ),
        ("menu_items".into(), V::N(menu_items as f64)),
        ("menu_checked".into(), V::S(menu_checked)),
        ("edit_mode".into(), V::S(edit_mode.into())),
        ("dirty".into(), V::N(f64::from(dirty))),
        ("active_block".into(), V::N(active)),
        ("caret".into(), V::N(caret as f64)),
        ("sel_len".into(), V::N(sel_len as f64)),
        ("revealed".into(), V::S(w.revealed_markers().join(" "))),
        ("f2_preview".into(), V::S(w.preview_kind().into())),
        ("lang_items".into(), V::S(w.lang_probe())),
        (
            "doc_blocks".into(),
            V::N(match &w.view {
                View::Doc(d) => d.content.len() as f64,
                _ => 0.0,
            }),
        ),
        ("file_changed_blocks".into(), V::N(fchg)),
        ("file_bom".into(), V::N(fbom)),
        ("file_crlf".into(), V::N(fcrlf)),
        ("table_cell".into(), V::S(w.table_probe())),
        (
            "table_picker".into(),
            V::S(w.wysi.pick.map(super::wysi::pick_label).unwrap_or_default()),
        ),
        (
            "tsv_offer".into(),
            V::N(f64::from(u8::from(w.tsv_offered()))),
        ),
        ("table_tools".into(), V::N(w.table_tools(sh).len() as f64)),
        ("img_sel".into(), V::N(f64::from(u8::from(w.img_probe().0)))),
        (
            "xsel".into(),
            V::S(w.xsel().map_or(String::new(), |r| {
                w.edit().map_or(String::new(), |e| esc_text(&e.ed.slice(r)))
            })),
        ),
        (
            "clip_md".into(),
            V::S(
                sys::clip_bytes(w.hwnd, sys::md_format(), 1 << 20)
                    .map(|b| esc_text(String::from_utf8_lossy(&b).trim_end_matches('\0')))
                    .unwrap_or_default(),
            ),
        ),
        (
            "clip_text".into(),
            V::S(sys::paste_text(w.hwnd).unwrap_or_default()),
        ),
        (
            "clip_img".into(),
            V::N(f64::from(u8::from(sys::clip_has(8)))),
        ),
        (
            "clip_html".into(),
            V::N(f64::from(u8::from(sys::clip_has(sys::html_format())))),
        ),
        (
            "link_pop".into(),
            V::S(
                w.wysi
                    .link
                    .as_ref()
                    .map(|p| p.fields.join("|"))
                    .unwrap_or_default(),
            ),
        ),
        ("undo_depth".into(), V::N(undo as f64)),
        ("buffer".into(), V::S(buffer)),
        ("preview".into(), V::S(preview)),
        ("file".into(), V::S(file)),
        ("title".into(), V::S(title)),
        ("out_len".into(), V::N(out.len() as f64)),
        (
            "out_body_bg".into(),
            V::S(
                out_text
                    .split_once("body{margin:0;background:")
                    .map(|(_, r)| r.chars().take_while(|c| *c != ';').collect())
                    .unwrap_or_default(),
            ),
        ),
        (
            "out_hex".into(),
            V::S(sys::hex(out.get(..out.len().min(8)).unwrap_or(&[]))),
        ),
        (
            "out_text".into(),
            V::S(esc_text(&out_text.chars().take(600).collect::<String>())),
        ),
        (
            "out_scripts".into(),
            V::N(lower.matches("<script").count() as f64),
        ),
        (
            "out_csp".into(),
            V::N(f64::from(u8::from(
                out_text.contains(yamdv_export::html::EXPORT_CSP),
            ))),
        ),
        ("out_imgs".into(), V::N(srcs.len() as f64)),
        ("last_launch".into(), V::S(launch.0.clone())),
        (
            "last_launch_roundtrip".into(),
            V::N(f64::from(u8::from(
                !launch.1.is_empty()
                    && yamdv_core::cmdline::split_args(&launch.0)
                        == std::iter::once(launch.1.clone())
                            .chain(launch.2.iter().cloned())
                            .collect::<Vec<_>>(),
            ))),
        ),
        ("doc_encoding".into(), V::S(doc_enc)),
        (
            "file_same".into(),
            V::N(f64::from(u8::from(BYTES_MARK.with(|m| {
                let m = m.borrow();
                !m.is_empty() && std::fs::read(&doc_path).is_ok_and(|b| b == *m)
            })))),
        ),
        (
            "out_csp_hash_ok".into(),
            V::N(f64::from(u8::from(yamdv_export::check::csp_hash_ok(
                &out_text,
            )))),
        ),
        (
            "out_ext_refs".into(),
            V::N(yamdv_export::check::external_refs(&out_text) as f64),
        ),
        ("out_svg".into(), V::N(lower.matches("<svg").count() as f64)),
        (
            "out_png_w".into(),
            V::N(yamdv_export::check::png_ok(&out).map_or(0.0, |(w, _)| f64::from(w))),
        ),
        (
            "clip_html_scripts".into(),
            V::N(cf.to_ascii_lowercase().matches("<script").count() as f64),
        ),
        (
            "clip_html_offsets_ok".into(),
            V::N(f64::from(u8::from(
                !cf.is_empty() && yamdv_export::check::cf_html_offsets_ok(&cf),
            ))),
        ),
        (
            "out_ext_src".into(),
            V::N(
                srcs.iter()
                    .filter(|s| !s.starts_with("data:image/"))
                    .count() as f64,
            ),
        ),
        ("t5_p99_ms".into(), V::N(t5.0)),
        ("t5_n".into(), V::N(t5.1 as f64)),
        ("popup_open".into(), V::N(f64::from(u8::from(p_open)))),
        ("popup_levels".into(), V::N(p_levels as f64)),
        ("popup_root_items".into(), V::N(p_root as f64)),
        ("popup_items".into(), V::N(p_items as f64)),
        ("popup_kind".into(), V::S(p_kind.into())),
        ("popup_first".into(), V::S(p_first)),
        ("maximized".into(), V::N(f64::from(u8::from(w.maximized())))),
        ("fullscreen".into(), V::N(f64::from(u8::from(w.fs)))),
        ("chrome_top".into(), V::N(f64::from(w.top))),
        (
            "chrome_visible".into(),
            V::N(f64::from(u8::from(w.chrome_visible()))),
        ),
        ("badge".into(), V::S(badge)),
        ("words".into(), V::S(words)),
        ("chars".into(), V::S(chars)),
        (
            "zoom_visible".into(),
            V::N(f64::from(u8::from(w.zoom_ind.is_some()))),
        ),
        (
            "zoom_text".into(),
            V::S(w.zoom_ind.clone().unwrap_or_default()),
        ),
        ("disk_scheme".into(), V::S(disk_scheme)),
    ];
    out.extend(
        w.v2e_state(sh)
            .into_iter()
            .map(|(k, n)| (k.to_string(), V::N(n))),
    );
    out.push(("pws_kib".into(), V::N((pws_now() / 1024) as f64)));
    out.push(("hc".into(), V::N(f64::from(u8::from(yamdv_core::hc::on())))));
    out.push((
        "uia_exit".into(),
        V::N(f64::from(UIA_EXIT.with(std::cell::Cell::get))),
    ));
    out.push(("pws_delta_kib".into(), V::N(pws_delta_kib())));
    let (g0, u0) = GUI_MARK.with(std::cell::Cell::get);
    let (g, u) = gui_now();
    out.push(("gdi_delta".into(), V::N(f64::from(g) - f64::from(g0))));
    out.push(("user_delta".into(), V::N(f64::from(u) - f64::from(u0))));
    out.push((
        "code_line_numbers".into(),
        V::N(f64::from(u8::from(sh.read.code_line_numbers))),
    ));
    // `ini.<section>.<key>` = settings.ini on disk; `theme.<light|dark>.<token>` = custom-theme.ini.
    let ini = yamdv_core::ini::Ini::parse(&disk);
    for sec in [
        "reading",
        "appearance",
        "typography",
        "shortcuts",
        "general",
        "privacy",
    ] {
        for (k, v) in ini.entries(sec) {
            out.push((format!("ini.{sec}.{k}"), V::S(v.to_string())));
        }
    }
    let th = std::fs::read(app::custom_theme_path()).unwrap_or_default();
    let th = yamdv_core::ini::Ini::parse(&th);
    for sec in ["light", "dark"] {
        for (k, v) in th.entries(sec) {
            out.push((format!("theme.{sec}.{k}"), V::S(v.to_string())));
        }
    }
    let new_mods: Vec<String> = MOD_MARK.with(|m| {
        m.borrow().as_ref().map_or_else(Vec::new, |mark| {
            crate::perf::modules()
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| !mark.contains(n))
                .collect()
        })
    });
    out.push(("modules_new".into(), V::N(new_mods.len() as f64)));
    out.push(("modules_new_list".into(), V::S(new_mods.join(" "))));
    // P-53: shell / property-system modules a jump-list update would pull into this process.
    const SHELL_MODS: [&str; 8] = [
        "shell32.dll",
        "propsys.dll",
        "windows.storage.dll",
        "explorerframe.dll",
        "twinapi.appcore.dll",
        "thumbcache.dll",
        "windows.staterepositoryps.dll",
        "cfgmgr32.dll",
    ];
    out.push((
        "modules_new_shell".into(),
        V::N(
            new_mods
                .iter()
                .filter(|n| SHELL_MODS.contains(&n.to_ascii_lowercase().as_str()))
                .count() as f64,
        ),
    ));
    JUMP.with(|j| {
        let (started, code, text) = &*j.borrow();
        out.push((
            "jumplist_started".into(),
            V::N(f64::from(u8::from(*started))),
        ));
        out.push(("jumplist_exit".into(), V::N(code.map_or(-1.0, f64::from))));
        let field = |k: &str| {
            text.lines()
                .filter_map(|l| l.strip_prefix(k))
                .collect::<Vec<_>>()
                .join("|")
        };
        out.push(("jumplist_written".into(), V::S(field("written "))));
        out.push(("jumplist_items".into(), V::S(field("item "))));
        out.push(("jumplist_category".into(), V::S(field("category "))));
    });
    TRACE.with(|t| {
        let t = t.borrow();
        let Some(t) = t.as_ref() else { return };
        let notch = f64::from(sys::wheel_lines()) * super::LINE_STEP;
        out.push(("trace_frames".into(), V::N(t.frames as f64)));
        out.push(("trace_p99".into(), V::N(t.p99_ms)));
        out.push(("trace_p95".into(), V::N(t.p95_ms)));
        out.push((
            "trace_monotonic".into(),
            V::N(f64::from(u8::from(t.monotonic))),
        ));
        out.push(("trace_max_stall".into(), V::N(t.max_stall as f64)));
        out.push(("trace_end_err".into(), V::N(t.end_err)));
        out.push(("trace_end_notches".into(), V::N(t.end_err / notch.max(1.0))));
        out.push(("trace_settle_ms".into(), V::N(t.settle_ms)));
        out.push(("trace_settles".into(), V::N(t.settles as f64)));
        out.push(("trace_inputs".into(), V::N(f64::from(t.inputs))));
    });
    let (samples, seen, mono, seen_total) = JOB_SEQ.with(std::cell::Cell::get);
    out.push(("job_samples".into(), V::N(f64::from(samples))));
    out.push(("job_seen_n".into(), V::N(f64::from(seen))));
    out.push(("job_seen_total".into(), V::N(f64::from(seen_total))));
    out.push((
        "cpu_delta_ms".into(),
        V::N(crate::perf::cpu_ms().saturating_sub(CPU_MARK.with(std::cell::Cell::get)) as f64),
    ));
    out.push(("job_monotonic".into(), V::N(f64::from(u8::from(mono)))));
    out.push((
        "worker".into(),
        V::N(f64::from(u8::from(super::jobui::process().is_some()))),
    ));
    let km = app::keymap();
    for a in yamdv_core::keymap::ACTIONS {
        out.push((format!("key.{}", a.id), V::S(km.shown(a.id))));
    }
    for t in yamdv_core::colors::TOKENS {
        let c = yamdv_core::colors::token(&w.pal, t).unwrap_or(0);
        out.push((format!("pal.{t}"), V::S(yamdv_core::colors::to_hex(c))));
    }
    out.extend(
        w.tb_vars(sh)
            .into_iter()
            .map(|(k, v)| (k.to_string(), V::S(v))),
    );
    out.extend(
        w.panel_state(sh.lang)
            .into_iter()
            .map(|(k, v)| (k.to_string(), V::S(v))),
    );
    out.extend(
        w.nav_state(sh)
            .into_iter()
            .map(|(k, v)| (k.to_string(), V::S(v))),
    );
    out
}

/// Minimal JSON for the state dump (no serde in the product dependency set).
mod serde_like {
    /// Value.
    pub(super) enum V {
        S(String),
        N(f64),
    }
    pub(super) trait Get {
        fn get(&self, k: &str) -> Option<&V>;
    }
    impl Get for Vec<(String, V)> {
        fn get(&self, k: &str) -> Option<&V> {
            self.iter().find(|(n, _)| n == k).map(|(_, v)| v)
        }
    }
    pub(super) fn to_json(v: &[(String, V)]) -> String {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let body: Vec<String> = v
            .iter()
            .map(|(k, x)| match x {
                V::S(s) => format!("\"{}\":\"{}\"", esc(k), esc(s)),
                V::N(n) => format!("\"{}\":{n:.1}", esc(k)),
            })
            .collect();
        format!("{{{}}}", body.join(","))
    }
}

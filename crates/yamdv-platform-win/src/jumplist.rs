// SPDX-License-Identifier: AGPL-3.0-or-later
//! Taskbar jump list of recent files (R-SYS-08, FUT-25): `ICustomDestinationList` with one
//! `IShellLink` per History entry pointing at our exe with the file argument. An empty list
//! (cleared History or History hidden) deletes it.
//!
//! P-53 (S-PROC-07, PERFORMANCE rule 10): the shell components never load into the main
//! process. The main process only notices that the recent list changed and starts
//! `--dialog-host jumplist <pid> <nonce>`, which reads `settings.ini` / `state.ini` and builds
//! the list. At most once per 30 s without a timer (F-38): a change inside the 30 s waits for
//! the next change, a document close or exit.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::time::Instant;

use yamdv_core::throttle::{JUMPLIST_GAP_MS, Throttle};

use windows::Win32::Foundation::PROPERTYKEY;
#[cfg(feature = "dev-tools")]
use windows::Win32::Foundation::WAIT_OBJECT_0;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
#[cfg(feature = "dev-tools")]
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows::Win32::System::Variant::VT_LPWSTR;
use windows::Win32::UI::Shell::Common::{IObjectArray, IObjectCollection};
use windows::Win32::UI::Shell::PropertiesSystem::IPropertyStore;
use windows::Win32::UI::Shell::{
    DestinationList, EnumerableObjectCollection, ICustomDestinationList, IShellLinkW, ShellLink,
};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR};

/// `PKEY_Title` (propkey.h).
const PKEY_TITLE: PROPERTYKEY = PROPERTYKEY {
    fmtid: windows::core::GUID::from_u128(0xf29f85e0_4ff9_1068_ab91_08002b27b3d9),
    pid: 2,
};

thread_local! {
    /// The list the last started host was asked to show (`None` = never started).
    static LAST: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    /// The newest wanted list while a start is pending.
    static WANT: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    /// F-38: at most one host start per 30 s, without a timer.
    static GATE: Cell<Throttle> = const { Cell::new(Throttle::new(JUMPLIST_GAP_MS)) };
    static EPOCH: Instant = Instant::now();
    /// The last started host (driver: `ui jumplist-flush` waits for it).
    static HOST: Cell<Option<(u32, isize)>> = const { Cell::new(None) };
}

fn now_ms() -> u64 {
    EPOCH.with(|e| u64::try_from(e.elapsed().as_millis()).unwrap_or(u64::MAX))
}

fn gate<R>(f: impl FnOnce(&mut Throttle) -> R) -> R {
    GATE.with(|g| {
        let mut t = g.get();
        let r = f(&mut t);
        g.set(t);
        r
    })
}

/// Main process: the recent list is now `paths` (newest first). Starts the host only when the
/// list really changed and never in this process. F-38: no timer — the host starts now when the
/// last start is > 30 s old, else the change waits for the next change, a document close
/// ([`flush`]) or exit.
pub fn update(paths: &[String]) {
    let same_as_last = LAST.with(|l| l.borrow().as_deref() == Some(paths));
    let same_as_want = WANT.with(|w| w.borrow().as_deref() == Some(paths));
    if same_as_last && WANT.with(|w| w.borrow().is_none()) {
        return;
    }
    if same_as_last {
        // Changed back before the host ran: nothing to do.
        WANT.with(|w| *w.borrow_mut() = None);
        gate(Throttle::clear);
        return;
    }
    if !same_as_want {
        WANT.with(|w| *w.borrow_mut() = Some(paths.to_vec()));
    }
    let now = now_ms();
    if gate(|t| t.change(now)) {
        let _ = start_pending();
    }
}

/// Start-up: `paths` is what the jump list already shows (written by the last session).
pub fn seed(paths: Vec<String>) {
    LAST.with(|l| {
        let mut l = l.borrow_mut();
        if l.is_none() {
            *l = Some(paths);
        }
    });
}

/// Document close: a pending change reaches the jump list now (F-38).
pub fn flush() {
    let now = now_ms();
    if gate(|t| t.flush(now)) {
        let _ = start_pending();
    }
}

/// Starts the host for the pending list now. Returns its pid.
fn start_pending() -> Option<u32> {
    let want = WANT.with(|w| w.borrow_mut().take())?;
    let (pid, process) = crate::dialog_host::spawn_detached("jumplist")?;
    LAST.with(|l| *l.borrow_mut() = Some(want));
    if let Some((_, old)) = HOST.with(|h| h.replace(Some((pid, process.0 as isize)))) {
        // SAFETY: handle owned by HOST, closed once.
        let _ = unsafe { CloseHandle(HANDLE(old as *mut _)) };
    }
    Some(pid)
}

/// End of the message loop: a pending change still reaches the jump list.
pub fn at_exit() {
    gate(Throttle::clear);
    let _ = start_pending();
    if let Some((_, h)) = HOST.with(|h| h.take()) {
        // SAFETY: handle owned by HOST, closed once (the host keeps running on its own).
        let _ = unsafe { CloseHandle(HANDLE(h as *mut _)) };
    }
}

/// Driver (`ui jumplist-flush`): start any pending update now and wait for the host. Returns
/// (started, exit code).
#[cfg(feature = "dev-tools")]
pub fn flush_and_wait(ms: u32) -> (bool, Option<u32>) {
    gate(Throttle::clear);
    let started = start_pending().is_some();
    let Some((_, h)) = HOST.with(Cell::get) else {
        return (started, None);
    };
    let h = HANDLE(h as *mut _);
    // SAFETY: valid process handle owned by HOST.
    unsafe {
        if WaitForSingleObject(h, ms) != WAIT_OBJECT_0 {
            return (started, None);
        }
        let mut c = 0u32;
        GetExitCodeProcess(h, &mut c)
            .ok()
            .map_or((started, None), |()| (started, Some(c)))
    }
}

/// Host process (`--dialog-host jumplist`): replaces the recent category with `paths`.
/// Returns the number of items written.
pub fn apply_list(paths: &[String], category: &str) -> windows::core::Result<usize> {
    // SAFETY: COM is initialized by the host; all interfaces are owned smart pointers.
    unsafe { apply(paths, category) }
}

unsafe fn apply(paths: &[String], category: &str) -> windows::core::Result<usize> {
    // SAFETY: see `apply_list`.
    unsafe {
        let dl: ICustomDestinationList =
            CoCreateInstance(&DestinationList, None, CLSCTX_INPROC_SERVER)?;
        if paths.is_empty() {
            return dl.DeleteList(PCWSTR::null()).map(|()| 0);
        }
        let mut slots = 0u32;
        let removed: IObjectArray = dl.BeginList(&mut slots)?;
        let gone = removed_args(&removed);
        let coll: IObjectCollection =
            CoCreateInstance(&EnumerableObjectCollection, None, CLSCTX_INPROC_SERVER)?;
        let exe = HSTRING::from(crate::shell::exe_path());
        let mut n = 0u32;
        for p in paths {
            if n >= slots.max(1) {
                break;
            }
            let args = format!("\"{p}\"");
            if gone.contains(&args) || p.contains('"') {
                continue;
            }
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            link.SetPath(&exe)?;
            link.SetArguments(&HSTRING::from(args.as_str()))?;
            link.SetDescription(&HSTRING::from(p.as_str()))?;
            let shown = yamdv_core::path::display_form(p);
            let title = yamdv_core::path::file_name(&shown);
            let mut w: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
            let mut pv = PROPVARIANT::default();
            (*pv.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*pv.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(w.as_mut_ptr());
            let ps: IPropertyStore = link.cast()?;
            // SetValue copies the value; `w` outlives the call and `pv` is never cleared.
            ps.SetValue(&PKEY_TITLE, &pv)?;
            ps.Commit()?;
            std::mem::forget(pv);
            coll.AddObject(&link)?;
            n += 1;
        }
        let arr: IObjectArray = coll.cast()?;
        dl.AppendCategory(&HSTRING::from(category), &arr)?;
        dl.CommitList()?;
        Ok(n as usize)
    }
}

/// Arguments of the items the user removed from the list (they must not be added again).
unsafe fn removed_args(a: &IObjectArray) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: valid object array from BeginList.
    unsafe {
        for i in 0..a.GetCount().unwrap_or(0) {
            if let Ok(l) = a.GetAt::<IShellLinkW>(i) {
                let mut buf = [0u16; 1024];
                if l.GetArguments(&mut buf).is_ok() {
                    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                    out.push(String::from_utf16_lossy(buf.get(..n).unwrap_or(&[])));
                }
            }
        }
    }
    out
}

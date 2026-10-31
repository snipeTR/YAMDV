// SPDX-License-Identifier: AGPL-3.0-or-later
//! Custom app icon (R-SYS-08, FUT-25): a user `.ico` (S-FILE-04 path rules, ≤ 1 MB, parsed by
//! the ADR-0022 ICO directory parser and decoded by WIC) used as the window/taskbar icon and,
//! when YAMDV is registered, as the document-type `DefaultIcon` (HKCU only, S-REG).
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, HICON, ICON_BIG, ICON_SMALL, ICONINFO, SM_CXICON,
    SM_CXSMICON, SendMessageW, WM_SETICON,
};

/// Largest accepted icon file.
pub const MAX_BYTES: u64 = 1024 * 1024;

thread_local! {
    /// Icons made so far, per (path, pixel size); kept for the process lifetime.
    static CACHE: RefCell<HashMap<(String, i32), Option<isize>>> = RefCell::new(HashMap::new());
}

/// The validated bytes of icon file `path` (`None` = rejected).
pub fn read_valid(path: &str) -> Option<Vec<u8>> {
    let full = std::path::absolute(path)
        .ok()?
        .to_string_lossy()
        .into_owned();
    // S1 (prompt 022): the shared S-RES-02 check (never on the network, no reparse to it).
    if !yamdv_core::path::icon_candidate(&full)
        || yamdv_core::path::local_policy(
            &full,
            yamdv_core::path::Scope::Local,
            &mut crate::images::WinFs,
        )
        .is_err()
    {
        return None;
    }
    let long = yamdv_core::path::long_form(&full);
    let meta = std::fs::metadata(&long).ok()?;
    if !meta.is_file() || meta.len() > MAX_BYTES {
        return None;
    }
    let b = std::fs::read(&long).ok()?;
    if b.len() as u64 > MAX_BYTES || yamdv_core::image::ico::directory(&b)?.is_empty() {
        return None;
    }
    Some(b)
}

/// `true` when `path` is an acceptable custom icon.
pub fn valid(path: &str) -> bool {
    read_valid(path).is_some()
}

/// The custom icon `path` at `px` pixels (cached), or `None`.
pub fn icon(path: &str, px: i32) -> Option<HICON> {
    if path.is_empty() {
        return None;
    }
    let key = (path.to_string(), px);
    if let Some(h) = CACHE.with(|c| c.borrow().get(&key).copied()) {
        return h.map(|h| HICON(h as *mut _));
    }
    let h = make(path, px);
    CACHE.with(|c| c.borrow_mut().insert(key, h.map(|h| h.0 as isize)));
    h
}

fn make(path: &str, px: i32) -> Option<HICON> {
    let bytes = read_valid(path)?;
    let wic = crate::images::factory()?;
    let side = u32::try_from(px.clamp(8, 256)).ok()?;
    let src = yamdv_core::image::Source::Data(bytes);
    let bmp = crate::images::decode(&wic, &src, side, side).ok()?;
    let mut px_data = bmp.px;
    // Premultiplied → straight alpha (icons are straight).
    yamdv_core::image::ico::unpremultiply(&mut px_data);
    let (w, h) = (i32::try_from(bmp.w).ok()?, i32::try_from(bmp.h).ok()?);
    if px_data.len() != (bmp.w as usize) * (bmp.h as usize) * 4 {
        return None;
    }
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: 40,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: a fresh 32-bpp top-down DIB of w×h; `bits` points at w*h*4 bytes we fill once;
    // the GDI objects are deleted after CreateIconIndirect copied them.
    unsafe {
        let mut bits = std::ptr::null_mut();
        let color = CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(color.into());
            return None;
        }
        std::ptr::copy_nonoverlapping(px_data.as_ptr(), bits.cast::<u8>(), px_data.len());
        let mask = CreateBitmap(w, h, 1, 1, None);
        let ii = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let r = CreateIconIndirect(&ii).ok();
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        r
    }
}

/// Sets (or, with no custom icon, resets to the class icon) the window/taskbar icons.
pub fn apply(hwnd: HWND, path: &str) {
    // SAFETY: plain metric queries and WM_SETICON on a window of ours.
    unsafe {
        for (which, metric) in [(ICON_BIG, SM_CXICON), (ICON_SMALL, SM_CXSMICON)] {
            let h = icon(path, GetSystemMetrics(metric)).map_or(0, |h| h.0 as isize);
            SendMessageW(
                hwnd,
                WM_SETICON,
                Some(WPARAM(which as usize)),
                Some(LPARAM(h)),
            );
        }
    }
}

/// Updates the document-type `DefaultIcon` when YAMDV's ProgID is registered (explicit
/// settings change only; HKCU; S-REG). Empty `path` restores the exe icon.
pub fn set_document_icon(path: &str) {
    let key = format!(
        "Software\\Classes\\{}\\DefaultIcon",
        yamdv_core::reg::PROGID
    );
    if !crate::shell::key_exists(&key) {
        return;
    }
    let data = if path.is_empty() || !valid(path) {
        format!("\"{}\",0", crate::shell::exe_path())
    } else {
        std::path::absolute(path)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    crate::shell::set_values(&[yamdv_core::reg::Value {
        key,
        name: String::new(),
        data,
    }]);
    crate::shell::notify_assoc_changed();
}

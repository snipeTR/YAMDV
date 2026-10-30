// SPDX-License-Identifier: AGPL-3.0-or-later
//! Windows app mode (light/dark) detection (R-THEME-01) and High Contrast (FUT-47).
#![allow(unsafe_code)]

use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::core::w;

/// Theme mode of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Light app mode.
    Light,
    /// Dark app mode.
    Dark,
}

impl Mode {
    /// Theme token `bg` as a GDI `COLORREF` value (0x00BBGGRR): light `#ffffff`, dark `#1e1e1e`.
    pub fn bg_colorref(self) -> u32 {
        match self {
            Self::Light => 0x00ff_ffff,
            Self::Dark => 0x001e_1e1e,
        }
    }
}

/// Read `HKCU\…\Themes\Personalize\AppsUseLightTheme`; missing value → light (Windows default).
pub fn app_mode() -> Mode {
    let mut value = 1u32;
    let mut size = u32::try_from(size_of::<u32>()).unwrap_or(4);
    // SAFETY: value/size are valid out-pointers sized for a REG_DWORD; strings are static.
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut value).cast()),
            Some(&raw mut size),
        )
    };
    if r.is_ok() && value == 0 {
        Mode::Dark
    } else {
        Mode::Light
    }
}

/// Windows High Contrast (FUT-47): `SPI_GETHIGHCONTRAST`; when on, the system colours YAMDV
/// draws with (`GetSysColor`).
pub fn high_contrast() -> Option<yamdv_core::hc::SysColors> {
    use windows::Win32::Graphics::Gdi::{
        COLOR_BTNFACE, COLOR_BTNTEXT, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT,
        COLOR_HOTLIGHT, COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor, SYS_COLOR_INDEX,
    };
    use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let size = u32::try_from(size_of::<HIGHCONTRASTW>()).unwrap_or(16);
    let mut hc = HIGHCONTRASTW {
        cbSize: size,
        ..Default::default()
    };
    // SAFETY: `hc` is a valid HIGHCONTRASTW with cbSize set; uiParam is its size.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            size,
            Some((&raw mut hc).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    if ok.is_err() || !hc.dwFlags.contains(HCF_HIGHCONTRASTON) {
        return None;
    }
    // GetSysColor returns 0x00BBGGRR.
    let c = |i: SYS_COLOR_INDEX| {
        // SAFETY: no preconditions.
        let v = unsafe { GetSysColor(i) };
        ((v & 0xFF) << 16) | (v & 0xFF00) | ((v >> 16) & 0xFF)
    };
    Some(yamdv_core::hc::SysColors {
        window: c(COLOR_WINDOW),
        window_text: c(COLOR_WINDOWTEXT),
        hotlight: c(COLOR_HOTLIGHT),
        highlight: c(COLOR_HIGHLIGHT),
        highlight_text: c(COLOR_HIGHLIGHTTEXT),
        gray_text: c(COLOR_GRAYTEXT),
        btn_face: c(COLOR_BTNFACE),
        btn_text: c(COLOR_BTNTEXT),
    })
}

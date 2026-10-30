// SPDX-License-Identifier: AGPL-3.0-or-later
//! High Contrast (FUT-47, prompt 019 §4): when Windows High Contrast is on, every colour comes
//! from the system colours below — no scheme tint, no `custom-theme.ini`. The platform layer
//! reads `SPI_GETHIGHCONTRAST` / `GetSysColor` (start-up and `WM_SETTINGCHANGE`) and stores
//! the result here; layout and chrome code read it while painting.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::theme::{Palette, luminance};

/// The system colours YAMDV uses in High Contrast (`0xRRGGBB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SysColors {
    /// `COLOR_WINDOW`: page and panel background.
    pub window: u32,
    /// `COLOR_WINDOWTEXT`: text, borders, code, diagram strokes.
    pub window_text: u32,
    /// `COLOR_HOTLIGHT`: links.
    pub hotlight: u32,
    /// `COLOR_HIGHLIGHT`: selection, focus ring, hovered menu item.
    pub highlight: u32,
    /// `COLOR_HIGHLIGHTTEXT`: text on `highlight`.
    pub highlight_text: u32,
    /// `COLOR_GRAYTEXT`: muted / disabled text.
    pub gray_text: u32,
    /// `COLOR_BTNFACE`: button and menu background.
    pub btn_face: u32,
    /// `COLOR_BTNTEXT`: button and menu text.
    pub btn_text: u32,
}

/// Windows "High Contrast Black" / "Night sky"-style defaults (driver `ui hc-on black`).
pub const BLACK: SysColors = SysColors {
    window: 0x000000,
    window_text: 0xffffff,
    hotlight: 0xffff00,
    highlight: 0x1aebff,
    highlight_text: 0x000000,
    gray_text: 0x3ff23f,
    btn_face: 0x000000,
    btn_text: 0xffffff,
};

/// Windows "High Contrast White" / "Desert"-style defaults (driver `ui hc-on white`).
pub const WHITE: SysColors = SysColors {
    window: 0xffffff,
    window_text: 0x000000,
    hotlight: 0x00009f,
    highlight: 0x37006e,
    highlight_text: 0xffffff,
    gray_text: 0x600000,
    btn_face: 0xffffff,
    btn_text: 0x000000,
};

static ON: AtomicBool = AtomicBool::new(false);
static COLORS: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

/// Stores the current High Contrast state (`None` = off).
pub fn set(c: Option<SysColors>) {
    if let Some(c) = c {
        let v = [
            c.window,
            c.window_text,
            c.hotlight,
            c.highlight,
            c.highlight_text,
            c.gray_text,
            c.btn_face,
            c.btn_text,
        ];
        for (slot, x) in COLORS.iter().zip(v) {
            slot.store(x, Ordering::Relaxed);
        }
    }
    ON.store(c.is_some(), Ordering::Release);
}

/// High Contrast is on.
pub fn on() -> bool {
    ON.load(Ordering::Acquire)
}

/// The current system colours, when High Contrast is on.
/// Text colour on a `highlight` fill (selected menu item, section or list row): the system
/// `COLOR_HIGHLIGHTTEXT` in High Contrast, else `normal`.
pub fn on_highlight(normal: u32) -> u32 {
    get().map_or(normal, |c| c.highlight_text)
}

/// The current system colours, `None` when High Contrast is off.
pub fn get() -> Option<SysColors> {
    if !on() {
        return None;
    }
    let [a, b, c, d, e, f, g, h] = COLORS.each_ref().map(|x| x.load(Ordering::Relaxed));
    Some(SysColors {
        window: a,
        window_text: b,
        hotlight: c,
        highlight: d,
        highlight_text: e,
        gray_text: f,
        btn_face: g,
        btn_text: h,
    })
}

/// The system colours when `pal` is the High Contrast palette being drawn on screen (exports
/// lay out with their own palettes and keep their normal colours).
pub fn active(pal: &Palette) -> Option<SysColors> {
    get().filter(|c| palette(c) == *pal)
}

impl SysColors {
    /// The High Contrast theme is dark (light text on a dark window).
    pub fn dark(&self) -> bool {
        luminance(self.window) < 0.4
    }
}

/// The document palette in High Contrast: system colours only; blocks get borders instead of
/// fills (`code_bg` = page, `code_border` = text).
pub fn palette(c: &SysColors) -> Palette {
    Palette {
        bg: c.window,
        text: c.window_text,
        heading: c.window_text,
        link: c.hotlight,
        accent: c.highlight,
        quote_bar: c.window_text,
        quote_text: c.window_text,
        code_bg: c.window,
        inline_code_bg: c.window,
        table_header_bg: c.window,
        code_border: c.window_text,
        table_border: c.window_text,
        border_subtle: c.window_text,
        text_muted: c.gray_text,
        marker: c.gray_text,
        selection_alpha_pct: 50,
        error: c.window_text,
        line_number: c.gray_text,
        highlight_bg: c.highlight,
        highlight_text: c.highlight_text,
        zebra_bg: c.window,
        selection: c.highlight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fut_47_hc_palette_uses_only_system_colors() {
        for c in [BLACK, WHITE] {
            let sys = [
                c.window,
                c.window_text,
                c.hotlight,
                c.highlight,
                c.highlight_text,
                c.gray_text,
                c.btn_face,
                c.btn_text,
            ];
            let p = palette(&c);
            for v in [
                p.bg,
                p.text,
                p.heading,
                p.link,
                p.accent,
                p.quote_bar,
                p.quote_text,
                p.code_bg,
                p.inline_code_bg,
                p.table_header_bg,
                p.code_border,
                p.table_border,
                p.border_subtle,
                p.text_muted,
                p.marker,
                p.error,
                p.line_number,
                p.highlight_bg,
                p.highlight_text,
                p.zebra_bg,
                p.selection,
            ] {
                assert!(sys.contains(&v), "{v:06x} is not a system colour");
            }
            // Borders instead of fills.
            assert_eq!(p.code_bg, p.bg);
            assert_eq!(p.code_border, p.text);
        }
        assert!(BLACK.dark());
        assert!(!WHITE.dark());
    }
}

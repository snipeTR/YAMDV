// SPDX-License-Identifier: AGPL-3.0-or-later
//! The toolbar under the title bar (prompt 020 §5.2, UI_SPEC B.4.1) and the shared icon masks
//! of the toolbar and the menus.
//!
//! Icons come from the owner's SVGs compiled by `cargo xtask icons` into `assets/icons/icons.bin`
//! (YIC1 path table, no SVG text in the exe). A mask is rasterized the first time an icon is
//! painted at a pixel size and kept in a cache of at most [`CACHE_MAX`] bytes.

use super::*;
use yamdv_core::chrome as ch;
use yamdv_ui::toolbar::{self as tb, BAR_H as TB_H, BTN, TOOLS};

/// The embedded path table.
static TABLE: &[u8] = include_bytes!("../../../../assets/icons/icons.bin");
/// Mask cache budget (prompt 020 §5.2 item 2).
pub(super) const CACHE_MAX: usize = 64 * 1024;

/// `(icon, px) → alpha` masks and their total size.
type Masks = (Vec<(String, u32, Rc<[u8]>)>, usize);

thread_local! {
    static MASKS: RefCell<Masks> =
        const { RefCell::new((Vec::new(), 0)) };
}

/// Bytes held by the mask cache (driver / memory checks).
#[cfg_attr(not(feature = "dev-tools"), allow(dead_code))]
pub(super) fn cache_bytes() -> usize {
    MASKS.with(|m| m.borrow().1)
}

fn mask(name: &str, px: u32) -> Option<Rc<[u8]>> {
    if let Some(m) = MASKS.with(|m| {
        m.borrow()
            .0
            .iter()
            .find(|e| e.0 == name && e.1 == px)
            .map(|e| e.2.clone())
    }) {
        return Some(m);
    }
    let shapes = yamdv_svg::icons::Table::new(TABLE)?.get(name)?;
    let a: Rc<[u8]> = yamdv_svg::icons::mask(&shapes, px).into();
    MASKS.with(|m| {
        let mut m = m.borrow_mut();
        if m.1 + a.len() > CACHE_MAX {
            m.0.clear();
            m.1 = 0;
        }
        m.1 += a.len();
        m.0.push((name.to_string(), px, a.clone()));
    });
    Some(a)
}

/// Draws icon `name` (`size` DIPs square) at DIP `(x, y)` tinted with `color` (0xRRGGBB).
pub(super) fn draw_icon(b: &Band<'_>, name: &str, x: f32, y: f32, size: f32, color: u32) {
    let px = (size * b.scale).round().max(1.0) as u32;
    let s = px as f32 / b.scale;
    if !b.visible(y, s) {
        return;
    }
    let Some(a) = mask(name, px) else { return };
    let (r, g, bl) = ((color >> 16) & 255, (color >> 8) & 255, color & 255);
    let mut buf = Vec::with_capacity(a.len() * 4);
    for &v in a.iter() {
        let v = u32::from(v);
        buf.extend([
            (bl * v / 255) as u8,
            (g * v / 255) as u8,
            (r * v / 255) as u8,
            v as u8,
        ]);
    }
    // Whole device pixels keep the mask crisp.
    let sx = (x * b.scale).round() / b.scale;
    let sy = (y * b.scale).round() / b.scale;
    b.image(sx, sy, px, px, &buf);
}

/// Command of a toolbar tool (`None`: unknown).
pub(super) fn tool_cmd(id: &str) -> Option<u32> {
    let fmt = |i: u32| Some(app::FMT0 + i);
    let ins = |i: u32| Some(app::INS0 + i);
    match id {
        "open_file" => Some(app::OPEN),
        "reload" => Some(app::RELOAD),
        "copy" => Some(app::COPY),
        "copy_markdown" => Some(app::COPY_MD),
        "live_edit" => Some(app::LIVE_EDIT),
        "split_edit" => Some(app::SPLIT_EDIT),
        "external_editor" => Some(app::EDITOR),
        "find" => Some(app::FIND),
        "heading_jump" => Some(app::GO_HEADING),
        "toggle_sidebar" => Some(app::OUTLINE),
        "nav_back" => Some(app::NAV_BACK),
        "nav_forward" => Some(app::NAV_FORWARD),
        "export_pdf" => Some(app::EXPORT_PDF),
        "export_html" => Some(app::EXPORT_HTML),
        "print" => Some(app::PRINT),
        "mind_map" => Some(app::MIND_MAP),
        "toggle_favorite" => Some(app::FAVORITE),
        "theme" => Some(app::THEME_CYCLE),
        "fullscreen" => Some(app::FULLSCREEN),
        "toggle_word_count" => Some(app::WORD_COUNT),
        "font_bigger" => Some(app::FONT_BIG),
        "font_smaller" => Some(app::FONT_SMALL),
        "settings" => Some(app::SETTINGS_MENU),
        "shortcuts_help" => Some(app::SHORTCUTS),
        "about" => Some(app::ABOUT),
        "undo" => Some(app::UNDO),
        "redo" => Some(app::REDO),
        "bold" => fmt(0),
        "italic" => fmt(1),
        "strikethrough" => fmt(2),
        "inline_code" => fmt(3),
        "link" => fmt(4),
        "heading_1" => fmt(5),
        "heading_2" => fmt(6),
        "heading_3" => fmt(7),
        "quote" => fmt(9),
        "bullet_list" => fmt(10),
        "numbered_list" => fmt(11),
        "task_list" => fmt(12),
        "insert_table" => ins(0),
        "code_block" => ins(1),
        "math_block" => ins(7),
        "mermaid_block" => ins(8),
        "horizontal_rule" => ins(9),
        "image" => ins(10),
        "footnote" => ins(12),
        _ => None,
    }
}

/// Icon of a menu command (B.7.1 icon column), when the command has one.
// SAFETY/bounds: each table index is `i - BASE` under a range guard of the table's length.
#[allow(clippy::indexing_slicing)]
pub(super) fn menu_icon(id: u32) -> Option<String> {
    let named = |s: &str| Some(s.to_string());
    match id {
        app::COPY_LINK => named("copy_link"),
        app::SELECT_ALL => named("select_all"),
        app::COPY_RICH => named("copy_rich_html"),
        app::CUT => named("cut"),
        app::PASTE => named("paste"),
        app::EXPLORER => named("show_in_explorer"),
        app::COPY_PATH => named("copy_file_path"),
        app::SCHEME_CARDS => named("color_scheme"),
        app::CSV_COPY => named("table_copy_csv"),
        app::CSV_EXPORT => named("table_export_csv"),
        app::DIAGRAM_PNG => named("copy_png"),
        app::DIAGRAM_SRC => named("copy_source"),
        app::DIAGRAM_SAVE => named("save_png"),
        app::SINGLE_NL => named("toggle_single_newline_break"),
        app::FONT_RESET => named("font_reset"),
        app::FAVORITE => named("toggle_favorite_off"),
        i if (app::MODE0..app::MODE0 + 3).contains(&i) => {
            named(["theme_system", "theme_light", "theme_dark"][(i - app::MODE0) as usize])
        }
        i if (app::INS0 + 2..app::INS0 + 7).contains(&i) => named(
            [
                "callout_note",
                "callout_tip",
                "callout_important",
                "callout_warning",
                "callout_caution",
            ][(i - app::INS0 - 2) as usize],
        ),
        i if i == app::INS0 + 11 => named("link"),
        i if i == app::FMT0 + 8 => named("paragraph"),
        _ => TOOLS
            .iter()
            .find(|t| tool_cmd(t.id) == Some(id))
            .map(|t| t.id.to_string()),
    }
    .filter(|n| yamdv_svg::icons::Table::new(TABLE).is_some_and(|t| t.has(n)))
}

/// A toolbar slot: a tool (index into [`TOOLS`]) or the » overflow button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Slot {
    Tool(usize),
    More,
}

/// Toolbar state of a window.
#[derive(Debug, Default)]
pub(super) struct Bar {
    /// `toolbar.visible`.
    pub on: bool,
    pub hover: Option<Slot>,
    pub pressed: Option<Slot>,
    /// Keyboard focus (index into [`Win::tb_slots`]).
    pub focus: usize,
}

impl Win {
    /// The bar is shown: on, not fullscreen, no fullscreen viewer, no mind map.
    pub(super) fn tb_shown(&self) -> bool {
        self.tb.on && !self.fs && !self.fv_shown() && self.nav.mind.is_none()
    }

    /// Height of the title bar plus the toolbar (the content top without banners).
    pub(super) fn bars_h(&self) -> f32 {
        let bar = if self.fs { 0.0 } else { ch::BAR_H };
        bar + if self.tb_shown() { TB_H } else { 0.0 }
    }

    /// Re-applies the content top after the toolbar was shown or hidden.
    pub(super) fn tb_sync(&mut self) {
        if let Some(sh) = shared() {
            self.tz_relayout(&sh);
        }
    }

    fn tb_items(sh: &Shared, editing: bool) -> Vec<usize> {
        tb::shown(&sh.tb_items, editing)
    }

    pub(super) fn tb_layout(&self, sh: &Shared) -> tb::Layout {
        tb::layout(&Self::tb_items(sh, self.edit_mode_id() != 0), self.cw)
    }

    /// Visible slots in keyboard order.
    pub(super) fn tb_slots(&self, sh: &Shared) -> Vec<(Slot, f32)> {
        let l = self.tb_layout(sh);
        let mut v: Vec<(Slot, f32)> = l.buttons.iter().map(|&(i, x)| (Slot::Tool(i), x)).collect();
        if let Some(x) = l.more {
            v.push((Slot::More, x));
        }
        v
    }

    fn tb_doc(&self) -> Option<bool> {
        match &self.view {
            View::Doc(d) => Some(d.virt),
            _ => None,
        }
    }

    /// (enabled, on) of tool `i`.
    pub(super) fn tb_state(&self, sh: &Shared, i: usize) -> (bool, bool) {
        let Some(t) = TOOLS.get(i) else {
            return (false, false);
        };
        let doc = self.tb_doc();
        let real = doc == Some(false);
        let enabled = match t.id {
            "open_file" | "theme" | "settings" | "shortcuts_help" | "about" | "toggle_sidebar"
            | "font_bigger" | "font_smaller" | "fullscreen" => true,
            "nav_back" => self.hist_pos > 0,
            "nav_forward" => self.hist_pos + 1 < self.history.len(),
            "live_edit" | "split_edit" | "external_editor" | "toggle_favorite" | "reload" => real,
            _ => doc.is_some(),
        };
        let on = match t.id {
            "live_edit" => self.edit_mode_id() == 1,
            "split_edit" => self.edit_mode_id() == 2,
            "toggle_sidebar" => sh.side_visible,
            "toggle_word_count" => sh.word_badge,
            "toggle_favorite" => self.tb_favorite(),
            _ => false,
        };
        (enabled, on)
    }

    fn tb_favorite(&self) -> bool {
        match &self.view {
            View::Doc(d) if !d.virt => {
                let p = d.path.clone();
                app::with(|a| a.state.is_favorite(&p)).unwrap_or(false)
            }
            _ => false,
        }
    }

    /// Icon name of tool `i` in its current state.
    pub(super) fn tb_icon(&self, sh: &Shared, i: usize) -> String {
        let id = TOOLS.get(i).map_or("", |t| t.id);
        tb::icon(id, self.tb_favorite(), &sh.mode)
    }

    /// Runs a slot (click, keyboard, driver, UIA).
    pub(super) fn tb_run(&mut self, sh: &Shared, s: Slot) {
        match s {
            Slot::More => {
                let l = self.tb_layout(sh);
                let entries = l
                    .overflow
                    .iter()
                    .filter_map(|&i| {
                        let t = TOOLS.get(i)?;
                        Some(app::Entry::Item {
                            id: tool_cmd(t.id)?,
                            label: tb::name(t, sh.lang).to_string(),
                            key: app::keys(t.id),
                            mark: 0,
                            sub: None,
                        })
                    })
                    .collect();
                let x = l.more.unwrap_or(self.cw) + BTN;
                let mut pt = POINT {
                    x: (x * self.scale).round() as i32,
                    y: ((ch::BAR_H + TB_H) * self.scale).round() as i32,
                };
                // SAFETY: valid hwnd and point.
                let _ =
                    unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(self.hwnd, &mut pt) };
                self.actions.push(Action::ToolbarMenu(
                    (pt.x, pt.y),
                    app::Model {
                        menus: vec![entries],
                    },
                ));
            }
            Slot::Tool(i) => {
                if !self.tb_state(sh, i).0 {
                    return;
                }
                if let Some(c) = TOOLS.get(i).and_then(|t| tool_cmd(t.id)) {
                    self.actions.push(Action::Command(c));
                }
            }
        }
    }

    /// Slot under raw client DIPs.
    pub(super) fn tb_hit(&self, sh: &Shared, x: f32, y: f32) -> Option<Slot> {
        let y0 = ch::BAR_H;
        if !self.tb_shown() || !(y0..y0 + TB_H).contains(&y) {
            return None;
        }
        self.tb_slots(sh)
            .into_iter()
            .find(|&(_, bx)| x >= bx && x < bx + BTN)
            .map(|(s, _)| s)
    }

    /// The toolbar band is under raw client y.
    pub(super) fn tb_contains(&self, y: f32) -> bool {
        self.tb_shown() && (ch::BAR_H..ch::BAR_H + TB_H).contains(&y)
    }

    pub(super) fn invalidate_toolbar(&self) {
        let rc = RECT {
            left: 0,
            top: (ch::BAR_H * self.scale).floor() as i32,
            right: (self.cw * self.scale).ceil() as i32 + 1,
            bottom: ((ch::BAR_H + TB_H) * self.scale).ceil() as i32,
        };
        // SAFETY: valid hwnd.
        let _ = unsafe { InvalidateRect(Some(self.hwnd), Some(&rc), false) };
    }

    /// Mouse move at raw client DIPs over the bar; `true` when the bar takes it.
    pub(super) fn tb_move(&mut self, sh: &Shared, x: f32, y: f32) -> bool {
        let inside = self.tb_contains(y) && !self.selecting;
        let h = if inside { self.tb_hit(sh, x, y) } else { None };
        if h != self.tb.hover {
            self.tb.hover = h;
            self.invalidate_toolbar();
        }
        if inside && !self.tracking {
            let mut tme = TRACKMOUSEEVENT {
                cbSize: u32::try_from(size_of::<TRACKMOUSEEVENT>()).unwrap_or(0),
                dwFlags: TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            // SAFETY: valid structure.
            if unsafe { TrackMouseEvent(&mut tme) }.is_ok() {
                self.tracking = true;
            }
        }
        inside
    }

    pub(super) fn tb_down(&mut self, sh: &Shared, x: f32, y: f32) -> bool {
        if !self.tb_contains(y) {
            return false;
        }
        if self.dialog.is_none() {
            self.tb.pressed = self.tb_hit(sh, x, y);
            self.tb.hover = self.tb.pressed;
            self.invalidate_toolbar();
        }
        true
    }

    pub(super) fn tb_up(&mut self, sh: &Shared) -> bool {
        let Some(p) = self.tb.pressed.take() else {
            return false;
        };
        self.invalidate_toolbar();
        if self.tb.hover == Some(p) {
            self.tb_run(sh, p);
        }
        true
    }

    pub(super) fn tb_leave(&mut self) {
        if self.tb.hover.take().is_some() {
            self.invalidate_toolbar();
        }
    }

    /// Tooltip of the hovered slot: name and shortcut.
    pub(super) fn tb_tip(&self, sh: &Shared) -> Option<String> {
        match self.tb.hover? {
            Slot::More => Some(tr(sh.lang, Msg::TipMore).to_string()),
            Slot::Tool(i) => {
                let t = TOOLS.get(i)?;
                let k = app::keys(t.id);
                let n = tb::name(t, sh.lang);
                Some(if k.is_empty() {
                    n.to_string()
                } else {
                    format!("{n} ({k})")
                })
            }
        }
    }

    /// Draws the bar (client origin).
    pub(super) fn draw_toolbar(&self, sh: &Shared, b: &Band<'_>) {
        if !self.tb_shown() || !b.visible(ch::BAR_H, TB_H) {
            return;
        }
        let pal = &self.pal;
        let tok = ch::tokens(self.dark);
        let y0 = ch::BAR_H;
        // P-60: a faint red / muted burgundy bar while Live or Split edit is on (never in HC).
        let tinted = self.edit_mode_id() != 0 && !yamdv_core::hc::on();
        let bg = if tinted {
            yamdv_core::theme::toolbar_edit_bg(pal.bg, self.dark)
        } else {
            pal.bg
        };
        // On the tint an "on" icon keeps ≥ 3:1 (WCAG 1.4.11): `accent`, or `text` where the
        // scheme's accent is too light on its 14 % accent fill (e.g. Lychee / Mint light).
        let on_color = {
            let fill = yamdv_core::theme::blend(bg, pal.accent, 140);
            if tinted && yamdv_core::theme::contrast(pal.accent, fill) < 3.0 {
                pal.text
            } else {
                pal.accent
            }
        };
        b.fill(Rect::new(0.0, y0, self.cw, TB_H), bg, 1.0, 0.0);
        if self.scroll_pos() > 0.5 {
            b.fill(
                Rect::new(0.0, y0 + TB_H - 1.0, self.cw, 1.0),
                pal.border_subtle,
                1.0,
                0.0,
            );
        }
        let l = self.tb_layout(sh);
        let by = y0 + (TB_H - BTN) / 2.0;
        for &sx in &l.seps {
            b.fill(
                Rect::new(sx, y0 + 8.0, 1.0, TB_H - 16.0),
                tok.separator,
                1.0,
                0.0,
            );
        }
        let icon = sh.tb_icon;
        let mut slots: Vec<(Slot, f32)> =
            l.buttons.iter().map(|&(i, x)| (Slot::Tool(i), x)).collect();
        if let Some(x) = l.more {
            slots.push((Slot::More, x));
        }
        let focus = (self.nav.kbd || yamdv_core::hc::on())
            && self.nav.region == super::nav::Region::Toolbar;
        for (k, (s, x)) in slots.into_iter().enumerate() {
            let r = Rect::new(x, by, BTN, BTN);
            let (enabled, on) = match s {
                Slot::Tool(i) => self.tb_state(sh, i),
                Slot::More => (true, false),
            };
            let hover = enabled && self.tb.hover == Some(s);
            let pressed = hover && self.tb.pressed == Some(s);
            if on {
                b.fill(r, pal.accent, 0.14, tb::RADIUS);
            }
            if pressed {
                b.fill(
                    r,
                    tok.pressed.0,
                    f32::from(tok.pressed.1) / 100.0,
                    tb::RADIUS,
                );
            } else if hover {
                b.fill(r, tok.hover.0, f32::from(tok.hover.1) / 100.0, tb::RADIUS);
            }
            let color = if !enabled {
                pal.text_muted
            } else if on {
                on_color
            } else {
                pal.text
            };
            let (ix, iy) = (x + (BTN - icon) / 2.0, by + (BTN - icon) / 2.0);
            match s {
                Slot::Tool(i) => draw_icon(b, &self.tb_icon(sh, i), ix, iy, icon, color),
                Slot::More => {
                    let t = self.shape_ui(sh, "»", 15.0, 400, color, None);
                    b.text(
                        t.as_ref(),
                        (x + (BTN - t.width()) / 2.0).round(),
                        (by + (BTN - t.height()) / 2.0).round(),
                    );
                }
            }
            if focus && k == self.tb.focus {
                super::nav::ring(
                    b,
                    Rect::new(x + 2.0, by + 2.0, BTN - 4.0, BTN - 4.0),
                    pal.accent,
                );
            }
        }
    }

    /// Toolbar region keys (F6): ←/→/Tab move, Enter/Space run, Esc leaves.
    pub(super) fn tb_key(&mut self, sh: &Shared, vk: u16, shift: bool) -> bool {
        let n = self.tb_slots(sh).len().max(1);
        match vk {
            0x09 | 0x27 | 0x25 => {
                let back = vk == 0x25 || (vk == 0x09 && shift);
                self.tb.focus = (self.tb.focus.min(n - 1) + if back { n - 1 } else { 1 }) % n;
                self.invalidate_toolbar();
                true
            }
            0x0D | 0x20 => {
                if let Some(&(s, _)) = self.tb_slots(sh).get(self.tb.focus) {
                    self.tb_run(sh, s);
                }
                true
            }
            0x1B => {
                self.nav.region = super::nav::Region::Document;
                self.invalidate();
                true
            }
            _ => false,
        }
    }

    /// UIA: the ToolBar with a Button per slot (Toggle for the stateful ones).
    pub(super) fn uia_toolbar(&self, sh: &Shared, t: &mut super::uia::Tree) {
        use super::uia::{At, K, Pat};
        if !self.tb_shown() {
            return;
        }
        let y0 = ch::BAR_H;
        let l = sh.lang;
        let name = match l {
            Lang::Tr => "Araç çubuğu",
            Lang::En => "Toolbar",
        };
        let bar = t.add(
            0,
            K::ToolBar,
            name,
            At::Dip(Rect::new(0.0, y0, self.cw, TB_H)),
        );
        let by = y0 + (TB_H - BTN) / 2.0;
        for (k, (s, x)) in self.tb_slots(sh).into_iter().enumerate() {
            let (label, toggle) = match s {
                Slot::Tool(i) => {
                    let Some(tool) = TOOLS.get(i) else { continue };
                    let st = self.tb_state(sh, i);
                    let toggle = matches!(
                        tool.id,
                        "live_edit"
                            | "split_edit"
                            | "toggle_sidebar"
                            | "toggle_favorite"
                            | "toggle_word_count"
                    )
                    .then_some(st.1);
                    (tb::name(tool, l), toggle)
                }
                Slot::More => (tr(l, Msg::TipMore), None),
            };
            let n = t.add(bar, K::Button, label, At::Dip(Rect::new(x, by, BTN, BTN)));
            match toggle {
                Some(on) => t.pat(n, Pat::Toggle(on)),
                None => t.pat(n, Pat::Invoke),
            }
            if self.nav.kbd && self.nav.region == super::nav::Region::Toolbar && self.tb.focus == k
            {
                t.focus(n);
            }
        }
    }
}

#[cfg(feature = "dev-tools")]
impl Win {
    /// `ui toolbar-click <id|more>`.
    pub(super) fn tb_drive(&mut self, sh: &Shared, arg: &str) -> String {
        let slot = if arg == "more" {
            Slot::More
        } else {
            match tb::index(arg) {
                Some(i) => Slot::Tool(i),
                None => return format!("FAIL ui toolbar-click {arg}: unknown item"),
            }
        };
        if !self.tb_shown() {
            return format!("FAIL ui toolbar-click {arg}: toolbar hidden");
        }
        let shown = self.tb_slots(sh).iter().any(|(s, _)| *s == slot);
        if !shown {
            return format!("FAIL ui toolbar-click {arg}: not on the bar");
        }
        if let Slot::Tool(i) = slot
            && !self.tb_state(sh, i).0
        {
            return format!("FAIL ui toolbar-click {arg}: disabled");
        }
        self.tb_run(sh, slot);
        format!("ok ui toolbar-click {arg}")
    }

    /// Driver variables of the toolbar.
    pub(super) fn tb_vars(&self, sh: &Shared) -> Vec<(&'static str, String)> {
        let l = self.tb_layout(sh);
        let id = |i: &usize| TOOLS.get(*i).map_or("", |t| t.id);
        let shown = self.tb_shown();
        let items: Vec<&str> = if shown {
            l.buttons.iter().map(|(i, _)| id(i)).collect()
        } else {
            Vec::new()
        };
        let pressed: Vec<&str> = l
            .buttons
            .iter()
            .filter(|(i, _)| shown && self.tb_state(sh, *i).1)
            .map(|(i, _)| id(i))
            .collect();
        let fav = tb::index("toggle_favorite").map_or_else(String::new, |i| self.tb_icon(sh, i));
        let theme = tb::index("theme").map_or_else(String::new, |i| self.tb_icon(sh, i));
        vec![
            ("toolbar_shown", u8::from(shown).to_string()),
            ("toolbar_items", items.join(",")),
            ("toolbar_pressed", pressed.join(",")),
            (
                "toolbar_overflow",
                if shown { l.overflow.len() } else { 0 }.to_string(),
            ),
            (
                "toolbar_overflow_items",
                l.overflow.iter().map(id).collect::<Vec<_>>().join(","),
            ),
            (
                "toolbar_more",
                u8::from(shown && l.more.is_some()).to_string(),
            ),
            ("toolbar_fav_icon", fav),
            ("toolbar_theme_icon", theme),
            ("toolbar_icon_size", format!("{}", sh.tb_icon)),
            ("icon_cache_bytes", cache_bytes().to_string()),
            ("icon_table_bytes", TABLE.len().to_string()),
        ]
    }
}

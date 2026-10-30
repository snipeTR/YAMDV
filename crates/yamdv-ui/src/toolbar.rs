// SPDX-License-Identifier: AGPL-3.0-or-later
//! Top toolbar model (prompt 020 §5.2, UI_SPEC B.4.1): the catalog of toolbar-capable command
//! IDs (= keymap action IDs = icon names), the `toolbar.items` list and the button layout with
//! the » overflow. Pure code; the window layer maps IDs to commands and draws.

use yamdv_core::i18n::Lang;

/// Bar height (DIPs), directly under the title bar.
pub const BAR_H: f32 = 32.0;
/// Button side (DIPs).
pub const BTN: f32 = 28.0;
/// Gap between buttons (DIPs).
pub const GAP: f32 = 4.0;
/// Left / right padding of the bar (DIPs).
pub const PAD: f32 = 8.0;
/// Width of a group separator slot: gap, 1 px line, gap.
pub const SEP_W: f32 = 9.0;
/// Hover / pressed overlay radius.
pub const RADIUS: f32 = 6.0;
/// Icon sizes of `toolbar.icon_size`.
pub const ICON_SIZES: [u32; 3] = [16, 20, 24];

/// One toolbar-capable command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    /// Keymap action ID (also the icon name, except the stateful ones).
    pub id: &'static str,
    /// Group: a separator is drawn where the group changes.
    pub group: u8,
    /// Shown only in Live / Split Edit (Format and Insert items).
    pub edit_only: bool,
    /// English name (tooltip, settings, UIA).
    pub en: &'static str,
    /// Turkish name.
    pub tr: &'static str,
}

macro_rules! tools {
    ($($id:literal $g:literal $e:literal $en:literal $tr:literal;)*) => {
        /// Every toolbar-capable command, in catalog (settings) order.
        pub const TOOLS: &[Tool] = &[$(Tool { id: $id, group: $g, edit_only: $e, en: $en, tr: $tr },)*];
    };
}

tools! {
    "open_file" 0 false "Open file" "Dosya aç";
    "reload" 0 false "Reload" "Yeniden yükle";
    "copy" 0 false "Copy" "Kopyala";
    "copy_markdown" 0 false "Copy as Markdown" "Markdown olarak kopyala";
    "live_edit" 1 false "Live Edit" "Canlı düzenleme";
    "split_edit" 1 false "Split Edit" "Bölünmüş düzenleme";
    "external_editor" 1 false "Open in external editor" "Harici düzenleyicide aç";
    "find" 2 false "Find" "Bul";
    "heading_jump" 2 false "Go to heading" "Başlığa git";
    "toggle_sidebar" 2 false "Sidebar" "Kenar çubuğu";
    "nav_back" 2 false "Back" "Geri";
    "nav_forward" 2 false "Forward" "İleri";
    "export_pdf" 3 false "Export PDF" "PDF olarak dışa aktar";
    "export_html" 3 false "Export HTML" "HTML olarak dışa aktar";
    "print" 3 false "Print" "Yazdır";
    "mind_map" 3 false "Mind map" "Zihin haritası";
    "toggle_favorite" 4 false "Favorite" "Sık kullanılan";
    "theme" 4 false "Theme (System → Light → Dark)" "Tema (Sistem → Açık → Koyu)";
    "fullscreen" 4 false "Full screen" "Tam ekran";
    "toggle_word_count" 4 false "Word count" "Kelime sayısı";
    "font_bigger" 4 false "Bigger text" "Daha büyük yazı";
    "font_smaller" 4 false "Smaller text" "Daha küçük yazı";
    "settings" 5 false "Settings" "Ayarlar";
    "shortcuts_help" 5 false "Keyboard shortcuts" "Klavye kısayolları";
    "about" 5 false "About" "Hakkında";
    "undo" 6 true "Undo" "Geri al";
    "redo" 6 true "Redo" "Yinele";
    "bold" 7 true "Bold" "Kalın";
    "italic" 7 true "Italic" "İtalik";
    "strikethrough" 7 true "Strikethrough" "Üstü çizili";
    "inline_code" 7 true "Inline code" "Satır içi kod";
    "link" 7 true "Link" "Bağlantı";
    "heading_1" 7 true "Heading 1" "Başlık 1";
    "heading_2" 7 true "Heading 2" "Başlık 2";
    "heading_3" 7 true "Heading 3" "Başlık 3";
    "quote" 7 true "Quote" "Alıntı";
    "bullet_list" 7 true "Bulleted list" "Madde işaretli liste";
    "numbered_list" 7 true "Numbered list" "Numaralı liste";
    "task_list" 7 true "Task list" "Görev listesi";
    "insert_table" 8 true "Insert table" "Tablo ekle";
    "code_block" 8 true "Insert code block" "Kod bloğu ekle";
    "math_block" 8 true "Insert math" "Matematik ekle";
    "mermaid_block" 8 true "Insert diagram" "Diyagram ekle";
    "horizontal_rule" 8 true "Insert rule" "Yatay çizgi ekle";
    "image" 8 true "Insert image" "Resim ekle";
    "footnote" 8 true "Insert footnote" "Dipnot ekle";
}

/// `toolbar.items` default (prompt 020 §5.2 item 3).
pub const DEFAULT_ITEMS: &str = "open_file,live_edit,split_edit,find,heading_jump,toggle_sidebar,export_pdf,print,mind_map,toggle_favorite,theme,settings";

/// Index of `id` in [`TOOLS`].
pub fn index(id: &str) -> Option<usize> {
    TOOLS.iter().position(|t| t.id == id)
}

/// Localized name of a tool.
pub fn name(t: &Tool, lang: Lang) -> &'static str {
    match lang {
        Lang::En => t.en,
        Lang::Tr => t.tr,
    }
}

/// Parses `toolbar.items`: known IDs in order, duplicates dropped.
pub fn parse(s: &str) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    for id in s.split(',').map(str::trim) {
        if let Some(i) = index(id)
            && !out.contains(&i)
        {
            out.push(i);
        }
    }
    out
}

/// Serializes item indices back to `toolbar.items`.
pub fn join(items: &[usize]) -> String {
    items
        .iter()
        .filter_map(|&i| TOOLS.get(i).map(|t| t.id))
        .collect::<Vec<_>>()
        .join(",")
}

/// Adds tool `i` (at the end) or removes it.
pub fn toggle(items: &[usize], i: usize) -> Vec<usize> {
    if items.contains(&i) {
        items.iter().copied().filter(|&x| x != i).collect()
    } else {
        let mut v = items.to_vec();
        v.push(i);
        v
    }
}

/// Moves tool `i` one place up (`up`) or down; unchanged at the ends or when absent.
pub fn move_by(items: &[usize], i: usize, up: bool) -> Vec<usize> {
    let mut v = items.to_vec();
    if let Some(p) = v.iter().position(|&x| x == i) {
        let q = if up { p.checked_sub(1) } else { Some(p + 1) };
        if let Some(q) = q.filter(|&q| q < v.len()) {
            v.swap(p, q);
        }
    }
    v
}

/// Moves tool `i` to position `to` (drag and drop).
pub fn move_to(items: &[usize], i: usize, to: usize) -> Vec<usize> {
    let mut v: Vec<usize> = items.iter().copied().filter(|&x| x != i).collect();
    if v.len() == items.len() {
        return v;
    }
    v.insert(to.min(v.len()), i);
    v
}

/// The items shown in the current mode (Format / Insert only in an edit mode).
pub fn shown(items: &[usize], editing: bool) -> Vec<usize> {
    items
        .iter()
        .copied()
        .filter(|&i| TOOLS.get(i).is_some_and(|t| editing || !t.edit_only))
        .collect()
}

/// Full order of every tool (prompt 021 §2.1): the known IDs of `toolbar.order`, then any tool it
/// misses in catalog order. Settings saved before `toolbar.order` existed (empty) keep a hand-made
/// `toolbar.items` order: its items first, then the rest in catalog order.
pub fn full_order(order: &str, items: &str) -> Vec<usize> {
    let mut v = parse(order);
    if v.is_empty() {
        let it = parse(items);
        if it.windows(2).any(|w| w.first() > w.get(1)) {
            v = it;
        }
    }
    for i in 0..TOOLS.len() {
        if !v.contains(&i) {
            v.push(i);
        }
    }
    v
}

/// The bar's items: the enabled set `items` in `order`.
pub fn bar(order: &[usize], items: &[usize]) -> Vec<usize> {
    order
        .iter()
        .copied()
        .filter(|i| items.contains(i))
        .collect()
}

/// Turns tool `i` on or off; the result keeps `order` (an item never moves, §2.1).
pub fn toggle_in(order: &[usize], items: &[usize], i: usize) -> Vec<usize> {
    bar(order, &toggle(items, i))
}

/// Settings → Toolbar presets (§2.3): `off`, `essentials`, `all`, and the shown-only `custom`.
pub const PRESETS: [&str; 4] = ["off", "essentials", "all", "custom"];

/// Localized preset label.
pub fn preset_label(p: &str, lang: Lang) -> &'static str {
    match (p, lang) {
        ("off", Lang::En) => "Off",
        ("off", Lang::Tr) => "Kapalı",
        ("essentials", Lang::En) => "Essentials",
        ("essentials", Lang::Tr) => "Önemliler",
        ("all", Lang::En) => "All",
        ("all", Lang::Tr) => "Hepsi",
        (_, Lang::En) => "Custom",
        (_, Lang::Tr) => "Özel",
    }
}

fn same_set(a: &[usize], b: &[usize]) -> bool {
    a.len() == b.len() && a.iter().all(|x| b.contains(x))
}

/// The preset shown for `toolbar.visible` + the enabled set.
pub fn preset(visible: bool, items: &[usize]) -> &'static str {
    if !visible {
        "off"
    } else if same_set(items, &parse(DEFAULT_ITEMS)) {
        "essentials"
    } else if items.len() == TOOLS.len() {
        "all"
    } else {
        "custom"
    }
}

/// Enabled set of preset `p` in `order` (`None` for `off` / `custom`, which keep the set).
pub fn preset_items(p: &str, order: &[usize]) -> Option<Vec<usize>> {
    match p {
        "essentials" => Some(bar(order, &parse(DEFAULT_ITEMS))),
        "all" => Some(order.to_vec()),
        _ => None,
    }
}

/// Placed buttons of a bar `width` DIPs wide.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    /// `(tool, x)` of each visible button (y is centered in the bar).
    pub buttons: Vec<(usize, f32)>,
    /// x of each separator line.
    pub seps: Vec<f32>,
    /// Tools in the » overflow menu, in order.
    pub overflow: Vec<usize>,
    /// x of the » button when something overflows.
    pub more: Option<f32>,
}

/// Lays out `items` left to right; what does not fit goes into the » overflow at the right.
pub fn layout(items: &[usize], width: f32) -> Layout {
    let place = |limit: f32| {
        let mut l = Layout::default();
        let mut x = PAD;
        let mut last: Option<u8> = None;
        for (k, &i) in items.iter().enumerate() {
            let g = TOOLS.get(i).map_or(0, |t| t.group);
            let sep = last.is_some_and(|p| p != g);
            let need = if sep { SEP_W } else { 0.0 } + BTN;
            if x + need > limit {
                l.overflow = items.get(k..).unwrap_or_default().to_vec();
                return l;
            }
            if sep {
                l.seps.push((x + (SEP_W - 1.0) / 2.0).round());
                x += SEP_W;
            }
            l.buttons.push((i, x));
            x += BTN + GAP;
            last = Some(g);
        }
        l
    };
    let all = place(width - PAD);
    if all.overflow.is_empty() {
        return all;
    }
    let mut l = place(width - PAD - BTN - GAP);
    l.more = Some((width - PAD - BTN).max(PAD));
    l
}

/// Icon name of tool `id` in its current state: favorite on / off, the theme mode.
pub fn icon(id: &str, on: bool, mode: &str) -> String {
    match id {
        "toggle_favorite" if on => "toggle_favorite_on".into(),
        "toggle_favorite" => "toggle_favorite_off".into(),
        "theme" => match mode {
            "light" => "theme_light".into(),
            "dark" => "theme_dark".into(),
            _ => "theme_system".into(),
        },
        _ => id.into(),
    }
}

/// The next theme mode of the theme button (System → Light → Dark → System).
pub fn next_mode(mode: &str) -> &'static str {
    match mode {
        "system" => "light",
        "light" => "dark",
        _ => "system",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_parse_and_round_trip() {
        let d = parse(DEFAULT_ITEMS);
        assert_eq!(d.len(), 12);
        assert_eq!(join(&d), DEFAULT_ITEMS);
        assert_eq!(
            yamdv_core::settings::def("toolbar.items").map(|d| d.default),
            Some(DEFAULT_ITEMS)
        );
        assert_eq!(
            parse("find, nope,find,print"),
            vec![index("find").unwrap(), index("print").unwrap()]
        );
        let f = index("find").unwrap();
        let t = toggle(&d, f);
        assert!(!t.contains(&f));
        assert_eq!(toggle(&t, f).last(), Some(&f));
        let up = move_by(&d, f, true);
        assert_eq!(up[2], f);
        assert_eq!(move_by(&d, d[0], true), d);
        assert_eq!(move_to(&d, f, 0)[0], f);
        assert_eq!(move_to(&d, index("bold").unwrap(), 0), d);
    }

    #[test]
    fn order_is_fixed_and_presets_keep_it() {
        let cat: Vec<usize> = (0..TOOLS.len()).collect();
        assert_eq!(
            full_order("", DEFAULT_ITEMS),
            cat,
            "default = catalog order"
        );
        let o = full_order("print,find", DEFAULT_ITEMS);
        assert_eq!(&o[..2], &[index("print").unwrap(), index("find").unwrap()]);
        assert_eq!(o.len(), TOOLS.len());
        let legacy = full_order("", "print,find");
        assert_eq!(legacy[0], index("print").unwrap());
        let items = parse(DEFAULT_ITEMS);
        let f = index("find").unwrap();
        let off = toggle_in(&o, &items, f);
        assert!(!off.contains(&f));
        let on = toggle_in(&o, &off, f);
        assert_eq!(
            on.iter().position(|&x| x == f),
            Some(1),
            "back in its old place"
        );
        assert_eq!(preset(true, &items), "essentials");
        assert_eq!(preset(false, &items), "off");
        assert_eq!(preset(true, &off), "custom");
        let all = preset_items("all", &o).unwrap();
        assert_eq!(all, o);
        assert_eq!(preset(true, &all), "all");
        assert!(same_set(&preset_items("essentials", &o).unwrap(), &items));
        assert_eq!(preset_items("custom", &o), None);
        assert_eq!(preset_label("custom", Lang::Tr), "Özel");
    }

    #[test]
    fn every_tool_has_icons_and_names() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons");
        for t in TOOLS {
            for name in [
                icon(t.id, false, "system"),
                icon(t.id, true, "dark"),
                icon(t.id, false, "light"),
            ] {
                assert!(dir.join(format!("{name}.svg")).exists(), "{name}");
            }
            assert!(!t.tr.is_empty() && !t.en.is_empty());
        }
        assert_eq!(next_mode("system"), "light");
        assert_eq!(next_mode("dark"), "system");
    }

    #[test]
    fn layout_groups_and_overflow() {
        let d = parse(DEFAULT_ITEMS);
        let wide = layout(&d, 1000.0);
        assert!(wide.overflow.is_empty() && wide.more.is_none());
        assert_eq!(wide.buttons.len(), 12);
        assert_eq!(wide.seps.len(), 5, "6 groups");
        assert_eq!(wide.buttons[0], (d[0], PAD));
        let narrow = layout(&d, 200.0);
        assert!(!narrow.overflow.is_empty());
        let more = narrow.more.unwrap();
        assert!(narrow.buttons.iter().all(|(_, x)| x + BTN <= more));
        assert_eq!(narrow.buttons.len() + narrow.overflow.len(), 12);
        let edit = shown(&parse("bold,find"), false);
        assert_eq!(edit, vec![index("find").unwrap()]);
        assert_eq!(shown(&parse("bold,find"), true).len(), 2);
    }
}

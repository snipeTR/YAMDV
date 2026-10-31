// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pure parts of the native PDF writer (ADR-0018, 018 items 1–6), split out of the Windows
//! writer so 018a can unit-test them on every platform: the outline tree, link rectangles, the
//! URI allow-list, font embedding rules (`fsType`), the cluster → ToUnicode mapping, the
//! ToUnicode CMap and the subset widths.

use std::collections::HashMap;

use pdf_writer::Name;
use pdf_writer::Str;
use pdf_writer::types::{SystemInfo, UnicodeCmap};

/// Points per DIP (96 DPI → 72 DPI).
pub const PT: f64 = 0.75;

/// S-EXP-03: only `http`, `https` and `mailto` become URI actions. S6 (prompt 022): a web
/// URI with userinfo (`https://bank.com@evil.example/`) is rejected (the text stays).
#[must_use]
pub fn safe_uri(d: &str) -> bool {
    let l = d.trim().to_ascii_lowercase();
    let web = l
        .strip_prefix("http://")
        .or_else(|| l.strip_prefix("https://"));
    let authority_ok = web.is_none_or(|rest| {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        rest.get(..end).is_some_and(|a| !a.contains(['@', '\\']))
    });
    (web.is_some() || l.starts_with("mailto:")) && authority_ok && !d.chars().any(char::is_control)
}

/// OS/2 `fsType` allows embedding: installable, editable or preview & print, and not
/// bitmap-only. A restricted (`0x0002`) or bitmap-only (`0x0200`) font is drawn as outlines.
#[must_use]
pub fn fs_type_embeddable(fs: u16) -> bool {
    let restricted = fs & 0x000F == 0x0002;
    let bitmap_only = fs & 0x0200 != 0;
    !restricted && !bitmap_only
}

/// Whether the (collection member) font in `data` has table `tag`.
#[must_use]
pub fn has_table(data: &[u8], index: u32, tag: [u8; 4]) -> bool {
    let be32 = |i: usize| -> Option<usize> {
        let b: [u8; 4] = data.get(i..i + 4)?.try_into().ok()?;
        Some(u32::from_be_bytes(b) as usize)
    };
    let off = if data.starts_with(b"ttcf") {
        be32(12 + 4 * index as usize).unwrap_or(0)
    } else {
        0
    };
    let n = data
        .get(off + 4..off + 6)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .map_or(0, |b| usize::from(u16::from_be_bytes(b)));
    (0..n).any(|i| data.get(off + 12 + 16 * i..off + 16 + 16 * i) == Some(&tag[..]))
}

/// `(start, end)` text ranges of equal cluster-map values (one cluster each).
#[must_use]
pub fn cluster_spans(map: &[u16]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, w) in map.windows(2).enumerate() {
        if w.first() != w.get(1) {
            out.push((start, i + 1));
            start = i + 1;
        }
    }
    if start < map.len() {
        out.push((start, map.len()));
    }
    out
}

/// ToUnicode pairs `(original glyph id, text)` of one glyph run: a cluster with as many glyphs
/// as UTF-16 units maps one to one; otherwise the cluster's text goes to its first glyph
/// (ligatures). `clusters[i]` is the first glyph of UTF-16 unit `i` (DirectWrite cluster map).
#[must_use]
pub fn cluster_pairs(ids: &[u16], clusters: &[u16], text: &[u16]) -> Vec<(u16, String)> {
    let n = ids.len();
    let spans = cluster_spans(clusters);
    let mut out = Vec::new();
    for (k, &(ci, cj)) in spans.iter().enumerate() {
        let gi = clusters.get(ci).map_or(usize::MAX, |&c| usize::from(c));
        let gend = spans
            .get(k + 1)
            .and_then(|&(nc, _)| clusters.get(nc))
            .map_or(n, |&c| usize::from(c));
        let Some(t) = text.get(ci..cj) else { continue };
        if gend.saturating_sub(gi) == t.len() && t.iter().all(|u| !(0xD800..0xE000).contains(u)) {
            out.extend(
                ids.get(gi..gend)
                    .unwrap_or(&[])
                    .iter()
                    .zip(t)
                    .map(|(&id, &u)| (id, String::from_utf16_lossy(&[u]))),
            );
        } else if let Some(&id) = ids.get(gi) {
            out.push((id, String::from_utf16_lossy(t)));
        }
    }
    out.retain(|(id, t)| *id != 0 && !t.is_empty());
    out
}

/// The ToUnicode CMap of a subset font: new glyph id → text, sorted by glyph id.
#[must_use]
pub fn to_unicode_cmap(uni: &HashMap<u16, String>) -> Vec<u8> {
    let info = SystemInfo {
        registry: Str(b"Adobe"),
        ordering: Str(b"Identity"),
        supplement: 0,
    };
    let mut um = UnicodeCmap::new(Name(b"Custom"), info);
    let mut keys: Vec<_> = uni.iter().collect();
    keys.sort_by_key(|(k, _)| **k);
    for (g, t) in keys {
        um.pair_with_multiple(*g, t.chars());
    }
    um.finish().into_vec()
}

/// `/W` widths of a subset font in new-glyph-id order (1/1000 em; 0 when unknown).
#[must_use]
pub fn cid_widths(remap: &subsetter::GlyphRemapper, widths: &HashMap<u16, f32>) -> Vec<f32> {
    remap
        .remapped_gids()
        .map(|old| widths.get(&old).copied().unwrap_or(0.0))
        .collect()
}

/// Subset font name: a six-capital tag derived from the font index, `+`, the cleaned
/// PostScript name (or `YamdvFont<k>`).
#[must_use]
pub fn subset_name(k: usize, ps: &str) -> Vec<u8> {
    let clean: String = ps
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(60)
        .collect();
    let base = if clean.is_empty() {
        format!("YamdvFont{k}")
    } else {
        clean
    };
    let mut tag = String::new();
    let mut v = (k as u64).wrapping_mul(7919).wrapping_add(17);
    for _ in 0..6 {
        tag.push(char::from(b'A' + (v % 26) as u8));
        v /= 26;
    }
    format!("{tag}+{base}").into_bytes()
}

/// The outline (bookmark) tree of headings nested by level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineTree {
    /// Parent item of each heading (`None` = top level).
    pub parent: Vec<Option<usize>>,
    /// Children of each item; index `n` (one past the last heading) holds the top level.
    pub kids: Vec<Vec<usize>>,
    /// `(previous, next)` sibling of each item.
    pub siblings: Vec<(Option<usize>, Option<usize>)>,
}

impl OutlineTree {
    /// Top-level items.
    #[must_use]
    pub fn top(&self) -> &[usize] {
        self.kids.last().map_or(&[][..], Vec::as_slice)
    }
}

/// Nests headings of `levels` (1–6): the parent of an item is the nearest earlier item with a
/// lower level, so skipped levels (h1 → h3) still nest.
#[must_use]
pub fn outline_tree(levels: &[u8]) -> OutlineTree {
    let n = levels.len();
    let mut parent: Vec<Option<usize>> = Vec::with_capacity(n);
    let mut kids: Vec<Vec<usize>> = vec![Vec::new(); n + 1];
    let mut stack: Vec<(usize, u8)> = Vec::new();
    for (i, &lv) in levels.iter().enumerate() {
        while stack.last().is_some_and(|&(_, l)| l >= lv) {
            stack.pop();
        }
        let p = stack.last().map(|&(j, _)| j);
        parent.push(p);
        if let Some(list) = kids.get_mut(p.unwrap_or(n)) {
            list.push(i);
        }
        stack.push((i, lv));
    }
    let mut siblings = vec![(None, None); n];
    for list in &kids {
        for (k, &i) in list.iter().enumerate() {
            if let Some(slot) = siblings.get_mut(i) {
                *slot = (
                    k.checked_sub(1).and_then(|k| list.get(k)).copied(),
                    list.get(k + 1).copied(),
                );
            }
        }
    }
    OutlineTree {
        parent,
        kids,
        siblings,
    }
}

/// A link rectangle on the page in PDF points `[x0, y0, x1, y1]` (bottom-left origin) from a
/// rectangle `(x, y, w, h)` in page DIPs (top-left origin). `x0` is the left margin and `top`
/// the top content edge in points, `scale` the content scale of the page.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // page coordinates in points are far below f32::MAX
pub fn link_rect(x0: f64, top: f64, scale: f64, r: (f32, f32, f32, f32)) -> [f32; 4] {
    let (x, y, w, h) = r;
    let px0 = x0 + f64::from(x) * PT * scale;
    let py0 = top - f64::from(y) * PT * scale;
    let px1 = px0 + f64::from(w) * PT * scale;
    let py1 = py0 - f64::from(h) * PT * scale;
    [px0 as f32, py1 as f32, px1 as f32, py0 as f32]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s6_safe_uri_rejects_userinfo() {
        for ok in [
            "https://example.com/a@b",
            "http://example.com/?q=a@b",
            "https://example.com#x@y",
            "mailto:a@b.c",
        ] {
            assert!(safe_uri(ok), "{ok}");
        }
        for bad in [
            "https://user@example.com/",
            "https://bank.com:pw@evil.example/",
            "HTTP://a@b",
            "https://x\\@y/",
            "file:///C:/x",
            "javascript:alert(1)",
            "https://e.com/\u{7}",
        ] {
            assert!(!safe_uri(bad), "{bad}");
        }
    }

    #[test]
    fn uri_allow_list() {
        assert!(safe_uri("https://example.org/a"));
        assert!(safe_uri("HTTP://x"));
        assert!(safe_uri("mailto:a@b.c"));
        for bad in [
            "javascript:alert(1)",
            "file:///c:/x",
            "data:text/html,x",
            "vbscript:x",
            "https://x\n.y",
            "ftp://x",
        ] {
            assert!(!safe_uri(bad), "{bad}");
        }
    }

    #[test]
    fn fs_type_outline_fallback() {
        assert!(fs_type_embeddable(0x0000), "installable");
        assert!(fs_type_embeddable(0x0004), "preview & print");
        assert!(fs_type_embeddable(0x0008), "editable");
        assert!(
            fs_type_embeddable(0x0100),
            "no subsetting bit alone is fine"
        );
        assert!(!fs_type_embeddable(0x0002), "restricted → outlines");
        assert!(!fs_type_embeddable(0x0204), "bitmap-only → outlines");
    }

    #[test]
    fn has_table_plain_and_collection() {
        // Plain sfnt with two tables: glyf, head.
        let mut f = vec![0, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0];
        f.extend_from_slice(b"glyf");
        f.extend_from_slice(&[0; 12]);
        f.extend_from_slice(b"head");
        f.extend_from_slice(&[0; 12]);
        assert!(has_table(&f, 0, *b"glyf"));
        assert!(!has_table(&f, 0, *b"CFF "));
        // Collection: member 1 at offset 20 has only CFF.
        let mut c = b"ttcf".to_vec();
        c.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 2]);
        c.extend_from_slice(&0u32.to_be_bytes());
        c.extend_from_slice(&20u32.to_be_bytes());
        c.extend_from_slice(&[0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]);
        c.extend_from_slice(b"CFF ");
        c.extend_from_slice(&[0; 12]);
        assert!(has_table(&c, 1, *b"CFF "));
        assert!(!has_table(&c, 1, *b"glyf"));
        assert!(!has_table(&[], 0, *b"glyf"));
    }

    #[test]
    fn cluster_pairs_one_to_one_and_ligature() {
        let u = |s: &str| s.encode_utf16().collect::<Vec<u16>>();
        // "abc" → three glyphs, one per character.
        let p = cluster_pairs(&[10, 11, 12], &[0, 1, 2], &u("abc"));
        assert_eq!(
            p,
            vec![(10, "a".into()), (11, "b".into()), (12, "c".into())]
        );
        // "ffi" → one ligature glyph: the whole text goes to it.
        let p = cluster_pairs(&[42], &[0, 0, 0], &u("ffi"));
        assert_eq!(p, vec![(42, "ffi".into())]);
        // A surrogate pair (emoji) → one glyph with both units.
        let p = cluster_pairs(&[7], &[0, 0], &u("😀"));
        assert_eq!(p, vec![(7, "😀".into())]);
        // Glyph 0 (.notdef) never gets a mapping.
        assert!(cluster_pairs(&[0], &[0], &u("x")).is_empty());
    }

    #[test]
    fn tounicode_cmap_maps_new_ids() {
        let mut uni = HashMap::new();
        uni.insert(2u16, "é".to_string());
        uni.insert(1u16, "ffi".to_string());
        let cmap = String::from_utf8_lossy(&to_unicode_cmap(&uni)).into_owned();
        assert!(cmap.contains("begincmap") && cmap.contains("endcmap"));
        assert!(cmap.contains("<0001> <006600660069>"), "{cmap}");
        assert!(cmap.contains("<0002> <00E9>"), "{cmap}");
        let a = cmap.find("<0001>").unwrap_or(usize::MAX);
        let b = cmap.find("<0002>").unwrap_or(0);
        assert!(a < b, "sorted by glyph id");
    }

    #[test]
    fn glyph_remap_after_subsetting() {
        let mut remap = subsetter::GlyphRemapper::new();
        // .notdef stays 0; new ids follow first use.
        assert_eq!(remap.remap(500), 1);
        assert_eq!(remap.remap(36), 2);
        assert_eq!(remap.remap(500), 1, "stable");
        let mut w = HashMap::new();
        w.insert(500u16, 600.0);
        w.insert(36u16, 250.0);
        assert_eq!(cid_widths(&remap, &w), vec![0.0, 600.0, 250.0]);
        // ToUnicode keys are the new ids.
        let mut uni = HashMap::new();
        uni.insert(remap.remap(36), "A".to_string());
        let cmap = String::from_utf8_lossy(&to_unicode_cmap(&uni)).into_owned();
        assert!(cmap.contains("<0002> <0041>"));
    }

    #[test]
    fn subset_names() {
        let a = String::from_utf8_lossy(&subset_name(0, "Segoe UI")).into_owned();
        let (tag, base) = a.split_once('+').unwrap_or_default();
        assert_eq!(tag.len(), 6);
        assert!(tag.chars().all(|c| c.is_ascii_uppercase()));
        assert_eq!(base, "SegoeUI");
        assert_ne!(subset_name(1, "X"), subset_name(2, "X"));
        assert!(String::from_utf8_lossy(&subset_name(3, "")).ends_with("+YamdvFont3"));
    }

    #[test]
    fn outline_nesting() {
        // h1, h2, h3, h2, h1, h3 (skipped level), h2
        let t = outline_tree(&[1, 2, 3, 2, 1, 3, 2]);
        assert_eq!(
            t.parent,
            vec![None, Some(0), Some(1), Some(0), None, Some(4), Some(4)]
        );
        assert_eq!(t.top(), &[0, 4]);
        assert_eq!(t.kids[0], vec![1, 3]);
        assert_eq!(t.kids[4], vec![5, 6]);
        assert_eq!(t.siblings[1], (None, Some(3)));
        assert_eq!(t.siblings[3], (Some(1), None));
        assert_eq!(t.siblings[0], (None, Some(4)));
        // Starting below h1 and going up: all top level.
        let t = outline_tree(&[3, 2, 1]);
        assert_eq!(t.top(), &[0, 1, 2]);
        assert!(outline_tree(&[]).top().is_empty());
    }

    /// The object writer (pdf-writer) and the parts above together: a two-page document with
    /// an outline, a GoTo and a URI link and a Type0 font with ToUnicode parses back with a
    /// valid cross-reference table (lopdf, dev-dependency only).
    #[test]
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // tiny test counts
    fn xref_and_objects_parse_back() {
        use pdf_writer::types::{ActionType, AnnotationType};
        use pdf_writer::{Finish, Pdf, Rect, Ref, TextStr};
        let mut pdf = Pdf::new();
        let (cat, tree, p1, p2, root) = (
            Ref::new(1),
            Ref::new(2),
            Ref::new(3),
            Ref::new(4),
            Ref::new(5),
        );
        let (a1, a2, cmap, c1, c2) = (
            Ref::new(6),
            Ref::new(7),
            Ref::new(8),
            Ref::new(9),
            Ref::new(10),
        );
        let heads = [1u8, 2, 2];
        let items: Vec<Ref> = (0..heads.len()).map(|i| Ref::new(11 + i as i32)).collect();
        pdf.catalog(cat).pages(tree).outlines(root);
        pdf.pages(tree).kids([p1, p2]).count(2);
        pdf.stream(c1, b"BT ET");
        pdf.stream(c2, b"BT ET");
        let r = link_rect(54.0, 788.0, 1.0, (0.0, 0.0, 100.0, 20.0));
        let mut a = pdf.annotation(a1);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(r[0], r[1], r[2], r[3]));
        a.action()
            .action_type(ActionType::GoTo)
            .destination()
            .page(p2)
            .xyz(0.0, 700.0, None);
        a.finish();
        let mut a = pdf.annotation(a2);
        a.subtype(AnnotationType::Link)
            .rect(Rect::new(r[0], r[1], r[2], r[3]));
        a.action()
            .action_type(ActionType::Uri)
            .uri(Str(b"https://example.org"));
        a.finish();
        pdf.page(p1)
            .parent(tree)
            .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .contents(c1)
            .annotations([a1, a2]);
        pdf.page(p2)
            .parent(tree)
            .media_box(Rect::new(0.0, 0.0, 595.0, 842.0))
            .contents(c2);
        let t = outline_tree(&heads);
        for (i, id) in items.iter().enumerate() {
            let mut it = pdf.outline_item(*id);
            it.title(TextStr("h"))
                .parent(t.parent[i].map_or(root, |p| items[p]));
            if let (Some(f), Some(l)) = (t.kids[i].first(), t.kids[i].last()) {
                it.first(items[*f])
                    .last(items[*l])
                    .count(-(t.kids[i].len() as i32));
            }
            if let Some(p) = t.siblings[i].0 {
                it.prev(items[p]);
            }
            if let Some(n) = t.siblings[i].1 {
                it.next(items[n]);
            }
            it.dest().page(p1).xyz(0.0, 800.0, None);
        }
        pdf.outline(root).first(items[0]).last(items[0]).count(1);
        let mut uni = HashMap::new();
        uni.insert(1u16, "A".to_string());
        pdf.cmap(cmap, &to_unicode_cmap(&uni));
        let bytes = pdf.finish();
        let doc = lopdf::Document::load_mem(&bytes).unwrap_or_default();
        assert_eq!(doc.get_pages().len(), 2);
        assert_eq!(doc.objects.len(), 13, "every object is in the xref");
        // Every xref offset points at "<n> 0 obj".
        let text = String::from_utf8_lossy(&bytes);
        let xref = text.rfind("xref").unwrap_or(0);
        for (n, line) in text[xref..].lines().skip(3).take(13).enumerate() {
            let off: usize = line.get(..10).and_then(|s| s.parse().ok()).unwrap_or(0);
            assert!(
                text[off..].starts_with(&format!("{} 0 obj", n + 1)),
                "object {}",
                n + 1
            );
        }
        let outline = doc.get_toc().map(|t| t.toc.len()).unwrap_or(0);
        assert_eq!(outline, 3);
    }

    #[test]
    fn link_rects_flip_y() {
        // A4: 595 pt wide, 842 pt high, 54 pt margin, scale 1.
        let r = link_rect(54.0, 842.0 - 54.0, 1.0, (100.0, 40.0, 80.0, 20.0));
        assert_eq!(r, [129.0, 743.0, 189.0, 758.0]);
        assert!(r[0] < r[2] && r[1] < r[3], "normalised");
        let s = link_rect(54.0, 788.0, 0.5, (100.0, 40.0, 80.0, 20.0));
        assert_eq!(s, [91.5, 765.5, 121.5, 773.0]);
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
//! Markdown → layout (mock shaper) → pagination → PDF objects (pdf-writer + the pure parts of
//! the exporter: outline tree, link rectangles, S-EXP-03 URI filter, footer) → the written file
//! has a valid cross-reference table (HARD-02). The DirectWrite glyph path is Windows-only.
#![no_main]
use libfuzzer_sys::fuzz_target;
use pdf_writer::types::{ActionType, AnnotationType};
use pdf_writer::{Finish, Name, Pdf, Rect, Ref, Str, TextStr};
use yamdv_core::doc::{Document, ReadOpts};
use yamdv_export::paginate::{Unit, footer, paginate};
use yamdv_export::pdfparts::{link_rect, outline_tree, safe_uri};
use yamdv_layout::block::{EnStrings, Metrics};
use yamdv_layout::shaper::MockShaper;
use yamdv_layout::viewer::{Content, Env, Viewer};

/// Every xref entry points at `n 0 obj`; `startxref` points at the table (byte offsets).
fn check_xref(b: &[u8]) {
    let find = |hay: &[u8], pat: &[u8]| hay.windows(pat.len()).rposition(|w| w == pat);
    let at = |o: usize, pat: &[u8]| b.get(o..).is_some_and(|t| t.starts_with(pat));
    let sx = find(b, b"startxref").expect("startxref");
    let tail = std::str::from_utf8(&b[sx + 9..]).expect("ascii trailer");
    let off: usize = tail.trim().lines().next().unwrap_or("").trim().parse().expect("offset");
    assert!(at(off, b"xref"), "startxref");
    let table = &b[off..sx];
    let table = std::str::from_utf8(table).expect("ascii xref");
    let mut lines = table.lines().skip(1);
    let head = lines.next().expect("subsection");
    let mut it = head.split_whitespace();
    let (first, count): (usize, usize) = (
        it.next().and_then(|x| x.parse().ok()).expect("first"),
        it.next().and_then(|x| x.parse().ok()).expect("count"),
    );
    for (k, l) in lines.take(count).enumerate() {
        let id = first + k;
        if l.trim_end().ends_with('f') {
            continue;
        }
        let o: usize = l.get(..10).and_then(|x| x.parse().ok()).expect("entry");
        assert!(at(o, format!("{id} 0 obj").as_bytes()), "object {id} at {o}");
    }
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 32 * 1024 {
        return;
    }
    let text = String::from_utf8_lossy(data).into_owned();
    let urls: Vec<String> = text
        .match_indices("](")
        .filter_map(|(i, _)| {
            let rest = &text[i + 2..];
            rest.find(')').map(|j| rest[..j].to_string())
        })
        .take(32)
        .collect();
    let mut content = Content::Markdown(Document::new(text, ReadOpts::default()));
    let m = Metrics::default();
    let pal = yamdv_core::theme::palette("default", false);
    let mut shaper = MockShaper;
    let mut v = Viewer::new(&content, 500.0, m);
    let rich = yamdv_layout::rich::MockRich::default();
    let mut env = Env {
        shaper: &mut shaper,
        m,
        pal: &pal,
        strings: &EnStrings,
        single_newline_break: false,
        images: &yamdv_layout::img::NO_IMAGES,
        rich: &rich,
    };
    let h = v.total_height() + 1.0;
    v.prepare(&mut content, &mut env, h);
    let n = content.len();
    let units: Vec<Unit> = (0..n)
        .map(|i| {
            let (top, hh) = v.slot(i);
            Unit { top, bottom: top + hh.max(0.0), breaks: Vec::new() }
        })
        .collect();
    let page_h = 700.0;
    let pages = paginate(&units, page_h);
    assert!(pages.iter().all(|p| p.scale > 0.0 && p.scale <= 1.0 && p.y1 >= p.y0));
    let heads: Vec<(usize, u8)> =
        (0..n).filter_map(|i| content.heading_level(i).map(|l| (i, l.clamp(1, 6)))).take(500).collect();

    let mut pdf = Pdf::new();
    let mut next = 1i32;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let (cat, tree, font, root) = (alloc(), alloc(), alloc(), alloc());
    let page_refs: Vec<(Ref, Ref)> = pages.iter().map(|_| (alloc(), alloc())).collect();
    let items: Vec<Ref> = heads.iter().map(|_| alloc()).collect();
    let annots: Vec<Ref> = urls.iter().filter(|u| safe_uri(u)).map(|_| alloc()).collect();
    if heads.is_empty() {
        pdf.catalog(cat).pages(tree);
    } else {
        pdf.catalog(cat).pages(tree).outlines(root);
    }
    let count = i32::try_from(page_refs.len()).unwrap_or(i32::MAX);
    pdf.pages(tree).kids(page_refs.iter().map(|p| p.0)).count(count);
    pdf.type1_font(font).base_font(Name(b"Helvetica"));
    for (k, (pr, cr)) in page_refs.iter().enumerate() {
        let txt = footer(k + 1, pages.len());
        pdf.stream(*cr, format!("BT /F1 9 Tf 280 20 Td ({txt}) Tj ET").as_bytes());
        let mut p = pdf.page(*pr);
        p.parent(tree).media_box(Rect::new(0.0, 0.0, 595.0, 842.0)).contents(*cr);
        p.resources().fonts().pair(Name(b"F1"), font);
        if k == 0 && !annots.is_empty() {
            p.annotations(annots.iter().copied());
        }
        p.finish();
    }
    let page_of = |y: f64| pages.iter().position(|p| y < p.y1).unwrap_or(pages.len().saturating_sub(1));
    let mut ai = 0;
    for u in urls.iter().filter(|u| safe_uri(u)) {
        let r = link_rect(54.0, 788.0, 1.0, (0.0, 20.0 * ai as f32, 100.0, 14.0));
        let mut a = pdf.annotation(annots[ai]);
        a.subtype(AnnotationType::Link).rect(Rect::new(r[0], r[1], r[2], r[3]));
        a.action().action_type(ActionType::Uri).uri(Str(u.as_bytes()));
        a.finish();
        ai += 1;
    }
    if !heads.is_empty() && !page_refs.is_empty() {
        let levels: Vec<u8> = heads.iter().map(|h| h.1).collect();
        let t = outline_tree(&levels);
        for (i, id) in items.iter().enumerate() {
            let pg = page_refs[page_of(units[heads[i].0].top)].0;
            let mut it = pdf.outline_item(*id);
            it.title(TextStr("h")).parent(t.parent[i].map_or(root, |p| items[p]));
            if let (Some(f), Some(l)) = (t.kids[i].first(), t.kids[i].last()) {
                it.first(items[*f]).last(items[*l]).count(-i32::try_from(t.kids[i].len()).unwrap_or(0));
            }
            if let Some(p) = t.siblings[i].0 {
                it.prev(items[p]);
            }
            if let Some(nx) = t.siblings[i].1 {
                it.next(items[nx]);
            }
            it.dest().page(pg).xyz(0.0, 800.0, None);
            it.finish();
        }
        let top = t.top();
        let mut o = pdf.outline(root);
        if let (Some(f), Some(l)) = (top.first(), top.last()) {
            o.first(items[*f]).last(items[*l]);
        }
        o.count(i32::try_from(top.len()).unwrap_or(0));
        o.finish();
    }
    let bytes = pdf.finish();
    assert!(bytes.starts_with(b"%PDF-"));
    check_xref(&bytes);
});

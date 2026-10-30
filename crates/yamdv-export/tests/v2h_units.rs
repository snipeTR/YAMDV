// SPDX-License-Identifier: AGPL-3.0-or-later
//! 018a §2-D unit tests: rowspan (model / CSV / HTML), BUG-03 Markdown slices, BUG-07
//! highlight pairing across inline events, named entities.
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use yamdv_core::doc::{Document, ReadOpts};

const ROWSPAN: &str = "<table>\n<tr><td rowspan=\"2\">A</td><td>B</td></tr>\n<tr><td>C</td></tr>\n<tr><td>D</td><td>E</td></tr>\n</table>\n";

struct NoHost;
impl yamdv_export::html::ExportHost for NoHost {
    fn image(
        &self,
        _img: &yamdv_core::doc::detail::ImageRef,
    ) -> Option<(&'static str, Vec<u8>, u32, u32)> {
        None
    }
    fn math(&self, _tex: &str, _display: bool) -> Option<yamdv_export::html::Png> {
        None
    }
    fn diagram(&self, _source: &str) -> Option<yamdv_export::html::Png> {
        None
    }
}

fn export(src: &str) -> String {
    let doc = Document::new(src.into(), ReadOpts::default());
    let o = yamdv_export::html::DocOpts {
        title: "t".into(),
        pal: yamdv_core::theme::palette("default", false),
        dark: false,
        toc: false,
    };
    yamdv_export::html::document(&doc, &NoHost, &o)
}

#[test]
fn rowspan_model_csv_and_html() {
    let doc = Document::new(ROWSPAN.into(), ReadOpts::default());
    let d = doc.detail_uncached(0);
    assert_eq!(d.rowspans.len(), 1);
    assert_eq!(d.rowspan(d.rowspans[0].0 as usize), 2);
    // CSV: the spanned cell leaves an empty field in the next row, columns stay aligned.
    let rows = yamdv_export::csv::table_at(&doc, 0, 0).unwrap();
    assert_eq!(
        rows,
        vec![
            vec!["A".to_string(), "B".into()],
            vec![String::new(), "C".into()],
            vec!["D".into(), "E".into()],
        ]
    );
    // HTML export keeps the attribute (and nothing invents a second one).
    let out = export(ROWSPAN);
    assert_eq!(out.matches("rowspan=\"2\"").count(), 1, "{out}");
}

#[test]
fn bug_03_markdown_slice_never_cuts_a_construct() {
    let src = "See [the docs][d] and <https://example.org/x> or `code span` and $x^2$.\n\n[d]: https://example.org/d\n";
    let doc = Document::new(src.into(), ReadOpts::default());
    for (needle, want) in [
        ("docs", "[the docs][d]"),
        ("example.org/x", "<https://example.org/x>"),
        ("de sp", "`code span`"),
        ("^2", "$x^2$"),
    ] {
        let at = u32::try_from(src.find(needle).unwrap()).unwrap();
        let end = at + u32::try_from(needle.len()).unwrap();
        let s = yamdv_export::html::markdown_slice(&doc, at..end);
        assert!(s.contains(want), "{needle}: {s:?}");
    }
}

#[test]
fn bug_07_highlight_spans_emphasis_code_and_entities() {
    let out = export("A ==mark *em* `c` &amp; &copy; end== B\n");
    // Every piece between the markers is highlighted, including inside emphasis and code.
    let p = &out[out.find("<p>").unwrap()..];
    let p = &p[..p.find("</p>").unwrap()];
    let marked: String = p
        .split("<mark>")
        .skip(1)
        .map(|m| m.split("</mark>").next().unwrap_or_default())
        .collect();
    assert_eq!(marked, "mark em c &amp; \u{a9} end", "{p}");
    assert!(p.contains("<em><mark>em</mark></em>"), "{p}");
    assert!(p.contains("<code><mark>c</mark></code>"), "{p}");
    assert!(!out.contains("=="), "markers stay hidden: {out}");
}

#[test]
fn bug_07_escaped_marker_is_not_a_highlight() {
    // Found by the `highlight` golden (018a diag run): `\==x==` stays literal.
    let out = export("an escaped \\==not highlighted== marker and ==yes==\n");
    let p = &out[out.find("<p>").unwrap()..];
    let p = &p[..p.find("</p>").unwrap()];
    assert_eq!(p.matches("<mark>").count(), 1, "{p}");
    assert!(p.contains("<mark>yes</mark>"), "{p}");
    assert!(p.contains("==not highlighted=="), "{p}");
}

#[test]
fn named_entity_table_is_complete() {
    // WHATWG list: 2231 names (with and without `;`).
    for (name, want) in [
        ("amp;", "&"),
        ("AElig", "\u{c6}"),
        ("NotNestedGreaterGreater;", "\u{2aa2}\u{338}"),
        ("zwnj;", "\u{200c}"),
        ("copy", "\u{a9}"),
    ] {
        assert_eq!(yamdv_core::html::named(name), Some(want), "{name}");
    }
    assert_eq!(yamdv_core::html::named("notanentity;"), None);
    assert_eq!(yamdv_core::html::named_count(), 2231);
}

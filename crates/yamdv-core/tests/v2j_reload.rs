// SPDX-License-Identifier: AGPL-3.0-or-later
//! PERF-06 (prompt 020 §3.6): incremental live reload. An external change is diffed against the
//! loaded text and applied as one splice; only the changed blocks are re-parsed, and the result
//! equals a full parse of the new text.

use yamdv_core::doc::{Document, ReadOpts};

fn paras(n: usize, changed: Option<usize>) -> String {
    (0..n)
        .map(|i| {
            if Some(i) == changed {
                format!("Paragraph {i} was edited outside the app.\n")
            } else {
                format!("Paragraph {i} filler text.\n")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn same_as_full_parse(doc: &Document, text: &str) {
    let full = Document::new(text.to_string(), doc.opts);
    assert_eq!(doc.source, text);
    assert_eq!(doc.index.blocks, full.index.blocks);
    assert_eq!(doc.index.headings, full.index.headings);
}

#[test]
fn one_changed_paragraph_reparses_at_most_two_blocks() {
    let old = paras(200, None);
    let new = paras(200, Some(120));
    let mut doc = Document::new(old, ReadOpts::default());
    let (_, sp) = doc.sync_to(&new).expect("changed");
    // The splice re-checks the block before the edit (a paragraph can continue lazily).
    assert!(
        sp.replaced.len() <= 2 && sp.replaced.contains(&120),
        "{sp:?}"
    );
    assert_eq!(sp.inserted, sp.replaced.len(), "{sp:?}");
    same_as_full_parse(&doc, &new);
    assert!(doc.sync_to(&new).is_none(), "no change, no splice");
}

#[test]
fn appended_and_removed_text_and_headings_match_a_full_parse() {
    let cases = [
        ("# A\n\none\n\ntwo\n", "# A\n\none\n\ntwo\n\nthree\n"),
        ("# A\n\none\n\ntwo\n\nthree\n", "# A\n\none\n\nthree\n"),
        ("# A\n\none\n", "## B\n\n# A\n\none\n"),
        ("text\n\n```\ncode\n```\n", "text\n\n```\ncode\nmore\n```\n"),
        ("a [r]\n\n[r]: /x\n", "a [r]\n\n[r]: /y\n"),
        ("Çay\n\nşeker\n", "Çay\n\nşeker 😀\n"),
        ("x\n", ""),
        ("", "# New\n"),
    ];
    for (old, new) in cases {
        let mut doc = Document::new(old.to_string(), ReadOpts::default());
        doc.sync_to(new);
        same_as_full_parse(&doc, new);
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
//! Clipboard HTML → Markdown paste converter (prompt 015 §3.5, HARD-02): no panic, bounded
//! output, and no live script in the produced Markdown.
#![no_main]
use libfuzzer_sys::fuzz_target;

/// A script *tag* at the start of raw HTML: `<script` + space, `/`, `>` or the end.
fn script_tag(raw: &str) -> bool {
    let low = raw.to_ascii_lowercase();
    low.match_indices("<script").any(|(i, m)| {
        low.get(i + m.len()..)
            .and_then(|r| r.chars().next())
            .is_none_or(|c| c.is_whitespace() || c == '/' || c == '>')
    })
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let s = String::from_utf8_lossy(data);
    let mut saved = 0usize;
    let mut save = |b: &[u8], ext: &'static str| -> Option<String> {
        saved += 1;
        (b.len() < 1 << 20).then(|| format!("assets/p{saved}.{ext}"))
    };
    if let Some(md) = yamdv_core::paste::html2md::convert(&s, &mut save) {
        assert!(md.len() <= 64 * s.len() + 4096, "output blow-up");
        // Parse the Markdown as CommonMark (+ GFM): no raw HTML event may hold a script tag;
        // code blocks, code spans and escaped text are inert by construction.
        use pulldown_cmark::{Event, Options, Parser};
        let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        for ev in Parser::new_ext(&md, opts) {
            if let Event::Html(h) | Event::InlineHtml(h) = ev {
                assert!(!script_tag(&h), "{h:?}");
            }
        }
    }
});

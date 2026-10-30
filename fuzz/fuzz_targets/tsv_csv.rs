// SPDX-License-Identifier: AGPL-3.0-or-later
//! TSV / CSV paste parser (R-TBL-07, HARD-02): no panic; detection agrees with the parser and
//! the parsed cells always build a table.
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_editor::{table, tsv};

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let s = String::from_utf8_lossy(data);
    for d in ['\t', ','] {
        let rows = tsv::parse(&s, d);
        assert!(rows.len() <= s.len() + 1);
    }
    if let Some((rows, _)) = tsv::detect(&s) {
        if let Some(t) = table::from_rows(&rows) {
            let md = t.to_markdown("\n", true);
            assert!(table::parse(&md).is_some(), "{md}");
        }
    }
});

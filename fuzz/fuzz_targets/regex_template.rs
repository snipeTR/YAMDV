// SPDX-License-Identifier: AGPL-3.0-or-later
//! Find / replace (R-EDIT find, regex-lite + time guard, HARD-02): pattern compile, search with
//! the guard, and `$1` / `${name}` replacement expansion never panic or hang.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::time::Duration;
use yamdv_editor::replace::{Query, expand, find_all};

fuzz_target!(|data: &[u8]| {
    if data.len() > 8 * 1024 {
        return;
    }
    let s = String::from_utf8_lossy(data);
    // Layout: flags byte, then pattern \0 replacement \0 haystack.
    let flags = data.first().copied().unwrap_or(0);
    let mut parts = s.get(1..).unwrap_or("").splitn(3, '\0');
    let (pat, with, hay) = (
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
        parts.next().unwrap_or(""),
    );
    let q = Query {
        needle: pat.to_string(),
        case: flags & 1 != 0,
        whole_word: flags & 2 != 0,
        regex: flags & 4 == 0,
    };
    if let Ok(found) = find_all(hay, &q, Duration::from_millis(200)) {
        for r in found.iter().take(64) {
            assert!(r.start < r.end && r.end <= hay.len());
            let _ = expand(hay, r, &q, with);
        }
    }
});

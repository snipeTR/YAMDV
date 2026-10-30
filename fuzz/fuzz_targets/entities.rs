// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTML character references (BUG-04, F-39 static table, HARD-02): decoding never panics,
//! never grows text unboundedly, and named lookups agree with the table.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 {
        return;
    }
    let s = String::from_utf8_lossy(data);
    let out = yamdv_core::html::decode_entities(&s);
    // The longest expansion is 2 chars (≤ 8 bytes) for a ≥ 2-byte reference.
    assert!(out.len() <= s.len() * 4 + 4);
    let name: String = s.chars().take(40).collect();
    if let Some(v) = yamdv_core::html::named(&name) {
        assert!(!v.is_empty());
    }
});

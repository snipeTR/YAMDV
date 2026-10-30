// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recovery journal parser (prompt 015 §3.6, HARD-02): never panics; whatever decodes
//! re-encodes to an equal journal.
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_core::journal;

fuzz_target!(|data: &[u8]| {
    let Some((h, text)) = journal::decode(data) else { return };
    assert!(text.len() <= journal::MAX_TEXT);
    // Header values that contain line breaks cannot come out of a decode.
    assert!(!h.path.contains(['\r', '\n']) && !h.encoding.contains(['\r', '\n']));
    let again = journal::encode(&h, &text).expect("encodable");
    let (h2, t2) = journal::decode(&again).expect("round trip");
    assert_eq!((h2, t2), (h, text));
});

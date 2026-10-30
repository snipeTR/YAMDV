// SPDX-License-Identifier: AGPL-3.0-or-later
//! CF_HTML header + fragment parsing (HARD-02): offsets never index outside the payload or
//! inside a UTF-8 sequence; the fragment then goes through the paste converter.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() > 64 * 1024 {
        return;
    }
    let raw = String::from_utf8_lossy(data);
    let f = yamdv_core::paste::cf_html_fragment(&raw);
    assert!(f.len() <= raw.len());
    let mut save = |_: &[u8], _: &'static str| -> Option<String> { None };
    let _ = yamdv_core::paste::html2md::convert(f, &mut save);
});

// SPDX-License-Identifier: AGPL-3.0-or-later
//! ICO directory parser + entry payloads (ADR-0022, HARD-02): validated entries stay inside the
//! file; payload extraction, mask and unpremultiply never panic.
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_core::image::ico;

fuzz_target!(|data: &[u8]| {
    let Some(dir) = ico::directory(data) else { return };
    assert!(!dir.is_empty() && dir.len() <= ico::MAX_ENTRIES);
    for e in &dir {
        assert!(e.len > 0 && e.off + e.len <= data.len());
    }
    for want in [16, 32, 48, 256] {
        if let Some(e) = ico::pick(&dir, want) {
            assert!(dir.contains(&e));
        }
    }
    for e in dir.iter().take(8) {
        if let Some(ico::Payload::Bgra { mut px, w, h }) = ico::payload(data, e) {
            assert_eq!(px.len() as u64, u64::from(w) * u64::from(h) * 4);
            ico::unpremultiply(&mut px);
        }
    }
});

// SPDX-License-Identifier: AGPL-3.0-or-later
//! Mermaid source → every supported diagram parser → layout + init theme with the mock measurer (prompt 007 §3.5).
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_mermaid::draw::{Ink, MockMeasure, Role};

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };
    if let Ok(d) = yamdv_mermaid::parse(s) {
        let (n, e) = d.counts();
        if n > 2000 || e > 6000 {
            return;
        }
        let out = d.layout_at(&mut MockMeasure::default(), 1.0, Some(1_790_000_000));
        // The init directive is applied at paint time (`Init::color`), so resolve a few roles.
        if let Some(i) = yamdv_mermaid::theme::init(s) {
            for r in [
                Role::NodeFill,
                Role::Text,
                Role::LabelBg,
                Role::LabelText,
                Role::Title,
                Role::ClusterFill,
            ] {
                for dark in [false, true] {
                    let _ = i.color(Ink::Role(r), dark, &|_| 0x00ff_ffff);
                }
            }
        }
        assert!(out.w.is_finite() && out.h.is_finite());
    }
});

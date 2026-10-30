// SPDX-License-Identifier: AGPL-3.0-or-later
//! Prompt 020 §5.2 (A-10): the committed toolbar icon table equals a fresh build from
//! `assets/icons/*.svg`, stays within the 60 KB budget, and every icon round-trips.
#![allow(clippy::indexing_slicing)]

use std::path::Path;
use yamdv_svg::icons::{Table, build, mask, outlines, read_dir};

/// Embedded-data budget (prompt 020 §5.2 item 1).
const BUDGET: usize = 60 * 1024;

#[test]
fn committed_table_matches_the_svgs_and_fits_the_budget() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons");
    let svgs = read_dir(&dir).expect("assets/icons");
    assert!(svgs.len() >= 95, "{} icons", svgs.len());
    let fresh = build(&svgs).expect("every icon parses");
    let committed = std::fs::read(dir.join("icons.bin")).expect("assets/icons/icons.bin");
    assert!(
        committed == fresh,
        "assets/icons/icons.bin is stale: run `cargo xtask icons`"
    );
    assert!(
        fresh.len() <= BUDGET,
        "table {} B > {BUDGET} B",
        fresh.len()
    );
    let t = Table::new(&committed).expect("YIC1");
    assert_eq!(t.names().len(), svgs.len());
    for (name, src) in &svgs {
        let a = outlines(src).expect("parses");
        let b = t.get(name).expect("in table");
        assert_eq!(a.len(), b.len(), "{name}");
        assert!(!b.is_empty(), "{name} has shapes");
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.fill, y.fill, "{name}");
            assert_eq!(x.stroke.is_some(), y.stroke.is_some(), "{name}");
            let (p, q) = (x.path.bounds(), y.path.bounds());
            assert!((p.left() - q.left()).abs() <= 0.07, "{name}");
            assert!((p.bottom() - q.bottom()).abs() <= 0.07, "{name}");
        }
        let m = mask(&b, 20);
        assert!(
            m.iter().any(|a| *a > 128),
            "{name} draws something at 20 px"
        );
    }
}

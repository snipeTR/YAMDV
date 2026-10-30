// SPDX-License-Identifier: AGPL-3.0-or-later
//! Table model (R-TBL, HARD-02): parse → edit operations → serialize → parse round trip keeps
//! the shape and the cells.
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_editor::table::{self, Align};

fuzz_target!(|data: &[u8]| {
    if data.len() > 16 * 1024 {
        return;
    }
    let split = data.iter().position(|&b| b == 0xff).unwrap_or(data.len());
    let text = String::from_utf8_lossy(&data[..split]);
    let ops = data.get(split + 1..).unwrap_or(&[]);
    let Some(mut t) = table::parse(&text) else { return };
    let _ = table::grid(&text);
    for w in ops.chunks(3).take(64) {
        let (op, a, b) = (w[0], usize::from(*w.get(1).unwrap_or(&0)), usize::from(*w.get(2).unwrap_or(&0)));
        let (rows, cols) = (t.rows.len(), t.cols());
        match op % 9 {
            0 => t.insert_row(a % (rows + 1)),
            1 if cols < 64 => t.insert_col(a % (cols + 1)),
            2 => { let _ = t.delete_row(a % rows.max(1)); }
            3 => { let _ = t.delete_col(a % cols.max(1)); }
            4 => t.set_align(a % cols.max(1), [Align::None, Align::Left, Align::Center, Align::Right][b % 4]),
            5 => t.sort(a % cols.max(1), b % 2 == 1),
            6 => { let _ = t.move_row(a % rows.max(1), b % rows.max(1)); }
            7 => { let _ = t.move_col(a % cols.max(1), b % cols.max(1)); }
            _ => {}
        }
        assert!(t.cols() >= 1 && t.rows.iter().all(|r| r.len() == t.cols()));
    }
    for pretty in [false, true] {
        let md = t.to_markdown("\n", pretty);
        let back = table::parse(&md).unwrap_or_else(|| panic!("re-parse failed:\n{md}"));
        assert_eq!(back.cols(), t.cols(), "{md}");
        assert_eq!(back.rows.len(), t.rows.len(), "{md}");
        assert_eq!(back.rows, t.rows, "{md}");
    }
});

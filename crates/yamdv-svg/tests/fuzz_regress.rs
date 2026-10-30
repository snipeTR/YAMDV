// SPDX-License-Identifier: AGPL-3.0-or-later
//! HARD-02 fuzz-long regressions (run 41016337092): the committed `regress-*` seeds of
//! `svg_parse` size and render without a panic (the fuzz target's oracle).

#[test]
fn hard02_svg_parse_regression_seeds() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/svg_parse");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).expect("seeds") {
        let p = e.expect("entry").path();
        if !p
            .file_name()
            .is_some_and(|f| f.to_string_lossy().starts_with("regress-"))
        {
            continue;
        }
        let Ok(s) = String::from_utf8(std::fs::read(&p).expect("read")) else {
            continue;
        };
        if let Ok((w, h)) = yamdv_svg::size(&s) {
            assert!(w.is_finite() && h.is_finite() && w >= 1.0 && h >= 1.0);
        }
        if let Ok(img) = yamdv_svg::render(&s, 64, 48, [0x20, 0x20, 0x20, 0xff]) {
            assert_eq!(img.rgba.len(), 64 * 48 * 4, "{}", p.display());
        }
        n += 1;
    }
    assert!(n >= 1);
}

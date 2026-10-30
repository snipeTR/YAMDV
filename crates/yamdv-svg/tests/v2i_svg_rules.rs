// SPDX-License-Identifier: AGPL-3.0-or-later
//! HARD-01 S-RES-07 matrix (prompt 019 §2.3): external references, scripts, `foreignObject`,
//! event attributes and CSS `@import` have no effect; DTDs are rejected; every limit holds.
#![allow(clippy::indexing_slicing)]

use yamdv_svg::render::{MAX_DIM, MAX_PIXELS};
use yamdv_svg::{Error, render, xml};

fn blank(src: &str) -> bool {
    render(src, 32, 32, [0, 0, 0, 255]).is_ok_and(|img| img.rgba.chunks(4).all(|p| p[3] == 0))
}

const W: &str = "<svg xmlns='http://www.w3.org/2000/svg' xmlns:xlink='http://www.w3.org/1999/xlink' width='32' height='32'>";

#[test]
fn hard01_s_res_07_inert_constructs_draw_nothing() {
    for inner in [
        "<image href='file:///C:/Windows/win.ini' width='32' height='32'/>",
        "<image xlink:href='\\\\server\\share\\x.png' width='32' height='32'/>",
        "<image href='https://x.example/x.png' width='32' height='32'/>",
        "<use href='other.svg#r'/>",
        "<use xlink:href='file:///C:/x.svg#r'/>",
        "<rect width='32' height='32' fill='url(other.svg#g)'/>",
        "<rect width='32' height='32' fill='url(https://x.example/#g)'/>",
        "<script>document.body.innerHTML='x'</script>",
        "<foreignObject width='32' height='32'><div xmlns='http://www.w3.org/1999/xhtml'>x</div></foreignObject>",
        "<foreignObject width='32' height='32'><rect width='32' height='32'/></foreignObject>",
        "<style>@import url(file:///C:/x.css);</style>",
        "<a href='javascript:alert(1)'></a>",
        "<use href='#missing'/>",
    ] {
        assert!(blank(&format!("{W}{inner}</svg>")), "{inner}");
    }
}

#[test]
fn hard01_s_res_07_event_attributes_are_ignored() {
    let plain = render(
        &format!("{W}<rect width='32' height='32' fill='red'/></svg>"),
        32,
        32,
        [0; 4],
    );
    let evil = render(
        &format!("{W}<rect width='32' height='32' fill='red' onload='x()' onclick='y()'/></svg>"),
        32,
        32,
        [0; 4],
    );
    assert_eq!(plain.map(|i| i.rgba), evil.map(|i| i.rgba));
}

#[test]
fn hard01_s_res_07_limits() {
    for (src, want) in [
        ("<!DOCTYPE svg><svg/>".to_string(), Error::Dtd),
        (
            "<!DOCTYPE svg [<!ENTITY a 'x'>]><svg>&a;</svg>".to_string(),
            Error::Dtd,
        ),
        (
            "<?xml version='1.0'?><!DOCTYPE x SYSTEM 'file:///C:/x.dtd'><svg/>".to_string(),
            Error::Dtd,
        ),
        (
            format!(
                "<svg>{}{}</svg>",
                "<g>".repeat(xml::MAX_DEPTH),
                "</g>".repeat(xml::MAX_DEPTH)
            ),
            Error::TooDeep,
        ),
        (
            format!("<svg>{}</svg>", "<g/>".repeat(xml::MAX_ELEMENTS)),
            Error::TooMany,
        ),
        (
            format!("<svg>{}</svg>", " ".repeat(xml::MAX_INPUT)),
            Error::TooLarge,
        ),
    ] {
        assert_eq!(
            xml::parse(&src).err(),
            Some(want),
            "{}",
            &src[..src.len().min(60)]
        );
    }
    // Output size follows S-RES-04 (16 384 px per side, 64 MP).
    assert_eq!(MAX_PIXELS, 64_000_000);
    assert_eq!(
        render("<svg/>", MAX_DIM + 1, 1, [0; 4]).err(),
        Some(Error::TooLarge)
    );
    assert_eq!(
        render("<svg/>", 8001, 8000, [0; 4]).err(),
        Some(Error::TooLarge)
    );
    // A `use` bomb (10 levels × 10 fan-out) stays bounded and terminates.
    let mut s = String::from(W);
    s.push_str("<defs><rect id='l0' width='1' height='1'/>");
    for i in 1..10 {
        s.push_str(&format!("<g id='l{i}'>"));
        for _ in 0..10 {
            s.push_str(&format!("<use href='#l{}'/>", i - 1));
        }
        s.push_str("</g>");
    }
    s.push_str("</defs><use href='#l9'/></svg>");
    let t = std::time::Instant::now();
    let _ = render(&s, 32, 32, [0; 4]);
    assert!(t.elapsed().as_secs() < 5);
    // A self-referencing `use` cycle terminates.
    let _ = render(
        &format!("{W}<g id='a'><use href='#a'/></g></svg>"),
        32,
        32,
        [0; 4],
    );
}

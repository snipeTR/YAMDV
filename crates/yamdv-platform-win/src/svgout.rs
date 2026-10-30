// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recorded band commands → inline SVG for the interactive HTML export (R-EXP-02): paths,
//! clips and images as SVG elements; glyphs as `<use>` of shared `<symbol>` outlines from
//! `GetGlyphRunOutline`, so the file does not depend on the reader's fonts.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::collections::HashMap;
use std::fmt::Write as _;

use tiny_skia::{Path, PathSegment};
use windows::Win32::Graphics::DirectWrite::IDWriteFontFace;
use windows::core::Interface;

use crate::render::{Cmd, GlyphPaint, GlyphRec};

/// Glyph outlines are stored at this em size and scaled by `<use>`.
const EM: f32 = 1000.0;

/// Shared definitions of one export: glyph symbols and unique ids.
#[derive(Default)]
pub struct Defs {
    /// Faces kept alive so their pointers stay unique keys.
    faces: Vec<IDWriteFontFace>,
    glyphs: HashMap<(usize, u16), Option<u32>>,
    out: String,
    next: u32,
}

impl Defs {
    /// The `<symbol>` elements written so far.
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.out)
    }

    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    /// Symbol id of glyph `gid` of `face` (`None` = empty outline, e.g. a space).
    fn glyph(&mut self, face: &IDWriteFontFace, gid: u16) -> Option<u32> {
        let key = (face.as_raw() as usize, gid);
        if let Some(v) = self.glyphs.get(&key) {
            return *v;
        }
        if !self.faces.iter().any(|f| f.as_raw() == face.as_raw()) {
            self.faces.push(face.clone());
        }
        let g = GlyphRec {
            face: face.clone(),
            size: EM,
            x: 0.0,
            y: 0.0,
            ids: vec![gid],
            adv: vec![0.0],
            offs: vec![(0.0, 0.0)],
            rtl: false,
            color: 0,
            text: Vec::new(),
            clusters: Vec::new(),
            paint: GlyphPaint::Outline,
        };
        let v = crate::pdf::glyph_outline(&g, 0.0, 0.0).map(|p| {
            let id = self.id();
            let _ = write!(
                self.out,
                "<symbol id=\"g{id}\" overflow=\"visible\"><path d=\"{}\"/></symbol>",
                path_d(&p, 0)
            );
            id
        });
        self.glyphs.insert(key, v);
        v
    }
}

fn num(out: &mut String, v: f32, prec: usize) {
    let s = format!("{v:.prec$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        &s
    };
    out.push_str(if s == "-0" { "0" } else { s });
}

/// SVG path data of `p` with `prec` decimals.
pub fn path_d(p: &Path, prec: usize) -> String {
    let mut d = String::new();
    let pt = |d: &mut String, c: char, xy: &[(f32, f32)]| {
        d.push(c);
        for (k, (x, y)) in xy.iter().enumerate() {
            if k > 0 {
                d.push(' ');
            }
            num(d, *x, prec);
            d.push(' ');
            num(d, *y, prec);
        }
    };
    for s in p.segments() {
        match s {
            PathSegment::MoveTo(a) => pt(&mut d, 'M', &[(a.x, a.y)]),
            PathSegment::LineTo(a) => pt(&mut d, 'L', &[(a.x, a.y)]),
            PathSegment::QuadTo(a, b) => pt(&mut d, 'Q', &[(a.x, a.y), (b.x, b.y)]),
            PathSegment::CubicTo(a, b, c) => {
                pt(&mut d, 'C', &[(a.x, a.y), (b.x, b.y), (c.x, c.y)]);
            }
            PathSegment::Close => d.push('Z'),
        }
    }
    d
}

fn hex(c: u32) -> String {
    format!("#{:06x}", c & 0xff_ffff)
}

fn png_uri(w: u32, h: u32, px: &[u8]) -> Option<String> {
    let mut px = px.to_vec();
    for p in px.as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    let png = crate::snapshot::encode_png(w, h, &px).ok()?;
    Some(yamdv_export::html::data_uri("image/png", &png))
}

/// SVG children for `cmds` (DIPs = CSS px). `Clear` is skipped (the page background shows).
pub fn body(cmds: &[Cmd], defs: &mut Defs) -> String {
    let mut s = String::new();
    let mut depth = 0usize;
    for c in cmds {
        match c {
            Cmd::Clear(_) => {}
            Cmd::Fill { path, color, alpha } => {
                let _ = write!(
                    s,
                    "<path d=\"{}\" fill=\"{}\"",
                    path_d(path, 2),
                    hex(*color)
                );
                if *alpha < 0.999 {
                    let _ = write!(s, " fill-opacity=\"{alpha:.3}\"");
                }
                s.push_str("/>");
            }
            Cmd::Stroke {
                path,
                color,
                width,
                round,
                dash,
            } => {
                let _ = write!(
                    s,
                    "<path d=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"{width:.2}\"",
                    path_d(path, 2),
                    hex(*color)
                );
                if *round {
                    s.push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
                }
                if let Some((on, off)) = dash {
                    let _ = write!(s, " stroke-dasharray=\"{on:.2} {off:.2}\"");
                }
                s.push_str("/>");
            }
            Cmd::PushClip(r) => {
                let id = defs.id();
                let _ = write!(
                    s,
                    "<clipPath id=\"c{id}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\"/></clipPath><g clip-path=\"url(#c{id})\">",
                    r.x,
                    r.y,
                    r.w.max(0.0),
                    r.h.max(0.0)
                );
                depth += 1;
            }
            Cmd::PopClip => {
                if depth > 0 {
                    s.push_str("</g>");
                    depth -= 1;
                }
            }
            Cmd::Image { rect, w, h, px } => {
                if let Some(uri) = png_uri(*w, *h, px) {
                    let _ = write!(
                        s,
                        "<image x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"{uri}\"/>",
                        rect.x, rect.y, rect.w, rect.h
                    );
                }
            }
            Cmd::Jpeg { rect, bytes, .. } => {
                let uri = yamdv_export::html::data_uri("image/jpeg", bytes);
                let _ = write!(
                    s,
                    "<image x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" preserveAspectRatio=\"none\" href=\"{uri}\"/>",
                    rect.x, rect.y, rect.w, rect.h
                );
            }
            Cmd::Glyphs(g) => glyphs(&mut s, g, defs),
        }
    }
    for _ in 0..depth {
        s.push_str("</g>");
    }
    s
}

fn glyphs(s: &mut String, g: &GlyphRec, defs: &mut Defs) {
    if g.paint == GlyphPaint::Invisible {
        return;
    }
    let k = g.size / EM;
    let _ = write!(s, "<g fill=\"{}\">", hex(g.color));
    let mut pen = 0.0f32;
    for (i, &gid) in g.ids.iter().enumerate() {
        let (ao, asc) = g.offs.get(i).copied().unwrap_or((0.0, 0.0));
        let a = g.adv.get(i).copied().unwrap_or(0.0);
        let x = if g.rtl {
            pen += a;
            g.x - pen - ao
        } else {
            let x = g.x + pen + ao;
            pen += a;
            x
        };
        let y = g.y - asc;
        if let Some(id) = defs.glyph(&g.face, gid) {
            let _ = write!(s, "<use href=\"#g{id}\" transform=\"translate(");
            num(s, x, 2);
            s.push(' ');
            num(s, y, 2);
            s.push_str(") scale(");
            num(s, k, 5);
            s.push_str(")\"/>");
        }
    }
    s.push_str("</g>");
}

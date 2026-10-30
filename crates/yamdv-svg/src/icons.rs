// SPDX-License-Identifier: AGPL-3.0-or-later
//! Toolbar / menu icon table (prompt 020 §5.2, UI_SPEC B.4.1). The owner's 24 × 24 stroke SVGs
//! are parsed once at build time (`cargo xtask icons`, S-RES-07 rules of this crate) into a
//! compact binary path table that the exe embeds; the SVG text itself is never embedded.
//!
//! Format `YIC1` (little endian):
//! `"YIC1" count:u16` then per icon `name_len:u8 name payload_len:u16 payload`, where the
//! payload is `shapes:u8` and per shape `flags:u8 width:u8 verbs:u16 verb* point*`.
//! Flags: 1 fill, 2 stroke, 4 round cap, 8 round join; `width` = stroke width × 16. Verbs:
//! 0 move (1 point), 1 line (1), 2 quad (2), 3 cubic (3), 4 close (0). A point is two `u8`
//! coordinates quantized as `(v + 4) × 8` (−4 … 27.875 in steps of 1/8 of the 24-unit box).
//!
//! Casts below quantize values that are clamped first; slices are bounded by `min`.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing
)]

use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, PathSegment, Pixmap, Stroke, Transform,
};

use crate::Error;

/// Icon box side (user units).
pub const BOX: f32 = 24.0;
const MAGIC: &[u8; 4] = b"YIC1";
const Q: f32 = 8.0;
const OFF: f32 = 4.0;

/// One shape of an icon in box units.
#[derive(Debug, Clone)]
pub struct Outline {
    /// The outline (dashes already applied).
    pub path: Path,
    /// Filled with the tint.
    pub fill: bool,
    /// Stroked with this width (box units).
    pub stroke: Option<f32>,
    /// Round line caps.
    pub round_cap: bool,
    /// Round line joins.
    pub round_join: bool,
}

/// The shapes of an icon SVG mapped onto the 24 × 24 box (no pixels are drawn).
pub fn outlines(svg: &str) -> Result<Vec<Outline>, Error> {
    crate::render::outlines(svg, BOX as u32, BOX as u32)
}

fn quant(v: f32) -> u8 {
    ((v + OFF) * Q).round().clamp(0.0, 255.0) as u8
}

fn dequant(b: u8) -> f32 {
    f32::from(b) / Q - OFF
}

/// Encodes named icons into a `YIC1` table.
pub fn encode(icons: &[(String, Vec<Outline>)]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(icons.len() as u16).to_le_bytes());
    for (name, shapes) in icons {
        let mut p = vec![shapes.len().min(255) as u8];
        for s in shapes.iter().take(255) {
            let flags = u8::from(s.fill)
                | (u8::from(s.stroke.is_some()) << 1)
                | (u8::from(s.round_cap) << 2)
                | (u8::from(s.round_join) << 3);
            p.push(flags);
            p.push((s.stroke.unwrap_or(0.0) * 16.0).round().clamp(0.0, 255.0) as u8);
            let mut verbs = Vec::new();
            let mut pts = Vec::new();
            let pt = |q: tiny_skia::Point, pts: &mut Vec<u8>| {
                pts.push(quant(q.x));
                pts.push(quant(q.y));
            };
            for seg in s.path.segments() {
                match seg {
                    PathSegment::MoveTo(a) => {
                        verbs.push(0);
                        pt(a, &mut pts);
                    }
                    PathSegment::LineTo(a) => {
                        verbs.push(1);
                        pt(a, &mut pts);
                    }
                    PathSegment::QuadTo(a, b) => {
                        verbs.push(2);
                        pt(a, &mut pts);
                        pt(b, &mut pts);
                    }
                    PathSegment::CubicTo(a, b, c) => {
                        verbs.push(3);
                        pt(a, &mut pts);
                        pt(b, &mut pts);
                        pt(c, &mut pts);
                    }
                    PathSegment::Close => verbs.push(4),
                }
            }
            p.extend_from_slice(&(verbs.len() as u16).to_le_bytes());
            p.extend_from_slice(&verbs);
            p.extend_from_slice(&pts);
        }
        let name = &name.as_bytes()[..name.len().min(255)];
        out.push(name.len() as u8);
        out.extend_from_slice(name);
        out.extend_from_slice(&(p.len() as u16).to_le_bytes());
        out.extend_from_slice(&p);
    }
    out
}

/// Builds the table from `(name, svg text)` pairs (sorted by name; the first failing icon is
/// reported).
pub fn build(svgs: &[(String, String)]) -> Result<Vec<u8>, (String, Error)> {
    let mut v: Vec<&(String, String)> = svgs.iter().collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    let mut icons = Vec::with_capacity(v.len());
    for (name, src) in v {
        icons.push((name.clone(), outlines(src).map_err(|e| (name.clone(), e))?));
    }
    Ok(encode(&icons))
}

/// The `(name, svg text)` pairs of every `*.svg` in `dir` (the owner's icon set).
pub fn read_dir(dir: &std::path::Path) -> std::io::Result<Vec<(String, String)>> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "svg")
            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
        {
            out.push((stem.to_string(), std::fs::read_to_string(&p)?));
        }
    }
    Ok(out)
}

/// A read-only view of a `YIC1` table (embedded bytes).
#[derive(Debug, Clone, Copy)]
pub struct Table<'a> {
    data: &'a [u8],
}

impl<'a> Table<'a> {
    /// `None` when `data` is not a `YIC1` table.
    pub fn new(data: &'a [u8]) -> Option<Self> {
        (data.len() >= 6 && data.starts_with(MAGIC)).then_some(Self { data })
    }

    /// `(name, payload)` of every icon, in table order.
    fn entries(&self) -> impl Iterator<Item = (&'a str, &'a [u8])> + 'a {
        let d = self.data;
        let mut at = 6usize;
        std::iter::from_fn(move || {
            let n = usize::from(*d.get(at)?);
            let name = std::str::from_utf8(d.get(at + 1..at + 1 + n)?).ok()?;
            let lo = at + 1 + n;
            let len = usize::from(u16::from_le_bytes([*d.get(lo)?, *d.get(lo + 1)?]));
            let payload = d.get(lo + 2..lo + 2 + len)?;
            at = lo + 2 + len;
            Some((name, payload))
        })
    }

    /// Icon names in table order.
    pub fn names(&self) -> Vec<&'a str> {
        self.entries().map(|(n, _)| n).collect()
    }

    /// The table has icon `name`.
    pub fn has(&self, name: &str) -> bool {
        self.entries().any(|(n, _)| n == name)
    }

    /// The shapes of icon `name`.
    pub fn get(&self, name: &str) -> Option<Vec<Outline>> {
        let (_, p) = self.entries().find(|(n, _)| *n == name)?;
        decode(p)
    }
}

fn decode(p: &[u8]) -> Option<Vec<Outline>> {
    let shapes = usize::from(*p.first()?);
    let mut at = 1usize;
    let mut out = Vec::with_capacity(shapes);
    for _ in 0..shapes {
        let flags = *p.get(at)?;
        let width = f32::from(*p.get(at + 1)?) / 16.0;
        let nv = usize::from(u16::from_le_bytes([*p.get(at + 2)?, *p.get(at + 3)?]));
        let verbs = p.get(at + 4..at + 4 + nv)?;
        at += 4 + nv;
        let mut pb = PathBuilder::new();
        let mut next = || -> Option<(f32, f32)> {
            let (x, y) = (*p.get(at)?, *p.get(at + 1)?);
            at += 2;
            Some((dequant(x), dequant(y)))
        };
        for &v in verbs {
            match v {
                0 => {
                    let (x, y) = next()?;
                    pb.move_to(x, y);
                }
                1 => {
                    let (x, y) = next()?;
                    pb.line_to(x, y);
                }
                2 => {
                    let (a, b) = (next()?, next()?);
                    pb.quad_to(a.0, a.1, b.0, b.1);
                }
                3 => {
                    let (a, b, c) = (next()?, next()?, next()?);
                    pb.cubic_to(a.0, a.1, b.0, b.1, c.0, c.1);
                }
                4 => pb.close(),
                _ => return None,
            }
        }
        if let Some(path) = pb.finish() {
            out.push(Outline {
                path,
                fill: flags & 1 != 0,
                stroke: (flags & 2 != 0).then_some(width),
                round_cap: flags & 4 != 0,
                round_join: flags & 8 != 0,
            });
        }
    }
    Some(out)
}

/// Rasterizes `shapes` into a `px × px` alpha mask (one byte per pixel, row major).
pub fn mask(shapes: &[Outline], px: u32) -> Vec<u8> {
    let px = px.clamp(1, 256);
    let Some(mut pm) = Pixmap::new(px, px) else {
        return Vec::new();
    };
    #[allow(clippy::cast_precision_loss)]
    let ts = Transform::from_scale(px as f32 / BOX, px as f32 / BOX);
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    for s in shapes {
        if s.fill {
            pm.fill_path(&s.path, &paint, FillRule::Winding, ts, None);
        }
        if let Some(w) = s.stroke {
            let st = Stroke {
                width: w,
                line_cap: if s.round_cap {
                    LineCap::Round
                } else {
                    LineCap::Butt
                },
                line_join: if s.round_join {
                    LineJoin::Round
                } else {
                    LineJoin::Miter
                },
                ..Stroke::default()
            };
            pm.stroke_path(&s.path, &paint, &st, ts, None);
        }
    }
    pm.data().iter().skip(3).step_by(4).copied().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><metadata>x</metadata><path d="M4 4h16v16H4z"/><circle cx="12" cy="12" r="3" fill="currentColor"/><rect x="2" y="2" width="6" height="6" rx="1" stroke-dasharray="2.6 2.4"/></svg>"#;

    #[test]
    fn round_trip() {
        let o = outlines(SAMPLE).expect("parses");
        assert_eq!(o.len(), 3);
        assert!(o[0].stroke.is_some_and(|w| (w - 1.5).abs() < 1e-3) && !o[0].fill);
        assert!(o[1].fill && o[0].round_cap && o[0].round_join);
        let bin = encode(&[("a".into(), o.clone()), ("b".into(), Vec::new())]);
        let t = Table::new(&bin).expect("magic");
        assert_eq!(t.names(), ["a", "b"]);
        let back = t.get("a").expect("icon a");
        assert_eq!(back.len(), o.len());
        for (x, y) in o.iter().zip(&back) {
            assert_eq!(
                (x.fill, x.round_cap, x.round_join),
                (y.fill, y.round_cap, y.round_join)
            );
            assert_eq!(x.stroke.is_some(), y.stroke.is_some());
            let (bx, by) = (x.path.bounds(), y.path.bounds());
            for (u, v) in [
                (bx.left(), by.left()),
                (bx.top(), by.top()),
                (bx.right(), by.right()),
                (bx.bottom(), by.bottom()),
            ] {
                assert!((u - v).abs() <= 0.5 / Q, "{u} vs {v}");
            }
        }
        assert!(t.get("missing").is_none());
        assert!(Table::new(b"nope").is_none());
        let m = mask(&back, 20);
        assert_eq!(m.len(), 400);
        assert!(m.iter().any(|a| *a > 200), "something is drawn");
        assert_eq!(m[0], 0, "the corner outside the dashed rect stays clear");
    }

    #[test]
    fn truncated_tables_do_not_panic() {
        let o = outlines(SAMPLE).expect("parses");
        let bin = encode(&[("a".into(), o)]);
        for n in 0..bin.len() {
            if let Some(t) = Table::new(&bin[..n]) {
                let _ = t.get("a");
                let _ = t.names();
            }
        }
    }
}

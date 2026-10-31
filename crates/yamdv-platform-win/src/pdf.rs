// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native PDF writer (ADR-0023, R-EXP-01, S-EXP-03, FUT-39).
//!
//! The document is laid out and paginated exactly like printing ([`crate::print`]); each page is
//! drawn by `draw_doc` into a recording [`crate::render::Band`] and the recorded commands are
//! written as PDF content: vector paths, glyph runs in subset Type0 fonts with a ToUnicode CMap
//! (selectable text), images (JPEG passed through), an outline from the headings, GoTo links
//! for internal targets and URI links for `http`, `https` and `mailto` only.
#![allow(unsafe_code)]
// PDF geometry: float<->int conversions are bounded by page and font sizes (ADR-0007).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap
)]

use std::cell::RefCell;
use std::collections::HashMap;

use pdf_writer::types::{
    ActionType, AnnotationType, CidFontType, FontFlags, LineCapStyle, LineJoinStyle, SystemInfo,
    TextRenderingMode,
};
use pdf_writer::{Content, Date, Filter, Finish, Name, Pdf, Rect as PRect, Ref, Str, TextStr};
use tiny_skia::{Path, PathBuilder, PathSegment};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_BEZIER_SEGMENT, D2D1_FIGURE_BEGIN, D2D1_FIGURE_END, D2D1_FIGURE_END_CLOSED,
    D2D1_FILL_MODE, D2D1_PATH_SEGMENT, ID2D1SimplifiedGeometrySink,
    ID2D1SimplifiedGeometrySink_Impl,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FONT_METRICS, DWRITE_FONT_SIMULATIONS_BOLD, DWRITE_FONT_SIMULATIONS_OBLIQUE,
    DWRITE_GLYPH_METRICS, DWRITE_GLYPH_OFFSET, IDWriteFontFace, IDWriteFontFace5,
};
use windows::core::{BOOL, implement};
use windows_numerics::Vector2;
use yamdv_core::doc::Document;
use yamdv_core::progress::Phase;
use yamdv_export::paginate::{Page, footer};
use yamdv_layout::block::HotKind;
use yamdv_layout::viewer::{Content as Doc, Env, FootTarget, Viewer};

use crate::dwrite::{DWrite, DwShaper};
use crate::export::Job;
use crate::print::{Error, Printed, Target};
use crate::render::{Cmd, GlyphPaint, GlyphRec, Renderer};

/// Device scale the images are decoded for (192 dpi).
const IMG_SCALE: f32 = 2.0;
/// Points per DIP.
const PT: f64 = 0.75;

// ---------------------------------------------------------------------------------------------
// Glyph outlines (also used by the interactive HTML export for math and emoji).

#[implement(ID2D1SimplifiedGeometrySink)]
struct Sink {
    pb: RefCell<PathBuilder>,
    at: (f32, f32),
}

impl ID2D1SimplifiedGeometrySink_Impl for Sink_Impl {
    fn SetFillMode(&self, _m: D2D1_FILL_MODE) {}
    fn SetSegmentFlags(&self, _f: D2D1_PATH_SEGMENT) {}
    fn BeginFigure(&self, p: &Vector2, _b: D2D1_FIGURE_BEGIN) {
        self.pb
            .borrow_mut()
            .move_to(p.X + self.at.0, p.Y + self.at.1);
    }
    fn AddLines(&self, pts: *const Vector2, n: u32) {
        if pts.is_null() {
            return;
        }
        // SAFETY: Direct2D passes `n` valid points.
        let pts = unsafe { std::slice::from_raw_parts(pts, n as usize) };
        let mut pb = self.pb.borrow_mut();
        for p in pts {
            pb.line_to(p.X + self.at.0, p.Y + self.at.1);
        }
    }
    fn AddBeziers(&self, b: *const D2D1_BEZIER_SEGMENT, n: u32) {
        if b.is_null() {
            return;
        }
        // SAFETY: Direct2D passes `n` valid segments.
        let b = unsafe { std::slice::from_raw_parts(b, n as usize) };
        let (dx, dy) = self.at;
        let mut pb = self.pb.borrow_mut();
        for s in b {
            pb.cubic_to(
                s.point1.X + dx,
                s.point1.Y + dy,
                s.point2.X + dx,
                s.point2.Y + dy,
                s.point3.X + dx,
                s.point3.Y + dy,
            );
        }
    }
    fn EndFigure(&self, e: D2D1_FIGURE_END) {
        if e == D2D1_FIGURE_END_CLOSED {
            self.pb.borrow_mut().close();
        }
    }
    fn Close(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Outline of a glyph run with its baseline origin at `(x, y)` (DIPs, y down).
pub(crate) fn glyph_outline(g: &GlyphRec, x: f32, y: f32) -> Option<Path> {
    let sink = windows::core::ComObject::new(Sink {
        pb: RefCell::new(PathBuilder::new()),
        at: (x, y),
    });
    let offs: Vec<DWRITE_GLYPH_OFFSET> = g
        .offs
        .iter()
        .map(|o| DWRITE_GLYPH_OFFSET {
            advanceOffset: o.0,
            ascenderOffset: o.1,
        })
        .collect();
    let s: ID2D1SimplifiedGeometrySink = sink.to_interface();
    // SAFETY: the arrays hold `ids.len()` entries and outlive the synchronous call.
    unsafe {
        g.face
            .GetGlyphRunOutline(
                g.size,
                g.ids.as_ptr(),
                Some(g.adv.as_ptr()),
                Some(offs.as_ptr()),
                g.ids.len() as u32,
                false,
                g.rtl,
                &s,
            )
            .ok()?;
    }
    drop(s);
    sink.pb.take().finish()
}

// ---------------------------------------------------------------------------------------------
// Fonts.

/// One embedded font (a DirectWrite face).
struct Font {
    face: IDWriteFontFace,
    /// `Some` when the face is embedded as text; `None` = drawn as outlines.
    embed: bool,
    remap: subsetter::GlyphRemapper,
    /// New glyph id → text (ToUnicode).
    uni: HashMap<u16, String>,
    /// Old glyph id → advance in 1/1000 em.
    widths: HashMap<u16, f32>,
    upem: f32,
    cff: bool,
    bold: bool,
    oblique: bool,
    id: Ref,
    name: Vec<u8>,
}

/// The whole font file behind `face` (DirectWrite file loader stream) and its TTC index.
fn font_bytes(face: &IDWriteFontFace) -> Option<(Vec<u8>, u32)> {
    // SAFETY: plain COM calls; the fragment is copied before it is released.
    unsafe {
        let mut n = 0u32;
        face.GetFiles(&mut n, None).ok()?;
        if n == 0 {
            return None;
        }
        let mut files = vec![None; n as usize];
        face.GetFiles(&mut n, Some(files.as_mut_ptr())).ok()?;
        let file = files.into_iter().next()??;
        let mut key = std::ptr::null_mut();
        let mut key_len = 0u32;
        file.GetReferenceKey(&mut key, &mut key_len).ok()?;
        let loader = file.GetLoader().ok()?;
        let stream = loader.CreateStreamFromKey(key, key_len).ok()?;
        let size = stream.GetFileSize().ok()?;
        if size > 64 * 1024 * 1024 {
            return None;
        }
        let mut frag = std::ptr::null_mut();
        let mut ctx = std::ptr::null_mut();
        stream.ReadFileFragment(&mut frag, 0, size, &mut ctx).ok()?;
        let out = std::slice::from_raw_parts(frag.cast::<u8>(), size as usize).to_vec();
        stream.ReleaseFileFragment(ctx);
        Some((out, face.GetIndex()))
    }
}

/// OS/2 `fsType` allows embedding (installable, editable or preview & print; not bitmap-only).
fn embeddable(face: &IDWriteFontFace) -> bool {
    // SAFETY: TryGetFontTable hands out a table pointer valid until ReleaseFontTable.
    unsafe {
        let tag = u32::from_le_bytes(*b"OS/2");
        let mut data = std::ptr::null_mut();
        let mut size = 0u32;
        let mut ctx = std::ptr::null_mut();
        let mut exists = BOOL(0);
        if face
            .TryGetFontTable(tag, &mut data, &mut size, &mut ctx, &mut exists)
            .is_err()
        {
            return false;
        }
        if !exists.as_bool() {
            return true;
        }
        let t = std::slice::from_raw_parts(data.cast::<u8>(), size as usize);
        let fs = match (t.get(8), t.get(9)) {
            (Some(a), Some(b)) => u16::from_be_bytes([*a, *b]),
            _ => 0,
        };
        face.ReleaseFontTable(ctx);
        yamdv_export::pdfparts::fs_type_embeddable(fs)
    }
}

/// A variable face away from its default instance (the subset would show the default).
fn off_default_instance(face: &IDWriteFontFace) -> bool {
    use windows::core::Interface;
    let Ok(f5) = face.cast::<IDWriteFontFace5>() else {
        return false;
    };
    // SAFETY: plain COM calls with buffers of the reported axis count.
    unsafe {
        if !f5.HasVariations().as_bool() {
            return false;
        }
        let n = f5.GetFontAxisValueCount() as usize;
        let Ok(res) = f5.GetFontResource() else {
            return true;
        };
        let mut cur = vec![Default::default(); n];
        let mut def = vec![Default::default(); n];
        if f5.GetFontAxisValues(&mut cur).is_err()
            || res.GetDefaultFontAxisValues(&mut def).is_err()
        {
            return true;
        }
        cur.iter().zip(&def).any(
            |(a, b): (
                &windows::Win32::Graphics::DirectWrite::DWRITE_FONT_AXIS_VALUE,
                _,
            )| { a.axisTag != b.axisTag || (a.value - b.value).abs() > 0.01 },
        )
    }
}

fn ps_name(face: &IDWriteFontFace, k: usize) -> Vec<u8> {
    use windows::Win32::Graphics::DirectWrite::{
        DWRITE_INFORMATIONAL_STRING_POSTSCRIPT_NAME, IDWriteFontFace3,
    };
    use windows::core::Interface;
    let mut name = String::new();
    if let Ok(f3) = face.cast::<IDWriteFontFace3>() {
        let mut strings = None;
        let mut exists = BOOL(0);
        // SAFETY: plain COM calls; the buffer is sized from GetStringLength.
        unsafe {
            if f3
                .GetInformationalStrings(
                    DWRITE_INFORMATIONAL_STRING_POSTSCRIPT_NAME,
                    &mut strings,
                    &mut exists,
                )
                .is_ok()
                && exists.as_bool()
                && let Some(s) = strings
                && let Ok(len) = s.GetStringLength(0)
            {
                let mut buf = vec![0u16; len as usize + 1];
                if s.GetString(0, &mut buf).is_ok() {
                    buf.truncate(len as usize);
                    name = String::from_utf16_lossy(&buf);
                }
            }
        }
    }
    yamdv_export::pdfparts::subset_name(k, &name)
}

#[derive(Default)]
struct Fonts {
    list: Vec<Font>,
    by_ptr: HashMap<usize, usize>,
}

impl Fonts {
    fn get(&mut self, face: &IDWriteFontFace, alloc: &mut impl FnMut() -> Ref) -> usize {
        use windows::core::Interface;
        let key = face.as_raw() as usize;
        if let Some(&k) = self.by_ptr.get(&key) {
            return k;
        }
        let k = self.list.len();
        let mut embed = embeddable(face) && !off_default_instance(face);
        let mut cff = false;
        if embed {
            // Probe once: the font must parse and subset (TrueType or CFF, not CFF2).
            match font_bytes(face) {
                Some((data, ix)) => {
                    embed = subsetter::subset(&data, ix, &subsetter::GlyphRemapper::new()).is_ok();
                    cff = !yamdv_export::pdfparts::has_table(&data, ix, *b"glyf");
                }
                None => embed = false,
            }
        }
        let mut m = DWRITE_FONT_METRICS::default();
        // SAFETY: plain COM calls.
        let sims = unsafe {
            face.GetMetrics(&mut m);
            face.GetSimulations()
        };
        self.list.push(Font {
            face: face.clone(),
            embed,
            remap: subsetter::GlyphRemapper::new(),
            uni: HashMap::new(),
            widths: HashMap::new(),
            upem: f32::from(m.designUnitsPerEm.max(1)),
            cff,
            bold: sims.0 & DWRITE_FONT_SIMULATIONS_BOLD.0 != 0,
            oblique: sims.0 & DWRITE_FONT_SIMULATIONS_OBLIQUE.0 != 0,
            id: alloc(),
            name: ps_name(face, k),
        });
        self.by_ptr.insert(key, k);
        k
    }
}

/// Advance widths (1/1000 em) of `ids` in `f`.
fn ensure_widths(f: &mut Font, ids: &[u16]) {
    let missing: Vec<u16> = ids
        .iter()
        .copied()
        .filter(|g| !f.widths.contains_key(g))
        .collect();
    if missing.is_empty() {
        return;
    }
    let mut gm = vec![DWRITE_GLYPH_METRICS::default(); missing.len()];
    // SAFETY: arrays of equal length.
    if unsafe {
        f.face.GetDesignGlyphMetrics(
            missing.as_ptr(),
            missing.len() as u32,
            gm.as_mut_ptr(),
            false,
        )
    }
    .is_ok()
    {
        for (g, m) in missing.iter().zip(&gm) {
            f.widths.insert(*g, m.advanceWidth as f32 * 1000.0 / f.upem);
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Content.

fn rgb(c: u32) -> (f32, f32, f32) {
    (
        ((c >> 16) & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        (c & 0xFF) as f32 / 255.0,
    )
}

fn path_ops(c: &mut Content, p: &Path) {
    let mut last = (0.0f32, 0.0f32);
    for s in p.segments() {
        match s {
            PathSegment::MoveTo(a) => {
                c.move_to(a.x, a.y);
                last = (a.x, a.y);
            }
            PathSegment::LineTo(a) => {
                c.line_to(a.x, a.y);
                last = (a.x, a.y);
            }
            PathSegment::QuadTo(q, a) => {
                let c1 = (
                    last.0 + 2.0 / 3.0 * (q.x - last.0),
                    last.1 + 2.0 / 3.0 * (q.y - last.1),
                );
                let c2 = (a.x + 2.0 / 3.0 * (q.x - a.x), a.y + 2.0 / 3.0 * (q.y - a.y));
                c.cubic_to(c1.0, c1.1, c2.0, c2.1, a.x, a.y);
                last = (a.x, a.y);
            }
            PathSegment::CubicTo(a, b, e) => {
                c.cubic_to(a.x, a.y, b.x, b.y, e.x, e.y);
                last = (e.x, e.y);
            }
            PathSegment::Close => {
                c.close_path();
            }
        }
    }
}

/// Per-page resources collected while writing the content stream.
#[derive(Default)]
struct Res {
    fonts: Vec<(Vec<u8>, Ref)>,
    xobjects: Vec<(Vec<u8>, Ref)>,
    gstates: Vec<(Vec<u8>, Ref)>,
}

impl Res {
    fn add(list: &mut Vec<(Vec<u8>, Ref)>, name: Vec<u8>, id: Ref) {
        if !list.iter().any(|(n, _)| *n == name) {
            list.push((name, id));
        }
    }
}

/// Everything shared across pages while writing.
struct Writer {
    pdf: Pdf,
    next: i32,
    fonts: Fonts,
    /// Image cache: (w, h, hash) → object.
    images: HashMap<(u32, u32, u64), Ref>,
    /// Alpha (percent) → ExtGState.
    alphas: HashMap<u32, Ref>,
    helv: Option<Ref>,
}

impl Writer {
    fn alloc(&mut self) -> Ref {
        self.next += 1;
        Ref::new(self.next)
    }

    fn alpha(&mut self, res: &mut Res, a: f32) -> Vec<u8> {
        let pc = (a.clamp(0.0, 1.0) * 100.0).round() as u32;
        let id = match self.alphas.get(&pc) {
            Some(&id) => id,
            None => {
                let id = self.alloc();
                self.pdf
                    .ext_graphics(id)
                    .non_stroking_alpha(pc as f32 / 100.0)
                    .stroking_alpha(pc as f32 / 100.0);
                self.alphas.insert(pc, id);
                id
            }
        };
        let name = format!("A{pc}").into_bytes();
        Res::add(&mut res.gstates, name.clone(), id);
        name
    }

    fn hash(b: &[u8]) -> u64 {
        use std::hash::Hasher;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        h.write(b);
        Hasher::finish(&h)
    }

    fn image(&mut self, res: &mut Res, w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let key = (w, h, Self::hash(px));
        let id = if let Some(&id) = self.images.get(&key) {
            id
        } else {
            let n = (w as usize) * (h as usize);
            let mut rgb = Vec::with_capacity(n * 3);
            let mut alpha = Vec::with_capacity(n);
            let mut opaque = true;
            for p in px.as_chunks::<4>().0.iter().take(n) {
                let [b0, g0, r0, a] = *p;
                let un = |c: u8| -> u8 {
                    if a == 0 || a == 255 {
                        c
                    } else {
                        ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
                    }
                };
                rgb.extend_from_slice(&[un(r0), un(g0), un(b0)]);
                alpha.push(a);
                opaque &= a == 255;
            }
            let id = self.alloc();
            let mask = (!opaque).then(|| self.alloc());
            let data = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
            let mut x = self.pdf.image_xobject(id, &data);
            x.filter(Filter::FlateDecode);
            x.width(w as i32).height(h as i32).bits_per_component(8);
            x.color_space().device_rgb();
            if let Some(m) = mask {
                x.s_mask(m);
            }
            x.finish();
            if let Some(m) = mask {
                let data = miniz_oxide::deflate::compress_to_vec_zlib(&alpha, 6);
                let mut x = self.pdf.image_xobject(m, &data);
                x.filter(Filter::FlateDecode);
                x.width(w as i32).height(h as i32).bits_per_component(8);
                x.color_space().device_gray();
                x.finish();
            }
            self.images.insert(key, id);
            id
        };
        let name = format!("I{}", id.get()).into_bytes();
        Res::add(&mut res.xobjects, name.clone(), id);
        name
    }

    fn jpeg(&mut self, res: &mut Res, w: u32, h: u32, comps: u8, bytes: &[u8]) -> Vec<u8> {
        let key = (w, h, Self::hash(bytes));
        let id = if let Some(&id) = self.images.get(&key) {
            id
        } else {
            let id = self.alloc();
            let mut x = self.pdf.image_xobject(id, bytes);
            x.filter(Filter::DctDecode);
            x.width(w as i32).height(h as i32).bits_per_component(8);
            if comps == 1 {
                x.color_space().device_gray();
            } else {
                x.color_space().device_rgb();
            }
            x.finish();
            self.images.insert(key, id);
            id
        };
        let name = format!("I{}", id.get()).into_bytes();
        Res::add(&mut res.xobjects, name.clone(), id);
        name
    }

    /// Writes one glyph run.
    fn glyphs(&mut self, c: &mut Content, res: &mut Res, g: &GlyphRec) {
        let mut next = self.next;
        let k = {
            let mut alloc = || {
                next += 1;
                Ref::new(next)
            };
            self.fonts.get(&g.face, &mut alloc)
        };
        self.next = next;
        let (r, gr, b) = rgb(g.color);
        let Some(f) = self.fonts.list.get_mut(k) else {
            return;
        };
        if g.paint == GlyphPaint::Outline || (!f.embed && g.paint == GlyphPaint::Text) {
            if let Some(p) = glyph_outline(g, g.x, g.y) {
                c.set_fill_rgb(r, gr, b);
                path_ops(c, &p);
                c.fill_nonzero();
            }
            return;
        }
        if !f.embed {
            return;
        }
        ensure_widths(f, &g.ids);
        // ToUnicode from the cluster map (pdfparts::cluster_pairs).
        let n = g.ids.len();
        for (old, t) in yamdv_export::pdfparts::cluster_pairs(&g.ids, &g.clusters, &g.text) {
            let new = f.remap.remap(old);
            f.uni.entry(new).or_insert(t);
        }
        // Absolute glyph positions (DIPs).
        let mut pos = Vec::with_capacity(n);
        let mut pen = 0.0f32;
        for i in 0..n {
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
            pos.push((x, g.y - asc));
        }
        let name = format!("F{k}").into_bytes();
        Res::add(&mut res.fonts, name.clone(), f.id);
        let skew = if f.oblique { 0.25 } else { 0.0 };
        c.begin_text();
        c.set_font(Name(&name), g.size);
        c.set_fill_rgb(r, gr, b);
        if g.paint == GlyphPaint::Invisible {
            c.set_text_rendering_mode(TextRenderingMode::Invisible);
        } else if f.bold {
            c.set_stroke_rgb(r, gr, b);
            c.set_line_width(g.size * 0.03);
            c.set_text_rendering_mode(TextRenderingMode::FillStroke);
        }
        // Runs of glyphs on one baseline share a text matrix; offsets become TJ adjustments.
        let mut glyphs = g.ids.iter().zip(&pos).peekable();
        while let Some((&first, &(x0, y0))) = glyphs.next() {
            c.set_text_matrix([1.0, 0.0, skew, -1.0, x0, y0]);
            let mut pen = x0;
            let mut items: Vec<(f32, [u8; 2])> = Vec::new();
            let mut cur = Some((first, x0));
            while let Some((old, gx)) = cur {
                let new = f.remap.remap(old);
                let adj = -(gx - pen) * 1000.0 / g.size.max(0.01);
                items.push((adj, new.to_be_bytes()));
                pen = gx + f.widths.get(&old).copied().unwrap_or(0.0) * g.size / 1000.0;
                cur = glyphs
                    .next_if(|(_, p)| (p.1 - y0).abs() < 0.01)
                    .map(|(&o, p)| (o, p.0));
            }
            {
                let mut sp = c.show_positioned();
                let mut it = sp.items();
                for (adj, bytes) in &items {
                    if adj.abs() > 0.05 {
                        it.adjust(*adj);
                    }
                    it.show(Str(bytes));
                }
            }
        }
        if g.paint == GlyphPaint::Invisible || f.bold {
            c.set_text_rendering_mode(TextRenderingMode::Fill);
        }
        c.end_text();
    }

    /// Writes the fonts used so far (end of the document).
    fn finish_fonts(&mut self) {
        let fonts = std::mem::take(&mut self.fonts.list);
        for f in fonts {
            if !f.embed {
                continue;
            }
            let Some((data, ix)) = font_bytes(&f.face) else {
                continue;
            };
            let Ok(sub) = subsetter::subset(&data, ix, &f.remap) else {
                continue;
            };
            drop(data);
            let (cid, desc, file, cmap) = (self.alloc(), self.alloc(), self.alloc(), self.alloc());
            let info = SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            };
            self.pdf
                .type0_font(f.id)
                .base_font(Name(&f.name))
                .encoding_predefined(Name(b"Identity-H"))
                .descendant_font(cid)
                .to_unicode(cmap);
            let mut cf = self.pdf.cid_font(cid);
            cf.subtype(if f.cff {
                CidFontType::Type0
            } else {
                CidFontType::Type2
            })
            .base_font(Name(&f.name))
            .system_info(info)
            .font_descriptor(desc)
            .default_width(0.0);
            if !f.cff {
                cf.cid_to_gid_map_predefined(Name(b"Identity"));
            }
            {
                let mut w = cf.widths();
                let ws = yamdv_export::pdfparts::cid_widths(&f.remap, &f.widths);
                if !ws.is_empty() {
                    w.consecutive(0, ws);
                }
            }
            cf.finish();
            let mut m = DWRITE_FONT_METRICS::default();
            // SAFETY: plain COM call.
            unsafe { f.face.GetMetrics(&mut m) };
            let s = 1000.0 / f.upem;
            let mut fd = self.pdf.font_descriptor(desc);
            fd.name(Name(&f.name))
                .flags(FontFlags::SYMBOLIC)
                .bbox(PRect::new(
                    0.0,
                    -f32::from(m.descent) * s,
                    1000.0,
                    f32::from(m.ascent) * s,
                ))
                .italic_angle(if f.oblique { -12.0 } else { 0.0 })
                .ascent(f32::from(m.ascent) * s)
                .descent(-f32::from(m.descent) * s)
                .cap_height(f32::from(m.capHeight) * s)
                .stem_v(80.0);
            if f.cff {
                fd.font_file3(file);
            } else {
                fd.font_file2(file);
            }
            fd.finish();
            let z = miniz_oxide::deflate::compress_to_vec_zlib(&sub, 6);
            let mut st = self.pdf.stream(file, &z);
            st.filter(Filter::FlateDecode);
            if f.cff {
                st.pair(Name(b"Subtype"), Name(b"OpenType"));
            }
            st.finish();
            let data = yamdv_export::pdfparts::to_unicode_cmap(&f.uni);
            self.pdf.cmap(cmap, &data);
        }
    }
}

/// A link annotation in page DIPs.
enum Target2 {
    Uri(String),
    /// Document y (DIPs).
    Doc(f64),
}

/// `#slug` / `{#id}` / HTML anchor / heading text → block (R-LINK-01).
pub(crate) fn anchor_block(doc: &Document, a: &str) -> Option<usize> {
    let ix = &doc.index;
    let lower = a.to_lowercase();
    ix.headings
        .iter()
        .find(|h| h.explicit_id.as_deref() == Some(a) || h.slug == a)
        .or_else(|| {
            ix.headings
                .iter()
                .find(|h| h.slug == lower || h.text.to_lowercase() == lower)
        })
        .map(|h| h.block as usize)
        .or_else(|| ix.html_anchors.get(a).map(|&b| b as usize))
}

/// B2: the block of the heading whose text or slug is `name` (case-insensitive).
fn heading_block(doc: &Document, name: &str) -> Option<usize> {
    let lower = name.to_lowercase();
    doc.index
        .headings
        .iter()
        .find(|h| h.slug == lower || h.text.to_lowercase() == lower)
        .map(|h| h.block as usize)
}

fn now_date() -> Date {
    // SAFETY: plain call.
    let t = unsafe { windows::Win32::System::SystemInformation::GetSystemTime() };
    Date::new(t.wYear)
        .month(t.wMonth as u8)
        .day(t.wDay as u8)
        .hour(t.wHour as u8)
        .minute(t.wMinute as u8)
        .second(t.wSecond as u8)
        .utc_offset_hour(0)
        .utc_offset_minute(0)
}

/// Exports `job` as a native PDF to `t.output` (atomic write; Cancel leaves no file).
#[allow(clippy::too_many_lines)]
pub fn export(
    dw: &DWrite,
    job: &Job,
    t: &Target,
    mut link: Option<&mut crate::job::Link>,
) -> Result<Printed, Error> {
    let out = t.output.clone().ok_or(Error::Failed)?;
    let s = crate::window::load_settings();
    let paper = t.paper_mm.unwrap_or((210.0, 297.0));
    let margin_mm = s.int("export.pdf_margins_mm").clamp(5, 40) as f64;
    let (pal, _dark) =
        crate::window::export_palette(&job.scheme, s.str("export.pdf_colors"), job.dark);
    let mm_dip = 96.0 / 25.4;
    let width_dips = ((paper.0 - 2.0 * margin_mm) * mm_dip).max(100.0) as f32;
    let page_h = ((paper.1 - 2.0 * margin_mm) * mm_dip).max(100.0);
    let (pw, ph) = (paper.0 * 72.0 / 25.4, paper.1 * 72.0 / 25.4);
    let mpt = margin_mm * 72.0 / 25.4;
    let mut content = Doc::Markdown(Document::new(
        job.source.clone(),
        crate::export::toc_opts(&s),
    ));
    let title = match &content {
        Doc::Markdown(d) => d.index.headings.first().map(|h| h.text.clone()),
        Doc::Plain { .. } => None,
    }
    .filter(|t| !t.trim().is_empty())
    .unwrap_or_else(|| job.stem());
    let mut viewer = Viewer::new(&content, width_dips, job.m);
    // Code blocks always wrap on paper.
    viewer.code_default.wrap = true;
    viewer.code_default.line_numbers = s.bool("reading.code_line_numbers");
    let images = crate::images::Store::synchronous(&job.path);
    let cache = yamdv_layout::rich::RichCache::default();
    let rich = crate::dwrite::Rich::new(dw, &cache, true, true);
    let strings = crate::window::strings(job.lang);
    let mut shaper = DwShaper(dw);
    let single = s.bool("reading.single_newline_break");
    let mut prepare = |content: &mut Doc, viewer: &mut Viewer, y: f64, vh: f64| {
        viewer.scroll_to(y, vh);
        let mut env = Env {
            images: &images,
            rich: &rich,
            shaper: &mut shaper,
            m: job.m,
            pal: &pal,
            strings: &strings,
            single_newline_break: single,
        };
        viewer.prepare(content, &mut env, vh);
    };
    let pages: Vec<Page> =
        crate::print::pass1(&mut content, &mut viewer, page_h, &mut prepare, &mut link)
            .ok_or(Error::Cancelled)?;
    let total = pages.len().max(1);
    let r = Renderer::new(dw, None, 1, IMG_SCALE).map_err(|_| Error::Failed)?;
    let mut w = Writer {
        pdf: Pdf::new(),
        next: 0,
        fonts: Fonts::default(),
        images: HashMap::new(),
        alphas: HashMap::new(),
        helv: None,
    };
    let catalog = w.alloc();
    let tree = w.alloc();
    let page_ids: Vec<Ref> = (0..pages.len()).map(|_| w.alloc()).collect();
    let numbers = s.bool("export.pdf_page_numbers");
    // Document y → (page, PDF y).
    let locate = |y: f64| -> (usize, f32) {
        let k = pages.iter().rposition(|p| p.y0 <= y + 0.5).unwrap_or(0);
        let p = pages.get(k).map_or((0.0, 1.0), |p| (p.y0, p.scale));
        (k, (ph - mpt - (y - p.0).max(0.0) * PT * p.1) as f32)
    };
    for (k, p) in pages.iter().enumerate() {
        if link
            .as_deref_mut()
            .is_some_and(|l| !l.report(Phase::Printing, k as u32, total as u32))
        {
            return Err(Error::Cancelled);
        }
        let vh = p.y1 - p.y0;
        prepare(&mut content, &mut viewer, p.y0, vh);
        let cmds = r.record(pal.text, &mut |band| {
            band.clear(pal.bg);
            crate::window::draw_doc(
                band,
                &content,
                &viewer,
                &pal,
                0.0,
                None,
                vh,
                Some(&images),
                (&[], None),
            );
        });
        // Links on this page (hot rectangles of the visible blocks).
        let mut raw: Vec<(yamdv_layout::Rect, usize, HotKind)> = Vec::new();
        for (bi, y, bl) in viewer.visible(&content, vh) {
            for h in &bl.hot {
                if matches!(
                    h.kind,
                    HotKind::Link { .. } | HotKind::FootRef { .. } | HotKind::FootBack { .. }
                ) {
                    let mut rc = h.rect;
                    rc.y += y as f32;
                    raw.push((rc, bi, h.kind.clone()));
                }
            }
        }
        let mut annots: Vec<(yamdv_layout::Rect, Target2)> = Vec::new();
        for (rc, bi, kind) in raw {
            let tgt = match kind {
                HotKind::Link { link: li } => {
                    let Doc::Markdown(doc) = &mut content else {
                        continue;
                    };
                    let det = doc.detail(bi);
                    let Some(l) = det.links.get(li as usize) else {
                        continue;
                    };
                    let d = l.dest.to_string();
                    // B2 (prompt 022): like the HTML export, a wikilink is a link only as
                    // `[[#Heading]]` or when it names a heading of this document; any other
                    // wikilink stays plain text without an annotation.
                    let (wiki, t) = (l.wiki, d.trim());
                    let anchor = match t.strip_prefix('#') {
                        Some(a) => Some(anchor_block(doc, a)),
                        None if wiki => Some(heading_block(doc, t)),
                        None => None,
                    };
                    if let Some(found) = anchor {
                        match found {
                            Some(b) => Target2::Doc(viewer.block_top(&content, b)),
                            None => continue,
                        }
                    } else if yamdv_export::pdfparts::safe_uri(&d) {
                        Target2::Uri(d.trim().to_string())
                    } else {
                        continue;
                    }
                }
                HotKind::FootRef { n, .. } => foot_y(&viewer, &content, FootTarget::Note(n)),
                HotKind::FootBack { n, k } => foot_y(&viewer, &content, FootTarget::Ref(n, k)),
                _ => continue,
            };
            annots.push((rc, tgt));
        }
        // Content stream: DIPs, y down, inside the margins.
        let mut c = Content::new();
        let mut res = Res::default();
        let kx = (PT * p.scale) as f32;
        let x0 = mpt + (pw - 2.0 * mpt) * (1.0 - p.scale) / 2.0;
        if let Some(Cmd::Clear(bg)) = cmds.first() {
            let (r, g, b) = rgb(*bg);
            c.set_fill_rgb(r, g, b);
            c.rect(0.0, 0.0, pw as f32, ph as f32);
            c.fill_nonzero();
        }
        c.save_state();
        c.transform([kx, 0.0, 0.0, -kx, x0 as f32, (ph - mpt) as f32]);
        // Content ends at the page's break: clip what lies below it (and above it).
        c.rect(-10_000.0, 0.0, 20_000.0, vh as f32);
        c.clip_nonzero();
        c.end_path();
        let mut depth = 0;
        for cmd in &cmds {
            match cmd {
                Cmd::Clear(_) => {}
                Cmd::Fill { path, color, alpha } => {
                    let (r, g, b) = rgb(*color);
                    let ga = (*alpha < 0.999).then(|| w.alpha(&mut res, *alpha));
                    if let Some(n) = &ga {
                        c.save_state();
                        c.set_parameters(Name(n));
                    }
                    c.set_fill_rgb(r, g, b);
                    path_ops(&mut c, path);
                    c.fill_nonzero();
                    if ga.is_some() {
                        c.restore_state();
                    }
                }
                Cmd::Stroke {
                    path,
                    color,
                    width,
                    round,
                    dash,
                } => {
                    let (r, g, b) = rgb(*color);
                    c.save_state();
                    c.set_stroke_rgb(r, g, b);
                    c.set_line_width(*width);
                    if *round {
                        c.set_line_cap(LineCapStyle::RoundCap);
                        c.set_line_join(LineJoinStyle::RoundJoin);
                    }
                    if let Some((on, off)) = dash {
                        c.set_dash_pattern([*on, *off], 0.0);
                    }
                    path_ops(&mut c, path);
                    c.stroke();
                    c.restore_state();
                }
                Cmd::PushClip(rc) => {
                    c.save_state();
                    c.rect(rc.x, rc.y, rc.w.max(0.0), rc.h.max(0.0));
                    c.clip_nonzero();
                    c.end_path();
                    depth += 1;
                }
                Cmd::PopClip => {
                    if depth > 0 {
                        c.restore_state();
                        depth -= 1;
                    }
                }
                Cmd::Image {
                    rect,
                    w: iw,
                    h: ih,
                    px,
                } => {
                    let n = w.image(&mut res, *iw, *ih, px);
                    c.save_state();
                    c.transform([rect.w, 0.0, 0.0, -rect.h, rect.x, rect.y + rect.h]);
                    c.x_object(Name(&n));
                    c.restore_state();
                }
                Cmd::Jpeg {
                    rect,
                    w: iw,
                    h: ih,
                    comps,
                    bytes,
                } => {
                    let n = w.jpeg(&mut res, *iw, *ih, *comps, bytes);
                    c.save_state();
                    c.transform([rect.w, 0.0, 0.0, -rect.h, rect.x, rect.y + rect.h]);
                    c.x_object(Name(&n));
                    c.restore_state();
                }
                Cmd::Glyphs(g) => w.glyphs(&mut c, &mut res, g),
            }
        }
        for _ in 0..depth {
            c.restore_state();
        }
        c.restore_state();
        if numbers {
            let id = match w.helv {
                Some(id) => id,
                None => {
                    let id = w.alloc();
                    w.pdf
                        .type1_font(id)
                        .base_font(Name(b"Helvetica"))
                        .encoding_predefined(Name(b"WinAnsiEncoding"));
                    w.helv = Some(id);
                    id
                }
            };
            Res::add(&mut res.fonts, b"H".to_vec(), id);
            let text = footer(k + 1, total);
            // Helvetica widths: digits 556, space and slash 278.
            let tw: f64 = text
                .chars()
                .map(|ch| if ch.is_ascii_digit() { 556.0 } else { 278.0 })
                .sum::<f64>()
                * 9.0
                / 1000.0;
            let (r, g, b) = rgb(pal.text_muted);
            c.begin_text();
            c.set_font(Name(b"H"), 9.0);
            c.set_fill_rgb(r, g, b);
            c.next_line(((pw - tw) / 2.0) as f32, (mpt / 2.0 - 3.0).max(4.0) as f32);
            c.show(Str(text.as_bytes()));
            c.end_text();
        }
        let raw = c.finish();
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        drop(raw);
        let cid = w.alloc();
        w.pdf.stream(cid, &z).filter(Filter::FlateDecode);
        // Annotations.
        let mut annot_ids = Vec::new();
        for (rc, tgt) in annots {
            let id = w.alloc();
            let [lx0, ly0, lx1, ly1] =
                yamdv_export::pdfparts::link_rect(x0, ph - mpt, p.scale, (rc.x, rc.y, rc.w, rc.h));
            let mut a = w.pdf.annotation(id);
            a.subtype(AnnotationType::Link)
                .rect(PRect::new(lx0, ly0, lx1, ly1))
                .border(0.0, 0.0, 0.0, None);
            match tgt {
                Target2::Uri(u) => {
                    a.action()
                        .action_type(ActionType::Uri)
                        .uri(Str(u.as_bytes()));
                }
                Target2::Doc(y) => {
                    let (pk, py) = locate(y);
                    if let Some(&pid) = page_ids.get(pk) {
                        a.action()
                            .action_type(ActionType::GoTo)
                            .destination()
                            .page(pid)
                            .xyz(0.0, py, None);
                    }
                }
            }
            a.finish();
            annot_ids.push(id);
        }
        let Some(&pid) = page_ids.get(k) else {
            continue;
        };
        let mut pg = w.pdf.page(pid);
        pg.media_box(PRect::new(0.0, 0.0, pw as f32, ph as f32))
            .parent(tree)
            .contents(cid);
        if !annot_ids.is_empty() {
            pg.annotations(annot_ids);
        }
        {
            let mut rs = pg.resources();
            if !res.fonts.is_empty() {
                let mut d = rs.fonts();
                for (n, id) in &res.fonts {
                    d.pair(Name(n), *id);
                }
            }
            if !res.xobjects.is_empty() {
                let mut d = rs.x_objects();
                for (n, id) in &res.xobjects {
                    d.pair(Name(n), *id);
                }
            }
            if !res.gstates.is_empty() {
                let mut d = rs.ext_g_states();
                for (n, id) in &res.gstates {
                    d.pair(Name(n), *id);
                }
            }
        }
        pg.finish();
    }
    if link.is_some_and(|l| !l.report(Phase::Printing, total as u32, total as u32)) {
        return Err(Error::Cancelled);
    }
    w.finish_fonts();
    w.pdf
        .pages(tree)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);
    // Outline nested by heading level.
    let heads: Vec<(u8, String, f64)> = match &content {
        Doc::Markdown(d) => d
            .index
            .headings
            .iter()
            .map(|h| {
                (
                    h.level,
                    h.text.clone(),
                    viewer.block_top(&content, h.block as usize),
                )
            })
            .collect(),
        Doc::Plain { .. } => Vec::new(),
    };
    let outline = (!heads.is_empty()).then(|| w.alloc());
    if let Some(root) = outline {
        let ids: Vec<Ref> = heads.iter().map(|_| w.alloc()).collect();
        // Parent of each item: nearest earlier item with a lower level (pdfparts).
        let n = heads.len();
        let levels: Vec<u8> = heads.iter().map(|h| h.0).collect();
        let yamdv_export::pdfparts::OutlineTree {
            parent,
            kids,
            siblings: sib_pos,
        } = yamdv_export::pdfparts::outline_tree(&levels);
        let id = |i: usize| ids.get(i).copied().unwrap_or(root);
        for (i, ((h, p), (prev, next))) in heads.iter().zip(&parent).zip(&sib_pos).enumerate() {
            let mut it = w.pdf.outline_item(id(i));
            it.title(TextStr(&h.1)).parent(p.map_or(root, id));
            if let Some(pv) = prev {
                it.prev(id(*pv));
            }
            if let Some(nx) = next {
                it.next(id(*nx));
            }
            let mine = kids.get(i).map_or(&[][..], Vec::as_slice);
            if let (Some(&f), Some(&l)) = (mine.first(), mine.last()) {
                it.first(id(f)).last(id(l)).count(-(mine.len() as i32));
            }
            let (pk, py) = locate(h.2);
            if let Some(&pid) = page_ids.get(pk) {
                it.dest().page(pid).xyz(0.0, py, None);
            }
            it.finish();
        }
        let top = kids.get(n).map_or(&[][..], Vec::as_slice);
        let mut o = w.pdf.outline(root);
        if let (Some(&f), Some(&l)) = (top.first(), top.last()) {
            o.first(id(f)).last(id(l));
        }
        o.count(top.len() as i32);
        o.finish();
    }
    let mut cat = w.pdf.catalog(catalog);
    cat.pages(tree);
    if let Some(o) = outline {
        cat.outlines(o)
            .page_mode(pdf_writer::types::PageMode::UseOutlines);
    }
    cat.viewer_preferences().display_doc_title(true);
    cat.finish();
    let info = w.alloc();
    w.pdf
        .document_info(info)
        .title(TextStr(&title))
        .producer(TextStr(&format!("YAMDV {}", env!("CARGO_PKG_VERSION"))))
        .creation_date(now_date());
    let n = page_ids.len();
    drop(viewer);
    drop(content);
    let bytes = w.pdf.finish();
    if crate::shell::write_atomic(&out, &bytes) {
        Ok(Printed { pages: n })
    } else {
        Err(Error::Failed)
    }
}

fn foot_y(viewer: &Viewer, content: &Doc, t: FootTarget) -> Target2 {
    let y = viewer.footnote_target(content, t).unwrap_or_else(|| {
        Viewer::footnote_block(content, t).map_or(0.0, |(b, _)| viewer.block_top(content, b))
    });
    Target2::Doc(y)
}

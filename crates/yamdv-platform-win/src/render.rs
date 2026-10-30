// SPDX-License-Identifier: AGPL-3.0-or-later
//! Backend A (ADR-0002), moved from `spikes/src/backend_a.rs` and cleaned up: DirectWrite GDI
//! interop (`IDWriteBitmapRenderTarget`) renders glyph runs into a reused 128-px band DIB,
//! tiny-skia draws shapes on the same bits, color emoji via `TranslateColorGlyphRun` (R-REN-20),
//! each band is `BitBlt`-ed to the window. Scrolling uses `ScrollWindowEx` + exposed-strip painting
//! (done by the window module).
#![allow(unsafe_code)]
// Pixel/DIP geometry: float<->int conversions are bounded by window/bitmap sizes (ADR-0007).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::indexing_slicing
)]

use std::cell::{Cell, RefCell};

use tiny_skia::{
    Color, FillRule, GradientStop, LinearGradient, Mask, Paint, PathBuilder, PixmapMut, Point,
    SpreadMode, Stroke, Transform,
};
use windows::Win32::Foundation::{COLORREF, DWRITE_E_NOCOLOR, HWND, RECT};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_GLYPH_IMAGE_FORMATS_CFF, DWRITE_GLYPH_IMAGE_FORMATS_COLR,
    DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE, DWRITE_GLYPH_OFFSET, DWRITE_GLYPH_RUN,
    DWRITE_GLYPH_RUN_DESCRIPTION, DWRITE_MATRIX, DWRITE_MEASURING_MODE,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_STRIKETHROUGH, DWRITE_UNDERLINE,
    IDWriteBitmapRenderTarget, IDWriteFactory4, IDWriteFontFace, IDWriteInlineObject,
    IDWritePixelSnapping_Impl, IDWriteRenderingParams, IDWriteTextRenderer,
    IDWriteTextRenderer_Impl,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, DIBSECTION, GdiFlush, GetCurrentObject, GetObjectW, HDC, MONITOR_DEFAULTTONEAREST,
    MonitorFromWindow, OBJ_BITMAP, SRCCOPY,
};
use windows::core::{BOOL, IUnknown, Interface, Ref, implement};
use windows_numerics::Vector2;
use yamdv_layout::Rect;
use yamdv_layout::shaper::Shaped;

use crate::dwrite::{DWrite, DwShaped};

/// `0xRRGGBB` → GDI `COLORREF` (`0x00BBGGRR`).
pub fn colorref(c: u32) -> COLORREF {
    COLORREF(((c >> 16) & 0xFF) | (c & 0xFF00) | ((c & 0xFF) << 16))
}

/// How a recorded glyph run is painted in the PDF (ADR-0023).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphPaint {
    /// Text in an embedded (subset) font: selectable.
    Text,
    /// Filled glyph outlines (COLR layers).
    Outline,
    /// Invisible text (render mode 3) over outlines, so colour emoji stay selectable.
    Invisible,
}

/// One recorded glyph run in page DIPs (ADR-0023 backend seam).
#[derive(Debug)]
pub struct GlyphRec {
    /// Font face.
    pub face: IDWriteFontFace,
    /// Em size (DIPs).
    pub size: f32,
    /// Baseline origin.
    pub x: f32,
    /// Baseline origin.
    pub y: f32,
    /// Glyph ids.
    pub ids: Vec<u16>,
    /// Advances (DIPs).
    pub adv: Vec<f32>,
    /// Offsets (advance, ascender) in DIPs.
    pub offs: Vec<(f32, f32)>,
    /// Right-to-left run.
    pub rtl: bool,
    /// `0xRRGGBB`.
    pub color: u32,
    /// UTF-16 text of the run (ToUnicode); empty for math.
    pub text: Vec<u16>,
    /// Cluster map (text unit → first glyph).
    pub clusters: Vec<u16>,
    /// Painting.
    pub paint: GlyphPaint,
}

/// A page draw command recorded by a [`Band`] in record mode (ADR-0023): the band renderer and
/// the PDF writer are the two consumers of the same `draw_doc` output.
#[derive(Debug)]
pub enum Cmd {
    /// Page background.
    Clear(u32),
    /// Filled path.
    Fill {
        /// Path (page DIPs).
        path: tiny_skia::Path,
        /// `0xRRGGBB`.
        color: u32,
        /// Opacity.
        alpha: f32,
    },
    /// Stroked path.
    Stroke {
        /// Path (page DIPs).
        path: tiny_skia::Path,
        /// `0xRRGGBB`.
        color: u32,
        /// Line width (DIPs).
        width: f32,
        /// Round caps and joins.
        round: bool,
        /// Dash (on, off).
        dash: Option<(f32, f32)>,
    },
    /// Push an (already intersected) clip rectangle.
    PushClip(Rect),
    /// Pop the innermost clip.
    PopClip,
    /// Premultiplied BGRA pixels drawn into `rect`.
    Image {
        /// Destination (page DIPs).
        rect: Rect,
        /// Pixel width.
        w: u32,
        /// Pixel height.
        h: u32,
        /// Pixels.
        px: Vec<u8>,
    },
    /// A JPEG file passed through as `DCTDecode`.
    Jpeg {
        /// Destination (page DIPs).
        rect: Rect,
        /// Pixel width.
        w: u32,
        /// Pixel height.
        h: u32,
        /// Components (1 gray, 3 RGB).
        comps: u8,
        /// File bytes.
        bytes: Vec<u8>,
    },
    /// Glyph run.
    Glyphs(Box<GlyphRec>),
}

/// Copies a DirectWrite glyph run into a [`GlyphRec`].
///
/// # Safety
/// `run` must be a valid glyph run; `desc` null or valid for the run.
unsafe fn glyph_rec(
    x: f32,
    y: f32,
    run: &DWRITE_GLYPH_RUN,
    desc: Option<&DWRITE_GLYPH_RUN_DESCRIPTION>,
    color: u32,
    paint: GlyphPaint,
) -> Option<GlyphRec> {
    let n = run.glyphCount as usize;
    let face = (*run.fontFace).clone()?;
    // SAFETY: DirectWrite run arrays hold `glyphCount` entries (advances/offsets may be null).
    unsafe {
        let ids = std::slice::from_raw_parts(run.glyphIndices, n).to_vec();
        let adv = if run.glyphAdvances.is_null() {
            vec![0.0; n]
        } else {
            std::slice::from_raw_parts(run.glyphAdvances, n).to_vec()
        };
        let offs = if run.glyphOffsets.is_null() {
            vec![(0.0, 0.0); n]
        } else {
            std::slice::from_raw_parts(run.glyphOffsets, n)
                .iter()
                .map(|o| (o.advanceOffset, o.ascenderOffset))
                .collect()
        };
        let (text, clusters) = match desc {
            Some(d) if !d.string.is_null() && !d.clusterMap.is_null() => {
                let len = d.stringLength as usize;
                (
                    std::slice::from_raw_parts(d.string.0, len).to_vec(),
                    std::slice::from_raw_parts(d.clusterMap, len).to_vec(),
                )
            }
            _ => (Vec::new(), Vec::new()),
        };
        Some(GlyphRec {
            face,
            size: run.fontEmSize,
            x,
            y,
            ids,
            adv,
            offs,
            rtl: run.bidiLevel % 2 == 1,
            color,
            text,
            clusters,
            paint,
        })
    }
}

fn rgb_of(c: COLORREF) -> u32 {
    ((c.0 & 0xFF) << 16) | (c.0 & 0xFF00) | ((c.0 >> 16) & 0xFF)
}

// The DIB is BGRA while tiny-skia writes RGBA: swap R/B so the bytes land as BGRA.
fn sk(c: u32, a: f32) -> Color {
    Color::from_rgba(
        (c & 0xFF) as f32 / 255.0,
        ((c >> 8) & 0xFF) as f32 / 255.0,
        ((c >> 16) & 0xFF) as f32 / 255.0,
        a.clamp(0.0, 1.0),
    )
    .unwrap_or(Color::BLACK)
}

fn paint(c: u32, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(sk(c, a));
    p.anti_alias = true;
    p
}

fn rounded(r: tiny_skia::Rect, rad: f32) -> Option<tiny_skia::Path> {
    if rad <= 0.0 {
        return Some(PathBuilder::from_rect(r));
    }
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let (l, t, rr, b) = (r.left(), r.top(), r.right(), r.bottom());
    let k = rad * 0.552_284_8;
    let mut pb = PathBuilder::new();
    pb.move_to(l + rad, t);
    pb.line_to(rr - rad, t);
    pb.cubic_to(rr - rad + k, t, rr, t + rad - k, rr, t + rad);
    pb.line_to(rr, b - rad);
    pb.cubic_to(rr, b - rad + k, rr - rad + k, b, rr - rad, b);
    pb.line_to(l + rad, b);
    pb.cubic_to(l + rad - k, b, l, b - rad + k, l, b - rad);
    pb.line_to(l, t + rad);
    pb.cubic_to(l, t + rad - k, l + rad - k, t, l + rad, t);
    pb.close();
    pb.finish()
}

/// Per-`Draw` context handed to the text renderer through `clientDrawingContext`.
struct DrawCtx<'a> {
    band: &'a Band<'a>,
    shaped: &'a DwShaped,
}

#[implement(IDWriteTextRenderer)]
struct TextRenderer {
    scale: Cell<f32>,
    /// Record mode: no pixel snapping (vector output).
    nosnap: Cell<bool>,
}

impl IDWritePixelSnapping_Impl for TextRenderer_Impl {
    fn IsPixelSnappingDisabled(&self, _c: *const core::ffi::c_void) -> windows::core::Result<BOOL> {
        Ok(BOOL(i32::from(self.nosnap.get())))
    }
    fn GetCurrentTransform(
        &self,
        _c: *const core::ffi::c_void,
        t: *mut DWRITE_MATRIX,
    ) -> windows::core::Result<()> {
        let s = self.scale.get();
        // SAFETY: DirectWrite passes a valid out-pointer.
        unsafe {
            *t = DWRITE_MATRIX {
                m11: s,
                m12: 0.0,
                m21: 0.0,
                m22: s,
                dx: 0.0,
                dy: 0.0,
            }
        };
        Ok(())
    }
    fn GetPixelsPerDip(&self, _c: *const core::ffi::c_void) -> windows::core::Result<f32> {
        Ok(self.scale.get())
    }
}

/// Record mode of `DrawGlyphRun`: COLR runs become outline layers plus invisible text.
///
/// # Safety
/// `run` valid; `desc` null or valid (DirectWrite callback arguments).
#[allow(clippy::too_many_arguments)]
unsafe fn band_record_run(
    band: &Band<'_>,
    rec: &RefCell<Vec<Cmd>>,
    x: f32,
    y: f32,
    mode: DWRITE_MEASURING_MODE,
    run: *const DWRITE_GLYPH_RUN,
    desc: *const DWRITE_GLYPH_RUN_DESCRIPTION,
    color: COLORREF,
) -> windows::core::Result<()> {
    // SAFETY: arguments come straight from DirectWrite (valid for this callback).
    unsafe {
        let Some(r) = run.as_ref() else {
            return Ok(());
        };
        let d = desc.as_ref();
        let base = rgb_of(color);
        let mut paint = GlyphPaint::Text;
        if let Some(f4) = band.r.f4.as_ref()
            && let Ok(en) = f4.TranslateColorGlyphRun(
                Vector2 { X: x, Y: y },
                run,
                Some(desc),
                DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE
                    | DWRITE_GLYPH_IMAGE_FORMATS_CFF
                    | DWRITE_GLYPH_IMAGE_FORMATS_COLR,
                mode,
                None,
                0,
            )
        {
            while en.MoveNext()?.as_bool() {
                let cr = &*en.GetCurrentRun()?;
                let b = &cr.Base;
                let col = if b.paletteIndex == 0xFFFF {
                    base
                } else {
                    let rc = b.runColor;
                    (((rc.r * 255.0) as u32) << 16)
                        | (((rc.g * 255.0) as u32) << 8)
                        | (rc.b * 255.0) as u32
                };
                if let Some(g) = glyph_rec(
                    b.baselineOriginX,
                    b.baselineOriginY,
                    &b.glyphRun,
                    None,
                    col,
                    GlyphPaint::Outline,
                ) {
                    rec.borrow_mut().push(Cmd::Glyphs(Box::new(g)));
                }
            }
            paint = GlyphPaint::Invisible;
        }
        if let Some(g) = glyph_rec(x, y, r, d, base, paint) {
            rec.borrow_mut().push(Cmd::Glyphs(Box::new(g)));
        }
        Ok(())
    }
}

impl IDWriteTextRenderer_Impl for TextRenderer_Impl {
    fn DrawGlyphRun(
        &self,
        c: *const core::ffi::c_void,
        x: f32,
        y: f32,
        mode: DWRITE_MEASURING_MODE,
        run: *const DWRITE_GLYPH_RUN,
        desc: *const DWRITE_GLYPH_RUN_DESCRIPTION,
        _effect: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        // SAFETY: `c` is the DrawCtx passed to IDWriteTextLayout::Draw by `Band::text`, alive for
        // the duration of the call; run/desc are valid for this callback.
        unsafe {
            let ctx = &*c.cast::<DrawCtx>();
            let band = ctx.band;
            band.gdi_begin();
            let pos = desc.as_ref().map_or(0, |d| d.textPosition);
            let color = colorref(ctx.shaped.style_at(pos).color);
            if let Some(rec) = band.rec {
                return band_record_run(band, rec, x, y, mode, run, desc, color);
            }
            if let Some(f4) = band.r.f4.as_ref() {
                match f4.TranslateColorGlyphRun(
                    Vector2 { X: x, Y: y },
                    run,
                    Some(desc),
                    DWRITE_GLYPH_IMAGE_FORMATS_TRUETYPE
                        | DWRITE_GLYPH_IMAGE_FORMATS_CFF
                        | DWRITE_GLYPH_IMAGE_FORMATS_COLR,
                    mode,
                    None,
                    0,
                ) {
                    Ok(en) => {
                        while en.MoveNext()?.as_bool() {
                            let cr = &*en.GetCurrentRun()?;
                            let b = &cr.Base;
                            let col = if b.paletteIndex == 0xFFFF {
                                color
                            } else {
                                let rc = b.runColor;
                                COLORREF(
                                    (rc.r * 255.0) as u32
                                        | (((rc.g * 255.0) as u32) << 8)
                                        | (((rc.b * 255.0) as u32) << 16),
                                )
                            };
                            band.r.brt.DrawGlyphRun(
                                b.baselineOriginX,
                                b.baselineOriginY,
                                cr.measuringMode,
                                &b.glyphRun,
                                &band.r.params,
                                col,
                                None,
                            )?;
                        }
                        return Ok(());
                    }
                    Err(e) if e.code() == DWRITE_E_NOCOLOR => {}
                    Err(e) => return Err(e),
                }
            }
            band.r
                .brt
                .DrawGlyphRun(x, y, mode, run, &band.r.params, color, None)
        }
    }
    fn DrawUnderline(
        &self,
        c: *const core::ffi::c_void,
        x: f32,
        y: f32,
        u: *const DWRITE_UNDERLINE,
        _: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        // SAFETY: see DrawGlyphRun; `u` is valid for this callback.
        unsafe {
            let ctx = &*c.cast::<DrawCtx>();
            if let Some(u) = u.as_ref() {
                ctx.band.decoration(
                    x,
                    y + u.offset,
                    u.width,
                    u.thickness.max(1.0 / ctx.band.scale),
                    ctx.shaped.base.color,
                );
            }
        }
        Ok(())
    }
    fn DrawStrikethrough(
        &self,
        c: *const core::ffi::c_void,
        x: f32,
        y: f32,
        s: *const DWRITE_STRIKETHROUGH,
        _: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        // SAFETY: see DrawGlyphRun; `s` is valid for this callback. B.6.2: strikethrough in `text`.
        unsafe {
            let ctx = &*c.cast::<DrawCtx>();
            if let Some(s) = s.as_ref() {
                ctx.band.decoration(
                    x,
                    y + s.offset,
                    s.width,
                    s.thickness.max(1.0 / ctx.band.scale),
                    ctx.band.text_color,
                );
            }
        }
        Ok(())
    }
    fn DrawInlineObject(
        &self,
        _: *const core::ffi::c_void,
        _: f32,
        _: f32,
        _: Ref<IDWriteInlineObject>,
        _: BOOL,
        _: BOOL,
        _: Ref<IUnknown>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Band renderer (one per window).
pub struct Renderer {
    brt: IDWriteBitmapRenderTarget,
    params: IDWriteRenderingParams,
    f4: Option<IDWriteFactory4>,
    text: IDWriteTextRenderer,
    scale_cell: windows::core::ComObject<TextRenderer>,
    w: i32,
    scale: f32,
}

impl std::fmt::Debug for Renderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Renderer({}px @ {})", self.w, self.scale)
    }
}

/// Band buffer bytes (for `mem-stats`).
pub fn band_bytes(w: i32) -> usize {
    usize::try_from(w.max(1)).unwrap_or(0) * usize::try_from(band_rows(w)).unwrap_or(0) * 4
}

/// Band height for a width (P-12).
pub fn band_rows(w: i32) -> i32 {
    yamdv_layout::geom::band_rows(w)
}

impl Renderer {
    /// Creates the renderer. `hwnd` selects the monitor rendering parameters (ClearType per the
    /// user's settings, B.2); `None` = default parameters (headless rendering).
    pub fn new(
        dw: &DWrite,
        hwnd: Option<HWND>,
        w_px: i32,
        scale: f32,
    ) -> windows::core::Result<Self> {
        // SAFETY: valid factory; sizes are clamped to ≥ 1.
        unsafe {
            let gi = dw.factory.GetGdiInterop()?;
            let brt = gi.CreateBitmapRenderTarget(
                None,
                u32::try_from(w_px.max(1)).unwrap_or(1),
                u32::try_from(band_rows(w_px)).unwrap_or(32),
            )?;
            brt.SetPixelsPerDip(scale)?;
            let params = match hwnd {
                Some(h) => dw
                    .factory
                    .CreateMonitorRenderingParams(MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST))?,
                None => dw.factory.CreateRenderingParams()?,
            };
            let f4 = dw.factory.cast::<IDWriteFactory4>().ok();
            let scale_cell = windows::core::ComObject::new(TextRenderer {
                scale: Cell::new(scale),
                nosnap: Cell::new(false),
            });
            let text: IDWriteTextRenderer = scale_cell.to_interface();
            Ok(Self {
                brt,
                params,
                f4,
                text,
                scale_cell,
                w: w_px.max(1),
                scale,
            })
        }
    }

    /// Width in device pixels.
    pub fn width(&self) -> i32 {
        self.w
    }

    /// Resizes the band (width only).
    pub fn resize(&mut self, w_px: i32) {
        if w_px > 0 && w_px != self.w {
            // SAFETY: valid render target.
            if unsafe {
                self.brt.Resize(
                    u32::try_from(w_px).unwrap_or(1),
                    u32::try_from(band_rows(w_px)).unwrap_or(32),
                )
            }
            .is_ok()
            {
                self.w = w_px;
            }
        }
    }

    /// Changes the DPI scale (WM_DPICHANGED) and refreshes the monitor rendering parameters.
    pub fn set_scale(&mut self, dw: &DWrite, hwnd: HWND, scale: f32) {
        self.scale = scale;
        self.scale_cell.scale.set(scale);
        // SAFETY: valid objects.
        unsafe {
            let _ = self.brt.SetPixelsPerDip(scale);
            if let Ok(p) = dw
                .factory
                .CreateMonitorRenderingParams(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST))
            {
                self.params = p;
            }
        }
    }

    fn bits(&self) -> Option<(*mut u8, i32, i32)> {
        // SAFETY: the memory DC of the render target holds a DIB section; GetObjectW fills `ds`.
        unsafe {
            let dc = self.brt.GetMemoryDC();
            let hbm = GetCurrentObject(dc, OBJ_BITMAP);
            let mut ds = DIBSECTION::default();
            if GetObjectW(
                hbm,
                i32::try_from(size_of::<DIBSECTION>()).unwrap_or(0),
                Some((&raw mut ds).cast()),
            ) == 0
                || ds.dsBm.bmBits.is_null()
            {
                return None;
            }
            Some((
                ds.dsBm.bmBits.cast::<u8>(),
                ds.dsBm.bmWidth,
                ds.dsBm.bmHeight.abs(),
            ))
        }
    }

    /// Paints `rc` (device px) band by band: `draw` gets a [`Band`] whose coordinate system is
    /// client DIPs; each band is then blitted to `hdc`.
    pub fn paint(&self, hdc: HDC, rc: RECT, text_color: u32, draw: &mut dyn FnMut(&Band<'_>)) {
        let Some((bits, w, h)) = self.bits() else {
            return;
        };
        let mut y = rc.top.max(0);
        while y < rc.bottom {
            let bh = band_rows(self.w).min(rc.bottom - y);
            let band = Band {
                r: self,
                bits,
                w,
                h,
                top_px: Cell::new(y),
                band_top: y,
                rows: bh,
                scale: self.scale,
                gdi: Cell::new(false),
                text_color,
                clips: RefCell::new(Vec::new()),
                rec: None,
            };
            // Same reused DIB as `render_bands`: start every band clean, so a draw that leaves
            // part of a band untouched (popup / menu levels, prompt 024) never shows the band
            // painted before it.
            {
                let rows = usize::try_from(bh).unwrap_or(0);
                let stride = usize::try_from(w).unwrap_or(0) * 4;
                // SAFETY: the DIB holds `h ≥ bh` rows of `stride` bytes; no GDI/D2D work is
                // pending (nothing drawn yet, or the previous band ended with `cpu_begin`).
                let buf = unsafe { std::slice::from_raw_parts_mut(bits, stride * rows) };
                yamdv_layout::geom::reset_band(buf, stride, rows);
            }
            draw(&band);
            band.cpu_begin();
            // SAFETY: valid DCs; the band holds `bh` valid rows.
            unsafe {
                let _ = BitBlt(
                    hdc,
                    0,
                    y,
                    self.w,
                    bh,
                    Some(self.brt.GetMemoryDC()),
                    0,
                    0,
                    SRCCOPY,
                );
            }
            y += bh;
        }
    }

    /// Renders `h_px` rows offscreen into a top-down BGRA buffer (`--render-png`).
    pub fn render_offscreen(
        &self,
        h_px: i32,
        text_color: u32,
        draw: &mut dyn FnMut(&Band<'_>),
    ) -> Vec<u8> {
        let stride = usize::try_from(self.w.max(1)).unwrap_or(0) * 4;
        let mut out = vec![0u8; stride * usize::try_from(h_px.max(0)).unwrap_or(0)];
        self.render_bands(h_px, text_color, draw, &mut |y, rows, src| {
            let start = usize::try_from(y).unwrap_or(0) * stride;
            let n = stride * usize::try_from(rows).unwrap_or(0);
            if let (Some(dst), Some(src)) = (out.get_mut(start..start + n), src.get(..n)) {
                dst.copy_from_slice(src);
            }
        });
        out
    }

    /// Renders `h_px` rows band by band; `sink(y, rows, bits)` receives each band's top-down
    /// BGRA rows (`width() * 4` bytes per row) right after it is drawn. No full-height bitmap is
    /// allocated (printing, R-EXP-03).
    pub fn render_bands(
        &self,
        h_px: i32,
        text_color: u32,
        draw: &mut dyn FnMut(&Band<'_>),
        sink: &mut dyn FnMut(i32, i32, &[u8]),
    ) {
        let Some((bits, w, h)) = self.bits() else {
            return;
        };
        let stride = usize::try_from(w).unwrap_or(0) * 4;
        let mut y = 0;
        while y < h_px {
            let bh = band_rows(self.w).min(h_px - y);
            let band = Band {
                r: self,
                bits,
                w,
                h,
                top_px: Cell::new(y),
                band_top: y,
                rows: bh,
                scale: self.scale,
                gdi: Cell::new(false),
                text_color,
                clips: RefCell::new(Vec::new()),
                rec: None,
            };
            let rows = usize::try_from(bh).unwrap_or(0);
            // The DIB is reused for every band: start each one transparent, or a band that
            // draws nothing (below a diagram's content) repeats the previous one (prompt 023).
            {
                // SAFETY: the DIB holds `h ≥ bh` rows of `stride` bytes; no GDI/D2D work is
                // pending (the previous band ended with `cpu_begin`).
                let buf = unsafe { std::slice::from_raw_parts_mut(bits, stride * rows) };
                yamdv_layout::geom::reset_band(buf, stride, rows);
            }
            draw(&band);
            band.cpu_begin();
            // SAFETY: the DIB holds `h ≥ bh` rows of `stride` bytes.
            let src = unsafe { std::slice::from_raw_parts(bits, stride * rows) };
            sink(y, bh, src);
            y += bh;
        }
    }

    /// Record mode (ADR-0023): runs `draw` on a band that covers the whole page and returns the
    /// draw commands in page DIPs instead of rasterising them; images are decoded for this
    /// renderer's scale.
    pub fn record(&self, text_color: u32, draw: &mut dyn FnMut(&Band<'_>)) -> Vec<Cmd> {
        let rec = RefCell::new(Vec::new());
        self.scale_cell.nosnap.set(true);
        {
            let band = Band {
                r: self,
                bits: std::ptr::null_mut(),
                w: 0,
                h: 0,
                top_px: Cell::new(0),
                band_top: 0,
                rows: i32::MAX / 4,
                scale: self.scale,
                gdi: Cell::new(false),
                text_color,
                clips: RefCell::new(Vec::new()),
                rec: Some(&rec),
            };
            draw(&band);
        }
        self.scale_cell.nosnap.set(false);
        rec.into_inner()
    }

    /// Grayscale antialiasing (print / PDF: no ClearType colour fringes on paper).
    pub fn set_grayscale(&mut self, dw: &DWrite) {
        // SAFETY: plain factory call; the new params replace the old ones (released by Drop).
        if let Ok(p) = unsafe {
            dw.factory.CreateCustomRenderingParams(
                1.8,
                0.5,
                0.0,
                windows::Win32::Graphics::DirectWrite::DWRITE_PIXEL_GEOMETRY_FLAT,
                windows::Win32::Graphics::DirectWrite::DWRITE_RENDERING_MODE_NATURAL_SYMMETRIC,
            )
        } {
            self.params = p;
        }
    }
}

/// One band being painted. Coordinates are client DIPs.
pub struct Band<'a> {
    r: &'a Renderer,
    bits: *mut u8,
    w: i32,
    h: i32,
    /// Device row of DIP y = 0 relative to the band's first row, negated (see [`Band::set_origin`]).
    top_px: Cell<i32>,
    /// First window row of this band.
    band_top: i32,
    rows: i32,
    /// DPI scale.
    pub scale: f32,
    gdi: Cell<bool>,
    text_color: u32,
    /// Clip stack (P-19): effective clip rectangle (client DIPs) and its band mask.
    clips: RefCell<Vec<(Rect, Option<Mask>)>>,
    /// Record mode (ADR-0023): commands instead of pixels.
    rec: Option<&'a RefCell<Vec<Cmd>>>,
}

impl Band<'_> {
    /// True in record mode (PDF): callers may hand over source data (JPEG) instead of pixels.
    pub fn recording(&self) -> bool {
        self.rec.is_some()
    }

    fn push(&self, c: Cmd) -> bool {
        match self.rec {
            Some(r) => {
                r.borrow_mut().push(c);
                true
            }
            None => false,
        }
    }

    /// Record mode: a JPEG file drawn into `rect` (passed through as `DCTDecode`).
    pub fn jpeg(&self, rect: Rect, w: u32, h: u32, comps: u8, bytes: Vec<u8>) {
        if self.clip_allows(&rect) {
            self.push(Cmd::Jpeg {
                rect,
                w,
                h,
                comps,
                bytes,
            });
        }
    }

    /// Moves the DIP origin to window row `y_dip` (client DIPs): the title bar draws at 0, the
    /// content below it at the bar height (V2-A). Only call with an empty clip stack.
    pub fn set_origin(&self, y_dip: f32) {
        let off = (y_dip * self.scale).round() as i32;
        self.top_px.set(self.band_top - off);
    }

    /// Draws `icon` (`HICON`) at DIP `(x, y)` with a side of `size` DIPs (title-bar app icon).
    pub fn icon(
        &self,
        icon: windows::Win32::UI::WindowsAndMessaging::HICON,
        x: f32,
        y: f32,
        size: f32,
    ) {
        if !self.visible(y, size) || icon.is_invalid() || self.rec.is_some() {
            return;
        }
        self.cpu_begin();
        let px = (size * self.scale).round() as i32;
        let (dx, dy) = (
            (x * self.scale).round() as i32,
            (y * self.scale).round() as i32 - self.top_px.get(),
        );
        // SAFETY: valid icon and the render target's memory DC (the band DIB is selected).
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DrawIconEx(
                self.r.brt.GetMemoryDC(),
                dx,
                dy,
                icon,
                px,
                px,
                0,
                None,
                windows::Win32::UI::WindowsAndMessaging::DI_NORMAL,
            );
        }
        self.gdi_begin();
    }

    /// Pushes a clip rectangle (intersected with the current clip). Everything drawn until the
    /// matching [`Band::pop_clip`] — shapes, glyph runs, images — stays inside it.
    pub fn push_clip(&self, r: Rect) {
        let eff = match self.clip() {
            Some(c) => c.intersect(&r),
            None => r,
        };
        if self.push(Cmd::PushClip(eff)) {
            self.clips.borrow_mut().push((eff, None));
            return;
        }
        let mut mask = Mask::new(self.w as u32, self.h as u32);
        if let Some(m) = mask.as_mut()
            && let Some(rc) =
                tiny_skia::Rect::from_xywh(eff.x, eff.y, eff.w.max(0.0), eff.h.max(0.0))
        {
            let t = Transform::from_row(
                self.scale,
                0.0,
                0.0,
                self.scale,
                0.0,
                -(self.top_px.get() as f32),
            );
            m.fill_path(&PathBuilder::from_rect(rc), FillRule::Winding, false, t);
        }
        self.clips.borrow_mut().push((eff, mask));
    }

    /// Pops the innermost clip.
    pub fn pop_clip(&self) {
        if self.clips.borrow_mut().pop().is_some() {
            self.push(Cmd::PopClip);
        }
    }

    /// Current effective clip (client DIPs).
    pub fn clip(&self) -> Option<Rect> {
        self.clips.borrow().last().map(|c| c.0)
    }

    /// False when `r` lies completely outside the current clip.
    fn clip_allows(&self, r: &Rect) -> bool {
        self.clip().is_none_or(|c| c.overlaps(r))
    }

    /// Runs a tiny-skia draw with the current clip mask (if any).
    fn with_clipped(&self, f: impl FnOnce(&mut PixmapMut<'_>, Transform, Option<&Mask>)) {
        let clips = self.clips.borrow();
        let mask = clips.last().and_then(|c| c.1.as_ref());
        self.with_pixmap(|pm, t| f(pm, t, mask));
    }
    /// DIP range covered by this band.
    pub fn dip_range(&self) -> (f32, f32) {
        (
            self.top_px.get() as f32 / self.scale,
            (self.top_px.get() + self.rows) as f32 / self.scale,
        )
    }

    /// Does the DIP rectangle intersect the band?
    pub fn visible(&self, y: f32, h: f32) -> bool {
        let (a, b) = self.dip_range();
        y < b && y + h > a
    }

    fn gdi_begin(&self) {
        self.gdi.set(true);
    }

    fn cpu_begin(&self) {
        if self.gdi.replace(false) {
            // SAFETY: flushes pending GDI work before the CPU touches the DIB bits.
            unsafe {
                let _ = GdiFlush();
            }
        }
    }

    fn with_pixmap(&self, f: impl FnOnce(&mut PixmapMut<'_>, Transform)) {
        self.cpu_begin();
        let len = usize::try_from(self.w).unwrap_or(0) * usize::try_from(self.h).unwrap_or(0) * 4;
        // SAFETY: `bits` points to the render target's DIB of w×h 32-bpp pixels; no other
        // reference to it exists while `f` runs (GDI work was flushed above).
        let buf = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        let Some(mut pm) = PixmapMut::from_bytes(
            buf,
            u32::try_from(self.w).unwrap_or(0),
            u32::try_from(self.h).unwrap_or(0),
        ) else {
            return;
        };
        let t = Transform::from_row(
            self.scale,
            0.0,
            0.0,
            self.scale,
            0.0,
            -(self.top_px.get() as f32),
        );
        f(&mut pm, t);
    }

    /// Fills the whole band.
    pub fn clear(&self, color: u32) {
        if self.push(Cmd::Clear(color)) {
            return;
        }
        self.with_pixmap(|pm, _| pm.fill(sk(color, 1.0)));
    }

    /// Fills a (rounded) rectangle.
    pub fn fill(&self, r: Rect, color: u32, alpha: f32, radius: f32) {
        if !self.visible(r.y, r.h) || r.w <= 0.0 || r.h <= 0.0 || !self.clip_allows(&r) {
            return;
        }
        let Some(p) =
            tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h).and_then(|rc| rounded(rc, radius))
        else {
            return;
        };
        if let Some(rec) = self.rec {
            rec.borrow_mut().push(Cmd::Fill {
                path: p,
                color,
                alpha,
            });
            return;
        }
        self.with_clipped(|pm, t, m| {
            pm.fill_path(&p, &paint(color, alpha), FillRule::Winding, t, m);
        });
    }

    /// Draws premultiplied pixels in band byte order (BGRA, as WIC `32bppPBGRA` delivers them)
    /// with their top-left at DIP `(x, y)`, one source pixel per device pixel (R-IMG-04).
    pub fn image(&self, x: f32, y: f32, w: u32, h: u32, px: &[u8]) {
        let (wd, hd) = (w as f32 / self.scale, h as f32 / self.scale);
        let r = Rect::new(x, y, wd, hd);
        if !self.visible(y, hd) || !self.clip_allows(&r) {
            return;
        }
        if self.push(Cmd::Image {
            rect: r,
            w,
            h,
            px: px.to_vec(),
        }) {
            return;
        }
        let Some(src) = tiny_skia::PixmapRef::from_bytes(px, w, h) else {
            return;
        };
        self.with_clipped(|pm, t, m| {
            let (dx, dy) = (
                (x * t.sx + t.tx).round() as i32,
                (y * t.sy + t.ty).round() as i32,
            );
            pm.draw_pixmap(
                dx,
                dy,
                src,
                &tiny_skia::PixmapPaint::default(),
                Transform::identity(),
                m,
            );
        });
    }

    /// Draws premultiplied BGRA pixels (`w`×`h`) scaled into the DIP rectangle `dst` with
    /// bilinear filtering (B.11 fullscreen viewer).
    pub fn image_scaled(&self, dst: Rect, w: u32, h: u32, px: &[u8]) {
        if w == 0 || h == 0 || !self.visible(dst.y, dst.h) || !self.clip_allows(&dst) {
            return;
        }
        if self.push(Cmd::Image {
            rect: dst,
            w,
            h,
            px: px.to_vec(),
        }) {
            return;
        }
        let Some(src) = tiny_skia::PixmapRef::from_bytes(px, w, h) else {
            return;
        };
        self.with_clipped(|pm, t, m| {
            let (sx, sy) = (dst.w * t.sx / w as f32, dst.h * t.sy / h as f32);
            let paint = tiny_skia::PixmapPaint {
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            };
            pm.draw_pixmap(
                0,
                0,
                src,
                &paint,
                Transform::from_row(sx, 0.0, 0.0, sy, dst.x * t.sx + t.tx, dst.y * t.sy + t.ty),
                m,
            );
        });
    }

    /// Strokes a (rounded) rectangle with a line of `width` DIPs inside its bounds.
    pub fn stroke(&self, r: Rect, color: u32, radius: f32, width: f32) {
        if !self.visible(r.y, r.h) || !self.clip_allows(&r) {
            return;
        }
        if let Some(rec) = self.rec {
            let half = width / 2.0;
            if let Some(p) =
                tiny_skia::Rect::from_xywh(r.x + half, r.y + half, r.w - width, r.h - width)
                    .and_then(|rc| rounded(rc, (radius - half).max(0.0)))
            {
                rec.borrow_mut().push(Cmd::Stroke {
                    path: p,
                    color,
                    width,
                    round: false,
                    dash: None,
                });
            }
            return;
        }
        self.with_clipped(|pm, t, m| {
            let half = width / 2.0;
            if let Some(rc) =
                tiny_skia::Rect::from_xywh(r.x + half, r.y + half, r.w - width, r.h - width)
                && let Some(p) = rounded(rc, (radius - half).max(0.0))
            {
                pm.stroke_path(
                    &p,
                    &paint(color, 1.0),
                    &Stroke {
                        width,
                        ..Default::default()
                    },
                    t,
                    m,
                );
            }
        });
    }

    /// Stroked polyline (vector icons, check marks, chevrons); `closed` joins the last point.
    pub fn polyline(&self, pts: &[(f32, f32)], color: u32, width: f32, closed: bool) {
        let (Some(x0), Some(x1), Some(y0), Some(y1)) = (
            pts.iter().map(|p| p.0).reduce(f32::min),
            pts.iter().map(|p| p.0).reduce(f32::max),
            pts.iter().map(|p| p.1).reduce(f32::min),
            pts.iter().map(|p| p.1).reduce(f32::max),
        ) else {
            return;
        };
        let bb = Rect::new(
            x0 - width,
            y0 - width,
            x1 - x0 + 2.0 * width,
            y1 - y0 + 2.0 * width,
        );
        if !self.visible(bb.y, bb.h) || !self.clip_allows(&bb) {
            return;
        }
        let mut pb = PathBuilder::new();
        for (k, p) in pts.iter().enumerate() {
            if k == 0 {
                pb.move_to(p.0, p.1);
            } else {
                pb.line_to(p.0, p.1);
            }
        }
        if closed {
            pb.close();
        }
        let path = pb.finish();
        if let Some(rec) = self.rec {
            if let Some(path) = path {
                rec.borrow_mut().push(Cmd::Stroke {
                    path,
                    color,
                    width,
                    round: true,
                    dash: None,
                });
            }
            return;
        }
        self.with_clipped(|pm, t, m| {
            if let Some(path) = path {
                pm.stroke_path(
                    &path,
                    &paint(color, 1.0),
                    &Stroke {
                        width,
                        line_cap: tiny_skia::LineCap::Round,
                        line_join: tiny_skia::LineJoin::Round,
                        ..Default::default()
                    },
                    t,
                    m,
                );
            }
        });
    }

    /// Diagram path (R-MMD-02): Bézier segments with optional fill and (dashed) stroke, offset by
    /// `(dx, dy)`.
    pub fn path(
        &self,
        segs: &[yamdv_mermaid::Seg],
        (dx, dy): (f32, f32),
        fill: Option<u32>,
        stroke: Option<(u32, f32, bool)>,
    ) {
        use yamdv_mermaid::Seg;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut grow = |x: f32, y: f32| {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        };
        for sg in segs {
            match *sg {
                Seg::M(a, b) | Seg::L(a, b) => grow(a, b),
                Seg::C(a, b, c, d, e, f) => {
                    grow(a, b);
                    grow(c, d);
                    grow(e, f);
                }
                Seg::Z => {}
            }
        }
        if x0 > x1 {
            return;
        }
        let sw = stroke.map_or(0.0, |s| s.1);
        let bb = Rect::new(
            x0 + dx - sw,
            y0 + dy - sw,
            x1 - x0 + 2.0 * sw,
            y1 - y0 + 2.0 * sw,
        );
        if !self.visible(bb.y, bb.h) || !self.clip_allows(&bb) {
            return;
        }
        let mut pb = PathBuilder::new();
        for sg in segs {
            match *sg {
                Seg::M(a, b) => pb.move_to(a + dx, b + dy),
                Seg::L(a, b) => pb.line_to(a + dx, b + dy),
                Seg::C(a, b, c, d, e, f) => {
                    pb.cubic_to(a + dx, b + dy, c + dx, d + dy, e + dx, f + dy);
                }
                Seg::Z => pb.close(),
            }
        }
        let Some(path) = pb.finish() else { return };
        if let Some(rec) = self.rec {
            let mut r = rec.borrow_mut();
            if let Some(c) = fill {
                r.push(Cmd::Fill {
                    path: path.clone(),
                    color: c,
                    alpha: 1.0,
                });
            }
            if let Some((c, w, dash)) = stroke {
                r.push(Cmd::Stroke {
                    path,
                    color: c,
                    width: w.max(0.5),
                    round: true,
                    dash: dash.then(|| (4.0 * w.max(1.0), 3.0 * w.max(1.0))),
                });
            }
            return;
        }
        self.with_clipped(|pm, t, m| {
            if let Some(c) = fill {
                pm.fill_path(&path, &paint(c, 1.0), FillRule::Winding, t, m);
            }
            if let Some((c, w, dash)) = stroke {
                let st = Stroke {
                    width: w.max(0.5),
                    line_join: tiny_skia::LineJoin::Round,
                    dash: if dash {
                        tiny_skia::StrokeDash::new(vec![4.0 * w.max(1.0), 3.0 * w.max(1.0)], 0.0)
                    } else {
                        None
                    },
                    ..Default::default()
                };
                pm.stroke_path(&path, &paint(c, 1.0), &st, t, m);
            }
        });
    }

    /// Filled circle or ring.
    pub fn circle(&self, cx: f32, cy: f32, r: f32, color: u32, ring: Option<f32>) {
        if !self.visible(cy - r, 2.0 * r)
            || !self.clip_allows(&Rect::new(cx - r, cy - r, 2.0 * r, 2.0 * r))
        {
            return;
        }
        if let Some(rec) = self.rec {
            let c = match ring {
                None => PathBuilder::from_circle(cx, cy, r).map(|path| Cmd::Fill {
                    path,
                    color,
                    alpha: 1.0,
                }),
                Some(w) => PathBuilder::from_circle(cx, cy, r - w / 2.0).map(|path| Cmd::Stroke {
                    path,
                    color,
                    width: w,
                    round: false,
                    dash: None,
                }),
            };
            if let Some(c) = c {
                rec.borrow_mut().push(c);
            }
            return;
        }
        self.with_clipped(|pm, t, m| match ring {
            None => {
                if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
                    pm.fill_path(&p, &paint(color, 1.0), FillRule::Winding, t, m);
                }
            }
            Some(w) => {
                if let Some(p) = PathBuilder::from_circle(cx, cy, r - w / 2.0) {
                    pm.stroke_path(
                        &p,
                        &paint(color, 1.0),
                        &Stroke {
                            width: w,
                            ..Default::default()
                        },
                        t,
                        m,
                    );
                }
            }
        });
    }

    /// Horizontal fade from transparent to `color` (left → right).
    pub fn fade_right(&self, r: Rect, color: u32) {
        if !self.visible(r.y, r.h) || !self.clip_allows(&r) || self.rec.is_some() {
            return;
        }
        self.with_clipped(|pm, t, m| {
            let Some(shader) = LinearGradient::new(
                Point::from_xy(r.x, 0.0),
                Point::from_xy(r.x + r.w, 0.0),
                vec![
                    GradientStop::new(0.0, sk(color, 0.0)),
                    GradientStop::new(1.0, sk(color, 1.0)),
                ],
                SpreadMode::Pad,
                Transform::identity(),
            ) else {
                return;
            };
            let p = Paint {
                shader,
                anti_alias: true,
                ..Default::default()
            };
            if let Some(rc) = tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h) {
                pm.fill_rect(rc, &p, t, m);
            }
        });
    }

    fn decoration(&self, x: f32, y_band_dip: f32, w: f32, th: f32, color: u32) {
        // Called from text callbacks: coordinates are already band-relative DIPs.
        let top = self.top_px.get() as f32 / self.scale;
        self.fill(Rect::new(x, y_band_dip + top, w, th), color, 1.0, 0.0);
        self.gdi_begin();
    }

    /// Draws a shaped paragraph with its origin at (x, y).
    pub fn text(&self, shaped: &dyn Shaped, x: f32, y: f32) {
        if !self.visible(y, shaped.height()) {
            return;
        }
        let bbox = Rect::new(x, y, shaped.width(), shaped.height());
        if self.rec.is_some() {
            if self.clip_allows(&bbox) {
                self.text_raw(shaped, x, y);
            }
            return;
        }
        match self.clip() {
            Some(c) if !c.overlaps(&bbox) => {}
            // Glyph runs are rasterized by DirectWrite straight into the DIB, which ignores GDI
            // clip regions: save the band rows, draw, and restore every pixel outside the clip.
            Some(c) if !c.contains_rect(&bbox) => {
                self.clipped_px(c, || self.text_raw(shaped, x, y))
            }
            _ => self.text_raw(shaped, x, y),
        }
    }

    /// Runs `draw`, then restores all band pixels outside `clip` (client DIPs) to their prior
    /// values. The saved copy is at most one band (≤ 512 KiB, P-12).
    fn clipped_px(&self, clip: Rect, draw: impl FnOnce()) {
        self.cpu_begin();
        let stride = self.w as usize * 4;
        let rows = self.rows.max(0) as usize;
        let len = stride * rows;
        // SAFETY: the DIB holds ≥ `rows` rows of `stride` bytes; GDI work was flushed above.
        let saved = unsafe { std::slice::from_raw_parts(self.bits, len) }.to_vec();
        draw();
        self.cpu_begin();
        let s = self.scale;
        let x0 = ((clip.x * s).floor().max(0.0) as usize).min(self.w as usize);
        let x1 = ((clip.right() * s).ceil().max(0.0) as usize).min(self.w as usize);
        let y0 = ((clip.y * s).floor() - self.top_px.get() as f32).clamp(0.0, rows as f32) as usize;
        let y1 = ((clip.bottom() * s).ceil() - self.top_px.get() as f32).clamp(0.0, rows as f32)
            as usize;
        // SAFETY: as above; no other reference to the bits exists here.
        let buf = unsafe { std::slice::from_raw_parts_mut(self.bits, len) };
        for row in 0..rows {
            let a = row * stride;
            if row < y0 || row >= y1 || x0 >= x1 {
                buf[a..a + stride].copy_from_slice(&saved[a..a + stride]);
            } else {
                buf[a..a + x0 * 4].copy_from_slice(&saved[a..a + x0 * 4]);
                buf[a + x1 * 4..a + stride].copy_from_slice(&saved[a + x1 * 4..a + stride]);
            }
        }
    }

    fn text_raw(&self, shaped: &dyn Shaped, x: f32, y: f32) {
        let Some(dw) = shaped.as_any().downcast_ref::<DwShaped>() else {
            return;
        };
        let Some(layout) = &dw.layout else { return };
        self.cpu_begin();
        let ctx = DrawCtx {
            band: self,
            shaped: dw,
        };
        let top = self.top_px.get() as f32 / self.scale;
        // SAFETY: valid layout and renderer; `ctx` outlives the synchronous Draw call.
        unsafe {
            let _ = layout.Draw(Some((&raw const ctx).cast()), &self.r.text, x, y - top);
        }
        self.gdi_begin();
    }

    /// Draws a laid-out formula (R-MATH-01) with its baseline origin at (x, y): glyphs from the
    /// math font as positioned glyph runs, rules (fraction bars, radical overbars) as fills.
    pub fn math(&self, mb: &yamdv_math::MathBox, x: f32, y: f32, color: u32) {
        // Glyph ink can exceed the box slightly (italic overhang, assembly parts).
        let pad = 4.0;
        let bbox = Rect::new(
            x - pad,
            y - mb.ascent - pad,
            mb.width + 2.0 * pad,
            mb.ascent + mb.descent + 2.0 * pad,
        );
        if !self.visible(bbox.y, bbox.h) {
            return;
        }
        if self.rec.is_some() {
            if self.clip_allows(&bbox) {
                self.math_raw(mb, x, y, color);
            }
            return;
        }
        match self.clip() {
            Some(c) if !c.overlaps(&bbox) => {}
            Some(c) if !c.contains_rect(&bbox) => {
                self.clipped_px(c, || self.math_raw(mb, x, y, color));
            }
            _ => self.math_raw(mb, x, y, color),
        }
    }

    fn math_raw(&self, mb: &yamdv_math::MathBox, x: f32, y: f32, color: u32) {
        use yamdv_math::Op;
        for op in &mb.ops {
            match op {
                Op::Rule {
                    x: rx,
                    y: ry,
                    w,
                    h,
                    color: c,
                } => self.fill(
                    Rect::new(x + rx, y + ry, *w, *h),
                    c.unwrap_or(color),
                    1.0,
                    0.0,
                ),
                // `\cancel`, `\not` (R-MATH-02).
                Op::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    w,
                    color: c,
                } => self.polyline(
                    &[(x + x1, y + y1), (x + x2, y + y2)],
                    c.unwrap_or(color),
                    *w,
                    false,
                ),
                Op::Glyph { .. } => {}
            }
        }
        let Some(face) = crate::dwrite::math_draw_face() else {
            return;
        };
        let top = self.top_px.get() as f32 / self.scale;
        // Consecutive glyphs of one size and color form one run (zero advances, explicit offsets).
        let glyphs: Vec<(u16, f32, f32, f32, u32)> = mb
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::Glyph {
                    gid,
                    x: gx,
                    y: gy,
                    size,
                    color: c,
                } => Some((*gid, *gx, *gy, *size, c.unwrap_or(color))),
                Op::Rule { .. } | Op::Line { .. } => None,
            })
            .collect();
        self.cpu_begin();
        let mut i = 0;
        while i < glyphs.len() {
            let (_, _, _, size, col) = glyphs[i];
            let mut j = i + 1;
            while j < glyphs.len() && glyphs[j].3.to_bits() == size.to_bits() && glyphs[j].4 == col
            {
                j += 1;
            }
            let run = &glyphs[i..j];
            let ids: Vec<u16> = run.iter().map(|g| g.0).collect();
            let adv = vec![0.0f32; run.len()];
            if let Some(rec) = self.rec {
                rec.borrow_mut().push(Cmd::Glyphs(Box::new(GlyphRec {
                    face: face.clone(),
                    size,
                    x,
                    y,
                    ids,
                    adv,
                    offs: run.iter().map(|g| (g.1, -g.2)).collect(),
                    rtl: false,
                    color: col,
                    text: Vec::new(),
                    clusters: Vec::new(),
                    paint: GlyphPaint::Text,
                })));
                i = j;
                continue;
            }
            let offs: Vec<DWRITE_GLYPH_OFFSET> = run
                .iter()
                .map(|g| DWRITE_GLYPH_OFFSET {
                    advanceOffset: g.1,
                    ascenderOffset: -g.2,
                })
                .collect();
            let mut gr = DWRITE_GLYPH_RUN {
                fontFace: std::mem::ManuallyDrop::new(Some(face.clone())),
                fontEmSize: size,
                glyphCount: ids.len() as u32,
                glyphIndices: ids.as_ptr(),
                glyphAdvances: adv.as_ptr(),
                glyphOffsets: offs.as_ptr(),
                isSideways: BOOL(0),
                bidiLevel: 0,
            };
            // SAFETY: the run's buffers outlive the synchronous call; the face reference taken
            // above is released right after.
            unsafe {
                let _ = self.r.brt.DrawGlyphRun(
                    x,
                    y - top,
                    DWRITE_MEASURING_MODE_NATURAL,
                    &gr,
                    &self.r.params,
                    colorref(col),
                    None,
                );
                std::mem::ManuallyDrop::drop(&mut gr.fontFace);
            }
            i = j;
        }
        self.gdi_begin();
    }
}

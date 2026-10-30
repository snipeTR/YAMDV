// SPDX-License-Identifier: AGPL-3.0-or-later
//! GFM tables (R-REN-07, UI_SPEC §B.6.8, prompt 003 §4.3).
//!
//! Auto layout: per column `min` = widest unbreakable token (≥ 48 px incl. padding) and `max` =
//! unwrapped content; columns get `max` when everything fits, otherwise `min` plus the remaining
//! width shared in proportion to `max − min` (CSS auto layout). When even the minimums do not fit
//! the widths are scaled down; the shaper's emergency breaking keeps text inside its column.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_arguments
)]

use yamdv_core::doc::detail::{Detail, NodeKind};
use yamdv_core::doc::text::node_text;

use crate::block::{Ctx, Item, Out, base, card, leaf_aligned, source_text, spans};
use crate::geom::Rect;
use crate::shaper::{ParaSpec, is_rtl};

/// S-PARSE-02 limits: more columns or cells render the source in a "table too large" card.
pub const MAX_COLS: usize = 1_000;
/// See [`MAX_COLS`].
pub const MAX_CELLS: usize = 100_000;
/// Minimum column width including padding (B.6.8).
pub const MIN_COL: f32 = 48.0;
/// Cell padding (each side).
pub const PAD: f32 = 8.0;
/// Rows sampled for column measurement (larger tables wrap in later rows).
pub const MEASURE_ROWS: usize = 1_000;

/// CSS-like auto distribution of `avail` among columns with `(min, max)` widths.
pub fn distribute(cols: &[(f32, f32)], avail: f32) -> Vec<f32> {
    let sum_max: f32 = cols.iter().map(|c| c.1).sum();
    let sum_min: f32 = cols.iter().map(|c| c.0).sum();
    if sum_max <= avail {
        return cols.iter().map(|c| c.1).collect();
    }
    if sum_min >= avail {
        // Water-filling: narrow columns keep their minimum, the widest share what is left (their
        // long tokens break anywhere).
        let mut order: Vec<usize> = (0..cols.len()).collect();
        order.sort_by(|&a, &b| cols[a].0.total_cmp(&cols[b].0));
        let mut out = vec![0.0; cols.len()];
        let mut left = avail;
        for (k, &c) in order.iter().enumerate() {
            let share = left / (cols.len() - k) as f32;
            out[c] = cols[c].0.min(share).max(1.0);
            left -= out[c];
        }
        return out;
    }
    let extra = avail - sum_min;
    let span = sum_max - sum_min;
    cols.iter()
        .map(|c| c.0 + extra * (c.1 - c.0) / span.max(f32::EPSILON))
        .collect()
}

fn mix(a: u32, b: u32) -> u32 {
    let ch = |s: u32| (((a >> s) & 0xff) + ((b >> s) & 0xff)) / 2;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Lays out table node `i`; returns its height.
pub(crate) fn table(
    ctx: &mut Ctx<'_>,
    src: &str,
    d: &Detail,
    i: usize,
    aligns: &[u8],
    x: f32,
    y: f32,
    w: f32,
    out: &mut Out,
) -> f32 {
    let m = ctx.m;
    let rows: Vec<(bool, Vec<usize>)> = d
        .children(i)
        .filter_map(|r| match d.nodes[r].kind {
            NodeKind::TableRow { head } => Some((head, d.children(r).collect())),
            _ => None,
        })
        .collect();
    // BUG-05: place cells on the grid, skipping columns an earlier `rowspan` still covers.
    let (slots, covered) = place(d, &rows);
    let ncols = aligns
        .len()
        .max(slots.iter().flatten().map(|s| s.0 + 1).max().unwrap_or(0));
    let cells: usize = rows.iter().map(|r| r.1.len()).sum();
    if ncols == 0 || ncols > MAX_COLS || cells > MAX_CELLS {
        let pt = source_text(src, &d.nodes[i].range);
        return card(ctx, pt, Some("table too large"), x, y, w, out);
    }
    // Measure.
    let mut cols = vec![(MIN_COL, MIN_COL); ncols];
    for ((head, _), row) in rows.iter().zip(&slots).take(MEASURE_ROWS) {
        let mut style = base(&m, ctx.pal.text);
        if *head {
            style.weight = 600;
        }
        for &(c, cell, _) in row {
            let pt = node_text(src, d, cell, ctx.single_newline_break);
            if pt.text.is_empty() || c >= ncols {
                continue;
            }
            let sp = spans(&pt.segs, &style, &m, ctx.pal);
            let full = ctx.shaper.shape(&ParaSpec {
                text: &pt.text,
                base: style,
                spans: sp,
                width: None,
                line_pitch: m.pitch(),
                rtl: is_rtl(&pt.text),
                tab_spaces: None,
                text_align: 0,
                objects: Vec::new(),
            });
            let token = pt
                .text
                .split_whitespace()
                .max_by_key(|t| t.chars().count())
                .unwrap_or("");
            let tok = ctx.shaper.shape(&ParaSpec {
                text: token,
                base: style,
                spans: vec![],
                width: None,
                line_pitch: m.pitch(),
                rtl: false,
                tab_spaces: None,
                text_align: 0,
                objects: Vec::new(),
            });
            let e = &mut cols[c];
            e.0 = e.0.max(tok.width() + 2.0 * PAD);
            e.1 = e.1.max(full.width() + 2.0 * PAD);
        }
    }
    for c in &mut cols {
        c.1 = c.1.max(c.0);
    }
    let widths = distribute(&cols, w.max(1.0));
    let total_w: f32 = widths.iter().sum();
    let radius = if m.rounded { 6.0 } else { 0.0 };
    let zebra = if ctx.pal.zebra_bg != 0 {
        ctx.pal.zebra_bg
    } else {
        mix(ctx.pal.bg, ctx.pal.table_header_bg)
    };
    let bg_at = out.items.len();
    let mut bgs: Vec<Item> = Vec::new();
    let mut lines: Vec<Item> = Vec::new();
    let mut yy = y;
    let mut body = 0usize;
    let col_x: Vec<f32> = widths
        .iter()
        .scan(x, |acc, w| {
            let at = *acc;
            *acc += w;
            Some(at)
        })
        .collect();
    // Open spans: (last row, top y, content height).
    let mut spans_open: Vec<(usize, f32, f32)> = Vec::new();
    for (r, ((head, _), row)) in rows.iter().zip(&slots).enumerate() {
        let mut style = base(&m, ctx.pal.text);
        if *head {
            style.weight = 600;
        }
        let mut rh = m.pitch();
        for &(c, cell, rs) in row {
            if c >= ncols {
                continue;
            }
            let cw = widths[c];
            let cx = col_x[c];
            let pt = node_text(src, d, cell, ctx.single_newline_break);
            let align = aligns.get(c).copied().unwrap_or(0);
            let h = leaf_aligned(
                ctx,
                pt,
                style,
                m.pitch(),
                cx + PAD,
                yy + PAD,
                (cw - 2.0 * PAD).max(1.0),
                align,
                out,
            );
            if rs > 1 {
                spans_open.push((r + rs - 1, yy, h + 2.0 * PAD));
            } else {
                rh = rh.max(h);
            }
        }
        let mut row_h = rh + 2.0 * PAD;
        // A span ending here grows its last row to fit its content.
        for &(last, top, need) in &spans_open {
            if last == r {
                row_h = row_h.max(need - (yy - top));
            }
        }
        spans_open.retain(|s| s.0 > r);
        if *head {
            bgs.push(Item::Fill {
                rect: Rect::new(x, yy, total_w, row_h),
                color: ctx.pal.table_header_bg,
                radius,
            });
            if radius > 0.0 && row_h > radius {
                bgs.push(Item::Fill {
                    rect: Rect::new(x, yy + radius, total_w, row_h - radius),
                    color: ctx.pal.table_header_bg,
                    radius: 0.0,
                });
            }
        } else {
            body += 1;
            if m.table_zebra && body.is_multiple_of(2) {
                bgs.push(Item::Fill {
                    rect: Rect::new(x, yy, total_w, row_h),
                    color: zebra,
                    radius: 0.0,
                });
            }
        }
        if r > 0 {
            // Row line, broken where a rowspan cell continues through it.
            let cov = covered.get(r).map_or(&[][..], Vec::as_slice);
            let mut c = 0;
            while c < ncols {
                if cov.get(c).copied().unwrap_or(false) {
                    c += 1;
                    continue;
                }
                let start = c;
                while c < ncols && !cov.get(c).copied().unwrap_or(false) {
                    c += 1;
                }
                let x0 = col_x[start];
                let x1 = col_x.get(c).copied().unwrap_or(x + total_w);
                lines.push(Item::Fill {
                    rect: Rect::new(x0, yy, x1 - x0, 1.0),
                    color: ctx.pal.table_border,
                    radius: 0.0,
                });
            }
        }
        yy += row_h;
    }
    let h = yy - y;
    let mut cx = x;
    for cw in widths.iter().take(ncols.saturating_sub(1)) {
        cx += cw;
        lines.push(Item::Fill {
            rect: Rect::new(cx, y, 1.0, h),
            color: ctx.pal.table_border,
            radius: 0.0,
        });
    }
    // Backgrounds go under the cell text; grid lines and the outer border on top.
    for (k, b) in bgs.into_iter().enumerate() {
        out.items.insert(bg_at + k, b);
    }
    // Inserting non-Para items does not change para indices (they index `out.paras`).
    out.items.extend(lines);
    out.items.push(Item::Stroke {
        rect: Rect::new(x, y, total_w, h),
        color: ctx.pal.table_border,
        radius,
    });
    out.hot.push(crate::block::Hot {
        rect: Rect::new(x, y, total_w, h),
        kind: crate::block::HotKind::Table {
            node: i,
            rows: u32::try_from(rows.len()).unwrap_or(u32::MAX),
            cols: u32::try_from(ncols).unwrap_or(u32::MAX),
        },
    });
    h
}

/// Grid placement (BUG-05): per row the `(column, cell, rowspan)` of each cell and the columns
/// covered by a `rowspan` from an earlier row.
#[allow(clippy::type_complexity)]
fn place(
    d: &Detail,
    rows: &[(bool, Vec<usize>)],
) -> (Vec<Vec<(usize, usize, usize)>>, Vec<Vec<bool>>) {
    let mut busy: Vec<usize> = Vec::new();
    let mut slots = Vec::with_capacity(rows.len());
    let mut covered = Vec::with_capacity(rows.len());
    for (k, (_, cs)) in rows.iter().enumerate() {
        let cov: Vec<bool> = busy.iter().map(|&b| b > 0).collect();
        let mut row = Vec::with_capacity(cs.len());
        let mut c = 0;
        for &cell in cs {
            while cov.get(c).copied().unwrap_or(false) {
                c += 1;
            }
            let rs = d.rowspan(cell).clamp(1, rows.len() - k);
            row.push((c, cell, rs));
            c += 1;
        }
        for b in &mut busy {
            *b = b.saturating_sub(1);
        }
        for &(c, _, rs) in &row {
            if rs > 1 {
                if busy.len() <= c {
                    busy.resize(c + 1, 0);
                }
                busy[c] = rs - 1;
            }
        }
        slots.push(row);
        covered.push(cov);
    }
    (slots, covered)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::block::{EnStrings, Metrics, layout_block};
    use crate::shaper::MockShaper;
    use yamdv_core::doc::{Document, ReadOpts};

    fn lay(src: &str, w: f32, m: Metrics) -> crate::block::BlockLayout {
        let doc = Document::new(src.into(), ReadOpts::default());
        let d = doc.detail_uncached(0);
        let pal = yamdv_core::theme::palette("default", false);
        let mut sh = MockShaper;
        let mut ctx = Ctx {
            images: &crate::img::NO_IMAGES,
            rich: &crate::rich::NO_RICH,
            shaper: &mut sh,
            m,
            pal: &pal,
            strings: &EnStrings,
            single_newline_break: false,
            block: 0,
            code_opts: &|_, _| crate::code::CodeOpts::default(),
        };
        layout_block(&mut ctx, src, &d, w)
    }

    #[test]
    fn r_ren_07_cells_stay_inside_columns() {
        let long = "x".repeat(400);
        let src = format!(
            "| a | b | c |\n|:-|:-:|-:|\n| **bold** `code` | {long} | 1 |\n| r | s | t |\n"
        );
        let bl = lay(&src, 600.0, Metrics::default());
        assert!(bl.paras.len() >= 9);
        for p in bl.paras.iter().filter(|p| p.shaped.width() < 1000.0) {
            assert!(
                p.x >= 0.0 && p.x + p.shaped.width() <= 600.0 + 0.5,
                "{} {}",
                p.x,
                p.shaped.width()
            );
        }
        let header = bl
            .items
            .iter()
            .any(|i| matches!(i, Item::Fill { color, .. } if *color == 0xf5f5f5));
        assert!(header, "header background");
        // Right-aligned column: the `1` ends at the column's right padding.
        let max_x = bl
            .paras
            .iter()
            .map(|p| p.x + p.shaped.width())
            .fold(0.0f32, f32::max);
        assert!(max_x <= 600.0);
    }

    #[test]
    fn zebra_only_when_enabled() {
        let src = "|a|\n|-|\n|1|\n|2|\n|3|\n";
        let count = |m: Metrics| {
            lay(src, 400.0, m)
                .items
                .iter()
                .filter(|i| matches!(i, Item::Fill { rect, radius, .. } if *radius == 0.0 && rect.h > 2.0))
                .count()
        };
        let off = count(Metrics::default());
        let on = count(Metrics {
            table_zebra: true,
            ..Metrics::default()
        });
        assert_eq!(on, off + 1, "one even body row");
    }

    #[test]
    fn s_parse_02_too_many_columns_is_a_card() {
        let n = MAX_COLS + 1;
        let src = format!("|{}\n|{}\n", "a|".repeat(n), "-|".repeat(n));
        let bl = lay(&src, 600.0, Metrics::default());
        assert!(bl.paras.len() <= 2, "source card, not cells");
    }

    #[test]
    fn bug_05_rowspan_cells_skip_covered_columns() {
        let src = "<table>\n<tr><td rowspan=\"2\">A</td><td>B</td></tr>\n<tr><td>C</td></tr>\n<tr><td>D</td><td>E</td></tr>\n</table>\n";
        let doc = Document::new(src.into(), ReadOpts::default());
        let d = doc.detail_uncached(0);
        let t = (0..d.nodes.len())
            .find(|&i| matches!(d.nodes[i].kind, NodeKind::Table { .. }))
            .unwrap();
        let rows: Vec<(bool, Vec<usize>)> = d
            .children(t)
            .filter_map(|r| match d.nodes[r].kind {
                NodeKind::TableRow { head } => Some((head, d.children(r).collect())),
                _ => None,
            })
            .collect();
        let (slots, covered) = place(&d, &rows);
        let cols: Vec<Vec<(usize, usize)>> = slots
            .iter()
            .map(|r| r.iter().map(|&(c, _, rs)| (c, rs)).collect())
            .collect();
        assert_eq!(
            cols,
            vec![vec![(0, 2), (1, 1)], vec![(1, 1)], vec![(0, 1), (1, 1)]]
        );
        assert_eq!(covered[1], vec![true]);
        assert!(covered[2].iter().all(|c| !c));
        // The laid-out `C` sits in the second column, right of `A`.
        let bl = lay(src, 600.0, Metrics::default());
        assert!(bl.paras.len() >= 5);
    }

    #[test]
    fn bug_06_per_line_alignment_keeps_the_box_in_the_column() {
        use crate::block::box_x;
        // DirectWrite aligns each wrapped line: the box spans the column.
        for a in 0..=3 {
            assert!((box_x(10.0, 200.0, 120.0, a, true) - 10.0).abs() < f32::EPSILON);
        }
        // Other shapers move the whole box (single-line cells).
        assert!((box_x(10.0, 200.0, 120.0, 2, false) - 50.0).abs() < 0.01);
        assert!((box_x(10.0, 200.0, 120.0, 3, false) - 90.0).abs() < 0.01);
        assert!((box_x(10.0, 200.0, 250.0, 3, false) - 10.0).abs() < 0.01);
        assert!((box_x(10.0, 200.0, 120.0, 1, false) - 10.0).abs() < 0.01);
    }

    #[test]
    fn fits_uses_max() {
        assert_eq!(
            distribute(&[(48.0, 60.0), (48.0, 100.0)], 500.0),
            vec![60.0, 100.0]
        );
    }

    #[test]
    fn shares_extra_by_slack() {
        let w = distribute(&[(50.0, 150.0), (50.0, 350.0)], 300.0);
        assert!(
            (w[0] - 100.0).abs() < 0.01 && (w[1] - 200.0).abs() < 0.01,
            "{w:?}"
        );
        assert!((w.iter().sum::<f32>() - 300.0).abs() < 0.01);
    }

    #[test]
    fn scales_when_minimums_overflow() {
        let w = distribute(&[(200.0, 300.0), (200.0, 300.0)], 200.0);
        assert!((w.iter().sum::<f32>() - 200.0).abs() < 0.01, "{w:?}");
        let w = distribute(&[(48.0, 48.0), (3000.0, 3000.0)], 600.0);
        assert_eq!(w, vec![48.0, 552.0]);
    }
}

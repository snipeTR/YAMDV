// SPDX-License-Identifier: AGPL-3.0-or-later
//! UI Automation text-range logic (prompt 019 §4): unit boundaries and endpoint moves over the
//! document's reading text (UTF-16), used by the read-only TextPattern of the Document
//! element. Pure; the platform layer wraps it in `ITextRangeProvider`.

/// `TextUnit` (UIA order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// `TextUnit_Character`.
    Character,
    /// `TextUnit_Format` (no formatting runs: treated as the whole document).
    Format,
    /// `TextUnit_Word`.
    Word,
    /// `TextUnit_Line` (reading-text lines).
    Line,
    /// `TextUnit_Paragraph` (same as lines: one block per line in the reading text).
    Paragraph,
    /// `TextUnit_Page` (the whole document).
    Page,
    /// `TextUnit_Document`.
    Document,
}

impl Unit {
    /// From the UIA `TextUnit` value (unknown → Document).
    pub fn from_uia(v: i32) -> Self {
        match v {
            0 => Self::Character,
            1 => Self::Format,
            2 => Self::Word,
            3 => Self::Line,
            4 => Self::Paragraph,
            5 => Self::Page,
            _ => Self::Document,
        }
    }
}

fn ws(c: u16) -> bool {
    matches!(c, 0x20 | 0x09 | 0x0A | 0x0D | 0xA0 | 0x3000)
}

fn low_surrogate(c: u16) -> bool {
    (0xDC00..0xE000).contains(&c)
}

/// Position `i` (0..=len) is a boundary of `unit`.
pub fn is_boundary(t: &[u16], i: usize, unit: Unit) -> bool {
    let n = t.len();
    if i == 0 || i >= n {
        return true;
    }
    let (Some(&prev), Some(&cur)) = (t.get(i - 1), t.get(i)) else {
        return true;
    };
    match unit {
        Unit::Character => !low_surrogate(cur),
        Unit::Word => ws(prev) && !ws(cur),
        Unit::Line | Unit::Paragraph => prev == 0x0A,
        Unit::Format | Unit::Page | Unit::Document => false,
    }
}

/// The unit enclosing `pos`: `(start, end)` with `start <= pos < end` (or `(len, len)` at the
/// end of a non-empty text: the last unit).
pub fn enclosing(t: &[u16], pos: usize, unit: Unit) -> (usize, usize) {
    let n = t.len();
    let mut s = pos.min(n);
    if s == n && n > 0 {
        s -= 1;
    }
    while s > 0 && !is_boundary(t, s, unit) {
        s -= 1;
    }
    let mut e = (s + 1).min(n);
    while e < n && !is_boundary(t, e, unit) {
        e += 1;
    }
    (s, e)
}

/// Moves `pos` across `count` unit boundaries (negative = backwards); returns the new position
/// and how many boundaries were crossed.
pub fn move_by(t: &[u16], pos: usize, unit: Unit, count: i32) -> (usize, i32) {
    let n = t.len();
    let mut p = pos.min(n);
    let mut moved = 0;
    while moved != count {
        if count > 0 {
            if p >= n {
                break;
            }
            p += 1;
            while p < n && !is_boundary(t, p, unit) {
                p += 1;
            }
            moved += 1;
        } else {
            if p == 0 {
                break;
            }
            p -= 1;
            while p > 0 && !is_boundary(t, p, unit) {
                p -= 1;
            }
            moved -= 1;
        }
    }
    (p, moved)
}

/// `ITextRangeProvider::Move`: the range becomes the unit `count` units away from the unit at
/// its start (a degenerate range stays degenerate). Returns the new range and units moved.
pub fn move_range(t: &[u16], r: (usize, usize), unit: Unit, count: i32) -> ((usize, usize), i32) {
    let degenerate = r.0 == r.1;
    let (s, _) = enclosing(t, r.0, unit);
    let (mut p, mut moved) = move_by(t, s, unit, count);
    // A forward move that ends at the end of the text has no unit to start there.
    if count > 0 && p >= t.len() && !t.is_empty() && !degenerate {
        moved -= 1;
        p = enclosing(t, t.len(), unit).0;
    }
    if degenerate {
        return ((p, p), moved);
    }
    (enclosing(t, p, unit), moved)
}

/// `ITextRangeProvider::MoveEndpointByUnit`: moves one endpoint; the other follows when they
/// would cross. Returns the new range and units moved.
pub fn move_endpoint(
    t: &[u16],
    r: (usize, usize),
    start: bool,
    unit: Unit,
    count: i32,
) -> ((usize, usize), i32) {
    let from = if start { r.0 } else { r.1 };
    let (p, moved) = move_by(t, from, unit, count);
    let out = if start {
        (p, r.1.max(p))
    } else {
        (r.0.min(p), p)
    };
    (out, moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn uia_text_units() {
        let t = u("Hello big world\nLine two\n");
        assert_eq!(enclosing(&t, 7, Unit::Word), (6, 10));
        assert_eq!(enclosing(&t, 3, Unit::Line), (0, 16));
        assert_eq!(enclosing(&t, 17, Unit::Paragraph), (16, 25));
        assert_eq!(enclosing(&t, 5, Unit::Document), (0, t.len()));
        assert_eq!(enclosing(&t, 2, Unit::Character), (2, 3));
        assert_eq!(move_by(&t, 0, Unit::Word, 2), (10, 2));
        assert_eq!(move_by(&t, 10, Unit::Word, -5), (0, -2));
        assert_eq!(move_by(&t, 0, Unit::Line, 9), (t.len(), 2));
        // Move: the next word / back to the first line.
        assert_eq!(move_range(&t, (0, 5), Unit::Word, 1), ((6, 10), 1));
        assert_eq!(move_range(&t, (16, 20), Unit::Line, -1), ((0, 16), -1));
        assert_eq!(move_range(&t, (16, 25), Unit::Line, 3).1, 0);
        // Endpoint moves; crossing collapses.
        assert_eq!(move_endpoint(&t, (0, 5), false, Unit::Word, 1), ((0, 6), 1));
        assert_eq!(
            move_endpoint(&t, (0, 5), true, Unit::Line, 1),
            ((16, 16), 1)
        );
        // Surrogate pairs stay whole.
        let e = u("a😀b");
        assert_eq!(enclosing(&e, 1, Unit::Character), (1, 3));
        assert_eq!(move_by(&e, 1, Unit::Character, 1), (3, 1));
        assert_eq!(enclosing(&[], 0, Unit::Word), (0, 0));
        assert_eq!(Unit::from_uia(2), Unit::Word);
        assert_eq!(Unit::from_uia(42), Unit::Document);
    }
}

// SPDX-License-Identifier: AGPL-3.0-or-later
//! Document-order scope of math macros (R-MATH-07): a definition applies to every later
//! formula — the later formulas of its own block and every formula of every later block
//! (F-34; a formula before the definition in the same block does not see it).

use std::hash::{Hash, Hasher};
use std::rc::Rc;

use yamdv_core::doc::index::Index;
use yamdv_math::macros::{Macros, expand};

/// Macro environments per top-level block.
#[derive(Debug, Default)]
pub struct MacroScopes {
    key: u64,
    /// (first block, environment from that block on), ascending.
    snaps: Vec<(usize, Rc<Macros>)>,
    /// Inside a defining block: (block, trimmed formula hash) → (id, environment before that
    /// formula); the first occurrence of a formula text wins.
    inner: std::collections::HashMap<(usize, u64), (u64, Rc<Macros>)>,
}

impl MacroScopes {
    /// Identity of the defining blocks (indices and text) this scope set was built from.
    pub fn key(&self) -> u64 {
        self.key
    }

    /// Builds the scopes of `text` with index `ix`.
    pub fn build(text: &str, ix: &Index) -> Self {
        let key = scope_key(text, ix);
        let mut env = Macros::default();
        let mut snaps = Vec::new();
        let mut inner = std::collections::HashMap::new();
        for &b in &ix.macro_blocks {
            let Some(blk) = ix.blocks.get(b as usize) else {
                continue;
            };
            let s = text
                .get(blk.range.start as usize..blk.range.end as usize)
                .unwrap_or("");
            let before = env.clone();
            for (n, f) in math_spans(s).into_iter().enumerate() {
                let id =
                    key ^ (((b as u64) << 16 | n as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
                inner
                    .entry((b as usize, formula_hash(f)))
                    .or_insert_with(|| (id, Rc::new(env.clone())));
                // A failing formula keeps the definitions made before its error.
                let mut trial = env.clone();
                if expand(f, &mut trial).is_ok() {
                    env = trial;
                }
            }
            if env != before {
                snaps.push((b as usize, Rc::new(env.clone())));
            }
        }
        Self { key, snaps, inner }
    }

    /// The environment of formula `tex` in top-level block `b` and its id (0 = no macros):
    /// in a defining block, only the definitions made before the formula apply.
    pub fn for_formula(&self, b: usize, tex: &str) -> (u64, Option<Rc<Macros>>) {
        match self.inner.get(&(b, formula_hash(tex))) {
            Some((_, m)) if m.is_empty() => (0, None),
            Some((id, m)) => (*id, Some(m.clone())),
            None => self.for_block(b),
        }
    }

    /// The environment of top-level block `b` and its id (0 = no macros).
    pub fn for_block(&self, b: usize) -> (u64, Option<Rc<Macros>>) {
        let i = self.snaps.partition_point(|s| s.0 <= b);
        match i.checked_sub(1).and_then(|i| self.snaps.get(i)) {
            Some((_, m)) if !m.is_empty() => (self.key ^ (i as u64 + 1), Some(m.clone())),
            _ => (0, None),
        }
    }
}

/// Hash of a formula's trimmed source (the lookup key of [`MacroScopes::for_formula`]).
fn formula_hash(tex: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tex.trim().hash(&mut h);
    h.finish()
}

/// Hash of the defining blocks of `text` (cheap: only those blocks are read).
pub fn scope_key(text: &str, ix: &Index) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for &b in &ix.macro_blocks {
        b.hash(&mut h);
        if let Some(blk) = ix.blocks.get(b as usize) {
            text.get(blk.range.start as usize..blk.range.end as usize)
                .hash(&mut h);
        }
    }
    h.finish() | 1
}

/// The formulas of a block's source: `$$…$$`, `$…$` and a fenced `math` block's body.
pub fn math_spans(s: &str) -> Vec<&str> {
    let t = s.trim_start();
    for fence in ["```", "~~~"] {
        if let Some(rest) = t.strip_prefix(fence) {
            let info = rest.trim_start_matches(fence.chars().next().unwrap_or('`'));
            if info.trim_start().to_ascii_lowercase().starts_with("math") {
                let body = info.split_once('\n').map_or("", |x| x.1);
                let end = body.rfind(fence).unwrap_or(body.len());
                return vec![body.get(..end).unwrap_or(body)];
            }
            return Vec::new();
        }
    }
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'$' => {
                let dd = b.get(i + 1) == Some(&b'$');
                let open = if dd { i + 2 } else { i + 1 };
                let close = if dd {
                    s.get(open..).and_then(|r| r.find("$$"))
                } else {
                    find_dollar(b, open)
                };
                match close {
                    Some(c) => {
                        let end = if dd { open + c } else { c };
                        if let Some(f) = s.get(open..end) {
                            out.push(f);
                        }
                        i = end + if dd { 2 } else { 1 };
                    }
                    None => break,
                }
            }
            _ => i += 1,
        }
    }
    out
}

fn find_dollar(b: &[u8], from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(&c) = b.get(i) {
        match c {
            b'\\' => i += 2,
            b'$' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use yamdv_core::doc::{Document, ReadOpts};

    #[test]
    fn f_34_definitions_reach_later_blocks_only() {
        let md = "$\\A$\n\nlater $\\A$ first, then $\\newcommand{\\A}{a}\\A$\n\n$$\\A + 1$$\n\n- item $\\A$\n";
        let d = Document::new(md.to_string(), ReadOpts::default());
        let s = MacroScopes::build(&d.source, &d.index);
        assert_eq!(d.index.macro_blocks, vec![1]);
        assert!(s.for_block(0).1.is_none(), "before the defining block");
        for b in 2..d.index.blocks.len() {
            let (id, m) = s.for_block(b);
            assert!(id != 0 && m.unwrap().get("A").is_some(), "block {b}");
        }
    }

    #[test]
    fn scopes_follow_document_order() {
        let md = "$\\RR$\n\n$\\newcommand{\\RR}{\\mathbb R}\\RR$ and `x`\n\n$$\\def\\v#1{\\vec{#1}}$$\n\ntext $\\RR \\v x$\n\n```math\n\\renewcommand{\\RR}{R}\n```\n\nend\n";
        let d = Document::new(md.to_string(), ReadOpts::default());
        assert_eq!(d.index.macro_blocks, vec![1, 2, 4]);
        let s = MacroScopes::build(&d.source, &d.index);
        assert_eq!(s.for_block(0), (0, None));
        let (id1, m1) = s.for_block(1);
        assert!(id1 != 0 && m1.unwrap().get("RR").is_some());
        let (_, m3) = s.for_block(3);
        let m3 = m3.unwrap();
        assert!(m3.get("v").is_some_and(|m| m.params == 1));
        assert_eq!(s.for_block(5).1.unwrap().get("RR").unwrap().body, "R");
        assert_eq!(math_spans(r"a \$ $x$ $$y$$ $z"), vec!["x", "y"]);
    }
}

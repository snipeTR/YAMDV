// SPDX-License-Identifier: AGPL-3.0-or-later
//! Visual flowchart editing (IDEA-05, R-F2-11): source rewrites for a node or edge picked in the
//! rendered diagram. Every operation returns the whole new diagram source; the caller applies
//! the difference through the editor (one undo step). Pure: no layout, no platform.
//!
//! Supported: `flowchart` / `graph`. Other diagram types are edited as source.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use crate::flow::{self, End, Flow, Line, OpSpan, Rec, RefSpan, StKind, Stmt};

/// A parsed flowchart with the source positions needed to rewrite it.
#[derive(Debug, Clone)]
pub struct FlowEdit {
    src: String,
    flow: Flow,
    rec: Rec,
}

/// What one chain statement keeps (see [`FlowEdit::chain_pieces`]).
struct Kept {
    pieces: Vec<String>,
    /// Nodes still written in `pieces`.
    alive: HashSet<usize>,
    /// Other live references: (node, text, carries shape text or a class).
    lone: Vec<(usize, String, bool)>,
}

/// Text replacements applied back to front.
type Edits = Vec<(Range<usize>, String)>;

fn apply(src: &str, mut edits: Edits) -> String {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut out = src.to_string();
    let mut limit = out.len();
    for (r, t) in edits {
        // Overlapping edits cannot happen by construction; skip one defensively.
        if r.end > limit || r.start > r.end || !out.is_char_boundary(r.start) {
            continue;
        }
        out.replace_range(r.clone(), &t);
        limit = r.start;
    }
    out
}

/// The text of a label between shape delimiters / pipes: quoted when it holds characters the
/// parser treats as syntax; a line break becomes `<br>`, `"` becomes `#quot;`.
pub fn encode_label(text: &str) -> String {
    let one = text.replace("\r\n", "\n").replace('\r', "\n");
    let special = one.trim() != one
        || one.chars().any(|c| {
            matches!(
                c,
                '[' | ']'
                    | '('
                    | ')'
                    | '{'
                    | '}'
                    | '|'
                    | '"'
                    | ';'
                    | '&'
                    | ':'
                    | '<'
                    | '>'
                    | '/'
                    | '\\'
                    | '#'
                    | '%'
                    | '-'
                    | '='
                    | '.'
            )
        });
    // B3 (prompt 022): `"` becomes the entity `#quot;` (exact round trip); a `#` that would
    // read as an entity, and a `<` that would read as `<br>`, are escaped the same way.
    let mut body = String::with_capacity(one.len() + 8);
    for (i, c) in one.char_indices() {
        let tail = one.get(i..).unwrap_or("");
        match c {
            '"' => body.push_str("#quot;"),
            '#' if crate::entity_at(tail).is_some() => body.push_str("#35;"),
            '<' if tail.get(..3).is_some_and(|t| t.eq_ignore_ascii_case("<br")) => {
                body.push_str("#lt;");
            }
            '\n' => body.push_str("<br>"),
            c => body.push(c),
        }
    }
    if special { format!("\"{body}\"") } else { body }
}

fn tail_text(e: End) -> &'static str {
    match e {
        End::Arrow => "<",
        End::Circle => "o",
        End::Cross => "x",
        _ => "",
    }
}

/// A plain operator for an edge (no label), e.g. `-->`, `-.->`, `==o`, `<-->`.
pub fn op_text(line: Line, head: End, tail: End, len: u8) -> String {
    let n = usize::from(len.clamp(1, 8));
    let mark = match head {
        End::Arrow => ">",
        End::Circle => "o",
        End::Cross => "x",
        _ => "",
    };
    let body = match line {
        Line::Dotted => {
            let dots = ".".repeat(n);
            if mark.is_empty() {
                format!("-{dots}-")
            } else if mark == ">" {
                format!("-{dots}->")
            } else {
                format!("-{dots}-{mark}")
            }
        }
        Line::Solid | Line::Thick => {
            let c = if line == Line::Solid { "-" } else { "=" };
            if mark.is_empty() {
                c.repeat(n + 2)
            } else {
                format!("{}{mark}", c.repeat(n + 1))
            }
        }
    };
    format!("{}{body}", tail_text(tail))
}

impl FlowEdit {
    /// Parses `src` (a ```` ```mermaid ```` body); `None` unless it is a valid flowchart.
    pub fn new(src: &str) -> Option<Self> {
        let first = crate::lines(src).next()?.1;
        let kw = first.split_whitespace().next().unwrap_or("");
        if !matches!(kw, "flowchart" | "graph" | "flowchart-elk") {
            return None;
        }
        let (flow, rec) = flow::parse_rec(src).ok()?;
        Some(Self {
            src: src.to_string(),
            flow,
            rec,
        })
    }

    /// The parsed flowchart.
    pub fn flow(&self) -> &Flow {
        &self.flow
    }

    /// Index of the node with id `id`.
    pub fn node_index(&self, id: &str) -> Option<usize> {
        self.flow.nodes.iter().position(|n| n.id == id)
    }

    fn refs(&self) -> impl Iterator<Item = (usize, &RefSpan)> {
        self.rec.stmts.iter().enumerate().flat_map(|(si, st)| {
            let groups: &[Vec<RefSpan>] = match &st.kind {
                StKind::Chain { groups, .. } => groups,
                _ => &[],
            };
            groups.iter().flatten().map(move |r| (si, r))
        })
    }

    fn op_of(&self, edge: usize) -> Option<(&Stmt, usize, &OpSpan)> {
        self.rec.stmts.iter().find_map(|st| match &st.kind {
            StKind::Chain { groups, ops } => ops.iter().enumerate().find_map(|(k, o)| {
                let n = groups.get(k)?.len() * groups.get(k + 1)?.len();
                (o.first_edge..o.first_edge + n)
                    .contains(&edge)
                    .then_some((st, k, o))
            }),
            _ => None,
        })
    }

    /// Indentation for new statements: that of the first statement line, else four spaces.
    fn indent(&self) -> String {
        self.rec
            .stmts
            .first()
            .and_then(|st| self.src.get(st.line.clone()))
            .map_or_else(
                || "    ".to_string(),
                |l| l[..l.len() - l.trim_start().len()].to_string(),
            )
    }

    /// Appends `stmt` as a new last line.
    fn append(&self, stmt: &str) -> String {
        let end = self.src.trim_end().len();
        let mut out = self.src.clone();
        out.insert_str(end, &format!("\n{}{stmt}", self.indent()));
        out
    }

    /// Sets node `i`'s label (empty: back to its id).
    pub fn set_node_label(&self, i: usize, text: &str) -> Option<String> {
        let node = self.flow.nodes.get(i)?;
        let text = if text.trim().is_empty() {
            node.id.as_str()
        } else {
            text
        };
        let enc = encode_label(text);
        // The parser keeps the last label given; rewrite that one.
        let last = self
            .refs()
            .filter(|(_, r)| r.node == i)
            .filter_map(|(_, r)| r.label.clone())
            .last();
        if let Some(r) = last {
            return Some(apply(&self.src, vec![(r, enc)]));
        }
        if let Some((_, r)) = self.refs().find(|(_, r)| r.node == i) {
            return Some(apply(
                &self.src,
                vec![(r.id.end..r.id.end, format!("[{enc}]"))],
            ));
        }
        Some(self.append(&format!("{}[{enc}]", node.id)))
    }

    /// Sets edge `e`'s label (empty removes it). B3 (prompt 022): when the operator draws
    /// several edges (`A & B --> C`), edge `e` is first split into its own statement
    /// (`A -->|text| C`, appended last) so the other edges keep their label.
    pub fn set_edge_label(&self, e: usize, text: &str) -> Option<String> {
        let edge = self.flow.edges.get(e)?;
        let (st, k, op) = self.op_of(e)?;
        let text = text.trim();
        let plain = op_text(edge.line, edge.head, edge.tail, edge.len);
        let piped = |t: &str| {
            let enc = encode_label(t);
            format!("|{enc}|")
        };
        let shared = match &st.kind {
            StKind::Chain { groups, .. } => {
                groups.get(k).map_or(0, Vec::len) * groups.get(k + 1).map_or(0, Vec::len) > 1
            }
            _ => false,
        };
        if shared {
            if edge.label.as_deref().unwrap_or("") == text {
                return Some(self.src.clone());
            }
            let (a, b) = (
                self.flow.nodes.get(edge.from)?,
                self.flow.nodes.get(edge.to)?,
            );
            let lab = if text.is_empty() {
                String::new()
            } else {
                piped(text)
            };
            let rest = FlowEdit::new(&self.remove(&HashSet::new(), &HashSet::from([e])))?;
            return Some(rest.append(&format!("{} {plain}{lab} {}", a.id, b.id)));
        }
        let edit = match &op.label {
            Some((r, true)) if !text.is_empty() => (r.clone(), encode_label(text)),
            Some(_) if text.is_empty() => (op.full.clone(), plain),
            Some(_) => (op.full.clone(), format!("{plain}{}", piped(text))),
            None if text.is_empty() => return Some(self.src.clone()),
            None => (op.full.end..op.full.end, piped(text)),
        };
        Some(apply(&self.src, vec![edit]))
    }

    /// First unused id `n1`, `n2`, ….
    fn fresh_id(&self) -> String {
        let used: HashSet<&str> = self
            .flow
            .nodes
            .iter()
            .map(|n| n.id.as_str())
            .chain(self.flow.subs.iter().map(|s| s.id.as_str()))
            .collect();
        (1..)
            .map(|k| format!("n{k}"))
            .find(|id| !used.contains(id.as_str()))
            .unwrap_or_default()
    }

    /// Adds a node labelled `label`, connected from node `from` when given. Returns the new
    /// source and the new node's id.
    pub fn add_node(&self, from: Option<usize>, label: &str) -> Option<(String, String)> {
        let id = self.fresh_id();
        let enc = encode_label(if label.trim().is_empty() { &id } else { label });
        let stmt = match from {
            Some(a) => format!("{} --> {id}[{enc}]", self.flow.nodes.get(a)?.id),
            None => format!("{id}[{enc}]"),
        };
        Some((self.append(&stmt), id))
    }

    /// Adds an edge `a --> b` (a new last statement, so it is the last edge).
    pub fn add_edge(&self, a: usize, b: usize) -> Option<String> {
        let (na, nb) = (self.flow.nodes.get(a)?, self.flow.nodes.get(b)?);
        Some(self.append(&format!("{} --> {}", na.id, nb.id)))
    }

    /// Deletes edge `e` (the nodes stay).
    pub fn delete_edge(&self, e: usize) -> Option<String> {
        self.flow.edges.get(e)?;
        Some(self.remove(&HashSet::new(), &HashSet::from([e])))
    }

    /// Deletes node `i`, its edges, and its `class` / `style` / `click` mentions.
    pub fn delete_node(&self, i: usize) -> Option<String> {
        self.flow.nodes.get(i)?;
        let edges: HashSet<usize> = self
            .flow
            .edges
            .iter()
            .enumerate()
            .filter(|(_, e)| e.from == i || e.to == i)
            .map(|(k, _)| k)
            .collect();
        Some(self.remove(&HashSet::from([i]), &edges))
    }

    /// Removing a statement: its whole line when alone on it, else the statement and a `;`.
    fn drop_stmt(&self, st: &Stmt) -> (Range<usize>, String) {
        let b = self.src.as_bytes();
        if st.alone {
            let end = st.line.end;
            if b.get(end) == Some(&b'\r') && b.get(end + 1) == Some(&b'\n') {
                return (st.line.start..end + 2, String::new());
            }
            if b.get(end) == Some(&b'\n') {
                return (st.line.start..end + 1, String::new());
            }
            // Last line: take the line break before it.
            let mut s = st.line.start;
            if s > 0 && b.get(s - 1) == Some(&b'\n') {
                s -= 1;
                if s > 0 && b.get(s - 1) == Some(&b'\r') {
                    s -= 1;
                }
            }
            return (s..end, String::new());
        }
        if b.get(st.range.end) == Some(&b';') {
            (st.range.start..st.range.end + 1, String::new())
        } else if st.range.start > 0 && b.get(st.range.start - 1) == Some(&b';') {
            (st.range.start - 1..st.range.end, String::new())
        } else {
            (st.range.clone(), String::new())
        }
    }

    fn lead(&self, st: &Stmt) -> String {
        let t = self.src.get(st.range.clone()).unwrap_or("");
        t[..t.len() - t.trim_start().len()].to_string()
    }

    fn text(&self, r: &Range<usize>) -> &str {
        self.src.get(r.clone()).unwrap_or("")
    }

    /// Rewrites every statement so that `dead_nodes` and `dead_edges` disappear while the
    /// remaining edges keep their order (so `linkStyle` indices are renumbered, not lost).
    fn remove(&self, dead_nodes: &HashSet<usize>, dead_edges: &HashSet<usize>) -> String {
        let kept_edge =
            |e: usize| {
                !dead_edges.contains(&e)
                    && self.flow.edges.get(e).is_some_and(|x| {
                        !dead_nodes.contains(&x.from) && !dead_nodes.contains(&x.to)
                    })
            };
        // New index of each kept edge.
        let mut renum = HashMap::new();
        let mut next = 0usize;
        for e in 0..self.flow.edges.len() {
            if kept_edge(e) {
                renum.insert(e, next);
                next += 1;
            }
        }
        let ids: HashMap<&str, usize> = self
            .flow
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        // Pass 1: what each touched chain keeps, and which nodes stay mentioned anywhere.
        let mut alive: HashSet<usize> = HashSet::new();
        let mut chains: HashMap<usize, Kept> = HashMap::new();
        for (si, st) in self.rec.stmts.iter().enumerate() {
            match &st.kind {
                StKind::Chain { groups, ops } => {
                    let touched = groups
                        .iter()
                        .flatten()
                        .any(|r| dead_nodes.contains(&r.node))
                        || ops.iter().enumerate().any(|(k, o)| {
                            let n = groups.get(k).map_or(0, Vec::len)
                                * groups.get(k + 1).map_or(0, Vec::len);
                            (o.first_edge..o.first_edge + n).any(|e| !kept_edge(e))
                        });
                    if touched {
                        let kept = self.chain_pieces(groups, ops, dead_nodes, &kept_edge);
                        alive.extend(kept.alive.iter().copied());
                        chains.insert(si, kept);
                    } else {
                        alive.extend(groups.iter().flatten().map(|r| r.node));
                    }
                }
                StKind::Class { ids: list, .. } => {
                    alive.extend(list.iter().filter_map(|id| ids.get(id.as_str())));
                }
                StKind::Of { id } => alive.extend(ids.get(id.as_str())),
                _ => {}
            }
        }
        alive.retain(|n| !dead_nodes.contains(n));
        // Pass 2: the edits.
        let mut edits: Edits = Vec::new();
        for (si, st) in self.rec.stmts.iter().enumerate() {
            match &st.kind {
                StKind::Chain { .. } => {
                    let Some(kept) = chains.remove(&si) else {
                        continue;
                    };
                    let mut pieces = kept.pieces;
                    for (node, text, decorated) in kept.lone {
                        // A lone node is written out when it carries its shape text, or when
                        // nothing else would keep it in the diagram.
                        if decorated || alive.insert(node) {
                            pieces.push(text);
                        }
                    }
                    if pieces.is_empty() {
                        edits.push(self.drop_stmt(st));
                    } else {
                        let lead = self.lead(st);
                        let sep = if st.alone {
                            let l = self.text(&st.line);
                            format!("\n{}", &l[..l.len() - l.trim_start().len()])
                        } else {
                            "; ".to_string()
                        };
                        edits.push((st.range.clone(), format!("{lead}{}", pieces.join(&sep))));
                    }
                }
                StKind::Class { ids: list, class } => {
                    let left: Vec<&String> = list
                        .iter()
                        .filter(|id| ids.get(id.as_str()).is_none_or(|i| !dead_nodes.contains(i)))
                        .collect();
                    if left.len() == list.len() {
                        continue;
                    }
                    if left.is_empty() {
                        edits.push(self.drop_stmt(st));
                    } else {
                        let l: Vec<&str> = left.iter().map(|s| s.as_str()).collect();
                        edits.push((
                            st.range.clone(),
                            format!("{}class {} {class}", self.lead(st), l.join(",")),
                        ));
                    }
                }
                StKind::Of { id } => {
                    if ids.get(id.as_str()).is_some_and(|i| dead_nodes.contains(i)) {
                        edits.push(self.drop_stmt(st));
                    }
                }
                StKind::LinkStyle { which, rest } => {
                    let new: Vec<String> = which
                        .iter()
                        .filter_map(|k| renum.get(k).map(usize::to_string))
                        .collect();
                    if new.len() == which.len() && which.iter().all(|k| renum.get(k) == Some(k)) {
                        continue;
                    }
                    if new.is_empty() {
                        edits.push(self.drop_stmt(st));
                    } else {
                        edits.push((
                            st.range.clone(),
                            format!("{}linkStyle {} {rest}", self.lead(st), new.join(",")),
                        ));
                    }
                }
                StKind::Other => {}
            }
        }
        apply(&self.src, edits)
    }

    /// What is left of one chain statement: runs of whole operators as they were written,
    /// single kept edges of a partly removed operator, and lone nodes that would otherwise
    /// lose their shape text or vanish.
    fn chain_pieces(
        &self,
        groups: &[Vec<RefSpan>],
        ops: &[OpSpan],
        dead_nodes: &HashSet<usize>,
        kept_edge: &dyn Fn(usize) -> bool,
    ) -> Kept {
        let mut pieces = Vec::new();
        let mut covered: HashSet<(usize, usize)> = HashSet::new();
        let span = |g: &[RefSpan]| -> Range<usize> {
            g.first().map_or(0, |r| r.full.start)..g.last().map_or(0, |r| r.full.end)
        };
        let mut run: Option<usize> = None;
        let close = |run: &mut Option<usize>,
                     upto: usize,
                     pieces: &mut Vec<String>,
                     covered: &mut HashSet<(usize, usize)>| {
            if let Some(s) = run.take() {
                let (a, b) = (
                    groups.get(s).map_or(0..0, |g| span(g)),
                    groups.get(upto).map_or(0..0, |g| span(g)),
                );
                pieces.push(self.text(&(a.start..b.end)).to_string());
                for g in s..=upto {
                    for k in 0..groups.get(g).map_or(0, Vec::len) {
                        covered.insert((g, k));
                    }
                }
            }
        };
        for (k, o) in ops.iter().enumerate() {
            let (l, r) = (
                groups.get(k).map_or(&[][..], Vec::as_slice),
                groups.get(k + 1).map_or(&[][..], Vec::as_slice),
            );
            let whole = (0..l.len() * r.len()).all(|j| kept_edge(o.first_edge + j));
            if whole && !l.is_empty() && !r.is_empty() {
                if run.is_none() {
                    run = Some(k);
                }
                continue;
            }
            close(&mut run, k, &mut pieces, &mut covered);
            let op = self.text(&o.full).to_string();
            for (ia, ra) in l.iter().enumerate() {
                for (ib, rb) in r.iter().enumerate() {
                    if kept_edge(o.first_edge + ia * r.len() + ib) {
                        pieces.push(format!(
                            "{} {op} {}",
                            self.text(&ra.full),
                            self.text(&rb.full)
                        ));
                        covered.insert((k, ia));
                        covered.insert((k + 1, ib));
                    }
                }
            }
        }
        close(&mut run, ops.len(), &mut pieces, &mut covered);
        let alive: HashSet<usize> = covered
            .iter()
            .filter_map(|&(g, k)| groups.get(g)?.get(k).map(|r| r.node))
            .collect();
        let lone = groups
            .iter()
            .enumerate()
            .flat_map(|(g, grp)| grp.iter().enumerate().map(move |(k, r)| (g, k, r)))
            .filter(|(g, k, r)| !covered.contains(&(*g, *k)) && !dead_nodes.contains(&r.node))
            .map(|(_, _, r)| (r.node, self.text(&r.full).to_string(), r.full != r.id))
            .collect();
        Kept {
            pieces,
            alive,
            lone,
        }
    }
}

/// The one-line label editor shown over a selected node or edge (IDEA-05): the text, a byte
/// caret on a char boundary, and a whole-text selection (typing replaces it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelInput {
    /// Current text.
    pub text: String,
    /// Caret (byte offset, char boundary).
    pub caret: usize,
    /// The whole text is selected.
    pub all: bool,
}

impl LabelInput {
    /// Starts on `text` with everything selected.
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            caret: text.len(),
            all: !text.is_empty(),
        }
    }

    fn take_all(&mut self) -> bool {
        if self.all {
            self.text.clear();
            self.caret = 0;
            self.all = false;
            return true;
        }
        false
    }

    fn prev(&self) -> usize {
        self.text
            .get(..self.caret)
            .and_then(|s| s.char_indices().next_back())
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text
            .get(self.caret..)
            .and_then(|s| s.chars().next())
            .map_or(self.caret, |c| self.caret + c.len_utf8())
    }

    /// Types `s` (control characters dropped).
    pub fn insert(&mut self, s: &str) {
        let s: String = s.chars().filter(|c| !c.is_control()).collect();
        if s.is_empty() {
            return;
        }
        self.take_all();
        self.text.insert_str(self.caret, &s);
        self.caret += s.len();
    }

    /// Backspace.
    pub fn backspace(&mut self) {
        if self.take_all() || self.caret == 0 {
            return;
        }
        let p = self.prev();
        self.text.replace_range(p..self.caret, "");
        self.caret = p;
    }

    /// Delete.
    pub fn delete(&mut self) {
        if self.take_all() {
            return;
        }
        let n = self.next();
        self.text.replace_range(self.caret..n, "");
    }

    /// Caret one character left (collapses the selection to the start).
    pub fn left(&mut self) {
        if std::mem::take(&mut self.all) {
            self.caret = 0;
        } else {
            self.caret = self.prev();
        }
    }

    /// Caret one character right (collapses the selection to the end).
    pub fn right(&mut self) {
        if std::mem::take(&mut self.all) {
            self.caret = self.text.len();
        } else {
            self.caret = self.next();
        }
    }

    /// Caret to the start.
    pub fn home(&mut self) {
        self.all = false;
        self.caret = 0;
    }

    /// Caret to the end.
    pub fn end(&mut self) {
        self.all = false;
        self.caret = self.text.len();
    }

    /// Selects everything.
    pub fn select_all(&mut self) {
        self.all = !self.text.is_empty();
        self.caret = self.text.len();
    }
}

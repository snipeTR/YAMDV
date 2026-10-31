// SPDX-License-Identifier: AGPL-3.0-or-later
//! IDEA-05 / R-F2-11 (ADR-0027): visual editing of a rendered Mermaid flowchart in Live (F2)
//! and Split (F3) Edit. Click a node or an edge to select it; double-click, Enter or typing
//! edits its label in place; Delete removes it; Tab adds a connected node; dragging a node
//! onto another one connects them. Every change is a rewrite of the fence body by
//! [`yamdv_mermaid::edit::FlowEdit`], applied to the buffer as ONE minimal splice (one undo
//! step). Read mode is unchanged; other diagram types stay source-edited.

use super::*;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use yamdv_core::doc::detail::NodeKind;
use yamdv_core::doc::text::node_text;
use yamdv_layout::diagram::{FlowHits, flow_hits};
use yamdv_mermaid::edit::{FlowEdit, LabelInput};

use super::edit::Mode;

/// What is selected in the diagram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    Node(usize),
    Edge(usize),
}

/// A node drag (connect on drop).
#[derive(Debug, Clone, Copy)]
pub(super) struct Connect {
    from: usize,
    at: (f32, f32),
    moved: bool,
}

/// The selected diagram element of the edited document.
#[derive(Debug, Clone)]
pub(super) struct MmdSel {
    /// Buffer offset of the fence body.
    start: usize,
    /// The fence body when selected / last rewritten (validates against the buffer).
    text: String,
    block: usize,
    node: usize,
    target: Target,
    input: Option<LabelInput>,
    drag: Option<Connect>,
    hi: Option<u16>,
    /// B3 (prompt 022): (body before, body after) a Tab-added node whose label editor is
    /// open; committing the rename folds both into one undo step.
    added: Option<(String, String)>,
}

/// Default label of a node added with Tab.
const NEW_LABEL: &str = "New node";

impl Win {
    fn mmd(&self) -> Option<&MmdSel> {
        self.edit().and_then(|e| e.mmd.as_ref())
    }

    fn mmd_mut(&mut self) -> Option<&mut MmdSel> {
        self.edit_mut().and_then(|e| e.mmd.as_mut())
    }

    /// A diagram element is selected (keys and characters go to it first).
    pub(super) fn mmd_active(&self) -> bool {
        self.mmd().is_some()
    }

    /// The label editor is open.
    pub(super) fn mmd_editing(&self) -> bool {
        self.mmd().is_some_and(|m| m.input.is_some())
    }

    /// Driver / UIA: `node:N`, `edge:N` (`+edit` while the label editor is open) or `none`.
    #[cfg_attr(not(feature = "dev-tools"), allow(dead_code))]
    pub(super) fn mmd_state(&self) -> String {
        match self.mmd() {
            None => "none".into(),
            Some(m) => {
                let t = match m.target {
                    Target::Node(i) => format!("node:{i}"),
                    Target::Edge(i) => format!("edge:{i}"),
                };
                if m.input.is_some() {
                    format!("{t}+edit")
                } else {
                    t
                }
            }
        }
    }

    pub(super) fn mmd_clear(&mut self) {
        let had = self
            .edit_mut()
            .and_then(|e| e.mmd.take())
            .is_some_and(|m| m.drag.is_some());
        if had {
            // SAFETY: no preconditions.
            let _ = unsafe { ReleaseCapture() };
        }
        self.invalidate();
    }

    /// Commits an open label editor and drops the selection (mode change, pane click).
    pub(super) fn mmd_leave(&mut self, sh: &Shared) {
        if self.mmd_active() {
            self.mmd_commit(sh);
            self.mmd_clear();
        }
    }

    /// Fence body (start offset, text) of detail node `node` of block `bi` in the preview
    /// document; `None` unless it is a code node whose text is contiguous in the source.
    fn mmd_body(&mut self, bi: usize, node: usize) -> Option<(usize, String)> {
        let View::Doc(d) = &mut self.view else {
            return None;
        };
        let Content::Markdown(doc) = &mut d.content else {
            return None;
        };
        let det = doc.detail(bi);
        if !matches!(det.nodes.get(node)?.kind, NodeKind::Code { .. }) {
            return None;
        }
        let pt = node_text(&doc.source, &det, node, true);
        let start = pt.source_range().start as usize;
        (doc.source.get(start..start + pt.text.len()) == Some(pt.text.as_str()))
            .then_some((start, pt.text))
    }

    /// The code node of the preview document whose body starts at `start`.
    fn mmd_find(&mut self, start: usize) -> Option<(usize, usize, String)> {
        let bi = {
            let View::Doc(d) = &self.view else {
                return None;
            };
            let Content::Markdown(doc) = &d.content else {
                return None;
            };
            doc.index.block_at(u32::try_from(start).ok()?)
        };
        let n = {
            let View::Doc(d) = &mut self.view else {
                return None;
            };
            let Content::Markdown(doc) = &mut d.content else {
                return None;
            };
            doc.detail(bi).nodes.len()
        };
        (0..n).find_map(|node| {
            self.mmd_body(bi, node)
                .filter(|(s, _)| *s == start)
                .map(|(_, t)| (bi, node, t))
        })
    }

    /// The diagram card under content point (x, y): (block, node, card in client
    /// coordinates, offset from block to client coordinates).
    fn mmd_card_at(&self, sh: &Shared, x: f32, y: f32) -> Option<(usize, usize, Rect, (f32, f32))> {
        let View::Doc(d) = &self.view else {
            return None;
        };
        let doc_y = f64::from(y) + d.viewer.scroll;
        let bi = d.viewer.block_at_y(doc_y);
        let (bi, node) = self.diagram_at(sh, x, y).filter(|(b, _)| *b == bi)?;
        let (card, off) = self.mmd_card(sh, bi, node)?;
        Some((bi, node, card, off))
    }

    /// Card (block coordinates) of diagram `node` in block `bi` and the block → client offset.
    fn mmd_card(&self, sh: &Shared, bi: usize, node: usize) -> Option<(Rect, (f32, f32))> {
        let View::Doc(d) = &self.view else {
            return None;
        };
        let (cx, _) = self.col(sh);
        let top = d.viewer.block_top(&d.content, bi) - d.viewer.scroll;
        let card = d
            .viewer
            .layout_of(bi)?
            .hot
            .iter()
            .find_map(|h| match h.kind {
                HotKind::Diagram { node: n } if n == node => Some(h.rect),
                _ => None,
            })?;
        #[allow(clippy::cast_possible_truncation)] // A client offset (DIPs).
        Some((card, (cx, top as f32)))
    }

    /// Hit geometry of the selection's diagram in client coordinates, when the buffer still
    /// holds the selected body and the diagram is laid out.
    fn mmd_geom(&self, sh: &Shared) -> Option<FlowHits> {
        let m = self.mmd()?;
        let e = self.edit()?;
        let end = m.start + m.text.len();
        if end > e.ed.len() || e.ed.slice(m.start..end) != m.text {
            return None;
        }
        let View::Doc(d) = &self.view else {
            return None;
        };
        let (card, (ox, oy)) = self.mmd_card(sh, m.block, m.node)?;
        let h = flow_hits(&d.rich, &m.text, card)?;
        let mv = |r: Rect| Rect::new(r.x + ox, r.y + oy, r.w, r.h);
        Some(FlowHits {
            nodes: h.nodes.into_iter().map(mv).collect(),
            edges: h
                .edges
                .into_iter()
                .map(|e| e.into_iter().map(|p| (p.0 + ox, p.1 + oy)).collect())
                .collect(),
            labels: h.labels.into_iter().map(|r| r.map(mv)).collect(),
        })
    }

    /// Re-validates the selection against the buffer (undo, typing in the pane): keeps it when
    /// the body still parses with the selected element, else drops it.
    fn mmd_refresh(&mut self, sh: &Shared) -> bool {
        let Some(m) = self.mmd().cloned() else {
            return false;
        };
        if self.edit_mode() == Some(Mode::Split) {
            self.sync_preview(sh);
        }
        if self.edit().is_some_and(|e| e.active.is_some()) {
            self.mmd_clear();
            return false;
        }
        let ok = self.mmd_find(m.start).and_then(|(bi, node, text)| {
            let fe = FlowEdit::new(&text)?;
            let f = fe.flow();
            let fits = match m.target {
                Target::Node(i) => i < f.nodes.len(),
                Target::Edge(i) => i < f.edges.len(),
            };
            fits.then_some((bi, node, text))
        });
        match ok {
            Some((bi, node, text)) => {
                if let Some(s) = self.mmd_mut() {
                    s.block = bi;
                    s.node = node;
                    s.text = text;
                }
                true
            }
            None => {
                self.mmd_clear();
                false
            }
        }
    }

    /// Writes `new` (the whole rewritten fence body) to the buffer as one minimal splice.
    fn mmd_apply(&mut self, sh: &Shared, new: String) -> bool {
        self.mmd_apply_as(sh, new, None)
    }

    /// [`Self::mmd_apply`]; with `undo_to = Some(body)` the last undo step (which turned
    /// `body` into the current text) is undone first, in the same edit, so the result is one
    /// undo step from `body` (B3, prompt 022).
    fn mmd_apply_as(&mut self, sh: &Shared, new: String, undo_to: Option<String>) -> bool {
        let Some((start, cur)) = self.mmd().map(|m| (m.start, m.text.clone())) else {
            return false;
        };
        let undo = undo_to.is_some();
        let old = undo_to.unwrap_or(cur);
        let Some((a, b)) = yamdv_editor::diff_span(&old, &new) else {
            return false;
        };
        let ins = new.get(b.clone()).unwrap_or_default().to_string();
        let range = start + a.start..start + a.end;
        let caret = start + b.end;
        self.with_ed(sh, |ed| {
            if undo {
                ed.undo();
            }
            ed.replace(range, &ins, caret);
        });
        if let Some(m) = self.mmd_mut() {
            m.text = new;
        }
        if self.edit_mode() == Some(Mode::Split) {
            self.sync_preview(sh);
        }
        self.invalidate();
        true
    }

    fn mmd_label(&self) -> Option<String> {
        let m = self.mmd()?;
        let fe = FlowEdit::new(&m.text)?;
        let f = fe.flow();
        Some(match m.target {
            Target::Node(i) => f.nodes.get(i)?.label.clone(),
            Target::Edge(i) => f.edges.get(i)?.label.clone().unwrap_or_default(),
        })
    }

    fn mmd_begin_edit(&mut self, text: Option<&str>) {
        let label = text.map_or_else(|| self.mmd_label().unwrap_or_default(), str::to_string);
        if let Some(m) = self.mmd_mut() {
            m.drag = None;
            m.input = Some(LabelInput::new(&label));
            m.added = None;
        }
        self.invalidate();
    }

    /// Writes the open label editor back (one undo step); `false` when nothing changed.
    fn mmd_commit(&mut self, sh: &Shared) -> bool {
        let Some(m) = self.mmd_mut() else {
            return false;
        };
        let Some(input) = m.input.take() else {
            return false;
        };
        let added = m.added.take().filter(|(_, after)| *after == m.text);
        let (text, target) = (m.text.clone(), m.target);
        let Some(fe) = FlowEdit::new(&text) else {
            return false;
        };
        let new = match target {
            Target::Node(i) => fe.set_node_label(i, input.text.trim()),
            Target::Edge(i) => fe.set_edge_label(i, input.text.trim()),
        };
        self.invalidate();
        let Some(n) = new.filter(|n| *n != text) else {
            return false;
        };
        // B3: "add node + rename" is ONE undo step: undo the add, then write the result.
        self.mmd_apply_as(sh, n, added.map(|(before, _)| before))
    }

    /// Mouse down at content point (x, y) in an edit mode; `true` = handled (a diagram
    /// element was hit, or the click only closed the label editor).
    pub(super) fn mmd_down(&mut self, sh: &Shared, x: f32, y: f32) -> bool {
        let Some(mode) = self.edit_mode() else {
            return false;
        };
        if self.mmd_editing() {
            if self.mmd_input_rect(sh).is_some_and(|r| r.contains(x, y)) {
                return true;
            }
            self.mmd_commit(sh);
        }
        if mode == Mode::Split {
            self.sync_preview(sh);
        }
        let Some((bi, node, card, (ox, oy))) = self.mmd_card_at(sh, x, y) else {
            self.mmd_clear();
            return false;
        };
        let active = self.edit().and_then(|e| e.active);
        if active.is_some_and(|a| a.block == bi) {
            self.mmd_clear();
            return false;
        }
        let Some((start, text)) = self.mmd_body(bi, node) else {
            self.mmd_clear();
            return false;
        };
        let hits = {
            let View::Doc(d) = &self.view else {
                return false;
            };
            flow_hits(&d.rich, &text, card)
        };
        let (lx, ly) = (x - ox, y - oy);
        let target = hits.as_ref().and_then(|h| {
            h.node_at(lx, ly)
                .map(Target::Node)
                .or_else(|| h.edge_at(lx, ly, 6.0).map(Target::Edge))
        });
        let Some(target) = target else {
            self.mmd_clear();
            return false;
        };
        let n = self.count_clicks(x, y);
        // F2: leave the active block first; the body moves by the pending edit when it lies
        // after that block.
        let mut start = start;
        if let Some(a) = active {
            if let View::Doc(d) = &self.view
                && let Some(e) = d.edit.as_deref()
                && start >= a.start + a.old_len
            {
                let delta = e.ed.len().cast_signed() - d.content.source().len().cast_signed();
                start = start.saturating_add_signed(delta);
            }
            self.edit_leave_block(sh);
        }
        let Some((bi, node, text)) = self.mmd_find(start) else {
            self.mmd_clear();
            return true;
        };
        let drag = match target {
            Target::Node(i) if n < 2 => Some(Connect {
                from: i,
                at: (x, y),
                moved: false,
            }),
            _ => None,
        };
        if let Some(e) = self.edit_mut() {
            e.mmd = Some(MmdSel {
                start,
                text,
                block: bi,
                node,
                target,
                input: None,
                drag,
                hi: None,
                added: None,
            });
        }
        if drag.is_some() {
            // SAFETY: valid hwnd.
            unsafe { SetCapture(self.hwnd) };
        }
        if n >= 2 {
            self.mmd_begin_edit(None);
        }
        self.edit_title();
        self.invalidate();
        true
    }

    /// Mouse move while dragging from a node.
    pub(super) fn mmd_move(&mut self, x: f32, y: f32) -> bool {
        let Some(c) = self.mmd_mut().and_then(|m| m.drag.as_mut()) else {
            return false;
        };
        if !c.moved && ((x - c.at.0).abs() > 4.0 || (y - c.at.1).abs() > 4.0) {
            c.moved = true;
        }
        c.at = (x, y);
        self.invalidate();
        true
    }

    /// Mouse up: a node dropped on another node connects them (`A --> B`).
    pub(super) fn mmd_up(&mut self, sh: &Shared) -> bool {
        let Some(c) = self.mmd_mut().and_then(|m| m.drag.take()) else {
            return false;
        };
        // SAFETY: no preconditions.
        let _ = unsafe { ReleaseCapture() };
        if c.moved
            && let Some(to) = self.mmd_geom(sh).and_then(|h| h.node_at(c.at.0, c.at.1))
            && to != c.from
            && let Some(new) = self
                .mmd()
                .and_then(|m| FlowEdit::new(&m.text))
                .and_then(|fe| fe.add_edge(c.from, to))
        {
            let edges = FlowEdit::new(&new).map_or(0, |f| f.flow().edges.len());
            if self.mmd_apply(sh, new)
                && let Some(m) = self.mmd_mut()
            {
                m.target = Target::Edge(edges.saturating_sub(1));
            }
        }
        self.invalidate();
        true
    }

    /// Keys while a diagram element is selected; `Some` = handled.
    pub(super) fn mmd_key(
        &mut self,
        sh: &Shared,
        vk: u16,
        ctrl: bool,
        shift: bool,
    ) -> Option<Option<Action>> {
        if !self.mmd_active() || self.dialog.is_some() || self.find.is_some() {
            return None;
        }
        if self.mmd_editing() {
            let m = self.mmd_mut()?;
            let input = m.input.as_mut()?;
            match vk {
                0x0D => {
                    self.mmd_commit(sh);
                    return Some(None);
                }
                0x1B => m.input = None,
                0x08 => input.backspace(),
                0x2E => input.delete(),
                0x25 => input.left(),
                0x27 => input.right(),
                0x24 => input.home(),
                0x23 => input.end(),
                0x41 if ctrl => input.select_all(),
                _ if ctrl => {
                    // Other shortcuts (Ctrl+S, Ctrl+Z…) act on the committed source.
                    self.mmd_commit(sh);
                    return None;
                }
                _ => return Some(None),
            }
            self.invalidate();
            return Some(None);
        }
        if ctrl {
            return None;
        }
        match vk {
            0x1B => {
                self.mmd_clear();
                Some(None)
            }
            0x0D if self.mmd_refresh(sh) => {
                self.mmd_begin_edit(None);
                Some(None)
            }
            0x2E | 0x08 if self.mmd_refresh(sh) => {
                let m = self.mmd()?;
                let fe = FlowEdit::new(&m.text)?;
                let new = match m.target {
                    Target::Node(i) => fe.delete_node(i),
                    Target::Edge(i) => fe.delete_edge(i),
                };
                if let Some(new) = new {
                    self.mmd_apply(sh, new);
                }
                self.mmd_clear();
                Some(None)
            }
            0x09 if !shift && self.mmd_refresh(sh) => {
                let m = self.mmd()?;
                let from = match m.target {
                    Target::Node(i) => Some(i),
                    Target::Edge(_) => None,
                };
                let fe = FlowEdit::new(&m.text)?;
                if let Some((new, id)) = fe.add_node(from, NEW_LABEL) {
                    let i = FlowEdit::new(&new).and_then(|f| f.node_index(&id));
                    let before = m.text.clone();
                    if self.mmd_apply(sh, new.clone())
                        && let Some(i) = i
                    {
                        self.mmd_begin_edit(Some(NEW_LABEL));
                        if let Some(m) = self.mmd_mut() {
                            m.target = Target::Node(i);
                            m.added = Some((before, new));
                        }
                    }
                }
                Some(None)
            }
            _ => None,
        }
    }

    /// WM_CHAR while a diagram element is selected: a printable character opens (or types
    /// into) the label editor; `true` = consumed.
    pub(super) fn mmd_char(&mut self, sh: &Shared, unit: u16) -> bool {
        let Some(m) = self.mmd_mut() else {
            return false;
        };
        let s = if (0xD800..0xDC00).contains(&unit) {
            m.hi = Some(unit);
            return true;
        } else if (0xDC00..0xE000).contains(&unit) {
            let Some(hi) = m.hi.take() else { return true };
            String::from_utf16(&[hi, unit]).unwrap_or_default()
        } else {
            m.hi = None;
            match char::from_u32(u32::from(unit)) {
                Some(c) if !c.is_control() => c.to_string(),
                // Enter / Esc / Backspace / Tab arrive as WM_KEYDOWN first.
                _ => return m.input.is_some(),
            }
        };
        self.mmd_type(sh, &s)
    }

    /// Typed text (WM_CHAR, driver `type`) while a diagram element is selected.
    pub(super) fn mmd_type(&mut self, sh: &Shared, s: &str) -> bool {
        if !self.mmd_active() {
            return false;
        }
        if !self.mmd_editing() {
            if s.chars().all(char::is_control) || !self.mmd_refresh(sh) {
                return false;
            }
            self.mmd_begin_edit(Some(""));
        }
        if let Some(i) = self.mmd_mut().and_then(|m| m.input.as_mut()) {
            i.insert(s);
        }
        self.invalidate();
        true
    }

    /// Where the label editor sits (client coordinates).
    fn mmd_anchor(&self, sh: &Shared) -> Option<Rect> {
        let m = self.mmd()?;
        let g = self.mmd_geom(sh)?;
        match m.target {
            Target::Node(i) => g.nodes.get(i).copied(),
            Target::Edge(i) => g.edge_anchor(i),
        }
    }

    fn mmd_input_shape(
        &self,
        sh: &Shared,
        input: &LabelInput,
        r: Rect,
    ) -> (Rect, Box<dyn yamdv_layout::shaper::Shaped>, f32) {
        let pal = &self.pal;
        let size = 14.0;
        let t = self.shape_ui(sh, &input.text, size, 400, pal.text, None);
        let pre = input.text.get(..input.caret).unwrap_or_default();
        let cx = if pre.is_empty() {
            0.0
        } else {
            self.shape_ui(sh, pre, size, 400, pal.text, None).width()
        };
        let w = (t.width() + 16.0).max(r.w).max(60.0);
        let h = (t.height().max(size * 1.3) + 8.0).max(r.h);
        let b = Rect::new(r.x + (r.w - w) / 2.0, r.y + (r.h - h) / 2.0, w, h);
        (b, t, cx)
    }

    fn mmd_input_rect(&self, sh: &Shared) -> Option<Rect> {
        let input = self.mmd()?.input.as_ref()?;
        let r = self.mmd_anchor(sh)?;
        Some(self.mmd_input_shape(sh, input, r).0)
    }

    /// Selection outline, connect line and label editor (content coordinates).
    pub(super) fn mmd_draw(&self, sh: &Shared, b: &Band<'_>) {
        let Some(m) = self.mmd() else { return };
        let Some(g) = self.mmd_geom(sh) else { return };
        let pal = &self.pal;
        let clip = self.split_rects().map(|(_, p, _)| p);
        if let Some(p) = clip {
            b.push_clip(p);
        }
        match m.target {
            Target::Node(i) => {
                if let Some(r) = g.nodes.get(i) {
                    let o = Rect::new(r.x - 4.0, r.y - 4.0, r.w + 8.0, r.h + 8.0);
                    b.stroke(o, pal.accent, 6.0, 2.0);
                }
            }
            Target::Edge(i) => {
                if let Some(pts) = g.edges.get(i) {
                    b.polyline(pts, pal.accent, 3.0, false);
                }
                if let Some(Some(r)) = g.labels.get(i) {
                    let o = Rect::new(r.x - 3.0, r.y - 3.0, r.w + 6.0, r.h + 6.0);
                    b.stroke(o, pal.accent, 4.0, 2.0);
                }
            }
        }
        if let Some(c) = m.drag.filter(|c| c.moved)
            && let Some(r) = g.nodes.get(c.from)
        {
            let from = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            b.polyline(&[from, c.at], pal.accent, 2.0, false);
            if let Some(j) = g.node_at(c.at.0, c.at.1).filter(|j| *j != c.from)
                && let Some(t) = g.nodes.get(j)
            {
                let o = Rect::new(t.x - 4.0, t.y - 4.0, t.w + 8.0, t.h + 8.0);
                b.stroke(o, pal.accent, 6.0, 2.0);
            }
        }
        if let Some(input) = &m.input
            && let Some(r) = self.mmd_anchor(sh)
        {
            let (bx, t, cx) = self.mmd_input_shape(sh, input, r);
            b.fill(bx, pal.bg, 1.0, 4.0);
            b.stroke(bx, pal.accent, 4.0, 1.5);
            let tx = (bx.x + (bx.w - t.width()) / 2.0).round();
            let ty = (bx.y + (bx.h - t.height()) / 2.0).round();
            if input.all {
                let sel_alpha = f32::from(pal.selection_alpha_pct) / 100.0;
                b.fill(
                    Rect::new(tx - 1.0, ty, t.width() + 2.0, t.height()),
                    pal.selection_color(),
                    sel_alpha,
                    0.0,
                );
            }
            b.text(t.as_ref(), tx, ty);
            b.fill(
                Rect::new((tx + cx).round(), ty, 1.5, t.height().max(14.0)),
                pal.text,
                1.0,
                0.0,
            );
        }
        if clip.is_some() {
            b.pop_clip();
        }
    }

    /// Driver: the client point of node `i` (or edge `i` with `edge`) of the first laid-out
    /// flowchart in the document.
    #[cfg_attr(not(feature = "dev-tools"), allow(dead_code))]
    pub(super) fn mmd_point(&mut self, sh: &Shared, i: usize, edge: bool) -> Option<(f32, f32)> {
        let n = match &self.view {
            View::Doc(d) => d.content.len(),
            _ => return None,
        };
        for bi in 0..n {
            let nodes: Vec<usize> = match &self.view {
                View::Doc(d) => d.viewer.layout_of(bi).map_or_else(Vec::new, |l| {
                    l.hot
                        .iter()
                        .filter_map(|h| match h.kind {
                            HotKind::Diagram { node } => Some(node),
                            _ => None,
                        })
                        .collect()
                }),
                _ => return None,
            };
            for node in nodes {
                let Some((_, text)) = self.mmd_body(bi, node) else {
                    continue;
                };
                let Some((card, (ox, oy))) = self.mmd_card(sh, bi, node) else {
                    continue;
                };
                let View::Doc(d) = &self.view else {
                    return None;
                };
                let Some(h) = flow_hits(&d.rich, &text, card) else {
                    continue;
                };
                let r = if edge {
                    h.edge_anchor(i)?
                } else {
                    *h.nodes.get(i)?
                };
                return Some((r.x + r.w / 2.0 + ox, r.y + r.h / 2.0 + oy));
            }
        }
        None
    }
}

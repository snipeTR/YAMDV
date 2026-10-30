# ADR-0027: Visual Mermaid editing (IDEA-05, R-F2-11, #86)

- Status: accepted (prompt 024, v2.1.0).

## Context
R-F2-11 (MAY): edit a rendered flowchart directly in Live (F2) and Split (F3) Edit, written back to the Markdown source, undoable. The source stays the single truth (R-EDIT-01, R-F2-10).

## Decision
- **Model.** `yamdv-mermaid::edit::FlowEdit` re-parses the fence body into records (`flow::Rec`: statement, byte span, node/edge indices) and produces a **new body** for each operation: `set_node_label`, `set_edge_label`, `add_node(from)`, `add_edge(a, b)`, `delete_edge`, `delete_node` (with its edges, `class`/`style`/`click` mentions; `linkStyle` indices renumbered). Untouched statements keep their exact bytes; touched ones are rewritten minimally (labels encoded with quotes / `#quot;` when needed). Pure, unit-tested (`tests_edit.rs`).
- **Write-back.** The window diffs old/new fence body (`buffer::diff_span`) and applies **one** `Editor::replace` (one undo step, normal dirty/autosave path). Split re-syncs its preview through the existing incremental path.
- **Hit-testing.** The layered layout already places node boxes, edge polylines and label boxes; `Drawing.labels` now keeps the label boxes and `yamdv-layout::diagram::flow_hits` maps them into card coordinates from the same cached drawing that is painted (no second layout).
- **Interaction.** Click selects (node first, then edge label, then edge path within 6 px); double-click / Enter / typing edits a single-line label over the shape; Delete deletes; Tab adds a connected "New node" and edits it; drag node → node adds `-->`. Esc cancels / deselects. Read mode is unchanged.
- **Flowchart first.** Only `flowchart`/`graph` are editable; sequence, class, state, ER, gantt, pie, mindmap, etc. stay source-edited (their layouts do not yet expose stable per-element geometry).

## Limits (accepted)
- No manual positions: layout is automatic, dragging means "connect".
- Diagrams wider than their card (h-scrolled), over the complex-layout limit, or in fences inside containers (e.g. blockquote) are source-only.
- In F2 the block holding the caret shows its source; clicking a diagram leaves that block first.
- Tab-add then rename is two undo steps; edge labels on chained operators (`A --> B --> C`) are edited per operator.

## Consequences
No new dependencies, no network, size impact within B-SIZE. Driver script `tests/driver/edit-mermaid-visual.yds` (runs with the Windows test job, A-12).

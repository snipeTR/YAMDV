# v2.1.0 report (prompt 024)

Owner 2026-10-30 23:20 UTC+3: "issues deki herşeyi inşaa et. kodu güncelle" — all three open issues built; none open now.

## #86 IDEA-05 Visual Mermaid editing (R-F2-11, ADR-0027)
- F2 and F3: click selects a flowchart node or edge; double-click / Enter / typing edits the label in place
  (Enter commits, Esc cancels); Delete removes (node + edges + `class`/`style`/`click`, `linkStyle` renumbered);
  Tab adds a connected node; drag node onto node adds `A --> B`. One undo step per change via the editor splice path.
- Limits: flowchart/graph only (sequence, class, state, ER, gantt, pie, mindmap … stay source-edited); diagrams
  wider than the card, over the complex-layout limit or in fences inside containers are source-only; no manual
  positions; Tab-add + rename = 2 undo steps; single-line labels; read mode unchanged.
- Tests: `yamdv-mermaid` tests_edit (+LabelInput), `flow_hits_pick_nodes_then_edges`; driver `edit-mermaid-visual.yds`.

## #59 PERF-06 Incremental re-render
- F2/F3 with no unsaved edits: `Editor::sync_external` splices only the differing bytes (one undo step, buffer
  clean, mode/caret/scroll kept); preview re-lays out only changed blocks; plain-text files likewise.
- Evidence: `sync_external_is_one_clean_undo_step`, `diff_span_cuts_on_char_boundaries`, proptest
  `sync_external_reaches_any_text`; driver `edit-reload-incremental.yds` (`reload_blocks` 1–2, dirty 0, undo 1).

## #26 FUT-27 Editor plugins (ADR-0028)
- `integrations/vscode/` (TS) and `integrations/sublime/` (Python): "YAMDV: Open preview" → `yamdv.exe [--line N]
  -- <path>`, argv only, detached. No network, no telemetry; README each; helper tests pass locally.
- `.vsix` not built in CI (needs npm packages); build documented (16 KB locally). Not published anywhere.

## Also
- Popup menus: reused band DIB cleared before each band. Spec patches P-62a…d (v2n.txt), spec-check green;
  TRACEABILITY, ROADMAP_V2, CHANGELOG, README.

## CI and release
- PR #102, CI run 41127007456 green (build job, A-12), squash-merged as a99c849c (tree == green head 4a420c02).
- `chore(release): v2.1.0` 24228e6a, tag v2.1.0, release run 41127800860 green.
- Draft (not published): draft `untagged-8047eeb37b633c360e0e` (previous repository)
- `YAMDV-2.1.0-win-x64.zip`, 1,850,585 B, SHA-256
  56b44f0875b6d2a935a991ca32bd14bfa529bf95f47dae2abfc511049099f524 (matches the .sha256 asset).
- `yamdv.exe` 3,595,264 B ≤ B-SIZE 3,600,000 (headroom 4,736 B; v2.0.3 was 3,547,648).
- Not run: the Windows test job (driver scripts, goldens, measurements) is workflow_dispatch-only (A-12);
  the two new driver scripts await the owner's run.

# V2.1 plan — owner prompt 024 ("issues deki herşeyi inşaa et. kodu güncelle", 2026-10-30 23:20 UTC+3)

Build every open issue, release v2.1.0 (draft). Branch `feat/v2.1.0`, one PR, A-12 (one CI run).

1. #59 PERF-06 incremental live reload — Pass 1 (prompt 020) did read-mode Markdown
   (`reload_incremental`: diff → `Document::sync_to` → `Viewer::splice`, anchor kept, T6 driver
   script `live-reload-incremental`). Missing: Live/Split edit with a clean buffer still exits,
   re-reads in full and re-enters; plain-text files reload in full. Finish both through the editor
   splice path (one undo step, caret/mode/scroll kept) and plain chunk sync.
2. #86 IDEA-05 visual Mermaid editing (R-F2-11) in Live/Split: select node/edge, inline label
   edit, add/delete nodes and edges, drag node onto node = connect; every change is a source
   rewrite applied through the editor (one undo step). Flowchart only (v1); pure rewriting in
   `yamdv-mermaid::edit` with unit tests. ADR-0027.
3. #26 FUT-27 editor plugins: `integrations/vscode/` (TypeScript) + `integrations/sublime/`
   (Python) launching `yamdv.exe [--line N] -- <path>`; no network, no telemetry; READMEs;
   .vsix build documented (not built in CI). ADR-0028.
4. popup.rs: same uncleared reused-band DIB as the v2.0.3 fullscreen fix.
5. Spec patches P-62 (v2n.txt), v2.1.0 + CHANGELOG + README, release draft, report.

### CHECKPOINT 024-01 (23:40 UTC+3)
- Read #86/#59/#26, SPEC R-F2-11 / R-REN-18 / R-READ-01, prompt 020 §3.6, app.rs reload path,
  flow.rs parser, layered.rs drawing boxes, diagram.rs emit. Plan above. Next: #59, popup.rs.

### CHECKPOINT 024-02 (23:51 UTC+3)
- Done (pushed, [skip ci]): #59 splice reload + driver script; popup band fix; #86 FlowEdit +
  LabelInput + flow_hits + window UI (mmdedit.rs) + driver script; #26 vscode + sublime plugins
  (tsc/test/vsce package and python tests pass locally). ADR-0027/0028, v2n.txt P-62a…d,
  TRACEABILITY, ROADMAP_V2. Next: full local checks, PR, one CI run, merge, v2.1.0 release.

### CHECKPOINT 024-03 (23:54 UTC+3)
- Local checks green (fmt, clippy, tests, deny, check-imports, exe 3,587,584 B). PR #102 opened;
  this commit starts its single CI run.

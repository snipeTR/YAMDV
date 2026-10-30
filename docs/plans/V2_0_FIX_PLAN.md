# V2.0 fix plan — prompt 021 (toolbar settings fixes, tests off, v2.0.1)

Branch `fix/v2.0-toolbar` (worktree /workspace/YAMDV-fix, from origin/main 82c42e85). Tag v2.0.0 exists
(9417fece, draft unpublished) → this release is **v2.0.1**.

Steps: (1) A-12 via PROCESS_AMENDMENTS + workflows (tests workflow_dispatch-only); (2) §2 toolbar settings
(order, ▲/▼, presets, inline edit-only note); (3) P-59 spec patch; (4) v2.0.1 + CHANGELOG; (5) PR, one CI run;
(6) merge (F-24), `chore(release): v2.0.1`, tag → draft; (7) report docs/reports/v2_0_fix_REPORT.md.

### CHECKPOINT 021-01 (2026-10-30 21:40 UTC+3)
- Prompt stored as docs/prompts/021_TOOLBAR_SETTINGS_FIXES.md; plan created. Next: A-12 + CI.

### CHECKPOINT 021-02 (2026-10-30 21:45 UTC+3)
- Done on fix/v2.0-toolbar: A-12 (amendments + PROCESS.md regenerated), ci.yml (build job; tests
  workflow_dispatch-only), §2 toolbar settings code, P-59a/b/c, v2.0.1 + CHANGELOG + README, report draft.
- Local: clippy (xwin, -D warnings) clean, fmt clean, cargo test --workspace green, spec-check green,
  cargo deny ok, release build + check-imports OK.
- PR #99 opened. This commit (no [skip ci]) triggers the one CI build run. Next: wait, merge (F-24),
  `chore(release): v2.0.1`, tag v2.0.1 → draft; fill the report [skip ci].

### CHECKPOINT 021-03 (2026-10-30 21:52 UTC+3) — DONE
- PR #99 run 41108517571 green → squash c82aa380 (the only tree difference from the head is the other
  worker's docs/qa/owner PNG on main). `chore(release): v2.0.1` 57170e53, tag v2.0.1, release run
  41109653090 green → draft untagged-d1fed091a1c7839c5829, zip 1,820,834 B, SHA-256 6c1e5468…574f.

## Prompt 022 (owner micro change 2026-10-30): edit-mode toolbar tint → v2.0.2

### CHECKPOINT 022-01 (2026-10-30 22:20 UTC+3)
- Branch fix/v2.0-toolbar-edit-tint, PR #100. `toolbar_edit_bg` (theme.rs: light 7 % → #C83232,
  dark 47 % → #5A1A24), toolbar.rs draws it in F2/F3 (not in HC), "on" icon falls back to `text`
  below 3:1. P-60 (docs/spec-patches/v2l.txt), v2.0.2 + CHANGELOG. Local: fmt, clippy, tests,
  spec-check, deny, release build + check-imports all OK.
- This commit (no [skip ci]) is the one CI build run. Next: green → squash → chore(release): v2.0.2
  + tag → draft; report docs/reports/v2_0_2_REPORT.md [skip ci].

### CHECKPOINT 022-02 (2026-10-30 22:16 UTC+3) — DONE
- Run 41111890069 green → squash 77aa9e6b (tree equal). `chore(release): v2.0.2` 7279dfad, tag
  v2.0.2, release run 41113091814 green → draft untagged-50d5c5aaa03fac3f8b5f,
  zip 1,821,375 B, SHA-256 6b6ad8ea…879b. Report docs/reports/v2_0_2_REPORT.md.

## Prompt 023 (owner, 2026-10-30 22:50 UTC+3) → v2.0.3, branch fix/v2.0.3
1. #98 word count → title bar next to the file name; floating bubble removed; setting kept.
2. Mermaid sequence `rect` (rgb/rgba/hex/name), nested, band behind content; dark-mode fallback;
   fixture tests/fixtures/mermaid/seq-rect.md + parser unit test.
3. Fullscreen viewer repeating bands (steering 22:51).

### CHECKPOINT 023-01 (22:58 UTC+3)
- (3) root cause: `Renderer::render_bands` reuses one DIB per band (ADR-0025) and never cleared it;
  `fv_blit` draws no background, so every band below the diagram kept the previous band's pixels.
  Fix: `geom::reset_band` zeroes each band before drawing (+ unit test). Fixture for (2) written.
- Next: seq.rs `rect`, diagram.rs band colour, word count to the title bar, P-61, v2.0.3.

### CHECKPOINT 023-02 (22:57 UTC+3)
- (2) done: `rect` bands in seq.rs, `theme::band` dark fallback (≤ 20 % tint, text ≥ 4.5:1),
  unit test `seq_rect_owner_example`.
- (1) done: count drawn in `draw_chrome` after name/pill/dot (11 px `text_muted`), bubble
  removed from `draw_overlays`/`invalidate_overlays`/content hover; title-bar tooltip; bar
  repaints when the text changes. Spec P-61a–d (UI_SPEC B.10.3, settings row, Mermaid table,
  SPEC R-READ-07); spec-check green. Version 2.0.3 + CHANGELOG.
- Local: fmt, clippy -D warnings, tests, deny, release build, check-imports all green.
- PR #101 opened (Closes #98). Next: one CI run, F-24 merge, release commit + tag v2.0.3.

### CHECKPOINT 023-03 (23:15 UTC+3) — done
- Run 41119527544 green → squash 26d61865 (tree equal), #98 closed. `chore(release): v2.0.3`
  ed9c825e, tag v2.0.3, release run 41120467954 green → draft untagged-87842078c64793f6ba2e,
  zip 1,825,185 B, SHA-256 7d818b20…1284. Report docs/reports/v2_0_3_REPORT.md.

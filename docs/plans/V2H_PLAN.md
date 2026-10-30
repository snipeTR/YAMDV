# V2-H plan (prompt 018: native PDF, interactive HTML, integrations, known bugs)

Order: §1 (A-9 item 11, A-10 item 4, F-37, F-33…F-36, BUG-12) → §2 (ADR-0023, size room ≥ 100 KB, P-49…P-52) → §3 Part 1 PDF → Part 2 HTML/clipboard/PNG → Part 3 integrations/bugs → §4 PR (Refs lines, one CI run, +1 fix) → merge → tag phase-v2h (no version bump) → report.
Done earlier (verify only): BUG-01 #61 (PR #89), BUG-02 #62 (PR #88), BUG-06 #66 (PR #89), FUT-10 #16 (PR #90).

## CHECKPOINT 1 (start)
- Branch phase/v2h-export off main b365a9a; prompt stored as docs/prompts/018_V2H_EXPORT_INTEGRATION.md (verbatim copy).
- PROCESS.md: A-9 item 11 (F-33 one issue per line), A-10 item 4 (build first, test separately).
- F-33: #20, #46, #25 closed by hand with a comment referencing #87 (2026-10-29).

## Owner note F-37 (2026-10-29, relayed)
The hitch was seen by a programmer friend with a free-spinning (notchless, MagSpeed-style) wheel: spinning fast on LARGE files, scrolling stops and gets stuck. Reproduce with high-rate WM_MOUSEWHEEL streams (small / non-120 deltas, hundreds per second, seconds long) on large-1mb.md. Required: a high-rate wheel keeps scrolling to the requested target; never stop at the laid-out boundary; any delta size; high message rate = fast scrolling (no snap, ≤ 60 ms ease). 018a adds a free-spin wheel test.

## CHECKPOINT 2 (F-37 in progress)
- Pure model yamdv-core::scroll (Wheel: accumulate, impulse retarget, critically damped follow; settle_target). Platform: wheel() steps at once per message; motion_shift() moves running motions by the viewer's anchor corrections (paint/prepare_now); TIMER_SETTLE 25 smart settle; setting reading.scroll_snap (smart/off).
- Dev-tools: `ui scroll-trace on|<file>`, `ui wheel-burst N[:delta]`; old path kept only behind env YAMDV_OLDWHEEL for the comparison trace (remove before PR).

## CHECKPOINT 3 (2026-10-29 22:20)
- F-37 done (7d4db9c): cause measured — old: free-spin stream 1500 × delta 15 on large-1mb (4.5 s input) drained in ~33 s, 1507 synchronous frames; new: 171 frames, done in 5.4 s, final 22500 = target. Anchor-fix events now shift the motion (−112/−138 px seen on a 12000 px jump). Temp YAMDV_OLDWHEEL branch removed.
- BUG-12 = #91. F-34 (per-formula scope in defining block), F-35 (milestone label flips left/clamps), F-36 (GIF frames on image worker) done.
- Local Wine driver suite: same failures as main b365a9a (links/math/pdf under Wine), no regressions.
- Next: §2 ADR-0023, size room, spec patches P-49..P-52.

## CHECKPOINT 2026-10-29 22:35 (Part 1 native PDF writer)
- pdf.rs: record-mode Band seam → PDF content (paths, clips, alpha ExtGState, images deflate+SMask,
  JPEG DCT pass-through), Type0/CIDFontType2|0 subset fonts (subsetter) with W + ToUnicode,
  outline fallback (fsType restricted/bitmap-only, non-default variable instance, subset failure),
  COLR → outlines + invisible text, outline nested by level, GoTo/URI(http/https/mailto) links,
  Info (Title/Producer/CreationDate), page clip at the break, optional "n / N" footer (Helvetica),
  atomic write (write_atomic now takes bytes; tmp removed on failure); cancel → no file.
- print_with dispatches t.pdf → pdf::export; start_export no longer requires Microsoft Print to PDF.
- pass 1 factored into print::pass1 (shared with printing; no behaviour change).
- Wine smoke: 9-page fixture OK (pdffonts: CID TrueType embedded subset + ToUnicode; text extracts
  incl. Turkish), 193 pages ≈ 2.5 s incl. Wine start. Wine 10 HitTestTextRange is a stub → link
  hot rects and sup/sub script paras cannot be checked under Wine (Windows CI/owner).
- Next: Part 2 (interactive HTML, CF_HTML copy, diagram PNG save).

## CHECKPOINT 2026-10-29 22:42 (Part 2 done, Part 3 in progress)
- Part 2: interactive HTML (5a43880): inline script 3,201 B, CSP sha256 computed in a const fn and
  checked against Python hashlib; Chrome headless: script runs under the CSP (44 fold buttons),
  find/toggle/fold verified; math/diagram SVG with glyph `<symbol>`s (svgout.rs). Copy as rich HTML,
  Save diagram / mind map as PNG (0b29df0).
- Part 3: found editor row, Reopen with encoding ▸ (encoding_rs, already linked), encoding in the
  word-count tooltip (0f8b2e1, e527f69).
- Next: jump list, custom app icon, BUG-03/04/05/07, TRACEABILITY, size, PR.

## CHECKPOINT 2026-10-29 22:49 (Part 3 continuing)
- Jump list (jumplist.rs; empty/DeleteList when history is off or cleared), custom app icon
  (appicon.rs: S-FILE-04 + ≤ 1 MB + ADR-0022 ICO → HICON; WM_SETICON, title-bar icon, HKCU
  DefaultIcon only when registered; e8c8cc8), BUG-04 full entity table (27.5 KB data), BUG-05
  rowspan in layout/export/CSV.
- Next: BUG-03 (copy as Markdown via source ranges), BUG-07 (highlight across emphasis),
  TRACEABILITY, size, PR.

## CHECKPOINT 2026-10-29 22:55 (Part 3 done; size, PR next)
- BUG-03 (5719a64, f3b9139: atomic constructs only), BUG-07 (cee4671), TRACEABILITY (trace_set.py).
- Note: the box was rolled back once mid-session (local branch behind origin); fast-forwarded
  from origin, nothing lost.
- Next: release sizes per part (ff8705f / 0b29df0 / HEAD), PR with Refs lines, one CI run.

## CHECKPOINT 2026-10-29 23:32 (§4 in progress)
- PR #92 open. Run 40946180003 failed only at `cargo deny` (new RUSTSEC-2026-0414, pulldown-cmark 0.13.4).
- Fix push aff180a (=0.13.5; deny/tests/gate green locally) = the one allowed fix push → run 40947414162.
- Next: on green → fill report gates from CI, merge (squash), tag phase-v2h. On red → no more pushes; §6 report.
- 23:50: run 40947414162 red (goldens menu-light, driver export-html/export-progress/titlebar, v1d-checks; M1 5.88 MiB).
  No further push allowed → §6 report written; PR #92 left open, no merge/tag.

# Prompt 018a (tests, fixes, v1.4.0)
## CHECKPOINT 018a-0 (2026-10-30 00:20)
- Prompt stored (docs/prompts/018a_V2H_TESTS_RELEASE.md), A-10 item 5 added. CI budget: diag 0/1, full 0/1, fix 0/2.
- Next: §1.1 local M1 diagnosis (modules/jump list), diag CI run.

## CHECKPOINT 018a-1 (2026-10-30 00:35)
- §1.1 code pushed (jump list out of process, P-53 host kind; mem-report/modules-mark/jumplist-flush; save-png KINDS fix).
- §1.3 test fixes + §1.4 P-53/P-54 pushed (02876e7). A-10 item 5 lives in PROCESS_AMENDMENTS.md (PROCESS.md is generated).
- Draw buffers (suspect b): screen bands carry no command buffer (rec: None), band DIB is bounded by band_rows,
  PDF jobs run in the worker process → not a cause; reported, no change.
- Box has no wine (Debian mirror unreachable): driver scripts are first run on the diag CI run.
- Next: §2 unit tests + pdf-inspect + 5 driver scripts + 2 golden inputs, then ONE diag run that also runs them.

## CHECKPOINT 018a-2 (2026-10-30 00:55)
- §2 A–D committed (3c3dc5a). T4 suspect found statically: V2-H set `opt-level = "z"` on
  yamdv-platform-win, which holds the band renderer (render.rs); reverted to the workspace "s".
  Record-mode band: screen bands have `rec: None` (one branch per call, no pass, no allocation);
  F-37 accumulator only runs in the wheel handler; layout scroll bench (MockShaper) 0.008 ms/step.
- Next: diag run (diag-v2h.yml: HEAD vs phase-v2g vs HEAD+z, M0/M1/modules/mem-stats/T4, drivers,
  goldens, pdf-native, all continue-on-error); cancel its duplicate ci.yml run; then remove diag files.

## CHECKPOINT 018a-3 (2026-10-30 01:08)
- Budget: diag 1/1 used (run 40961787027, in progress; its duplicate ci run 40961787140 cancelled), full 0/1, fix 0/2.
- Local (not pushed): eeae930 v1.4.0 + CHANGELOG. PR #92 body has 14 `Closes #n` lines (all linked).
- Next: analyse diag artifact; fix; remove diag workflow/script; full run.

## CHECKPOINT 018a-4 (2026-10-30 01:19)
- Diag 40961787027 (success): T4 head 8.3 / v2g 7.6 / head-z 8.1 ms median → runner variance; platform-win back to "z". M1 5.25 MiB (jump list out of process: 59 → 42 modules); +0.64 MiB vs v2g unexplained (partial).
- Fixes: scroll.rs fast_done threshold (scroll-fling); `\==` escape (highlight golden); test changes: scroll-fling live heading top, pdf-native layout warm-up. rowspan-light golden added; highlight-dark from the full run.
- Diag workflow/script removed. Budget: diag 1/1, full → this push (1/1), fix 0/2.
- Next: watch the full run; green → merge #92, tags, draft release, report.

## CHECKPOINT 018a-5 (2026-10-30 01:52) — final
- Full run 40964997763 green; PR #92 merged (1525786), phase-v2h tagged; chore(release) aa29c38 tagged v1.4.0; draft release with zip + SHA-256 (not published). 14 issues closed.
- Partial: M1 5.29 MiB (> 5.01). Next action: diag with `ui mem-report`, HEAD vs v2g-equivalent build.

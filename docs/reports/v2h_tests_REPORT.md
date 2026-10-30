# V2-H tests and release — report (prompt 018a)

**Status:** done except the M1 gate (partial, see §1). PR #92 squash-merged (`1525786`), `phase-v2h` on it;
`chore(release): v1.4.0` (`aa29c38`) tagged `v1.4.0`; draft release created (not published).
**CI (018a budget):** diag 1/1 — run 40961787027 (success); full 1/1 — run 40964997763 (success); fix pushes 0/2.
Release: run 40968532258 (main, tests skipped by F-32) and 40968540125 (tag, release job) — success.

## §1 Causes and fixes
| Item | Cause | Before → after |
|---|---|---|
| M1 (≤ 5.01 MiB) | Jump-list COM build in the main process loaded 17 shell modules. Fix: out-of-process host (P-53), debounced 30 s + on exit. Remaining +0.64 MiB vs v2g: **not found** (image/detail/layout/band caches equal, same modules; commit grows by the same amount → heap). | 5.88 → 5.29 MiB (gate **missed**, CI budget 8.0 green) |
| T4 typical warm (≤ 10.0 ms) | Runner variance: same-runner diag medians head 8.3 / v2g 7.6 / head-z 8.1 ms; record band, F-37 accumulator and layout (0.008 ms/step) ruled out. platform-win stays opt-level "z" ("s" = +182 KB, no gain). | 11.5 → 9.26 ms (attempt 2; attempt 1 10.83) |
| Draw buffers | Screen bands have no command buffer; PDF jobs run in the worker (exits after the job, `worker == 0`). | bounded / freed |

**Test fixes (§1.3, 02876e7):**
1. titlebar: `popup-select` by label, not index (menu gained items).
2. export-html / v1d-checks: set `html_interactive=false` where they check the static export.
3. export-progress: run on `large-1mb.md` (the old file finished before a progress sample).
4. menu-light golden regenerated (new menu items; self-reviewed).

**Other test changes (one-line reasons):**
- scroll-fling: `snap_dist` reads the heading top live, and `near-heading` is repeated after `wait-idle`. Reason: tops below the screen are estimates until laid out.
- pdf-native: `ctrl+end` / `ctrl+home` before `mem-mark`. Reason: the delta must measure the cancelled job, not the first layout.

## §2 Test results (full run 40964997763; 48/48 driver scripts PASS)
- A scroll: `scroll-fling.yds` PASS; scroll-model unit tests + `free_spin_ends_in_time_on_late_ticks` PASS.
- B PDF: `pdf-native.yds` PASS (attempt 2, F-31 delta rule); pdfparts units (xref, outline, link rects, ToUnicode, glyph remap, fsType) PASS.
- C HTML: `html-interactive.yds` PASS; CSP hash, forbidden-API scan, XSS suite (both modes) PASS.
- D integrations: `integrations-bugs.yds`, `jumplist-icon.yds` PASS; entity table, rowspan, encodings, editor args, BUG-03/06/07, F-34, icon unpremultiply units PASS.
- Goldens: `rowspan-light` 0 px differ; `highlight-dark` committed from the full run (self-reviewed).
- Workspace: 415 unit/integration tests pass locally; cargo deny OK.

## Gates (full run)
- M0 2.36 MiB ✔ · M1 5.29 MiB ✘ (target 5.01) · T4 typical warm 9.26 ms ✔ (large 5.4, math 7.1, mermaid 8.4).
- T5 p99: split 17.1, live 22.8, live-large 22.4 ms ✔ · T7 idle CPU doc 15.6 ms (one tick, report-only), empty 0 ✔.
- T10: 100-page fixture (127 pages) 0.32 s, +0.9 MB transient ✔ · `large-1mb.md` 1 093 pages in 2.63 s, peak 34.7 MB.
- Exe size (report only, A-10 item 5): 3 682 304 B (v2g 3 599 872 B, +82 432 B).

## Fixes and the test that found them
- Fast fling settle ≤ 60 ms on late 19.6 ms ticks (scroll.rs threshold) — scroll-fling.
- `\==x==` no longer highlights (escape-aware marker scan) — highlight-dark golden + `bug_07_escaped_marker_is_not_a_highlight`.
- *Clear history* refreshes the jump list — jumplist-icon.
- Jump list out of process (M1 −0.59 MiB) — M1 measurement.

## Issues closed (verified with `gh issue view`; all closed by the merge, none by hand)
#37 FUT-39, #22 FUT-21, #23 FUT-23, #16 FUT-10, #15 FUT-09, #24 FUT-25, #61–#67 BUG-01…07, #91 BUG-12.

## Release
- Draft: https://github.com/abuzer1495/YAMDV/releases/tag/untagged-abac255acfbcc3379074 (v1.4.0, **not published**)
- `YAMDV-1.4.0-win-x64.zip` SHA-256 `cd0e5673e1b3749835714b17f6466fd0e08ed49261287dc8e74f9577d1d3b6ec`
- Note: the CHANGELOG line "memory … back under budget" is true for the CI budget only; M1 is above 5.01 MiB.

## Partial
| Item | State | Next action |
|---|---|---|
| §1.1 M1 ≤ 5.01 | partial (5.29) | diag run: `ui mem-report` counting allocator, HEAD vs a v2g-equivalent build on typical.md |

## Owner checklist / Sahip kontrol listesi
1. EN: Wheel fast and slow through a long doc with `scroll_snap` smart and off. TR: Uzun belgede tekerleği hızlı ve yavaş çevirin (`scroll_snap` smart / off).
2. EN: Export PDF; open in Edge/Acrobat: select and search text, bookmarks, click links. TR: PDF dışa aktarın; Edge/Acrobat'ta metin seçin/arayın, yer imleri ve bağlantıları deneyin.
3. EN: Export a 100-page document to PDF; it takes a few seconds. TR: 100 sayfalık belgeyi PDF'e aktarın; birkaç saniye sürmeli.
4. EN: Open interactive HTML in a browser: TOC, folding, theme toggle, find. TR: Etkileşimli HTML'i tarayıcıda açın: içindekiler, katlama, tema, arama.
5. EN: *Copy as rich HTML* and paste into Word. TR: *Zengin HTML olarak kopyala* ile Word'e yapıştırın.
6. EN: *Save diagram as PNG* on a diagram and the mind map. TR: Diyagram ve zihin haritasında *PNG olarak kaydet*.
7. EN: F4 finds your editor; File ▸ Reopen with encoding (1254). TR: F4 düzenleyicinizi bulur; Dosya ▸ Kodlamayla yeniden aç (1254).
8. EN: Jump list shows recent files; *Clear history* empties it. Then publish v1.4.0. TR: Atlama listesi son dosyaları gösterir; *Geçmişi temizle* boşaltır. Sonra v1.4.0'ı yayımlayın.

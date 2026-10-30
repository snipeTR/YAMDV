# Phase V2-I report — hardening (prompt 019)

Branch `phase/v2i-hardening` (off main 657e671) · PR #94 squash-merged as 278192ca · tag `phase-v2i` → 278192ca · no release.
CI: diag-v2i 41013638607 (F-39), fuzz-long 41016337092, PR CI 41023143760 (green) + fix push 41025722307 (green, head 187fea4),
pack dispatch 41027413612 (red, see Partial), main push CI 41030323558 (green, M1 4.68).
Issues: #45 #50 #51 #52 closed by the merge; QA-01 #70 and HARD-05 #93 stay open.

| Item | Status | Notes |
|---|---|---|
| F-38 T7 | done | cause: 30 s jump-list debounce timer ticked while idle (T7 doc 15.6 ms) → timer-free throttle (start now if last > 30 s, else dirty → next change / close / exit); T7 0 / 0 µs |
| F-39 M1 | partial | V2-H 5.29 MiB → medians 4.59 / 5.29 / 4.78 / 4.68 MiB over 4 CI runs (bimodal ~4.6 vs ~5.3 per process); diag: excess came from the jump-list timer path; entity table now a static sorted array; live heap = V2-G (Δ +960 B) |
| F-40 | done | CHANGELOG + v1.4.0 notes edited; v1.4.0 stays a DRAFT |
| §2.1–§2.4 HARD-01 | done | SECURITY_AUDIT.md: 81 rules · 72 OK · 7 fixed · 1 open (S-PROC-08 → #93) · 1 manual (S-NET-05 → OWNER_QA 2); THREAT_MODEL.md 13 STRIDE sections (+ UIA) |
| §3 HARD-02 | done | 18 targets, 15.0 h, 1.98 G execs; 14 clean 1 h; 4 findings (below); 3.4 MB corpus samples committed |
| §4.1 ACG | done | ADR-0001 V2-I amendment: svg/ico/gif/png/jpg/bmp/tiff decode under ACG, webp placeholder; native PDF worker now runs ACG (late); HTML interactive, jump-list helper, font picker, UIA under ACG |
| §4.2 HC (FUT-47) | done | system colours (SPI_GETHIGHCONTRAST + GetSysColor), live on WM_SETTINGCHANGE; focus ring always on; WCAG test for every scheme/mode/HC; goldens `top-v2i-hc-*` |
| §4.3 UIA | done | lazy UIAutomationCore (System32) on WM_GETOBJECT(UiaRootObjectId); Document + TextPattern, title bar, sidebar, settings, menus, banners, viewer toolbar; `uia-client` exit 0; PWS Δ 56 KiB |
| §5 QA pack | not done | pack-exes failed in 41027413612 (my YAML bug: `BASELINE: 657e671` = float Infinity); no `qa-v2i` release, no SHA-256; fix on main |

## fuzz-long findings (all with regression test + seed)
1. `svg_parse` (46 s): tiny-skia 0.12 index panic on a ~1.7e18 px arc → device-bounds guard (paths beyond 30 000 px skipped).
2. `html_to_md` (177 s): backticks inside `<code>` ended the span → `<script` live → fence longer than any inner run.
3. `ini` (1 354 s): key starting with `[` re-read as a section → such lines are malformed.
4. `cf_html` (2 119 s, 8.06 M execs): libFuzzer RSS limit (2 560 MB) on the empty input; natively flat 4.8 MiB over 134 k runs → harness growth, no product leak.

## Gates (PR CI 41023143760; fix push 41025722307)
| Gate | Target | Result |
|---|---|---|
| M0 private WS | ≤ 2.70 MiB | 2.38 MiB |
| M1 private WS | ≤ 5.01 MiB | 4.59 (PR run) · **5.29 (fix run, FAIL)** · 4.78 (dispatch) · 4.68 (main); CI budget 8.0 OK |
| T4 typical warm p99 | ≤ 10.0 ms | 7.97 ms (cold 9.93) |
| T5 p99 split / live / live-large | within limits | 17.0 / 20.4 / 15.5 ms |
| T7 idle CPU empty / doc | 0 | 0 / 0 µs |
| T10 100 pages | unchanged | 258 ms, 127 pages, peak WS 25.9 MB |
| exe size | report | 3 780 608 B (v1.4.0: see CHANGELOG) |
| check-imports | OK | 21 DLLs (MSVC), 0xc160, CET, no W+X; UIAutomationCore not imported |

## Deviations
- fuzz-long started once by a temporary push trigger (workflow_dispatch needs the file on main); removed in a30d64e1.
- Before the long run, a local stable mini-fuzzer replaced libFuzzer (no C++ toolchain on the build box); the 4 early-stopped targets were not re-run for a full hour.
- SVG paths whose device bounds exceed 30 000 px are skipped (tiny-skia overflow).
- uia-tree PWS gate measures after one warm-up pass (first UIA DLL load).
- UIA: no Invoke/Toggle/Value patterns; text ranges have no bounding rectangles; names EN/TR static.
- Print job uses a temp file, not a folder; S-PROC-08 gap filed as HARD-05 #93.
- Fix push also corrected HC text on selected rows (black on highlight in HC White).
- F-24: the branch tip was restored to the green head 187fea4 (2c6a2e5a [skip ci]) before the squash-merge,
  so `git diff --quiet 187fea4 278192ca` holds; the HC White golden, plan and this report came on main afterwards ([skip ci]).
- That main commit also quotes `BASELINE` in ci.yml (pack-exes only; untested until the next pack dispatch).
- 41027413612 windows: scroll-fling `trace_p99` 17.5 > 16.7 ms (timing flake; same code green twice).
- Merged with the M1 phase target missed in 1 of 4 runs (owner asked to merge).

## Partial
- HARD-02: svg_parse, html_to_md, ini, cf_html ran 46 s / 177 s / 23 min / 35 min, not 1 h.
  **Next action:** after merge, `gh workflow run fuzz-long.yml --ref main` (1 h per target) and check these 4.
- F-39/M1: bimodal +0.7 MiB (commit too) → something allocates nondeterministically within the 3 s settle.
  **Next action:** alloc-sites diag of M1 with 10 runs, compare the low vs high runs.
- §5 qa-v2i: **Next action:** `gh workflow run ci.yml --ref main` → `tools/testpack/build-pack.sh <run-id>` →
  `gh release create qa-v2i --draft --prerelease` (zip + .sha256).
- QA-01 #70 stays open until the owner returns `qa-results.txt`.

Next action: pack dispatch on main (above), then owner runs `YAMDV-v2i-qa.zip` (OWNER_QA.md, 15 items); prompt 020 starts from its results and the 4-target fuzz re-run.

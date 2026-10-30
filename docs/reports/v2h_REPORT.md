# V2-H report (prompt 018: native PDF, interactive HTML, integrations, known bugs)

Branch `phase/v2h-export` (base b365a9a) · PR: https://github.com/abuzer1495/YAMDV/pull/92 · CI: 40946180003 (red: cargo deny), 40947414162 (red, below) ·
**PARTIAL (§6): not merged, not tagged** — the one fix push is used; no further push.

## AC
| ID | Result |
|---|---|
| AC-1 | Done. F-37 fixed with cause measured (below); F-33…F-36 done; ADR-0023 written; §2.2 savings 225,280 B (opt-level z for the UI crates); P-49…P-52 applied, `spec-check` green. |
| AC-2 | Done. Part 1 (1–6) native PDF; Part 2 (7–9) interactive HTML, Copy as rich HTML, diagram/mind-map PNG; Part 3 (10–13) found editor, Reopen with encoding, jump list, custom icon, BUG-03/04/05/07 (BUG-01/02/06 and FUT-10 were already on main); TRACEABILITY via `tools/trace_set.py`; S-EXP-02 (hash CSP, one script) and S-EXP-03 hold. |
| AC-3 | Partial: fmt, clippy, tests, deny, check-imports, size, measurements green; window goldens, driver scripts, V1-D checks red. |
| AC-4 | Not done: no merge, no tag `phase-v2h`. |

## Status (§6)
| Item | State |
|---|---|
| F-37 free-spin wheel | done (7d4db9c), cause below |
| §2 ADR-0023, savings, P-49…P-52; Parts 1–3 | done, pushed |
| CI green / merge / tag | **not done** — run 40947414162 red (rows below) |
| · window golden `menu-light.png` | all px differ: likely the ⋯ menu grew (new *Copy as rich HTML* item) → golden needs regen |
| · `titlebar.yds` popup_levels | 4×Down+Right likely lands on a non-submenu item now (same menu change; not verified) |
| · `export-html.yds`, `v1d-checks.ps1` | assert v1 static HTML (no `<script`, exact `EXPORT_CSP`, ≥3 imgs); default is now interactive |
| · `export-progress.yds` job_samples 2 < 3 | native PDF is faster than the old print path (timing) |

## Gates (run 40947414162; the measurement step itself passed)
| Gate | V2-G | V2-H | Limit | |
|---|---|---|---|---|
| M0 private WS | 2.40 MiB | 2.38 MiB | ≤ 2.70 | OK |
| M1 private WS | 4.71 MiB | **5.88 MiB** | ≤ 5.01 | **FAIL** (CI budget OK) |
| T4 typical cold / warm p99 | 13.4 / 8.7 ms | 13.7 / 11.5 ms | not worse | **worse (warm)**; large 5.6 / 5.7 |
| T5 split / live / live 1 MB p99 | 12.9 / 22.6 / 19.5 | 13.6 / 24.1 / 19.9 ms | ≤ 25 / 40 | OK |
| T7 idle CPU (empty, doc) | 0 / 0 µs | 0 / 0 µs | 0 | OK |
| Exe (CI MSVC release) | 3,599,872 B | 3,678,720 B | ≤ 4,000,000 | OK |

Savings §2.2: 3,644,928 → 3,419,648 B (−225,280). Shares: Part 1 PDF +186,368 B (pdf-writer, subsetter, miniz_oxide,
record band) · Part 2 +18,944 B (script 3.2 KB, SVG writer) · Part 3 +46,080 B (entity table 27.5 KB, icon, jump list, rowspan).

## ADR-0023 (native PDF writer)
pdf-writer 0.15 + subsetter 0.2 (no variable-fonts) + miniz_oxide; pages are drawn by the same pagination/draw code
into a record-mode band, so print, screen and PDF share one draw path. Fonts: DirectWrite bytes → subset Type0
(CIDFontType2/0, Identity-H, W, ToUnicode); outline fallback for restricted fonts; outline, GoTo/URI links, no JS/forms.

## F-37 cause
Each WM_MOUSEWHEEL was painted as its own synchronous frame and non-120 (free-spin) deltas skipped the animation,
so the queue backed up: 1500 × delta 15 on large-1mb (4.5 s of input) drained in ~33 s over 1507 frames. Now one
accumulator target, coalesced queue: 171 frames, done with the input; anchor fixes shift the running motion.

## Deviations
- Reopen with encoding uses encoding_rs (0 B) not MultiByteToWideChar; ISO-8859-9 shows as Windows-1254; read mode only.
- Copy as rich HTML runs on the UI thread; "history off" = `sidebar.show_history` false. BUG-05: rowspan only.
- Custom icon `general.app_icon` needs a UI_SPEC row patch (018a); DefaultIcon only rewritten if the ProgID exists.
- BUG-03: links, images, code, math, inline HTML never cut; emphasis keeps the old rule (existing test); no ref defs.
- BUG-04: legacy names without `;` decode only when not followed by `[A-Za-z0-9=]` (attribute rule used everywhere).
- CI fix push: RUSTSEC-2026-0414 (published during the phase) failed `cargo deny`; pulldown-cmark =0.13.4 → =0.13.5.
- Part 1: emoji COLR → outlines + invisible text; bold simulated by FillStroke, oblique by skew 0.25.

## Notes for 018a
- Risky: BUG-07 pairing runs per leaf over all text events (markers hidden; `hl` state in the detail builder);
  table `place()` grid; CF_HTML offsets (`cf_html_document`, HEADER_LEN 105); appicon premultiplied → straight alpha.
- Dev-tools verbs: `ui scroll-trace on|<file>`, `ui wheel-burst N[:D]`, `ui wheel-stream N:D:MS` (free-spin case:
  e.g. `ui wheel-stream 1500:15:3` on large-1mb.md must end at the target with ≤ 60 ms ease and no snap).
- Not checked (Wine): PDF link rects, sup/sub, math, Mermaid HTML/PNG, jump list, custom icon, CF_HTML paste into Word.

## Owner F-37 note
Free-spinning (notchless) wheel on large files used to stop and get stuck. Reproduced with high-rate non-120
WM_MOUSEWHEEL streams; fixed: any delta keeps scrolling to the requested target, no snap while scrolling.

Next action (018a or a new go-ahead): run the existing tests with `export.html_interactive=false` (driver
`export-html.yds`, `v1d-checks.ps1`), regen `menu-light.png`, fix the titlebar key path, relax/re-time job_samples;
find the M1 +1.17 MiB (suspect: jump-list COM/shell init on document open) and the T4 warm p99; then CI, merge, tag.

## Owner checklist / Sahip kontrol listesi
1. EN: Export a long doc to PDF; select text, use bookmarks and links. TR: Uzun bir belgeyi PDF'e aktarın; metin seçin, yer imleri ve bağlantıları deneyin.
2. EN: Export HTML; try sidebar, fold, theme toggle, find. TR: HTML aktarın; kenar çubuğu, katlama, tema, bul'u deneyin.
3. EN: Copy as rich HTML → paste into Word. TR: Zengin HTML olarak kopyalayıp Word'e yapıştırın.
4. EN: Right-click a diagram → Save as PNG. TR: Diyagrama sağ tıklayıp PNG olarak kaydedin.
5. EN: File ▸ Reopen with encoding on a Windows-1254 file. TR: Windows-1254 dosyada Kodlamayla yeniden aç.
6. EN: Pin YAMDV; check the jump list; set Settings ▸ App icon to an .ico. TR: YAMDV'yi sabitleyin; atlama listesini ve Uygulama simgesini deneyin.
7. EN: Spin a free-spin wheel fast on large-1mb.md. TR: large-1mb.md'de serbest dönen tekerleği hızlı çevirin.
8. EN: Publish nothing; 018a adds the tests and v1.4.0. TR: Yayın yok; testler ve v1.4.0 018a'da.

# V2-I plan (prompt 019, hardening)

Branch `phase/v2i-hardening` (off main 657e671). Issues: HARD-01 #50, HARD-02 #51, HARD-03 #52, FUT-47 #45; QA-01 #70 stays open.
Budget: F-39 diag 1/1 (run 41013638607), fuzz-long 0/1, full CI 0/1, fix push 0/1.

| Step | State |
|---|---|
| F-38 T7 timer-free jump list | code done (yamdv-core throttle + jumplist.rs) |
| F-39 M1 | diag done: HEAD 4.65 vs v2g 4.68 MiB; live heap equal (1 502 764 vs 1 501 804 B, same sites) |
| F-40 release notes | done |
| §2 audit / threat model / matrices / binary checks | done (72 OK / 7 fixed / 1 gap HARD-05 #93 / 1 manual) |
| §3 fuzz-long | workflow + 10 new targets + seeds; local mini-fuzz found 1 product bug (journal CR) + 2 checker false positives |
| §4 ACG / HC / UIA | not started |
| §5 qa-v2i pack | not started |

## CHECKPOINT 019-1 (2026-10-30 09:16)
- Prompt stored; F-38 coded (no timer; host starts at change if last > 30 s, else dirty → next change / window close / exit; seeded at start-up).
- Next: F-39 allocation-site tags + diag workflow.

## CHECKPOINT 019-2 (2026-10-30 09:21)
- F-40 done (CHANGELOG + v1.4.0 draft notes via gh release edit; still draft).
- F-39: entity table → static sorted array + binary search; allocsites.rs (alloc-sites feature) + temporary diag-v2i workflow (push on this branch; HEAD vs phase-v2g: M1, T7, top live sites).
- Budget: F-39 diag → this push (1/1).
- Next: read diag artifact; fix top new consumers; remove diag workflow.

## CHECKPOINT 019-3 (2026-10-30)
- F-39 diag (run 41013638607): M1 HEAD 4.65 / v2g 4.68 MiB; T7 HEAD 0 µs; top live sites identical (Δ +960 B). The V2-H excess came from the jump-list timer path (gone with F-38); diag workflow/scripts removed.
- §2: SECURITY_AUDIT.md + THREAT_MODEL.md; fixes S-FILE-06 (job file, uninstall helper), S-FILE-07 (corrupt rename), S-RES-02 (custom icon remote drive), S-RES-04/07 (SVG 16 384² / 64 MP), S-PRIV-02 (log rotation), S-SUP-02 (ADR-0024 lopdf); HARD-05 #93 (S-PROC-08) opened.
- §3: fuzz-long.yml (dispatch, 18 targets); local stable mini-fuzz (no libFuzzer on the box: apt mirror 500) → journal decode CR fix + regression; html_to_md / pdf_doc checker fixes.
- Next: push [skip ci]; dispatch fuzz-long once (temporary push trigger on a marker file); then §4.

## CHECKPOINT 019-4 (2026-10-30)
- Mini-fuzz found 2 more html_to_md issues (pre inside link / inline markers → fence lost): fixed in html2md (`wrap`, `a`), regression tests `hard02_*`, seeds; checker now parses the output with pulldown-cmark (raw HTML events only).
- fuzz-long: temporary push trigger on `.github/fuzz-long.run` (one run, budget 1/1) → wait ~75 min; then remove the trigger.

## CHECKPOINT 019-5 (2026-10-30)
- fuzz-long run 41016337092 in progress (18 jobs × 3600 s).
- §4 started: HC = `yamdv_core::hc` (system colours, atomics) → palette / chrome tokens / syntax / callouts / diagrams; live via WM_SETTINGCHANGE; driver `ui hc-on|hc-off`.
- UIA = `window/uia.rs`, UIAutomationCore loaded lazily (LoadLibraryExW SYSTEM32) on WM_GETOBJECT(UiaRootObjectId) only; client = `spikes` bin `uia-client`.

## CHECKPOINT 019-6 (2026-10-30 10:20 UTC+3)
- 4f32b3c: §4 HC (FUT-47) + UIA providers (window/uia.rs, lazy UIAutomationCore), spikes `uia-client`, driver `high-contrast` / `uia-tree`, goldens v2i-hc-* (SHELL_SHOTS 26). check-imports: 20 DLLs (no UIAutomationCore import).
- fuzz-long 41016337092: svg_parse (tiny-skia 16.16 overflow on a 1.7e18 arc) and html_to_md (code-span fence shorter than an inner `` run) crashed in minutes; both fixed + regress seeds + tests. 16 jobs still running.
- PR CI runs used for V2-I: 0 (diag-v2i 41013638607 + fuzz-long counted separately).
- Next: ADR-0001 V2-I rows, audit/threat-model UIA rows, OWNER_QA.md, collect fuzz-long artifacts, remove trigger, open PR.

### CHECKPOINT 019-7 (2026-10-30 10:30 UTC+3)
- Done: ADR-0001 V2-I ACG amendment (svg row in `acg-matrix`, late ACG in the native-PDF worker,
  font-picker step in `high-contrast.yds`); THREAT_MODEL §13 UIA; audit S-IPC-04/S-PROC-04;
  `tools/testpack/OWNER_QA.md` (EN+TR, 15 items, S-NET-05 in item 2); pack job builds
  `uia-client.exe`, BASELINE 657e671; build-pack.sh → `YAMDV-v2i-qa`.
- Waiting: fuzz-long 41016337092 (3/18 jobs done at 10:16). CI runs used: diag 41013638607,
  fuzz-long 41016337092; PR CI 0.
- Next: process fuzz artifacts → remove trigger ([skip ci]) → PR + 1 CI → fill `PRRUN` in ADR-0001
  → merge, tag → pack dispatch on main → `qa-v2i` draft → report.

### CHECKPOINT 019-8 (2026-10-30 11:05 UTC+3)
- fuzz-long 41016337092 done: 15.0 h, 1.98 G execs; findings svg_parse, html_to_md (fixed earlier),
  ini (fixed 1cfedd7), cf_html (harness RSS, no leak natively). Corpus samples 3.4 MB committed;
  temporary trigger + marker removed (a30d64e1). Local libFuzzer re-run impossible (no C++ compiler,
  apt has held packages — not forced).
- CI runs used: diag 41013638607, fuzz-long 41016337092; PR CI 0. No run in progress.
- Next: open PR → 1 CI → fill `PRRUN` → goldens/fixes (one push) → merge, tag → pack → qa-v2i → report.

### CHECKPOINT 019-9 (2026-10-30 11:10 UTC+3)
- PR #94 opened (Closes #50/#51/#52/#45). This commit (no [skip ci]) starts the single PR CI run.
- Next: wait for CI → collect gates/goldens → one fix push if needed → merge, tag phase-v2i.

### CHECKPOINT 019-10 (2026-10-30 11:25 UTC+3)
- PR CI 41023143760 green: M0 2.38, M1 4.59 MiB, T7 0/0, T4 typical warm p99 7.97 ms, T5 17.0/20.4/15.5,
  T10 258 ms/127 p, check-imports OK (21 DLLs MSVC), size 3 780 608 B; acg-matrix 8 yes + 3 webp placeholders;
  uia-tree uia_exit 0, PWS Δ 56 KiB; high-contrast font_list 89.
- Golden review: HC white settings — selected nav text was black on #37006e → fix (HIGHLIGHTTEXT on
  highlight fills: nav, font list, menus, marks). Fix push = this commit (budget 1/1).
  Black-dark golden committed; white-settings golden comes from this run's artifact ([skip ci]).
- Next: wait CI → commit white golden [skip ci] → merge, tag → pack → qa-v2i → report.

### CHECKPOINT 019-11 (2026-10-30 11:40 UTC+3)
- Fix push CI 41025722307 green; HC white golden fixed + committed. BUT M1 median 5.29 MiB
  (runs 5.26/4.60/5.29/5.37/5.38) vs 4.59 in 41023143760 (4.67/5.24/4.53/4.55/4.59): bimodal
  ~4.6 vs ~5.3 MiB (+0.7 MiB commit too) → phase gate M1 ≤ 5.01 not reliably met. Merge held.
- Next: pack workflow_dispatch on the branch (§5, also re-measures M1) → qa-v2i draft → report;
  merge/tag only if the owner accepts M1 or after a fix.

### CHECKPOINT 019-12 (2026-10-30 12:20 UTC+3)
- Pack dispatch 41027413612 red: pack-exes `BASELINE: 657e671` parsed as float (Infinity) → quoted on main;
  windows scroll-fling trace_p99 17.5 > 16.7 (flake); M1 median 4.78.
- Tip restored to green head 187fea4 (F-24); PR #94 squash-merged 278192ca; tag phase-v2i pushed; #45/#50/#51/#52 closed.
- No qa-v2i (owner: no further run this session). Next: dispatch ci.yml on main → build-pack → qa-v2i draft; M1 diag.

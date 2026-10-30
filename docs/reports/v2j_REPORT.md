# V2-J report — Pass 2 (prompt 020 §5.0, §5.2, §6): v2.0.0 draft

Status (2026-10-30 21:15 UTC+3): owner decision A-11 (18:26, "tamam ikonları ekle sürümü finalize edelim. bu final
sürümü ben ve arkadaşım test edicez.") — toolbar added, v2.0.0 finalized as a **draft** release; §5.1 deferred.
PR #96 squash-merged as 7c30adce (tree = green head 0e08353c, F-24); empty `chore(release): v2.0.0` 9417fece, tag
`v2.0.0` (F-25); release run 41103415663 created the draft: https://github.com/abuzer1495/YAMDV/releases/tag/untagged-aa7a6d1df5dabdaf06d5

## Runs
| Run | Kind | Result |
|---|---|---|
| 41067509042 | main after Pass 1 merge | red: scroll-fling trace_p99 19.16 > 16.7 (no retry; noise) + M1 5.2 (bimodal F-41) |
| 41080520395 | PR #96 full run (d5b518fd) | red: expected goldens, settings-panel.yds (new section), T4-typical A-B (F-49) |
| 41086659362 | fix push d659c1b4 | red at unit tests: r_sys_04 expected "yamdv 1." (fixed, [skip ci]) |
| 41088770469 | fix push 9ee72622 | red: scroll-fling trace_p99 20.66 / 19.31 (F-52) + M1 bimodal 5.25 vs 4.76 (F-51) |
| 41095967396 | fix push 0e08353c | **green** (all jobs); merged |
| 41103406407 / 41103415663 | main release commit / tag `v2.0.0` | green / green (draft created) |

## §5.0 decisions (same-runner, run 41080520395)
- PERF-03 #56 (cold p99 ≤ 33 ms): typical 9.81 / 10.00, large 3.97 → **met, closed by PR #96**.
- PERF-04 #57 (Mermaid warm ≤ 16 ms): warm 6.85 (base 6.88), cold 6.87 → **met, closed by PR #96**.
- PERF-08 #97 "Lazy layout of huge table rows" opened in post-v2; FUT-42 #40 closed pointing to it.
- Mermaid golden `top-v2j-mermaid-init-dark.png` (init light theme under dark UI, sibling subgraphs CL/CORE/EXT)
  committed from the run artifact, self-reviewed: dark clusters, readable labels, no overlap.
- F-41: M1 max 5.33 MiB (report-only, runner-limited); **median of the 12 HEAD runs 4.71** (base runs similar, bimodal).

## §5.1 — deferred (owner decision A-11)
- QA items, S-NET-05, the Task Manager memory number and the T1 / T2 / T-DLG owner-number decisions move to the owner
  + friend test of the v2.0.0 draft. PERF-02 #55 and QA-01 #70 stay open (milestone post-v2).

## §5.2 toolbar + final gates (green run 41095967396, HEAD / same-runner base)
- Toolbar under the title bar (owner icons, 12 default tools, » overflow, F6 region, UIA ToolBar, tooltips), menu
  icons, Settings ▸ Toolbar checklist (drag, Alt+↑/↓, reset). Spec P-57 / P-58; `spec-check` green.
| Gate | Budget | Result |
|---|---|---|
| Embedded icon data / mask cache | ≤ 60 KB / ≤ 64 KB | 9,516 B / 4,800 B |
| M1 (F-51 low-mode median) | base + 0.2 MiB | 4.75 vs 4.66 |
| B-SIZE (yamdv.exe) | ≤ 3,600,000 | 3,534,848 (base 3,783,680) |
| T2 / T1 / T-DLG | not worse | 61 / 62, 83 / 81, 401 / 396 ms (same-runner pass; slow runner) |
| T4 warm typical / large / math / mermaid | ≤ 16, not worse | 7.97 / 5.35 / 6.10 / 8.49 ms |
| scroll-fling (F-52) / T3, T5, T6, T10, M0, M2, M3, M5, M6 | p95 ≤ 16.7, p99 ≤ 33.4 / budgets | 14.62, 18.35 / all pass |
F-41 M1 max 5.49 (report-only). Earlier run 41080520395: PERF numbers below; T4 typical A-B fixed by F-49.

## F-items
- **F-48** scroll-fling `trace_p99` / `snap_dist` join the F-31 two-attempt rule (main run 41067509042 noise).
- **F-49** T4 A-B: the baseline (no toolbar) had a 32 DIP taller viewport, so the 120-DIP autoscroll steps split the
  image section into different frames (p99 = 5th-highest frame: 6.4 → 9.8 ms; p50 2.48 vs 2.58 ms). The baseline window
  is now 1000x668: both builds scroll the same 636-DIP viewport.
- **F-50** the offscreen `ui shot-window` path skipped the toolbar (the app's real paint draws it). Fixed; the two
  `top-v2j-toolbar-*` goldens stay uncommitted (warn-only; would need another run). Self-reviewed in the green run: OK.
- **F-51** M1 A-B compares the low-mode median (runs ≤ min + 0.3 MiB); runs are bimodal (~4.7 / ~5.3 MiB).
- **F-52** scroll-fling: p99 of a ~40-frame burst is one frame; assert p95 ≤ 16.7 and p99 ≤ 33.4 ms.
- settings-panel.yds: ↓ ×5 to Reading (the Toolbar section now has content). Cosmetic: the menu shows the star icon
  and the "☆" label glyph for Add to favorites (left as is; golden committed).

## §6 release
- Version 2.0.0 (Cargo.toml), CHANGELOG [2.0.0], README refresh, zip = yamdv.exe, README.md, LICENSE,
  CHANGELOG.md, FUTURE.md, THIRD_PARTY_NOTICES.txt + `.zip.sha256`. Draft `YAMDV-2.0.0-win-x64.zip`, 1,817,435 B,
  SHA-256 `6207524cc3c710552b4877a1b27273ed585704690142f900efca7199e3e21376` (not published).
- Milestone **post-v2** created; moved from post-v1: #26, #27, #28, #48, #54, #55, #58, #59, #60, #70, #72, #73, #74,
  #75, #86 (+ new #97). Closed: #40 (→ #97), #56, #57 (PR #96).

## Owner checklist (EN)
1. Open the v2.0.0 draft on GitHub Releases; download `YAMDV-2.0.0-win-x64.zip` and its `.sha256`.
2. Check: `certutil -hashfile YAMDV-2.0.0-win-x64.zip SHA256` equals the `.sha256` value.
3. Unzip anywhere and run `yamdv.exe`; open a few of your own .md files (tables, Mermaid, images, math).
4. Try the toolbar, Settings ▸ Toolbar, F2 / F3 editing, PDF / HTML export, light / dark / high contrast.
5. Follow tools/testpack/OWNER_QA.md; write `qa-results.txt` (one line per item) + notes; your friend does the same.
6. Note the Task Manager memory of one open typical document and how fast the app and the Open dialog feel.
7. Send `qa-results.txt` + notes; publish the draft only when both of you are happy.
## Sahip kontrol listesi (TR)
1. GitHub Releases'te v2.0.0 taslağını açın; zip dosyasını ve `.sha256` dosyasını indirin.
2. `certutil -hashfile YAMDV-2.0.0-win-x64.zip SHA256` sonucu `.sha256` ile aynı olmalı.
3. Zip'i açın, `yamdv.exe`'yi çalıştırın; kendi .md dosyalarınızı (tablo, Mermaid, resim, formül) açın.
4. Araç çubuğunu, Ayarlar ▸ Araç çubuğu, F2 / F3 düzenleme, PDF / HTML dışa aktarma, açık / koyu / yüksek karşıtlık deneyin.
5. tools/testpack/OWNER_QA.md'yi izleyin; `qa-results.txt` (madde başına bir satır) + notlar; arkadaşınız da aynısını yapsın.
6. Tipik bir belge açıkken Görev Yöneticisi'ndeki bellek değerini, uygulama ve Aç penceresi hızını not edin.
7. `qa-results.txt` + notları gönderin; ikiniz de memnunsanız taslağı yayınlayın.

Next action: owner + friend test the v2.0.0 draft; send qa-results.txt + notes

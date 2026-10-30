# 020_V2J_PERFORMANCE_2_0 — YAMDV Phase V2-J: QA pack, license, final budgets, performance, size → v2.0.0

> **Prompt 020.** This is the last phase of the v2 plan (`docs/ROADMAP_V2.md`).
> - Branch `phase/v2j-perf`, tags `phase-v2j` and **`v2.0.0`**
> - Report `docs/reports/v2j_REPORT.md` (≤ 80 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-10) > this prompt > `docs/PERFORMANCE.md` / `docs/SPEC.md` / `docs/SECURITY.md` > the rest of `docs/*`.

**Two passes.** The owner tests the QA pack (§1.1) on his own PC while you work.
- **Pass 1** covers §1–§4. At its end, check whether the owner has attached his QA results (`qa-results.txt` and notes) below this prompt.
  - **No results attached:** merge Pass 1, tag `phase-v2j`, write the report with **"Next action: waiting for owner QA results"**, and stop.
  - **Results attached:** continue with §5–§6 in the same pass.
- **Pass 2** happens when the owner gives this prompt again (revised 2026-10-30 after the Pass 1 report) with the QA results attached. Pass 1 is merged (`phase-v2j` on 1e84179d). Skip whatever is already done, and do **§5.0, §5.1, §5.2 (toolbar) and §6**.

**CI budget:**
- **Pass 1:** 1 diagnostic run (F-41), 1 full run, at most 1 fix push.
- **Pass 2:** 1 full run, at most 1 fix push.
- **Not counted:** the pack dispatch, the `fuzz-long` dispatch and the release tag run.
- **More runs** need the owner's OK.

**Resumable:** see §8.

---

## 1. Start here (Pass 1)
1. **Owner QA pack, before anything else** (from the 019 report):
   1. Make sure the `BASELINE` value in `ci.yml` is quoted.
   2. Run `gh workflow run ci.yml --ref main` (pack job), then `tools/testpack/build-pack.sh <run-id>`.
   3. Run `gh release create qa-v2i --draft --prerelease` with `YAMDV-v2i-qa.zip` and its `.sha256`.
   4. **Tell the owner in the chat right away.** Post the release link and the SHA-256, plus: "Run `run-tests.cmd` and the 15 items in `OWNER_QA.md`, then give prompt 020 again with `qa-results.txt` and your notes attached." Then keep working.
2. **Fuzz re-run** (HARD-02 leftovers): `gh workflow run fuzz-long.yml --ref main` for `svg_parse`, `html_to_md`, `ini` and `cf_html`, 1 h each.
   - For `cf_html`, fix the harness growth or raise `-rss_limit_mb`; the product is flat.
   - Check the results before the end of Pass 1. Every crash becomes a fix plus a regression test.
3. **License: AGPL-3.0-or-later** (owner decision, 2026-10-30; closes DEC-01 #71).
   - **`LICENSE`:** the full official text of the GNU AGPL v3, unchanged.
   - **Crates:** `license = "AGPL-3.0-or-later"` in every workspace crate, and an SPDX header `// SPDX-License-Identifier: AGPL-3.0-or-later` in every source file (by script).
   - **README and About:** "License: AGPL-3.0-or-later", plus the source URL `https://github.com/snipeTR/YAMDV`, with the exact tag in About.
   - **Third-party notices:** `cargo xtask notices` generates `THIRD_PARTY_NOTICES.txt` from `cargo metadata` and each dependency's license / copyright files. The file ships in the release zip.
   - **`cargo deny`:** the license policy allows only AGPL-3.0-compatible dependency licenses: MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode-3.0 / Unicode-DFS-2016, BSL-1.0, CC0-1.0, and MPL-2.0 (only without "Incompatible With Secondary Licenses"). List any other license in the report; do not add it silently.
4. **HARD-05 #93 (S-PROC-08):** temp and staging paths must not come from environment variables. Use `SHGetKnownFolderPath(FOLDERID_LocalAppData)` → `YAMDV\tmp\<random>\`, created with `CREATE_NEW` and deleted on success, on failure and at exit. Apply P-55, and close #93.

**Spec patches** (`docs/spec-patches/v2j.txt`, applied with `xtask spec-patch`):

**P-55** — `docs/SECURITY.md`, S-FILE-06. OLD:
```text
or `%TEMP%\YAMDV-<random>\` (export staging)
```
NEW:
```text
or `<LocalAppData>\YAMDV\tmp\<random>\` resolved with `SHGetKnownFolderPath(FOLDERID_LocalAppData)`, never from environment variables (export staging and print/PDF jobs; S-PROC-08)
```

**P-56** — `docs/SPEC.md`, R-OPEN-06. **Apply it only if ADR-0024 (§3.9) chooses the Windows code-page API.** OLD:
```text
and decode with `encoding_rs`.
```
NEW:
```text
and decode / encode with the Windows code-page API (`MultiByteToWideChar` with `MB_ERR_INVALID_CHARS`, `WideCharToMultiByte` with `WC_NO_BEST_FIT_CHARS` and a lossless check); UTF-8 and UTF-16 natively (ADR-0024).
```

## 2. Fix first (Pass 1)
| ID | Fix |
|---|---|
| **F-41** | **M1 is bimodal** (4.59 / 5.29 / 4.78 / 4.68 MiB over 4 runs; commit grows too), so something allocates nondeterministically within the 3 s settle. In one diagnostic run, measure M1 **10 times**, each with the allocation-site report, and compare the low and high runs. Suspects: background work whose timing varies (image worker decode buffers kept after the blit, a DirectWrite fallback / font cache built on demand, layout warming of off-screen blocks, the watcher buffer). Make that work deterministic and bounded. **Gate:** all 10 runs ≤ 5.0 MiB, median ≤ 4.8 MiB. |
| **F-42** | **UIA control patterns** (R-A11Y-03): Invoke (buttons, menu items), Toggle (toggles, checkboxes, fold toggles), Value / RangeValue (text inputs, sliders), ExpandCollapse (submenus, tree items, sections), SelectionItem (tabs, list items), so that Narrator can operate the UI. They are still zero-cost without a client. |
| **F-43** | **Copy as rich HTML** runs on the UI thread. For documents over 256 KB, run it on the export worker with the progress card (R-EXP-07). |

## 3. Performance and size (PERF-01…07, FUT-42) (Pass 1)
1. **Stable gates first** (PERF-07). Each measurement job runs the **baseline** exe (the `phase-v2i` build, from its artifact) and **HEAD** on the same runner, interleaved A-B-A-B over 3 rounds. A gate passes only if **both** hold:
   - the absolute final budget (D.2 "Final" column);
   - the same-runner comparison: times ≤ 1.15 × baseline, memory ≤ baseline + 0.2 MiB.

   The two-attempt rule still applies, and the report shows both numbers.
2. **Final budgets as hard gates** (PERF-01, D.2 "Final"):

   | Group | Final budgets |
   |---|---|
   | Memory | M0 ≤ 3.0 · M1 ≤ 5.0 · M2 ≤ 10 · M3 ≤ 6.0 · M4 ≤ +1.5 · M5 ≤ 6.0 MiB · M6 · M7 |
   | Times | T1 ≤ 80 · T2 ≤ 60 · T3 ≤ 50 · T4 ≤ 16 · T5 ≤ 16 / 25 · T6 ≤ 100 · T8 ≤ 50 · T9 ≤ 30 ms |
   | Other | T7 = 0 · T10 |

   - Bring back every measurement that A-5 made report-only.
   - A budget that the GPU-less runner cannot reach, even though the code is right, goes on a short list with its evidence. The owner decides about it, and the owner QA numbers are used where they exist.
3. **Dialog latency** T-DLG ≤ 400 ms (PERF-02).
   - Make the dialog host start lean: host mode skips the renderer, DirectWrite and settings work it does not need.
   - No pre-spawned host (RAM).
   - Report the CI numbers and the owner's number from QA.
4. **Cold scroll** p99 ≤ 33 ms, warm ≤ 16 ms (PERF-03).
   - Cache scaled image bitmaps and code-block rasters within the image LRU budget.
   - Paint only intersecting rects.
5. **Mermaid scroll** (PERF-04): cache rasters of visible diagrams within the LRU budget; it stays sharp at the current zoom.
6. **Incremental live reload** (PERF-06):
   - An external change is diffed against the buffer and applied through the editor's splice path, so only the changed blocks are re-parsed and laid out again.
   - The scroll anchor is kept.
   - **T6 ≤ 100 ms**, on both `typical.md` and `large-1mb.md`.
7. **FUT-42**
   - **Table rows:** layout is lazy, so only the visible rows of huge tables are laid out.
   - **Code toolbar:** it fades (B.15), with a timer only during the fade.
   - **ADR-0025 "DIB vs DDB"**: measured memory and speed; keep the faster or lighter option.
8. **T5 split p99:** 17.0 ms → **≤ 16 ms**.
9. **Binary size** (PERF-05), ADR-0024 "Size plan":
   - **Gate:** **≤ 3,500,000 B** (stretch 3.0 MB). It was 3,780,608 B.
   - **Candidates** (measure each with `cargo xtask size-map` and take them in order of savings ÷ risk):
     - (a) Replace the `encoding_rs` decode/encode tables (~180 KB) with the Windows code-page API. Check that `chardetng` stays for detection, then P-56. Before switching, byte-exact round-trip tests must pass for every encoding in the Reopen menu.
     - (b) The `[anonymous constants]` (~405 KB): find the big tables and remove duplicates or tables that are not needed.
     - (c) Monomorphization bloat in `yamdv_platform_win::window`.
     - (d) `#[cold]` / `opt-level` per module, while T4 and T5 stay green.
   - **Off-limits:** nothing may break a spec behavior or a gate.

**Tests (A-10):**
- **Driver scripts (2):**
  - `live-reload-incremental`: an external edit of one paragraph → only that block is re-laid out, T6 is measured, and the anchor is kept.
  - `uia-patterns`: the UIA client invokes a button, toggles a setting and expands a menu.
- **Unit tests:** the pure code (the diff → splice path, the encoding round trips for (a), the table-row laziness).
- **Gates:** the §3.2 gates.

**End of Pass 1:**
1. Make one CI run (+1 fix push), merge and tag `phase-v2j`.
2. If no QA results are attached, write the report and stop (see the header).

## 4. Report after Pass 1 (if you stop there)
- **§1:** the pack link and SHA-256, the fuzz results, the license (and any license outside the list), #93.
- **F-41…F-43:** before → after.
- **Gates:** absolute and same-runner, before → after.
- **Size:** before → after, by candidate.
- **ADRs:** ADR-0024 and ADR-0025, 2 lines each.
- **Budgets the runner cannot reach:** the list, with evidence.
- **Last line:** "Next action: waiting for owner QA results".

---

## 5. Owner QA results (Pass 2; QA-01)
### 5.0 Pass 1 leftovers — decide them, do not start new performance work before 2.0
- **Accepted from Pass 1:**
  - F-44…F-47 (the owner's Mermaid dark-mode and subgraph fixes);
  - size 3,495,424 B;
  - ADR-0025 / ADR-0026;
  - F-41 as report-only (owner decision).
- **§3.4 cold scroll and §3.5 Mermaid scroll:** measure them once with the same-runner A/B, in the Pass 2 CI run: cold p99 on `typical.md` / `large-1mb.md`, and warm / cold p99 on `mermaid-heavy.md`.
  - If PERF-03 (cold ≤ 33 ms) and PERF-04 (warm ≤ 16 ms) are met, close those issues.
  - Otherwise move them to `post-v2`, with the numbers.
- **§3.7 table-row laziness:** not done. Open `PERF-08: Lazy layout of huge table rows` in `post-v2`; FUT-42 closes with a comment pointing to it.
- **Runner-limited gates:**
  - **T1 / T2 / T-DLG** (85 / 62 / 412 ms on the runner): use the owner's numbers from `test-results.txt` / OWNER_QA.
    - Real PC within the budgets: close PERF-02 and record "runner-limited, met on real hardware".
    - Otherwise move them to `post-v2`, with the numbers.
  - **F-41 M1 max:** stays report-only. Report its median.
- **Missing golden:** commit one Mermaid golden for the F-44 / F-46 case (`%%{init}%%` light theme under the dark UI, plus sibling subgraphs) from the Pass 2 CI run, self-reviewed.

### 5.1 QA items
- **Each failed or odd QA item** becomes an F-item (F-48, F-49, …), fixed with a test where one is possible. Items that only real hardware can show are marked "owner re-check".
- **Network check:** S-NET-05 (no sockets, firewall test) comes from the owner's QA item 2; record it in `SECURITY_AUDIT.md`.
- **Memory:** the owner's Task Manager memory number is compared with M1 and reported.
- **QA-01 #70:** close it when every item passes or has its own issue.

### 5.2 Top toolbar with the owner's icons (owner decision, 2026-10-31)
The owner uploaded **95 SVG icons** to `icons/` on `main` (made by Claude Design from `decisions/TOOLBAR_ICONS.md`; IDs = keymap action IDs).
- **Checked:** every file uses `viewBox 0 0 24 24`, a 1.5 px stroke and `currentColor` only. There is no script, text, image, href, style, event attribute or entity, and all IDs in the list are covered.
- **Size:** the SVGs are verbose — 770 KB of text in total.

1. **Move and convert the files.**
   - `git mv icons/` → `assets/icons/`. Keep `index.json`; the review sheet goes to `docs/design/icons-review.png`.
   - A build step (`build.rs` or `cargo xtask icons`) parses every SVG with our `yamdv-svg` crate (S-RES-07 rules) and emits a **compact binary path table** (quantized commands + stroke flags) that is embedded in the exe.
   - **Never embed the raw SVG text.** Budget for the embedded data: **≤ 60 KB**.
2. **Rendering.**
   - Rasterize with tiny-skia into an **alpha mask** for each size × DPI, and tint it with the current token color: `text`, `text_muted` when disabled, `accent` for pressed/on, or the system colors in High Contrast.
   - The masks live in a small cache (≤ 64 KB) that is built only when the toolbar or a menu is first painted.
   - When nothing is shown, the cost is zero.
3. **The toolbar.**
   - **Placement:** a thin bar directly under the title bar, height **32 px**. Buttons are 28 × 28 with 20 px icons, a radius-6 hover/pressed overlay and 4 px gaps; a separator between groups is 1 px `separator`.
   - **Hidden in:** F11, the fullscreen viewers and the mind map.
   - **Default items** (`toolbar.items`, in this order): `open_file` · `live_edit`, `split_edit` | `find`, `heading_jump`, `toggle_sidebar` | `export_pdf`, `print`, `mind_map` | `toggle_favorite`, `theme` (cycles System → Light → Dark, with the matching icon) | `settings`.
   - **States:**
     - Toggle states are visible: favorite on/off, Live and Split pressed, sidebar on.
     - Commands that do not apply in the current mode are disabled.
     - Format and Insert items appear only in F2 / F3.
   - **Overflow:** items that do not fit go into a » overflow menu.
   - **Tooltip:** name + shortcut (V2-C tooltip).
   - **Keyboard and UIA:** `F6` reaches the bar and the arrow keys move inside it. UIA exposes a Toolbar with Button / Toggle patterns. High Contrast is supported.
4. **Settings → new "Toolbar" section.**
   - `toolbar.visible` (default on)
   - `toolbar.icon_size` (16 / 20 / 24, default 20)
   - **Choose and reorder** the items: a checklist of all toolbar-capable IDs, with drag and `Alt+↑/↓` like Favorites.
   - **Reset to default.**
5. **Menus use the same icons.** The context and ⋯ menus show the icon in their 20 px icon column (B.7.1), wherever an item has an ID with an icon.
6. **Spec.** Add a UI_SPEC section **B.4.1 Toolbar** and the three settings rows, by spec patch (P-57/P-58). Choose exact anchors that match once; `spec-check` must stay green.
7. **Tests (A-10):**
   - one driver script, `toolbar`: the default items show; Live Edit toggles and the button is pressed; toggling Favorite switches the icon; disabling an item in Settings removes it; a narrow window moves items into »;
   - one golden: the toolbar in light and in dark;
   - a unit test for the binary path table (round trip, size ≤ 60 KB).
8. **Gates:**
   - M0 and M1 within the final budgets; the toolbar may add at most **+0.15 MiB** to M1;
   - T2 and T4 not worse than in Pass 1 (same-runner A/B);
   - **B-SIZE ≤ 3,600,000 B** for v2.0.0. Owner decision: size is secondary; this limit replaces the 3,500,000 B gate for this pass only.

## 6. Release v2.0.0 (Pass 2, after §5)
1. **Version** 2.0.0 in the workspace, the version resource and About.
2. **`CHANGELOG.md`:** entries for V2-I and V2-J, plus a short **2.0 summary** (what v2 added since 1.0).
3. **`README.md` refresh:**
   - the features, with screenshots from goldens;
   - install / portable / uninstall;
   - privacy and how to verify it;
   - the keyboard summary;
   - the AGPL license.
4. **Release zip:** `yamdv.exe`, `README.md`, `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `CHANGELOG.md`.
5. **Milestone housekeeping:** list every issue that is still open in `post-v1`, move them to a new milestone `post-v2`, and report the list.
6. **Tag:** after CI is green, make the `chore(release): v2.0.0` commit and tag `v2.0.0` (F-25 / F-32). The tag job creates the **draft** release (zip + SHA-256); the owner publishes it.

**Final report (≤ 80 lines):**
- the §5.0 decisions, with numbers
- the §5.1 results
- the final gates
- the size
- the open issues moved to `post-v2`
- the `v2.0.0` draft link and its SHA-256
- the owner check list (≤ 8 steps, EN + TR) for the final release

## 7. Acceptance criteria
| ID | Criterion |
|---|---|
| AC-1 | The `qa-v2i` pack is uploaded and the owner was told at once; the fuzz re-run is done and its crashes fixed; the AGPL license is applied with the notices file and the deny policy; #71 and #93 are closed; P-55 is applied (and P-56 if chosen) with `spec-check` green. |
| AC-2 | F-41 (10 runs ≤ 5.0 MiB, median ≤ 4.8), F-42 and F-43 are done. |
| AC-3 | §3 items 1–9 are done: same-runner gates and the final budgets are hard gates (or listed with evidence); T6 ≤ 100 ms; T5 split ≤ 16 ms; the exe is ≤ 3,500,000 B; ADR-0024 and ADR-0025 are written; the 2 driver scripts pass. |
| AC-4 | **Pass 2:** the §5.0 leftovers are decided (closed or moved to `post-v2` with numbers); the QA results are processed (§5.1) and QA-01 is closed; the toolbar (§5.2) works with its test and golden; v2.0.0 is released as a draft per §6 with the zip + SHA-256; the issues are moved to `post-v2`; the final report follows §6. |

## 8. Stop rule (quota)
If the Grok quota or the Actions minutes run low at any point:
1. commit and push with `[skip ci]`;
2. write `docs/reports/v2j_REPORT.md` with a done / partial / not-started table (§1.1–§1.4, F-41…F-43, §3.1–§3.9, §5.0, §5.1, §5.2, §6) and **"Next action: …"**;
3. stop.

The owner will give this same prompt again.

**END OF PROMPT 020.** Start with §1.1 now.

# 018a_V2H_TESTS_RELEASE — YAMDV Phase V2-H, part 2: finish 018, tests, fixes, v1.4.0

> **Prompt 018a (revised 2026-10-30).** It completes Phase **V2-H**. Prompt 018 built the features (owner decision: no new tests) but stopped **partial**: PR #92 is not merged, CI is red and M1 grew by +1.17 MiB. This prompt first **finishes 018** (§1), then adds the tests (§2), fixes what they find, closes the issues and releases v1.4.0.
> - **Branch:** keep working on `phase/v2h-export` and **PR #92**; there is no new branch. Tags `phase-v2h` and **`v1.4.0`**.
> - Report `docs/reports/v2h_tests_REPORT.md` (≤ 80 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-10) > this prompt > prompt 018 (`docs/prompts/018_V2H_EXPORT_INTEGRATION.md`) > `docs/*`.
> **Order:** §1 → §2 → §3 → §4 → §5. **Resumable:** see §8.

**CI budget for this prompt (owner decision):** one diagnostic run for §1.1, then one full run, plus at most **two** fix pushes. If CI is still red after that, stop and report; the owner decides about more runs.

---

## 1. Finish 018 first (from `docs/reports/v2h_REPORT.md`)
Read that report, including its "Notes for 018a". If the owner pasted notes from his own checks below this prompt, each one becomes an F-item numbered after the last F-number used in 018; fix them here, each with a test.

### 1.1 M1 regression: 5.88 MiB vs 4.71 MiB (limit 5.01) — must be fixed
1. **Diagnose with one diagnostic CI run:**
   - M1 with the dev-tools counting allocator and `ui mem-report` (Rust heap, process heap commit, GDI/USER);
   - the **list of loaded modules with their sizes**, diffed against `phase-v2g`.
   Name the cause in the report.
2. **Suspects:**
   - (a) the jump-list update (`ICustomDestinationList` / `IShellLink`) on document open loads shell components into the main process; PERFORMANCE rule 10 keeps shell components out of it;
   - (b) the record-mode band of the shared draw path keeps a command buffer alive for screen drawing;
   - (c) the app-icon or encoding code runs at open.
3. **Fix rule:**
   - **Jump list:** if (a) is the cause, do the jump-list update **out of process**: a dialog-host kind `jumplist` (P-53). Start it only when the recent list really changes, debounced to at most once per 30 s plus once at exit.
   - **Draw buffers:** buffers of the shared draw path are reused and bounded, and they are freed after a PDF job.
   - **Target:** M1 ≤ 5.01 MiB; M0 stays ≤ 2.70 MiB.

### 1.2 T4 warm p99: 11.5 ms vs 8.7 ms (V2-G) — must be fixed
Use the scroll trace to find the extra cost. Suspects: the record-mode band adding a pass on screen paints; the F-37 accumulator running work per message. Target: **T4 warm p99 ≤ 10.0 ms** (two-attempt rule). T5 stays within its limits.

### 1.3 Existing tests that 018 broke — fix the tests only where the behavior change is intended
- **`menu-light.png`:** check the new ⋯ menu by eye (the *Copy as rich HTML* item), then regenerate the golden from the run artifact.
- **`titlebar.yds` popup_levels:** pick the submenu by its label, not by its position (4×Down), so that new items cannot break it.
- **`export-html.yds` and `.github/scripts/v1d-checks.ps1`:** they test the **static** export. Set `export.html_interactive=false` in them; the interactive mode gets its own test in §2-C.
- **`export-progress.yds` job_samples 2 < 3:** the native PDF is faster now. Use `large-1mb.md` (or a fixture of ≥ 300 pages) so that the card really shows progress. Keep the assertion: ≥ 3 samples, rising.

### 1.4 Spec patches (`docs/spec-patches/v2h-b.txt`, applied with `xtask spec-patch`)

**P-53** — `docs/SECURITY.md`, S-PROC-07. OLD:
```text
which shows exactly one native file-open/save or print dialog and exits
```
NEW:
```text
which shows exactly one native file-open/save or print dialog, runs one print/PDF export job (ADR-0018) or performs one taskbar jump-list update, and exits
```

**P-54** — `docs/UI_SPEC.md`, B.8.2. INSERT AFTER the anchor line:
```text
| `general.remember_window_geometry` | bool | `true` | |
```
NEW line:
```text
| `general.app_icon` | path | `""` | empty = default; absolute `.ico` ≤ 1 MB (R-SYS-08) |
```

### 1.5 Accepted from the 018 report
- the F-37 cause and fix (free-spin wheel deltas drained over ~33 s; now there is one accumulator target);
- the §2.2 savings (−225 KB);
- ADR-0023;
- `encoding_rs` for *Reopen with encoding*;
- the pulldown-cmark 0.13.5 bump (RUSTSEC-2026-0414);
- the BUG-03 and BUG-04 rules as described.

---

## 2. Tests to add
Every item below refers to the item numbers in prompt 018. Test budget for this prompt (owner decision): **5 driver scripts and 2 goldens**, plus unit tests. Put pure logic in platform-independent crates.

**A. Scrolling** (018 F-37, BUG-12)
- **Driver script `scroll-fling`**, on `large-1mb.md`, using the 018 scroll trace and its verbs `ui scroll-trace`, `ui wheel-burst` and `ui wheel-stream`:
  - send 30 wheel notches within 300 ms; `scroll_y` moves monotonically on every frame, with no stall of ≥ 2 frames and no reversal;
  - the end position is within one notch of the sum of the deltas (no snap);
  - frame time p99 is ≤ 16.7 ms;
  - one slow notch that ends 10 px from a heading settles onto it with `scroll_snap = smart`, and stays put with `off`;
  - touchpad-style fractional deltas scroll directly, with no animation;
  - the free-spin case `ui wheel-stream 1500:15:3` ends at the target within 60 ms after the last input, with no snap.
- **Unit tests:** the wheel accumulator / velocity model (retargeting, the fast threshold, the settle conditions).

**B. Native PDF** (018 items 1–6)
- **`cargo xtask pdf-inspect <file>`:** prints a JSON summary of a PDF. It uses `lopdf` **as a dev-dependency only**, so the exe size is not affected.
- **Driver script `pdf-native`:** export `typical.md` and `tests/fixtures/colors.md`, through the UI and headless. Check:
  - the page count;
  - the outline count equals the number of headings;
  - internal GoTo links exist, and URI links are only `http`, `https` or `mailto`;
  - there is no `/JavaScript`, `/Launch`, `/EmbeddedFile` or `/AcroForm`;
  - every font is subset-embedded with a ToUnicode map;
  - a known sentence is found in the extracted text, so the PDF is searchable;
  - with `pdf_colors=light` in dark UI mode, the page background is white;
  - Cancel after 5 pages leaves no file;
  - the main-process PWS is back within +0.2 MiB afterwards.
- **T10:** on a 100-page fixture, ≤ 3 s, with ≤ +30 MB transient memory in the worker. Also report `large-1mb.md` (pages and seconds).
- **Unit tests:**
  - the xref / object writer
  - the outline tree
  - link rectangles
  - the ToUnicode CMap
  - the glyph remapping after subsetting
  - the `fsType` → outline fallback

**C. HTML and clipboard** (018 items 7–9)
- **Driver script `html-interactive`:**
  - the CSP `sha256` in the meta tag equals the hash of the inline script;
  - there is no external reference of any kind;
  - math and Mermaid are inline `<svg>`;
  - with `html_interactive=false` the file contains no `<script`;
  - *Copy as rich HTML* puts `CF_HTML` without `<script` on the clipboard;
  - *Save as PNG…* on a diagram and on the mind map writes valid PNG files.
- **Unit tests:**
  - the script hash is recomputed and compared;
  - the script source is scanned for forbidden APIs (`eval`, `Function(`, `innerHTML`, `fetch`, `XMLHttpRequest`, `WebSocket`);
  - the XSS suite (≥ 50 vectors, S-TEST-02) runs in both modes.

**D. Integrations and known bugs** (018 items 10–13, F-34)
- **Driver script `integrations-bugs`:**
  - a windows-1254 file that is detected wrongly can be reopened as 1254; the text is then correct, and saving is byte-exact;
  - editor detection uses fake install folders (dev-tools override), and the `CreateProcessW` arguments are captured instead of launching;
  - BUG-01: `Ctrl+click` on `x.md#frag` scrolls the new window to the anchor;
  - BUG-02: a single-line `$$\frac{a}{$$` is shown as display math in the error style;
  - CF_HTML offsets (`cf_html_document`): the StartHTML/EndHTML/StartFragment/EndFragment values match the real byte offsets.
- **Driver script `jumplist-icon`:**
  - the jump-list update runs out of process (P-53) and its builder output contains the recent files;
  - the main process loads no new shell modules (module list);
  - *Clear history* empties the list;
  - a custom `.ico` is accepted, and an invalid path or a file over 1 MB is rejected.
- **Unit tests:**
  - the entity table (2 231 names, spot checks)
  - the rowspan model (layout, export, CSV)
  - encoding round trips
  - editor argument quoting
  - BUG-03 exact slices (reference links, autolinks)
  - BUG-06 per-line alignment
  - the BUG-07 event-stream pass
  - F-34 macros across blocks
- **Goldens (2):** a rowspan table with wrapped, per-line aligned cells (light); highlight across emphasis together with named entities (dark).
- **Extra unit test from the 018 notes:** app-icon premultiplied → straight alpha conversion.

---

## 3. Fix what the tests find
- **Fix the product, not the test.** Change a test only if the test itself is wrong, and give the reason in one line in the report.
- **Measurements:** the two-attempt rule (F-21 / F-22 / F-31) applies.
- **CI budget:** as in the header: one diagnostic run, one full run and at most two fix pushes for the whole prompt.

## 4. Close the issues
- **PR body:** edit the body of PR #92: replace the `Refs` lines with **one `Closes #n` per line** for FUT-39, FUT-21, FUT-23, FUT-10, FUT-09, FUT-25, BUG-01…BUG-07 and BUG-12.
- **After the merge:** check with `gh issue view` that each issue is closed; close any that are still open by hand, with a comment that references the PR.

## 5. Release v1.4.0
1. Set version 1.4.0 in the workspace, the version resource and About.
2. Add a `CHANGELOG.md` entry for V2-G and V2-H.
3. After CI is green, merge PR #92 and tag `phase-v2h` on the merge (F-24).
4. Make the `chore(release): v1.4.0` commit (no `[skip ci]`) and tag `v1.4.0` (F-25 / F-32). The tag job creates the **draft** release with the zip + SHA-256.

**Gates (V2-H as a whole, vs V2-G):**
- M0 and M1 at most +0.3 MiB (from 2.40 / 4.71 MiB): **M1 ≤ 5.01 MiB** (§1.1).
- **T4 warm p99 ≤ 10.0 ms** (§1.2), T5, T7, **T10**.
- **Exe size: report only.** Owner decision (2026-10-30): size work waits for V2-J, so there is no B-SIZE gate in this prompt. Record this as one line in A-10.

---

## 6. Report (≤ 80 lines)
- **§1:** the M1 cause and its fix (before → after), the T4 cause and its fix, and the 4 test fixes.
- **Test results:** one line per new script and unit-test group (pass/fail).
- **Gates:** including T10, the `large-1mb.md` PDF time and the exe size.
- **Fixes:** one line each, with the failing test that found it.
- **Issues:** the list of closed issues.
- **Release:** the `v1.4.0` draft link and its SHA-256.
- **Owner check list** (≤ 8 steps, EN + TR), covering:
  - fast and slow wheel scrolling through a long document (`scroll_snap` smart / off);
  - a PDF opened in Edge or Acrobat: selectable and searchable text, bookmarks, clickable links;
  - a 100-page PDF in a few seconds;
  - interactive HTML in a browser: TOC, folding, theme toggle, find;
  - *Copy as rich HTML* into Word;
  - *Save diagram as PNG*;
  - editor detection and *Reopen with encoding*;
  - the jump list.

## 7. Acceptance criteria
| ID | Criterion |
|---|---|
| AC-1 | §1 is done: M1 ≤ 5.01 MiB and T4 warm ≤ 10.0 ms with their causes named, the 4 broken tests fixed, P-53 and P-54 applied (`spec-check` green), and owner notes (if any) fixed with tests. |
| AC-2 | All §2 tests exist and pass on CI: 5 driver scripts, 2 goldens (committed and self-reviewed), and the unit tests. |
| AC-3 | Every failure was fixed in the product (or the test change is explained); the gates (M0, M1, T4, T5, T7, T10) are green; the exe size is reported. |
| AC-4 | PR #92 is merged; `phase-v2h` is tagged; the 14 issues are closed; `v1.4.0` is tagged on a `chore(release)` commit; the draft release has the zip + SHA-256; the report follows §6. |

## 8. Stop rule (quota)
If the Grok quota or the Actions minutes run low at any point:
1. commit and push with `[skip ci]`;
2. write `docs/reports/v2h_tests_REPORT.md` with a done / partial / not-started table (one row per §1.1–§1.4, per §2 block A–D and per §3–§5) and **"Next action: …"**;
3. stop.

The owner will give this same prompt again.

**END OF PROMPT 018a.** Start with §1 now.

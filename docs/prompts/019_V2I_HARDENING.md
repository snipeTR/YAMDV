# 019_V2I_HARDENING — YAMDV Phase V2-I: security review, long fuzzing, ACG, High Contrast + UI Automation, real-hardware QA pack

> **Prompt 019.** Phase **V2-I** (`docs/ROADMAP_V2.md`).
> - Branch `phase/v2i-hardening`, tag `phase-v2i` (no release in this phase; v2.0.0 comes in V2-J)
> - Report `docs/reports/v2i_REPORT.md` (≤ 80 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-10) > this prompt > `docs/SECURITY.md` / `docs/SPEC.md` / `docs/UI_SPEC.md` > the rest of `docs/*`.
> **Order:** §1 → §2 → §3 → §4 → §5. **Resumable:** see §7.

---

## 1. Review of V2-H — accepted
V2-H (prompts 018 + 018a) is **accepted**.
- **CI budget:** 1 diagnostic run and 1 full run, with 0 fix pushes.
- **Tests:** 48/48 driver scripts pass.
- **Issues:** 14 closed.
- **Release:** the v1.4.0 draft exists.
- **PDF speed:** 1 093 pages in 2.63 s, down from 205 s in v1.
- **Jump list:** it now runs out of process (P-53), which removed 17 shell modules from the main process.

Two numbers did not reach their targets; they are fixed first.

### 1.1 Fix first
| ID | Fix |
|---|---|
| **F-38** | **T7 regression: 15.6 ms of CPU while idle with a document open** (it was 0). One timer tick most likely comes from the jump-list debounce (30 s). Idle must have **no timers at all** (PERFORMANCE rule: timers only while something animates). **Rule:** on a history change, start the jump-list update now if the last one ran > 30 s ago; otherwise mark it dirty and flush it at the next history change, on document close, or at exit. No timer is used. **Gate:** T7 doc = 0 / empty = 0. |
| **F-39** | **M1 5.29 MiB vs the 5.01 MiB target.** The remaining +0.64 MiB vs V2-G is heap. (1) In one diagnostic run, add allocation-site tags to the dev-tools counting allocator, and list the **top 20 live allocations by bytes** on `typical.md` for HEAD and for a `phase-v2g` build. (2) Fix the biggest new consumers. Likely suspects:<br>• tables built at run time instead of static data: the entity table must be a static sorted array with binary search, not a HashMap filled at start-up; the same goes for emoji, lexer keywords and the keymap;<br>• F2 source maps or reveal state kept in read mode;<br>• caches that are created at open but used only by features that are off.<br>**Target:** M1 ≤ 5.01 MiB (ideally near 4.7). |
| **F-40** | **Release notes.** The 1.4.0 CHANGELOG line "memory … back under budget" is true only for the CI budget. Correct it in `CHANGELOG.md` and in the v1.4.0 draft release notes (`gh release edit`). |

### 1.2 Owner notes (only if attached)
If the owner pasted notes from his checks below this prompt, each one becomes an item F-41, F-42, …, fixed right after §1.1.

---

## 2. Security review (HARD-01; S-TEST-04)
1. **`docs/SECURITY_AUDIT.md`:** one row per S-* rule in `docs/SECURITY.md`, with these columns: rule · where it is enforced (file / function) · test or evidence · status (OK / gap / fixed in this phase). Fix every gap. A gap that cannot be fixed in this phase becomes an issue (`HARD-05`…, label `hardening`), and the report names it.
2. **`docs/THREAT_MODEL.md`:** complete it with STRIDE per entry point. The v2 entry points must be included:
   - document content: Markdown, HTML, math, Mermaid, SVG, images (PNG/JPEG/GIF/ICO/BMP/TIFF)
   - dropped files and folders, and the Files tab (enumeration, reparse points, network paths)
   - the clipboard: HTML → Markdown paste, CF_HTML, bitmaps, TSV / CSV
   - CLI arguments
   - IPC: dialog host, print / PDF worker, jump-list kind; nonce + PID
   - config and state files: `settings.ini`, `state.ini`, `custom-theme.ini`, the recovery journal
   - the registry: registration, `DefaultIcon` / custom icon, `App Paths` reads
   - external editor launch
   - exports: PDF, HTML (interactive script + CSP), CSV, PNG
   - regex (`regex-lite` + time guard)
3. **Security unit matrices (S-TEST-02):** extend them for the new surfaces:
   - HTML → Markdown paste: the XSS vectors produce no script and no `javascript:`;
   - the SVG rules (S-RES-07);
   - the ICO directory (malformed sizes / offsets);
   - recovery-journal parsing;
   - IPC records (progress, jumplist);
   - custom-icon path validation;
   - editor argument quoting.
4. **Binary checks (S-TEST-03):** re-run them and add their output to the audit (imports, DllCharacteristics, CET, no W+X sections).

## 3. Long fuzzing (HARD-02; S-TEST-01)
1. **Targets:**
   - **Existing (check that they still build):** parse_layout, ini, html, url, highlight, math, mermaid, svg_parse.
   - **New:**
     - `html_to_md` (the paste converter)
     - `cf_html` (CF_HTML header + fragment parsing)
     - `ico_dir`
     - `journal` (recovery file parser)
     - `ipc_records` (dialog-host / worker / jumplist payloads)
     - `entities`
     - `regex_template` (`$1` / `${name}` replacement parsing)
     - `tsv_csv` (paste parser)
     - `table_model` (parse → edit ops → serialize round trip)
     - `pdf_doc` (Markdown → PDF writer with a mock shaper: no panic, valid xref)
2. **Workflow:** `.github/workflows/fuzz-long.yml`, `workflow_dispatch` only, on **ubuntu-latest** (platform-independent crates).
   - A matrix with one job per target, `-max_total_time=3600`.
   - It uploads crashes and the minimized corpus as artifacts.
3. **Run it once.**
   - Every crash or timeout becomes a fixed bug plus a regression unit test.
   - Commit small seed corpora; keep the minimized corpora ≤ 5 MB in total.
   - Report the hours and execs per target.

## 4. ACG, High Contrast and UI Automation
1. **ACG and hardening (HARD-03):** re-run the ACG matrix on CI with the v2 features:
   - SVG, ICO, GIF;
   - DirectWrite font-file streams for the PDF;
   - interactive-HTML export;
   - the jump-list helper (record whether the helper has ACG and why);
   - the settings-panel font enumeration.

   The parts that need a real PC go into the owner QA pack (§5), and ADR-0001 is updated with the results.
2. **High Contrast** (R-THEME-08, R-A11Y-02; FUT-47)
   - **Trigger:** when `SPI_GETHIGHCONTRAST` is on, including live changes through `WM_SETTINGCHANGE`.
   - **Colors:** the whole UI and document use **system colors**: `COLOR_WINDOW`, `COLOR_WINDOWTEXT`, `COLOR_HOTLIGHT`, `COLOR_HIGHLIGHT` / `HIGHLIGHTTEXT`, `COLOR_GRAYTEXT`, `COLOR_BTNFACE` / `BTNTEXT`. Custom colors and scheme tinting are disabled.
   - **Content:**
     - borders are drawn instead of fills;
     - syntax highlighting falls back to `WINDOWTEXT`, with bold keywords;
     - Mermaid and math use the system colors;
     - images are unchanged;
     - focus rings use `COLOR_HIGHLIGHT` and are always visible.
   - **Contrast test:** R-A11Y-04 checks every scheme × mode (including the HC mapping) with a WCAG ratio unit test.
3. **UI Automation** (R-A11Y-03; FUT-47)
   - **Providers:** COM UIA providers (`IRawElementProviderSimple` / `Fragment` / `FragmentRoot`) for:
     - the window and title-bar buttons
     - the menus and menu items
     - the sidebar tabs and list / tree items
     - the settings-panel controls (toggle, slider, dropdown, edit, button)
     - the banners
     - the viewer toolbars
   - **Document:** the document is a `Document` control with a read-only **TextPattern** (text ranges over the rendered text, selection) and focus events for Tab stops.
   - **Cost:** **zero cost unless a UIA client connects.** Providers are created only on `WM_GETOBJECT` (`UiaRootObjectId`) and released when they are no longer referenced. M0 and M1 must not change.

**Tests (A-7/A-9 as reduced by A-10):**
- **Driver scripts (2):**
  - `high-contrast`: the dev-tools verb `ui hc-on` feeds the HC path with a fixed set of system colors. Check:
    - the custom colors are off and the scheme tint is gone;
    - the focus ring is visible;
    - a Mermaid diagram and code use the system colors;
    - `ui hc-off` restores everything.
  - `uia-tree`: a small UIA client in the driver harness (a separate process) walks the tree and asserts the names and roles of the title-bar buttons, the ⋯ menu items, the settings toggles and the sidebar items. It also checks that the document TextPattern returns the first paragraph. Afterwards, with the client disconnected, the main-process PWS is back within +0.2 MiB.
- **Goldens (2):** the reading view in HC black (dark); the settings panel in HC white (light).
- **Unit tests:** the §2.3 matrices, WCAG contrast, the F-38 flush logic, and the F-39 static tables.

**Gates (A-7/A-9):**
- M0 ≤ 2.70 MiB.
- **M1 ≤ 5.01 MiB** (F-39).
- T4 typical warm ≤ 10.0 ms.
- T5 within its limits.
- **T7 = 0** (F-38).
- T10 unchanged.
- The exe size is report only (owner decision; size work is in V2-J).

## 5. Owner real-hardware QA pack (QA-01, HARD-03)
1. **Build:** use the existing pack job (`workflow_dispatch`) to build `YAMDV-v2i-qa.zip` + SHA-256 as the draft pre-release `qa-v2i`. It contains `run-tests.cmd`, the release and dev exes, the fixtures, and **`OWNER_QA.md`**.
2. **`OWNER_QA.md`** (EN + TR, ≤ 15 numbered items; for each item: what to do, what you should see, and where to note the result):
   1. Run `run-tests.cmd`, and send `test-results.txt`.
   2. Memory: with `typical.md` open, read the Task Manager value "Memory (active private working set)", and also after 10 minutes idle.
   3. Display scaling at 100 / 150 / 200 %, and two monitors with different scaling: move the window across. Check that text, formulas, diagrams and split view stay sharp and correct.
   4. Switch Windows dark / light while YAMDV is open.
   5. Default app through Windows Settings; Explorer "Open with YAMDV"; double-click a `.md` file.
   6. Drag and drop files and folders from Explorer.
   7. Typing with the Turkish Q layout, including AltGr (`@ € { [ ] } \ |`) and dead keys, in F2 and F3.
   8. A CJK IME (Japanese or Chinese) composition in F2 and F3.
   9. The Open dialog in a shell-extension-heavy folder (OneDrive / Dropbox / TortoiseGit), plus drag and drop from it (ACG).
   10. Make an external change while there are unsaved edits: the conflict banner appears, and *Save as* works.
   11. Print to a real printer, and check ClearType crispness.
   12. High Contrast (`Left Alt+Left Shift+Print Screen`): every screen is readable.
   13. Narrator (`Ctrl+Win+Enter`): it reads the buttons, the menu items, the settings and the document text.
   14. A PDF in Edge or Acrobat, and the interactive HTML in a browser.
   15. The jump list, a custom icon, and the external editor (F4).
3. **Report:** the owner sends the results to the reviewer. Prompt 020 starts with the problems they show.

---

## 6. Merge and report
- **PR and CI:** one CI run at the end, plus at most one fix push. The `fuzz-long` workflow and the diagnostic run for F-39 are counted separately (both are allowed once).
- **Issues:** the PR body closes HARD-01, HARD-02, HARD-03 and FUT-47 with **one `Closes #n` per line**. QA-01 stays open until the owner reports the QA results.
- **After CI is green:** merge, then tag `phase-v2i`.
- **Report (≤ 80 lines):**
  - the F-38 / F-39 causes and fixes (before → after)
  - the audit summary: rules OK / gaps fixed / gaps open (with issue numbers)
  - the threat-model entry points covered
  - fuzzing: hours and execs per target, crashes found → fixed
  - ACG results
  - HC / UIA status
  - the gates
  - the `qa-v2i` pack link and its SHA-256
  - deviations

## 7. Stop rule (quota)
If the Grok quota or the Actions minutes run low at any point:
1. commit and push with `[skip ci]`;
2. write `docs/reports/v2i_REPORT.md` with a done / partial / not-started table (F-38…F-40, §2.1–§2.4, §3, §4.1–§4.3, §5) and **"Next action: …"**;
3. stop.

The owner will give this same prompt again.

## 8. Acceptance criteria
| ID | Criterion |
|---|---|
| AC-1 | F-38 (T7 = 0), F-39 (M1 ≤ 5.01 MiB) and F-40 are done (and §1.2 if notes were attached). |
| AC-2 | `SECURITY_AUDIT.md` covers every S-* rule; the gaps are fixed or filed; `THREAT_MODEL.md` is complete for every entry point in §2.2; the matrices pass. |
| AC-3 | The `fuzz-long` workflow exists and has run once (≥ 1 h per target); every crash is fixed with a regression test. |
| AC-4 | The ACG matrix is re-run and ADR-0001 updated; High Contrast and UIA work per §4; the 2 driver scripts and 2 goldens pass; UIA costs nothing without a client. |
| AC-5 | The gates are green; the PR is merged with the issues closed; `phase-v2i` is tagged; the `qa-v2i` pack with `OWNER_QA.md` is uploaded; the report follows §6. |

**END OF PROMPT 019.** Start with §1.1 now.

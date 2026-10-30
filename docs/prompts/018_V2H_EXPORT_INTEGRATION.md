# 018_V2H_EXPORT_INTEGRATION — YAMDV Phase V2-H: native PDF, interactive HTML, integrations, known bugs (tests and v1.4.0 in 018a)

> **Prompt 018.** Phase **V2-H** (`docs/ROADMAP_V2.md`).
> - Branch `phase/v2h-export`, tag `phase-v2h`
> - **No new tests and no release here:** the tests and `v1.4.0` come in prompt **018a** (owner decision, 2026-10-29)
> - Report `docs/reports/v2h_REPORT.md` (≤ 80 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-10) > this prompt > `docs/SPEC.md` / `docs/SECURITY.md` / `docs/UI_SPEC.md` > the rest of `docs/*`.
> **Order:** §1 → §2 → §3 Part 1 → Part 2 → Part 3 → §4. This is a large phase, so use the stop rule (§6) if needed. Do not tag `v1.4.0` in this prompt.

---

## 1. Review of V2-G — accepted
V2-G is **accepted**:
- **Gates:** M0 2.40 MiB, M1 4.71 MiB, T5 within target, T7 0.
- **Coverage:** math 123/123, 42 lexers.
- **Exe:** +213 KB, within the 250 KB budget.
- **Decisions accepted:** ADR-0021 (own SVG crate, +36 KB; resvg rejected at +561 KB) and ADR-0022 (own ICO parser; WebP keeps its placeholder).
- **Memory flake:** the measurement-order fix in the driver is accepted; the owner allowed the extra runs.

**Owner decision (2026-10-29): build first, test separately.** Prompt 018 adds **no new tests**: no driver scripts, no goldens and no measurement scripts. Unit tests are allowed only where you need them while coding. The existing CI suite must stay green. Prompt **018a** adds every test for this phase, fixes what the tests find, closes the issues and releases v1.4.0. Record this as one line in A-10.

### 1.1 Fix first
**F-37 — owner report, highest priority: scrolling hitches.** The owner's feedback, verbatim (2026-10-27): *"scroll ederken takılıyor muhtemelen başlıklar arası geçişte yapışkanlık yapılmaya çalışılmış ama hızlı scroll etmek isteyeni rahatsız eder. Onu biraz akıllı yapmak lazım: eğer scroll deltası belirli bir değerin üzerindeyse başlıklara yapışmasın; scrollun durduğu yere yapışabilir."* In English: scrolling hitches, apparently because the view sticks to headings while passing them; that bothers fast scrollers; above a delta threshold it must not stick to headings; it may settle where the scroll stops.

1. **Find the cause first.** Use a dev-tools scroll trace that logs, per frame: `scroll_y`, the animation target, frame time, relayouts and anchor corrections. Record a burst of 20 wheel notches on `large-1mb.md` and on `typical.md`, and name the cause(s) in the report. Suspects:
   - (a) each wheel notch restarts or queues the smooth-scroll animation instead of retargeting it;
   - (b) something aligns or snaps to headings while scrolling (scroll spy, anchor or position-restore logic);
   - (c) estimated block heights are corrected on screen when lazily laid-out blocks arrive, which often happens at headings;
   - (d) layout work on the UI thread takes longer than a frame.
2. **Required behavior (P-51, P-52).**
   - **Wheel:** notches accumulate into one moving target, and a new notch retargets the running animation from its current velocity. Scrolling never restarts from zero, never pauses and is never slowed down. When notches arrive faster than one per 60 ms, or more than 3 notches are pending, the view follows the target with at most a 60 ms ease.
   - **Touchpad and scrollbar:** precision-touchpad deltas and scrollbar-thumb drags scroll directly, without animation.
   - **No snapping while scrolling:** nothing snaps while the user scrolls.
     - With `reading.scroll_snap = smart` (default), the view may settle onto a heading (≤ 120 ms) only when all of these hold: it has rested for ≥ 150 ms, the last movement was slow (below the fast threshold above), and a heading top is within 24 px.
     - With `off`, it never settles.
     - Explicit jumps (TOC, `#anchor`, the heading palette, find) are unchanged.
   - **Height corrections** never move visible content: anchor on the topmost visible block, and correct estimated heights off screen.
   - **Scroll spy** never changes the scroll position, and runs at most once per frame.
3. **Tests:** they come in 018a (`scroll-fling` and the accumulator unit tests). Keep the dev-tools scroll trace, because 018a uses it. Put the wheel accumulator / velocity model in pure code, so that 018a can unit-test it.
4. **Issue:** create `BUG-12: Scrolling hitches; headings stick during fast scrolling` (label `bug`, milestone `post-v1`). 018a closes it.

| ID | Fix |
|---|---|
| **F-33** | **Open issues.** #20, #21, #46, #8 and #25 are still open, because one `Closes` line linked only the first issue. Close each by hand with a comment that references #87. From now on, the PR body has **one `Closes #n` per line**; add this rule to A-9. |
| **F-34** | **Math macro scope.** The report says "a macro applies to its whole block". R-MATH-07 and 017 §3.1 require document order: a macro defined in one block applies to every later formula in every later block. Verify this and fix it if needed; 018a adds the unit test. |
| **F-35** | **Gantt milestone label.** The label can overflow the card edge. Keep it inside the card: flip it to the left of the milestone, or clamp it. |
| **F-36** | **GIF decoding off the UI thread.** GIF frames are decoded on the UI thread. Move the decoding to the existing image worker (PERFORMANCE rule 7); the UI thread only composites and blits the finished frame. |

### 1.2 Owner notes (only if attached)
If the owner pasted notes from his checks below this prompt, each one becomes an item F-38, F-39, …, fixed right after §1.1.

---

## 2. Setup
1. **ADR-0023 "Native PDF writer"** (before any PDF code). It decides:
   - **Crates:** `pdf-writer` + `subsetter`. Check the licenses with `cargo deny`, and choose a deflate encoder for streams; reuse one already in the tree if there is one.
   - **Drawing:** the backend seam: the pagination engine emits page draw commands (rects, paths, glyph runs, images, links) that either the band renderer or the PDF writer consumes.
   - **Fonts:**
     - read the font bytes through DirectWrite (`IDWriteFontFace::GetFiles` → file stream; TTC face index);
     - subset them with `subsetter`;
     - embed them as Type0 / CIDFontType2 or CIDFontType0, with a **ToUnicode** CMap built from the cluster maps, so the text is selectable and searchable;
     - fonts whose OS/2 `fsType` forbids embedding are drawn as outlines;
     - color emoji (COLR) become layered filled outline paths.
   - **Images:** JPEG sources pass through as DCT; everything else is deflated RGB(A) with an SMask.
   - **Process:** the writer runs in the existing worker process (ADR-0018, R-EXP-07), so the main process memory stays flat.
2. **Size room first.** Before adding the PDF writer, use `cargo xtask size-map` to remove **≥ 100 KB** with **no behavior change**. Examples: `opt-level = "z"` for more crates while T4/T5 stay green; deduplicating big constant tables in `[anonymous constants]`; dead code. Record the before/after values.
   - **Phase limit:** the exe at the end of V2-H must be **≤ 4,000,000 B** (the B-SIZE limit).
3. **Spec patches** (`docs/spec-patches/v2h.txt`, applied with `xtask spec-patch`):

**P-49** — `docs/UI_SPEC.md`, B.8.2. INSERT AFTER the anchor line:
```text
| `export.html_include_toc` | bool | `true` | |
```
NEW line:
```text
| `export.html_interactive` | bool | `true` | `false` = static, script-free export (the v1 behavior) |
```

**P-50** — `docs/SPEC.md`, R-EXP-02. OLD:
```text
light/dark toggle, in-page find — implemented with a tiny inline script protected by a hash-based CSP (`S-EXP-02`).
```
NEW:
```text
light/dark toggle, in-page find — implemented with a tiny inline script protected by a hash-based CSP (`S-EXP-02`). With `export.html_interactive` off, the export is static and contains no script (its CSP then has no `script-src`).
```

**P-51** — `docs/SPEC.md`, R-REN-17. OLD:
```text
smooth (setting);
```
NEW:
```text
smooth (setting): wheel notches accumulate into one moving target that each new notch retargets, so fast scrolling is never slowed, paused or restarted; precision touchpad and scrollbar-thumb drags scroll directly; nothing snaps while the user scrolls — with `reading.scroll_snap = smart` the view may settle onto a heading only after resting ≥ 150 ms when the last movement was slow and a heading top is within 24 px, with `off` never; block-height corrections never move visible content;
```

**P-52** — `docs/UI_SPEC.md`, B.8.2. INSERT AFTER the anchor line:
```text
| `reading.smooth_scrolling` | bool | `true` | |
```
NEW line:
```text
| `reading.scroll_snap` | enum | `smart` | `smart`, `off` (R-REN-17) |
```

---

## 3. V2-H tasks
Specifications:
- `docs/SPEC.md` R-EXP-01/02/06/07, R-OPEN-06, R-EXT-01…03, R-SYS-08/09
- `docs/SECURITY.md` S-EXP-01…03, S-EXEC-03, S-FILE-04
- `docs/PERFORMANCE.md` T10

Issues: FUT-39, FUT-21, FUT-23, FUT-10, FUT-09, FUT-25, BUG-01…07.

### Part 1 — Native PDF writer (R-EXP-01, S-EXP-03, T10; FUT-39)
1. **Export PDF… and `--export-pdf`** use the native writer per ADR-0023. *Microsoft Print to PDF* is no longer used for Export PDF. **Print…** keeps its GDI band path.
2. **Content:**
   - **Text:** vector and selectable, with subset-embedded fonts.
   - **Pages:** colors per `pdf_colors`; page size, margins and the optional "n / N" footer from the settings.
   - **Code:** code blocks always wrap in PDF.
   - **Breaks:** no breaks inside short blocks or table rows, and never a blank trailing page; reuse the pagination engine.
   - **Math, Mermaid and SVG:** vector paths and glyph runs.
3. **Navigation:**
   - **Outline:** PDF bookmarks from the headings, nested by level.
   - **Internal links:** TOC, `[TOC]`, footnotes and anchors become GoTo links.
   - **External links:** `http`, `https` and `mailto` become URI annotations. Nothing else is linked (S-EXP-03): no JavaScript, launch actions, embedded files or forms.
4. **Metadata:** Title (the document title), Producer `YAMDV <version>`, CreationDate.
5. **Job:** progress card, Cancel and atomic write as in R-EXP-07 / S-EXP-01. Cancel leaves no file.
6. **Speed:** **T10** ≤ 3 s per 100 pages, with transient memory ≤ +30 MB inside the worker. Main-process PWS returns within +0.2 MiB afterwards. 018a measures it, together with `large-1mb.md` (205 s for 1 093 pages in v1).

### Part 2 — HTML and clipboard (R-EXP-02, R-EXP-06, S-EXP-02; FUT-21, FUT-23)
7. **Interactive HTML export** (`export.html_interactive`, default on)
   - **Features:**
     - a TOC sidebar
     - collapsible H1–H3
     - a light/dark toggle (both palettes in the CSS; the initial palette comes from `html_colors`)
     - in-page find
   - **Script:** one hand-written inline script, ≤ 8 KB.
     - It never uses `eval`, `Function`, `innerHTML` with document text, `fetch`, `XMLHttpRequest` or `WebSocket`; 018a adds a check of the script source.
     - The CSP `script-src 'sha256-…'` is computed from the exact script bytes at build time; 018a adds a test that recomputes it.
   - **Math and Mermaid:** inline **SVG** from our own layouts.
     - Glyph outlines from `GetGlyphRunOutline` go into reused `<symbol>`s, so the file does not depend on the reader's fonts.
     - Diagram labels may use `<text>` with a font stack.
   - **Static mode:** with the setting off, the export is the existing static output, with no script.
   - **XSS suite:** 018a extends it to both modes.
8. **Copy document as rich HTML** (R-EXP-06): *Copy as rich HTML* (in the ⋯ menu and the context menu) puts `CF_HTML` (static export markup, no script) and plain text on the clipboard.
9. **Save a diagram or the mind map as a PNG file** (R-EXP-06): in the diagram context menu and the mind-map viewer, *Save as PNG…* goes through the dialog-host `save` kind, 2×, with an atomic write.

### Part 3 — Integrations and known bugs
10. **External editor detection** (R-EXT-02; FUT-10)
    - **Detection order:** VS Code, VSCodium, Cursor, Sublime Text, Notepad++, EmEditor, Typora, Notepad, each with the R-EXT-02 arguments.
    - **Lookup:** known install folders, then the read-only `App Paths` keys. A candidate counts only if it resolves to a real `.exe`.
    - **Launch:** `CreateProcessW`, never through a shell (S-EXEC-03).
    - **Settings:** the panel's External editor section shows the editor that was found.
11. **Reopen with encoding ▸** (R-OPEN-06; FUT-09)
    - **Menu:** in the ⋯ menu: UTF-8, UTF-16 LE/BE, windows-1254, windows-1252, windows-1251, ISO-8859-9, ISO-8859-15, GB18030, Shift_JIS, EUC-KR, Big5.
    - **Effect:** the document is decoded again. Editing is allowed only if the decoding is lossless (R-EDIT-12), and saving keeps the chosen encoding byte-exactly.
    - **Display:** the detected or chosen encoding is shown in the word-count tooltip.
12. **Jump list and custom icons** (R-SYS-08/09 MAY; FUT-25)
    - **Jump list:** recent files appear in the taskbar jump list (`ICustomDestinationList`, `IShellLink` items to our exe with the file argument). It updates with History; *Clear history* and History off clear it.
    - **Custom icon:** Settings → General → *App icon*: default or a user `.ico`.
      - The path is validated (S-FILE-04), the file is ≤ 1 MB, and it is parsed by the ADR-0022 ICO parser.
      - It is used as the window/taskbar icon and as the document-type `DefaultIcon` (HKCU only, the S-REG rules).
13. **Known bugs BUG-01…07** (018a adds one check for each):
    - **BUG-01:** `Ctrl+click` on `x.md#frag` opens the new window scrolled to the anchor.
    - **BUG-02:** a single-line `$$\frac{a}{$$` renders as display math in the error style.
    - **BUG-03:** *Copy as Markdown* maps the selection through the parser source ranges, so reference links and autolinks are copied exactly.
    - **BUG-04:** the full HTML5 named-entity table (2 231 names), as compact static data ≤ 30 KB.
    - **BUG-05:** HTML table `rowspan` in the table model, for layout, export and CSV.
    - **BUG-06:** column alignment applies to every line in a wrapped cell.
    - **BUG-07:** `==highlight==` across emphasis, and the inline-extension pass runs on the event stream with source ranges, so escapes and entities no longer skip it.

**Out of scope:** the tests (018a); V2-I hardening and V2-J performance work, except the size room in §2.2.

**Tests:** none are added here (owner decision). The **existing** CI suite must stay green; 018a adds this phase's tests.

**Gates (existing CI):**
- M0 and M1 at most +0.3 MiB vs V2-G (2.40 / 4.71 MiB).
- T4 (it must not get worse after F-37), T5 and T7.
- **B-SIZE:** the exe at the end is ≤ 4,000,000 B. Report the §2.2 savings and the share of each part.
- T10 is measured in 018a.

---

## 4. Merge and report
- **PR and CI:** one CI run at the end, plus at most one fix push.
- **Issues:** the PR body lists FUT-39, FUT-21, FUT-23, FUT-10, FUT-09, FUT-25, BUG-01…BUG-07 and BUG-12 as **`Refs #n`, one per line**. Do not use `Closes` here; 018a closes them after the tests.
- **After CI is green:** merge, then tag `phase-v2h`. There is **no** version change and **no** `v1.4.0` tag here.
- **Report (≤ 80 lines):**
  - the AC table
  - the gates, including size and savings
  - ADR-0023 (3 lines)
  - the F-37 cause (1–2 lines)
  - deviations
  - **"Notes for 018a":** the risky spots you saw, the dev-tools verbs you added for testing, and anything you could not check yourself

## 5. Acceptance criteria
| ID | Criterion |
|---|---|
| AC-1 | F-37 (the scroll behavior, with its cause named in the report) and F-33…F-36 are done (and §1.2 if notes were attached); ADR-0023 is written; the §2.2 savings (≥ 100 KB) are recorded; P-49…P-52 are applied with `spec-check` green. |
| AC-2 | Part 1 items 1–6, Part 2 items 7–9 and Part 3 items 10–13 work per the listed specs; TRACEABILITY is updated by script; S-EXP-02/03 hold. |
| AC-3 | The existing CI suite is green (no new tests, by owner decision); the PR artifact `yamdv-exe` exists. |
| AC-4 | The gates (M0/M1/T4/T5/T7 and B-SIZE ≤ 4,000,000 B) are green; the PR is merged with `Refs` lines; `phase-v2h` is tagged; the report follows §4, including the "Notes for 018a". |

## 6. Stop rule (quota)
If the Grok quota or the Actions minutes run low at any point:
1. commit and push with `[skip ci]`;
2. write `docs/reports/v2h_REPORT.md` with a done / partial / not-started table (one row per item 1–13 and per F-item, F-37 first) and **"Next action: …"**;
3. stop.

The owner will give this same prompt again.

**END OF PROMPT 018.** Start with §1.1 now. The next prompt is 018a (tests + v1.4.0).

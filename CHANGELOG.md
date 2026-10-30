# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.1.0] - 2026-10-31

The open issues (prompt 024): #86, #59, #26.

### Added
- Visual flowchart editing in Live Edit (`F2`) and Split Edit (`F3`) (#86, R-F2-11, ADR-0027):
  click a node or an edge of a rendered `flowchart` / `graph` to select it; double-click, `Enter`
  or just typing edits its label in place (`Enter` commits, `Esc` cancels); `Delete` removes it
  (a node together with its edges and `class` / `style` / `click` lines); `Tab` adds a new node
  connected from the selected one; drag a node onto another to connect them. Every change is
  written to the Markdown source as one undo step. Other diagram types stay source-edited.
- Editor plugins (#26, FUT-27, ADR-0028): `integrations/vscode/` and `integrations/sublime/` add
  "YAMDV: Open preview", which starts `yamdv.exe [--line N] -- <file>`; YAMDV's live reload then
  follows every save. No network, no telemetry; not published to any marketplace.

### Changed
- Live reload in `F2` / `F3` (#59, PERF-06): an external change to a file without unsaved edits
  is spliced into the editor (only the changed bytes, one undo step); the mode, caret and scroll
  position are kept and only the changed blocks are re-laid out. Plain-text files reload the same
  way.

### Fixed
- Popup menus: each reused band bitmap is cleared before drawing, like the 2.0.3 fullscreen fix.

## [2.0.3] - 2026-10-30

Owner fixes (prompt 023).

### Changed
- The word / character count now sits in the title bar right after the file name, in small grey
  text ("file.md · 168584 sözcük · 787578 karakter"); the floating bottom-right bubble is gone
  (#98). The same setting, menu item and toolbar toggle show or hide it; hovering it still shows
  the details tooltip. When the window is too narrow the file name keeps priority.

### Added
- Mermaid sequence diagrams: `rect` blocks draw a filled band behind the enclosed messages, notes
  and frames. Colors: `rgb(…)`, `rgba(…)` (alpha 0–1 or %), `#rgb` / `#rrggbb` / `#rrggbbaa` and
  CSS color names. Bands nest and may contain `alt` / `else` / `loop` / `opt` / `par` and notes.
  In dark mode the author's color is kept where the text stays ≥ 4.5:1; a light band under a dark
  UI is toned down to a faint tint of that color (F-44/F-45).

### Fixed
- Mermaid fullscreen viewer: the lower part of a large diagram no longer repeats in horizontal
  bands; the reused band bitmap is cleared before each band is drawn.

## [2.0.2] - 2026-10-30

Owner micro change (prompt 022).

### Changed
- While Live edit (F2) or Split edit (F3) is on, the toolbar background takes a faint tint so the
  edit mode is visible at a glance: a barely-there red in light mode (white → `#FBF1F1`) and a
  muted dark burgundy in dark mode (`#1E1E1E` → `#3A1C21`). Every scheme blends its own
  background; High Contrast stays on the system colors. On the tint, an "on" icon whose
  accent color would fall below 3:1 (Lychee / Mint light) is drawn in the text color.

## [2.0.1] - 2026-10-30

Owner fixes after the first look at v2.0.0 (prompt 021).

### Changed
- Settings → Toolbar lists every toolbar command in one fixed order (`toolbar.order`, stored apart
  from the enabled set `toolbar.items`). Turning an item off or on never moves it; a re-enabled
  item comes back in its old place.
- Each toolbar row has ▲ Move up / ▼ Move down buttons next to its switch; `Alt+↑/↓` and
  drag-and-drop also reorder. The toolbar updates live.
- Presets at the top of the section: Off / Essentials / All (Kapalı / Önemliler / Hepsi). A preset
  never changes the order; turning one item on or off by hand shows Custom (Özel). Reset toolbar
  restores Essentials and the default order.
- The "Live / Split Edit only" note is now small grey text right of the item name, on the same line.
- CI (A-12): the owner tests every build by hand; CI runs only the build, fmt + clippy,
  `cargo deny`, `check-imports` and the release job. The test jobs run on manual dispatch only.

## [2.0.0] - 2026-10-30

V2-I hardening (prompt 019) and V2-J performance 2.0 + toolbar (prompt 020). The final v2 release.

### 2.0 summary (what v2 added since 1.0)
- A custom title bar with the ⋯ menu, a settings panel (13 sections, search, shortcut editor,
  custom colours, fonts) and a toolbar under the title bar with the owner's icons.
- WYSIWYG Live Edit (F2) and Split Edit (F3) with typing transforms, links, table editing,
  find & replace, autosave and conflict handling.
- Navigation: Files / Outline / Favorites / History sidebar, heading palette, mind map,
  fold toggles, footnote and link tooltips, image / diagram / table viewers.
- Content engines: Mermaid flowchart, sequence, gantt, pie, state, class, ER, mindmap and
  `%%{init}%%`; native math with macros; SVG, GIF and ICO images.
- Export: native PDF with selectable text and bookmarks, interactive or static HTML, rich-HTML
  copy, diagram / mind map PNG, table CSV.
- Hardening: high contrast, UI Automation (screen readers), ACG for decoders, a full
  security audit and fuzzing; AGPL-3.0-or-later license.

### Added
- Toolbar under the title bar (V2-J, B.4.1): the owner's 95 icons compiled into a compact path
  table (9.5 KB, no SVG text in the exe); toggles show their state, Format / Insert items only
  in F2 / F3, a » menu for what does not fit, tooltips with shortcuts, `F6` and arrow keys,
  UI Automation toolbar. Hidden in F11, the fullscreen viewers and the mind map.
- Settings → Toolbar: show / hide, icon size 16 / 20 / 24, choose and reorder the items
  (drag or `Alt+↑/↓`), reset to default. The ⋯ and context menus show the same icons.
- High Contrast themes follow the system colours; UI Automation tree with Document +
  TextPattern, title bar, sidebar, settings, menus and banners (V2-I).

### Changed
- Performance 2.0 (V2-J): same-runner A/B gates against the previous phase, final memory
  budgets as hard gates, faster live reload (T6) and split editing (T5), code-toolbar fade.
- Mermaid under a dark UI keeps `%%{init}%%` light themes readable; sibling subgraphs no
  longer overlap (F-44…F-46).
- License: AGPL-3.0-or-later with third-party notices in the zip.

### Fixed
- Fuzzing findings in the SVG, HTML-to-Markdown, INI and CF_HTML parsers (V2-I, V2-J).
- Idle CPU from the jump-list timer (F-38).

## [1.4.0] - 2026-10-30

V2-G content engines (prompt 017) and V2-H native PDF, interactive HTML, integrations and known
bugs (prompts 018, 018a).

### Added
- Native PDF export (V2-H, ADR-0023): selectable, searchable text with subset fonts, bookmarks
  from headings, internal and external links; print, screen and PDF share one draw path.
- Interactive HTML export (sidebar, folding, theme toggle, find; one hashed script, strict CSP);
  the static export stays available (`export.html_interactive = false`).
- Copy as rich HTML (`CF_HTML`), Save diagram / mind map as PNG.
- Open in external editor finds VS Code, VSCodium, Cursor, Sublime Text, EmEditor, Typora and
  Notepad++ (FUT-10); File ▸ Reopen with encoding (12 encodings, read mode).
- Windows jump list of recent files (built out of process, follows *Clear history* and
  `sidebar.show_history`); custom app icon (`general.app_icon`, `.ico` ≤ 1 MB).
- Math: `\newcommand`, `\renewcommand`, `\def` and more environments (V2-G, 123/123 coverage).
- Mermaid: gantt, pie, state, class, ER, mindmap, `%%{init}%%`; copy as PNG or source (V2-G).
- SVG images (own renderer, `yamdv-svg`), animated GIF, ICO; WebP placeholder (V2-G).
- Highlighting for Console, JSX, TSX, HTML and LESS (V2-G); F2 image resize handles (R-F2-12).

### Fixed
- Free-spinning (notchless) mouse wheels no longer stall on large documents (F-37).
- `Ctrl+click` on an anchor opens the new window at the target (BUG-01); single-line display math
  errors are counted (BUG-02); Copy as Markdown never cuts a construct (BUG-03); the full HTML
  named-entity table (BUG-04); table `rowspan` (BUG-05); per-line alignment in table cells
  (BUG-06); `==highlight==` across emphasis, code and entities (BUG-07).
- Memory after opening a document dropped from 5.88 to 5.29 MiB (jump list moved out of
  process); this is within the CI budget (8.0 MiB) but still above the 5.01 MiB phase target,
  which V2-I fixes (F-39). Scrolling is back to its V2-G frame times (018a).

## [1.3.0] - 2026-10-23

V2-E editing plus (prompt 015) and V2-F WYSIWYG Live Edit with table editing (prompt 016).

### Added
- Live Edit (F2) is WYSIWYG (V2-F, FUT-43, ADR-0020): the page stays rendered and Markdown
  markers appear muted only around the caret; code blocks show their fences with a language
  input and autocomplete; math and Mermaid get a live preview card (last good diagram kept).
- Typing transforms in F2: `# `…`###### `, `- `/`* `/`+ `, `1. `, `[ ] `/`[x] `, `> `,
  ```` ``` ````/`$$`/`---` + `Enter`, and a pipe header row + `Enter` → table; one `Ctrl+Z`
  leaves the typed literal.
- F2 links: `Ctrl+K` popover (text, URL, title), `Ctrl+click` follows; cross-block selection
  with Markdown + HTML + plain-text clipboard; checkbox click toggles; context menu Format ▸,
  Insert ▸ (13 items) and Table ▸.
- Table editing (V2-F, FUT-44): in-cell editing with `Tab`/`Shift+Tab`/`Enter`/`Shift+Enter`
  (`<br>`), `|` escaped automatically; a tool strip (rows, columns, alignment, sort, move,
  delete, ⛶, CSV), `Ctrl+Alt+arrows` and `Ctrl+Shift+Backspace`; row and column drag grips;
  a 10×10 insert picker (`Ctrl+T`, Insert ▸ Table…); TSV/CSV paste (e.g. from Excel) fills
  cells or offers "Convert to table"; edited tables are pretty-aligned
  (`editing.auto_format_tables`, East-Asian width aware).
- Insert image (`Ctrl+Shift+I`, rebindable) in F2 and F3.
- F2 format toolbar above a selection (V2-E, FUT-45; `editing.format_toolbar`).
- Find & replace (`Ctrl+H`) with optional regex (`regex-lite`, ADR-0019), `$1`/`${name}`,
  Replace all as one undo step, Turkish-aware case folding for plain search.
- Tab snippets (`editing.snippets`) and editor extras: duplicate line, select line, move lines,
  insert table/code block, auto-pair (FUT-12).
- Rich paste and drop (FUT-15): HTML is converted to Markdown through an allowlist; pasted or
  dropped images are saved under `assets/<doc>/`.
- Recovery journal: unsaved edits survive a crash and are offered on the next open.

### Changed
- Blocks and tables the user did not touch are never re-serialized; EOL style and BOM are kept.
- `cargo xtask size-map` lists the largest exe-size contributors from the linker map.

## [1.2.0] - 2026-10-23

V2-C tooltips, banners and viewers (prompt 013) and V2-D navigation (prompt 014).

### Added
- Files tab (V2-D, FUT-33): a lazy tree of the Markdown files under the document's folder
  (folders first, natural order, worker-thread listing, 5 000 entries / depth 8, hidden,
  system, build and reparse-point folders skipped, no network crossing), watched for changes;
  a dropped folder opens it; `Ctrl+Shift+1…4` switch tabs; `sidebar.show_files`.
- Floating sidebar (FUT-02): `left-floating` / `right-floating` cards over the page without
  reflow; Appearance ▸ Sidebar position ▸ Floating.
- Favorites reorder (FUT-03): drag with a drop line, or `Alt+↑/↓` on a focused favorite.
- Go to heading (FUT-08): `Ctrl+Shift+O` palette with fuzzy filtering.
- `[TOC]` / `[[_TOC_]]` render a linked "Contents" list (also in HTML export, PDF and Print).
- Mind map (FUT-37): `Ctrl+Shift+M` or Export ▸ View as mind map; right or balanced layout,
  zoom, pan, collapse, double-click to jump, copy as PNG.
- Keyboard reach (HARD-04): every interactive element is in the `Tab` order with a focus ring
  after keyboard use; `F6` / `Shift+F6` cycle title bar, banners, sidebar and document.
- Tooltips (V2-C, FUT-40): link titles, `<abbr>` and image titles, footnote previews, the
  word-count badge, title-bar and viewer buttons; 500 ms delay, also on keyboard focus; a link
  status pill at the bottom left.
- Banners (FUT-40/34): edit conflict, deleted file, read-only, decoding, Mark of the Web,
  elevated, export failure; at most two, newest on top; content keeps its anchor.
- Anchored Ask popover for external links (FUT-41 / R-LINK-04) with IDN and credential warnings.
- Fullscreen viewers (FUT-36, FUT-06): images, Mermaid diagrams and tables via a ⛶ badge or a
  double-click; zoom at the cursor (10–800 %), pan, fit / 1:1, copy (bitmap, PNG, CSV).
- Heading folding (FUT-34): ▸/▾ on H1–H3, Alt+click folds a level, find expands, session only.
- Sup/sub and footnote references as inline objects (FUT-41).

### Fixed
- Wide flowcharts keep ≥ 10 px text and scroll horizontally, also in HTML export (BUG-08).

### Changed
- The edit conflict and the external-link Ask are no longer modal dialogs.

## [1.1.0] - 2026-10-22

V2-A window chrome (prompt 011) and V2-B settings panel (prompt 012). Not tagged yet: the
`v1.1.0` tag waits for the owner test (A-8).

### Added
- Window chrome (V2-A, FUT-30/32/35/46): custom 32 px title bar with ⚙ / ⋯ and Min/Max/Close,
  layered popup menus with keyboard navigation, color-scheme cards, F11 full screen with a
  reveal strip, optional word-count badge and a zoom pill.
- Settings panel (V2-B, FUT-31): ⚙ / Ctrl+, open an in-window panel with 12 sections (General,
  Appearance, Typography, Sidebar, Reading, Editing, Shortcuts, External editor, Export, Files,
  Privacy, About); every change is
  live and saved to `settings.ini`; Reset section / Reset all; full keyboard path.
- Settings search (FUT-26, first half): filters rows by label and description in the UI
  language.
- Font pickers (FUT-49): body / heading / code font lists with live preview and the resolved
  fallback font shown under each row.
- Custom-color editor (FUT-04): per-token light/dark swatches with a hex popover, written to
  `custom-theme.ini`; "Use custom colors" switch.
- Shortcuts (FUT-05): every action can be rebound by capture, with conflict detection, a
  Typora preset and Reset; the Help page lists the active bindings.

### Changed
- The ⚙ button and Ctrl+, open the settings panel instead of the Settings submenu; the old
  "Body font…" dialog is replaced by the Typography section.
- The diagnostic log switch (Privacy) applies at once.

### Fixed
- BUG-10: PDF / print / HTML export use only the export color setting (default Light), never
  the dark UI palette, selection or find highlights.
- BUG-11: background PDF / HTML export runs in a worker process with page progress and
  Cancel; the window stays responsive and a cancelled job leaves no file behind.

## [1.0.0] - 2026-10-17

First complete release (owner decision 2026-10-17: v1 scope; deferred items are in FUTURE.md).

### Added
- Reading: CommonMark + GFM with callouts, footnotes, task lists, `<details>`, tables, 30 code
  lexers, native math (core TeX subset) and Mermaid flowchart + sequence diagrams, images
  (PNG, JPEG, GIF first frame, BMP) decoded under Arbitrary Code Guard.
- Shell: welcome page, sidebar outline / favorites / history, find bar, 11 color schemes with
  light/dark/system mode, `custom-theme.ini` hot reload, EN/TR UI, single instance, file watcher.
- Editing: F2 Live Edit and F3 Split Edit with undo/redo, formatting shortcuts, atomic saves,
  autosave, conflict handling; F4 opens an external editor.
- Export (V1-D): self-contained static HTML (no script, strict CSP, images/math/diagrams as
  `data:` URIs); Print… (Ctrl+P) and Export PDF… (Ctrl+Shift+E) through the print path
  ("Microsoft Print to PDF"); Copy / Export table as CSV (UTF-8 BOM, CRLF, RFC 4180, formula
  guard); headless `--export-html` / `--export-pdf`.
- Packaging (V1-D): `--install` / `--uninstall` per user (Start-menu shortcut, Apps & features
  entry, registration), welcome-page "Install YAMDV for this user", portable mode via
  `yamdv.portable`; version 1.0.0 in the version resource and About.

### Changed
- F-22: every measurement gate fails only on two consecutive failing attempts.

## Phase history (before 1.0.0)

### Added
- Phase 002 (reading MVP):
  - Settings model with INI read and write, EN/TR strings, an internal log, and theme data for all 11 schemes.
  - Defensive path checks, size and binary guards, and encoding detection (BOM, UTF-8, chardetng).
  - Two-level document model: a block index plus lazily parsed detail trees in an LRU cache, with splice support (ADR-0007).
  - Layout ↔ source mapping and the copy-text rules.
  - DirectWrite text shaping and rendering backend A, with virtualized layout and scroll anchoring.
  - Plain-text mode and interim placeholders for constructs that are rendered from Phase 003 on.
  - Reading window: open via CLI (multi-file), Ctrl+O and drag & drop; in-window confirm dialog; smooth scrolling with an overlay scrollbar; selection and copy.
  - Tooling: `--render-png` (dev-tools), `cargo xtask golden` and `gen-large`, memprobe `--size`/`--open-dialog`/`--cpu-ms` with budget gates, fuzz targets, owner kit, spec-check and spec-patch.
- ADR-0007…0012. Arbitrary Code Guard enabled (ADR-0001 amendment).

### Changed
- Memory and size numbers are in MiB with raw bytes (P-05). The B-SIZE soft budget is 4.0 MiB.

### Added (Phase 001)
- Phase 001: repository, Cargo workspace, documentation set (specification appendices A–H),
  `cargo xtask` tooling (`ci`, `check-imports`, `size`, `memprobe`, `gen-icons`), CI workflow,
  native window skeleton (`yamdv.exe`), rendering-backend spike and ADR-0002 (backend A: DirectWrite GDI interop + tiny-skia).
- THREAT_MODEL skeleton, TRACEABILITY matrix, ADR-0000/0001/0003–0006.

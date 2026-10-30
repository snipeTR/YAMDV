# 021_TOOLBAR_SETTINGS_FIXES — YAMDV: owner fixes to the toolbar settings, tests off, v2.0.x

> **Prompt 021.** These are the owner's fixes after his first look at v2.0.0. He reports: *"the app works well"*.
> - Branch `fix/v2.0-toolbar`
> - Report `docs/reports/v2_0_fix_REPORT.md` (≤ 40 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-11, plus A-12 below) > this prompt > `docs/*`.

---

## 1. A-12 — tests off; the owner tests by hand (owner decision, 2026-10-31)
Add this to `docs/PROCESS.md`:
```text
A-12 (owner decision, 2026-10-31): the owner and a friend test every build by hand with a Turkish checklist. Until the owner says otherwise:
1. CI runs no tests at all — no driver scripts, no goldens, no measurement gates and no unit-test job. Keep the test files in the repository, but do not run them, and do not add new ones.
2. CI keeps only: the build (cargo-xwin or MSVC release), fmt + clippy, `cargo deny`, `check-imports` (security), and the release job on `v*` tags.
3. One CI run per change set. No fix push is needed unless the build itself breaks.
4. Reports stay short (≤ 40 lines): what changed, the build status, the draft link + SHA-256.
```
Change the workflows to match. The test jobs become `workflow_dispatch`-only, so the owner can still start them by hand.

## 2. Toolbar settings (Settings → Toolbar)
1. **Fixed order.**
   - The list in Settings always shows **all** toolbar commands in one fixed order: the order saved in `toolbar.order`, or the default order.
   - Turning an item off or on **never** moves it. A re-enabled item comes back in its old place.
   - Store the order (`toolbar.order`) separately from the enabled set (`toolbar.items`).
2. **Up / down buttons.**
   - Each row has **▲ Move up** and **▼ Move down** buttons, next to its checkbox.
   - They change the order, and the toolbar updates live.
   - Keyboard: `Alt+↑/↓` does the same. Drag-and-drop stays available too.
3. **Three presets** at the top of the section, as a segmented control: **Kapalı / Önemliler / Hepsi** (EN: Off / Essentials / All).
   - **Kapalı:** the toolbar is hidden (`toolbar.visible = false`).
   - **Önemliler:** the default set of 12 items.
   - **Hepsi:** every toolbar-capable command. Commands that do not apply in the current mode still show as disabled; overflow goes to ».
   - Choosing a preset never changes `toolbar.order`; it only sets which items are on.
   - **Özel / Custom:** turning any single item on or off by hand switches the segmented control to a fourth state, "Özel / Custom", and keeps the choice.
   - **Reset to default** restores the Önemliler set and the default order.
4. **The "only in Live/Split edit" note.**
   - Today this text sits **under** the editing toolbar buttons. Move it next to the button: for example, to the right of the Undo button, in the Settings list, in small (11 px), grey (`text_muted` at about 70 %) text.
   - It must not take a line of its own.
   - Do the same for every edit-only item, then remove the text below the buttons.
   - In the toolbar itself, edit-only buttons stay hidden in read mode, as now.
5. **Spec:** update UI_SPEC B.4.1 and B.8.2 (`toolbar.order`, the presets) with a spec patch (P-59), choosing anchors that match once.

## 3. Release
- **Version:** if the `v2.0.0` tag **does not exist yet**, include these fixes in v2.0.0. Otherwise this is **v2.0.1**: bump the version and add a CHANGELOG entry.
- **Tag:** after the build is green and the PR is merged, make the `chore(release): vX` commit and tag (F-25). The tag job creates the draft (zip + SHA-256).

## 4. Report (≤ 40 lines)
- what changed (§1, §2)
- the CI build status
- the draft link and its SHA-256
- the location of the settings file, so the owner can reset it by hand if needed

**END OF PROMPT 021.** Start with §1 now.

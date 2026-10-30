# v2.0 fix report — prompt 021 (toolbar settings, tests off) → v2.0.1 draft

Status (2026-10-30 21:50 UTC+3): PR #99 merged (c82aa380, F-24); v2.0.1 **draft** created, not published.
The v2.0.0 tag already existed (9417fece, draft unpublished), so these fixes are **v2.0.1**.

## What changed
- **§1 A-12** (owner decision 2026-10-31) is in `docs/PROCESS.md` (via `docs/spec-patches/PROCESS_AMENDMENTS.md`).
  CI on pushes/PRs is one `build` job: fmt + clippy, `cargo deny`, release build, `check-imports`
  (+ the `yamdv-exe` artifact on PRs). The test jobs (unit tests, goldens, driver scripts,
  measurements, pack, owner kit) run only when started by hand (`workflow_dispatch`). Test files stay.
  Permissions unchanged: `contents: read`; `contents: write` only in the release job; `pull_request` only.
- **§2 Settings → Toolbar**
  - The list always shows all 46 toolbar commands in one fixed order, `toolbar.order` (new key; empty =
    default order). The enabled set stays in `toolbar.items`. Turning an item off or on never moves it.
  - Every row: ▲ Move up / ▼ Move down buttons next to the switch; `Alt+↑/↓` and drag also reorder; live.
  - Top of the section: **Kapalı / Önemliler / Hepsi** (Off / Essentials / All); a hand change shows
    **Özel / Custom**. Presets never change the order. Reset toolbar = Önemliler + default order + visible.
  - "Yalnızca Canlı / Bölünmüş düzenleme" is now an 11 px grey note right of the item name (no extra line).
  - Spec patch **P-59** (a/b/c): UI_SPEC B.4.1 and B.8.2 (`toolbar.order`, presets); `spec-check` green.
- **v2.0.1:** Cargo version, CHANGELOG `[2.0.1]`, README toolbar line.

## CI build
| Run | Head | Result |
|---|---|---|
| 41108517571 | PR #99 head c982de8c | build job **green** (one run; test jobs skipped per A-12) |
| 41109653090 | tag v2.0.1 (57170e53) | release job **green** |

## Draft
- https://github.com/abuzer1495/YAMDV/releases/tag/untagged-d1fed091a1c7839c5829 (draft)
- `YAMDV-2.0.1-win-x64.zip`, 1,820,834 B, SHA-256
  `6c1e5468cd4b250a24f3ed8e35ac40da3a78d7b160ba28637ee0479e7730574f` (downloaded and checked)

## Settings file (reset by hand)
- Installed: `%APPDATA%\YAMDV\settings.ini` (e.g. `C:\Users\<you>\AppData\Roaming\YAMDV\settings.ini`).
- Portable (a `yamdv.portable` file next to the exe): `<exe folder>\YAMDV-data\settings.ini`.
- To reset only the toolbar: close YAMDV, delete the `items`, `order` and `visible` lines under
  `[toolbar]`, save. To reset everything: close YAMDV and delete `settings.ini`.
- In the app: Settings → Toolbar → Reset toolbar, or Settings → Files → Open config folder.

Next action: owner + friend test the v2.0.1 draft by hand (Turkish checklist)

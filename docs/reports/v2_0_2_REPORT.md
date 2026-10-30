# v2.0.2 report — owner micro change (edit-mode toolbar tint)

Status (2026-10-30 22:15 UTC+3): PR #100 merged (77aa9e6b, tree = green head, F-24);
`chore(release): v2.0.2` 7279dfad + tag v2.0.2; **draft** created, not published.

## What changed
- While Live edit (F2) or Split edit (F3) is on, the visible toolbar gets the new token
  `toolbar_edit_bg` (`yamdv_core::theme::toolbar_edit_bg`): the scheme's own `bg` blended toward
  a muted red, so it works for all 11 schemes.
  - Light: 7 % toward `#C83232` → white `#FFFFFF` → **`#FBF1F1`** (barely-there red).
  - Dark: 47 % toward `#5A1A24` → `#1E1E1E` → **`#3A1C21`** (dark burgundy);
    GitHub dark `#0D1117` → `#31151D`.
  - High Contrast: no tint, system colours only.
- Hover / pressed overlays and the separator are unchanged (they are alpha overlays on the tint).
- An "on" icon (e.g. the active Live / Split button) uses `accent`, or `text` where `accent`
  would fall below 3:1 on its 14 % fill over the tint (Lychee and Mint light).
- Entering or leaving F2/F3 already repaints the window; no relayout is added for the tint.
- Spec patch **P-60** (`docs/spec-patches/v2l.txt`, UI_SPEC B.4.1); `spec-check` green.
- v2.0.2: Cargo version, CHANGELOG `[2.0.2]`, README.

## Contrast on the tint (worst case incl. hover 5–6 % / pressed 9–10 % overlays)
| | Text | "On" icon | Disabled icon |
|---|---|---|---|
| Light, Default | 9.30:1 | 3.86:1 | 3.16:1 |
| Light, all schemes | ≥ 8.81:1 | ≥ 3.21:1 (text fallback where < 3) | ≥ 2.82:1 |
| Dark, Default | 7.65:1 | 4.73:1 | 4.03:1 |
| Dark, all schemes | ≥ 7.65:1 | ≥ 3.63:1 | ≥ 4.03:1 |
Disabled controls are exempt from WCAG 1.4.11; they stay ≥ 2.8:1.

## CI
| Run | Head | Result |
|---|---|---|
| 41111890069 | PR #100 head 0e01d248 | build job **green** (one run, tests skipped per A-12) |
| 41113091814 | tag v2.0.2 (7279dfad) | release job **green** |

## Draft
- https://github.com/abuzer1495/YAMDV/releases/tag/untagged-50d5c5aaa03fac3f8b5f
- `YAMDV-2.0.2-win-x64.zip`, 1,821,375 B, SHA-256
  `6b6ad8eac2135d1404e0159dbca45a4682010abd3aea8c13af80c031b371879b` (downloaded and checked)

# v2.0.3 report — owner fixes (prompt 023)

Status (2026-10-30 23:14 UTC+3): PR #101 merged (26d61865, tree = green head, F-24); #98 closed;
`chore(release): v2.0.3` ed9c825e + tag v2.0.3; **draft** created, not published.

## What changed
1. **#98 word count in the title bar.** "file.md · 168584 sözcük · 787578 karakter": 11 px
   `text_muted`, right after the file name (and the mode pill / unsaved dot). The floating
   bottom-right bubble is removed. `reading.word_count_badge`, menu Word count and toolbar
   `toggle_word_count` still show / hide it; hover keeps the details tooltip; with a selection it
   shows "sel / total". The file name has priority: if the count does not fit, it is hidden; also
   hidden while F11 hides the title bar. Spec P-61a/b/d (UI_SPEC B.10.3 + settings row, SPEC R-READ-07).
2. **Mermaid sequence `rect`.** `rect rgb(r,g,b)`, `rect rgba(r,g,b,a)` (a 0–1 or %),
   `#rgb` / `#rrggbb` / `#rrggbbaa`, CSS names (e.g. LightBlue). A filled band, no border, behind
   lifelines, messages, notes and frames; bands nest and may contain alt/else/loop/opt/par/notes.
   Unknown colour or missing `end` → the normal parse-error card.
   Dark mode (F-44/F-45): the author's colour at its alpha over the card bg is kept if text stays
   ≥ 4.5:1; otherwise a ≤ 20 % tint of it (5 % steps down until ≥ 4.5:1), e.g. `#EEF3FB` →
   `#545556` (6.0:1). Light mode keeps the exact colour (text 11.3:1). High Contrast: no fill.
   Fixture `tests/fixtures/mermaid/seq-rect.md` (owner example) + unit test `seq_rect_owner_example`.
   Spec P-61c.
3. **Fullscreen viewer repeat.** Root cause: `render_bands` reuses one DIB for every band
   (ADR-0025) and never cleared it, and `fv_blit` paints no background, so each band below the
   diagram kept the previous band's pixels. Fix: `geom::reset_band` zeroes it per band (+ unit test).
- Version 2.0.3, CHANGELOG `[2.0.3]`, README.
- Local: fmt, clippy -D warnings, tests, deny, spec-check, release build, check-imports green.

## CI
| Run | Head | Result |
|---|---|---|
| 41119527544 | PR #101 head b77a8a56 | build job **green** (one run, tests skipped per A-12) |
| 41120467954 | tag v2.0.3 (ed9c825e) | release job **green** |

## Draft
- https://github.com/abuzer1495/YAMDV/releases/tag/untagged-87842078c64793f6ba2e
- `YAMDV-2.0.3-win-x64.zip`, 1,825,185 B, SHA-256
  `7d818b20b516b49281d556a678387f5d5a14b7caa37da76281361b7302e71284` (downloaded and checked)

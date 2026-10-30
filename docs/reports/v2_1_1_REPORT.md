# v2.1.1 report

Owner 2026-10-31 00:51 UTC+3: remove the old account from the whole project, release 2.1.1, add a
download link (release page) to the README, write a Turkish user guide.

## Changes
- Old-account references removed from 18 files (PROCESS.md + PROCESS_AMENDMENTS.md, plans V2B_FIX/V2C,
  prompts 013/020, 12 reports); amendment A-13 records the move to https://github.com/snipeTR/YAMDV.
- `docs/KULLANIM_KILAVUZU.md`: Turkish user guide, 13 sections (install / portable / uninstall, opening
  files, reading, appearance, F2 / F3 editing, Mermaid + visual flowchart editing + math, toolbar, export /
  print / copy, mind map + jump list + editor plugins, shortcuts table, settings files and reset, privacy
  and how to verify it, FAQ). Names follow the Turkish UI strings (i18n.rs, keymap.rs, toolbar.rs,
  settings_panel.rs); screenshots are existing goldens only.
- README.md / README.tr.md: Download / İndir section at the top (releases/latest + releases), guide link.
- Release zip also ships README.tr.md and KULLANIM_KILAVUZU.md (ci.yml release job).
- Version 2.1.1 (workspace, VS Code plugin manifest), CHANGELOG [2.1.1]. No program code changes.

## Checks
- Local: fmt, xwin clippy -D warnings, 40 test suites green, cargo deny ok, spec-check 0, check-imports OK.
- `rg -i -a abuzer` on the tree, `git grep -i -a` on main, commit metadata, the extracted zip and the
  exe strings (ASCII + UTF-16): 0 hits each.

## CI and release
- PR #1, CI run 41135065623 green (build job, A-12), squash-merged as aa64e03 (tree == green head 4bcc004).
- `chore(release): v2.1.1` d0c0df79 (its push run 41136297621 skipped by design, F-32), tag v2.1.1,
  release run 41136300594 green.
- Draft (not published): https://github.com/snipeTR/YAMDV/releases/tag/untagged-1e163ce857f31cd93f1d
- `YAMDV-2.1.1-win-x64.zip`, 1,866,982 B, SHA-256
  2273f5f0b60e45c18922a83195d9ef2a0624c7e570a51115fe0fa80145b93993 (matches the .sha256 asset).
- `yamdv.exe` 3,595,776 B ≤ B-SIZE 3,600,000 (headroom 4,224 B).
- About link https://github.com/snipeTR/YAMDV/tree/v2.1.1 resolves (HTTP 200).
- Not run: the Windows test job is workflow_dispatch-only (A-12).

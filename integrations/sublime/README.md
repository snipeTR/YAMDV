<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# YAMDV preview for Sublime Text (FUT-27)

Opens the Markdown file you are editing in **YAMDV** (`yamdv.exe`). YAMDV watches the file and
re-renders on every save, keeping its place, so its window works as a live preview.

- **YAMDV: Open preview**: in the Command Palette, in the editor context menu, and
  <kbd>Ctrl+Alt+M</kbd> in Markdown files.
- The caret line is passed on (`--line N`); a modified file is saved first.

**Privacy:** the plugin makes no network requests and collects no telemetry. It only starts the
local `yamdv.exe` with an argument list (`[--line N] -- <file>`, no shell).

## Install

Sublime Text 4 (Python 3.8 plugin host). Copy this folder as `YAMDV` into the Packages folder
(**Preferences → Browse Packages…**), e.g.

```
xcopy /E /I integrations\sublime "%APPDATA%\Sublime Text\Packages\YAMDV"
```

The package is not published to Package Control.

## Settings (`Preferences → Package Settings`, or `YAMDV.sublime-settings` in `Packages/User`)

| Setting | Default | Meaning |
|---|---|---|
| `path` | `""` | Full path of `yamdv.exe`. Empty: `yamdv.exe` on `PATH`, then `%LOCALAPPDATA%\Programs\YAMDV\yamdv.exe` (the per-user install made by `yamdv --install`). |
| `pass_line` | `true` | Pass the caret line (`--line N`). |
| `save_first` | `true` | Save a modified file before opening it. |

## Test the launcher logic

`python tests/test_yamdv.py` (no Sublime Text needed).

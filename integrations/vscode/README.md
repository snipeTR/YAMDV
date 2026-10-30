<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# YAMDV Preview for VS Code (FUT-27)

Opens the Markdown file you are editing in **YAMDV** (`yamdv.exe`). YAMDV watches the file and
re-renders on every save, keeping its place, so its window works as a live preview beside the
editor.

- **YAMDV: Open preview**: in the Command Palette, as the preview button in the editor title bar
  of a Markdown file, and in the Explorer context menu.
- The editor's current line is passed on (`--line N`), so the preview opens near your caret.
- A file with unsaved changes is saved first (`yamdv.saveFirst`).

**Privacy:** the extension makes no network requests and collects no telemetry. It only starts
the local `yamdv.exe` with an argument array (`[--line N] -- <file>`, no shell).

## Settings

| Setting | Default | Meaning |
|---|---|---|
| `yamdv.path` | empty | Full path of `yamdv.exe`. Empty: `yamdv.exe` on `PATH`, then `%LOCALAPPDATA%\Programs\YAMDV\yamdv.exe` (the per-user install made by `yamdv --install`). |
| `yamdv.passLine` | `true` | Pass the caret line (`--line N`). |
| `yamdv.saveFirst` | `true` | Save a dirty document before opening it. |

## Build and install (local; the extension is not published)

Needs Node.js 18+. The `.vsix` is not built in CI (see ADR-0028).

```
cd integrations/vscode
npm install            # typescript, @types/*, @vscode/vsce (dev only; nothing is bundled)
npm test               # compiles and runs the launcher tests
npm run package        # → yamdv-preview-2.1.1.vsix
code --install-extension yamdv-preview-2.1.1.vsix
```

Or, for development, open this folder in VS Code and press F5 (Extension Development Host).

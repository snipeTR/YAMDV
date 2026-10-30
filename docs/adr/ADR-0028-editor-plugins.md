# ADR-0028: Editor launcher plugins (FUT-27, #26)

- Status: accepted (prompt 024, v2.1.0).

## Context
FUT-27 asks for VS Code and Sublime Text "live preview" plugins. YAMDV already live-reloads the open file (R-READ-01) and accepts `--line N` (F.4), so a plugin only needs to start it.

## Decision
- `integrations/vscode/` (TypeScript) and `integrations/sublime/` (Python 3.8 plugin host) each add one command, "YAMDV: Open preview", that spawns `yamdv.exe [--line N] -- <path>` with an argument vector (no shell), detached. `--line` is the 1-based caret line (setting, default on); the file is optionally saved first.
- Executable lookup: the `yamdv.path` setting, then `PATH`, then `%LOCALAPPDATA%\Programs\YAMDV\yamdv.exe` (the self-install location, R-SYS-05).
- **No network, no telemetry**: neither plugin imports an HTTP module or sends anything; they only start a local process.
- **Build.** The `.vsix` is **not** built in CI: it needs `npm install` of `typescript`, `@types/*` and `@vscode/vsce` (third-party packages and network in CI) for a 16 KB artifact. The README documents `npm install`, `npm test`, `npm run package`. The Sublime package needs no build (copy the folder into `Packages/`).
- **Never published** to the VS Code Marketplace, Open VSX or Package Control by this project's automation; the owner decides.

## Consequences
The plugins are separate from the Rust workspace and from the release zip; their pure helpers have tests (`src/launch.test.ts`, `tests/test_yamdv.py`) run locally.

// SPDX-License-Identifier: AGPL-3.0-or-later
// Pure helpers (no VS Code API): where yamdv.exe is and how it is called. Unit-tested by
// `node out/launch.test.js`.

import * as path from 'path';

/** Command line for `file`: `[--line N] -- <file>` (`--` keeps a name starting with `-` a path). */
export function buildArgs(file: string, line?: number): string[] {
  const args: string[] = [];
  if (line !== undefined && Number.isInteger(line) && line > 0) {
    args.push('--line', String(line));
  }
  args.push('--', file);
  return args;
}

/**
 * yamdv.exe to start: the setting when set, else the first `yamdv.exe` on PATH, else the
 * per-user install under %LOCALAPPDATA%. `undefined` when none exists.
 */
export function resolveExe(
  setting: string,
  env: { [k: string]: string | undefined },
  exists: (p: string) => boolean,
): string | undefined {
  const s = setting.trim();
  if (s !== '') {
    return exists(s) ? s : undefined;
  }
  const pathVar = env['PATH'] ?? env['Path'] ?? '';
  for (const dir of pathVar.split(path.win32.delimiter)) {
    const d = dir.trim().replace(/^"(.*)"$/, '$1');
    if (d === '') {
      continue;
    }
    const p = path.win32.join(d, 'yamdv.exe');
    if (exists(p)) {
      return p;
    }
  }
  const local = env['LOCALAPPDATA'];
  if (local) {
    const p = path.win32.join(local, 'Programs', 'YAMDV', 'yamdv.exe');
    if (exists(p)) {
      return p;
    }
  }
  return undefined;
}

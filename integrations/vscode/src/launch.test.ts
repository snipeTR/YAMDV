// SPDX-License-Identifier: AGPL-3.0-or-later
// `npm run compile && node out/launch.test.js` — no test framework, plain assertions.

import * as assert from 'assert';
import { buildArgs, resolveExe } from './launch';

assert.deepStrictEqual(buildArgs('C:\\d\\a.md'), ['--', 'C:\\d\\a.md']);
assert.deepStrictEqual(buildArgs('-x.md', 12), ['--line', '12', '--', '-x.md']);
assert.deepStrictEqual(buildArgs('a.md', 0), ['--', 'a.md']);

const files = new Set(['C:\\Tools\\yamdv.exe', 'C:\\Users\\u\\AppData\\Local\\Programs\\YAMDV\\yamdv.exe']);
const has = (p: string) => files.has(p);
assert.strictEqual(resolveExe('', { PATH: 'C:\\Windows;"C:\\Tools"' }, has), 'C:\\Tools\\yamdv.exe');
assert.strictEqual(
  resolveExe('', { PATH: 'C:\\Windows', LOCALAPPDATA: 'C:\\Users\\u\\AppData\\Local' }, has),
  'C:\\Users\\u\\AppData\\Local\\Programs\\YAMDV\\yamdv.exe',
);
assert.strictEqual(resolveExe('D:\\y\\yamdv.exe', { PATH: 'C:\\Tools' }, has), undefined);
assert.strictEqual(resolveExe(' C:\\Tools\\yamdv.exe ', {}, has), 'C:\\Tools\\yamdv.exe');
assert.strictEqual(resolveExe('', {}, has), undefined);
console.log('launch tests ok');

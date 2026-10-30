// SPDX-License-Identifier: AGPL-3.0-or-later
// FUT-27 (ADR-0028): "YAMDV: Open preview" starts yamdv.exe on the current Markdown file.
// YAMDV watches the file and re-renders on every save, so the preview stays live. The
// extension makes no network requests and collects no telemetry; it only spawns the local
// executable with an argument array (no shell).

import * as vscode from 'vscode';
import { spawn } from 'child_process';
import * as fs from 'fs';
import { buildArgs, resolveExe } from './launch';

function exists(p: string): boolean {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

async function openPreview(uri?: vscode.Uri): Promise<void> {
  const editor = vscode.window.activeTextEditor;
  const target = uri ?? editor?.document.uri;
  if (!target || target.scheme !== 'file') {
    void vscode.window.showWarningMessage('YAMDV: open a Markdown file saved on disk first.');
    return;
  }
  const cfg = vscode.workspace.getConfiguration('yamdv');
  const doc = editor && editor.document.uri.toString() === target.toString() ? editor.document : undefined;
  if (doc && cfg.get<boolean>('saveFirst', true) && doc.isDirty) {
    await doc.save();
  }
  const exe = resolveExe(cfg.get<string>('path', ''), process.env, exists);
  if (!exe) {
    void vscode.window.showErrorMessage(
      'YAMDV: yamdv.exe not found. Set "yamdv.path" or put yamdv.exe on PATH.',
    );
    return;
  }
  const line = doc && cfg.get<boolean>('passLine', true) ? editor!.selection.active.line + 1 : undefined;
  try {
    const child = spawn(exe, buildArgs(target.fsPath, line), {
      detached: true,
      stdio: 'ignore',
      shell: false,
      windowsHide: false,
    });
    child.on('error', (e) => {
      void vscode.window.showErrorMessage(`YAMDV: could not start ${exe}: ${e.message}`);
    });
    child.unref();
  } catch (e) {
    void vscode.window.showErrorMessage(`YAMDV: could not start ${exe}: ${String(e)}`);
  }
}

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(vscode.commands.registerCommand('yamdv.openPreview', openPreview));
}

export function deactivate(): void {}

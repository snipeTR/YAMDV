# SPDX-License-Identifier: AGPL-3.0-or-later
"""FUT-27 (ADR-0028): open the current Markdown file in YAMDV (yamdv.exe).

YAMDV watches the file and re-renders on every save, so its window is a live preview. This
plugin makes no network requests and collects no telemetry; it only starts the local
executable with an argument list (no shell).
"""

import os
import subprocess

import sublime
import sublime_plugin

SETTINGS = "YAMDV.sublime-settings"
MARKDOWN_EXT = (".md", ".markdown", ".mdown", ".mkd", ".mkdn", ".mdwn", ".mdtxt", ".txt")


def build_args(exe, path, line=None):
    """`exe [--line N] -- path` (`--` keeps a name starting with `-` a path)."""
    args = [exe]
    if isinstance(line, int) and line > 0:
        args += ["--line", str(line)]
    return args + ["--", path]


def resolve_exe(setting, environ, exists):
    """The setting when set, else yamdv.exe on PATH, else the per-user install."""
    s = (setting or "").strip()
    if s:
        return s if exists(s) else None
    for d in environ.get("PATH", "").split(os.pathsep):
        d = d.strip().strip('"')
        if d:
            p = os.path.join(d, "yamdv.exe")
            if exists(p):
                return p
    local = environ.get("LOCALAPPDATA")
    if local:
        p = os.path.join(local, "Programs", "YAMDV", "yamdv.exe")
        if exists(p):
            return p
    return None


def is_markdown(view):
    path = view.file_name() or ""
    syntax = (view.settings().get("syntax") or "").lower()
    return "markdown" in syntax or path.lower().endswith(MARKDOWN_EXT)


class YamdvOpenPreviewCommand(sublime_plugin.TextCommand):
    """Command Palette: "YAMDV: Open preview"."""

    def is_enabled(self):
        return bool(self.view.file_name()) and is_markdown(self.view)

    def run(self, edit):
        path = self.view.file_name()
        if not path:
            sublime.status_message("YAMDV: save the file first")
            return
        settings = sublime.load_settings(SETTINGS)
        if settings.get("save_first", True) and self.view.is_dirty():
            self.view.run_command("save")
        exe = resolve_exe(settings.get("path", ""), os.environ, os.path.isfile)
        if not exe:
            sublime.error_message(
                'YAMDV: yamdv.exe not found. Set "path" in YAMDV.sublime-settings '
                "or put yamdv.exe on PATH."
            )
            return
        line = None
        if settings.get("pass_line", True) and len(self.view.sel()) > 0:
            line = self.view.rowcol(self.view.sel()[0].b)[0] + 1
        flags = 0
        if os.name == "nt":
            flags = subprocess.DETACHED_PROCESS | subprocess.CREATE_NEW_PROCESS_GROUP
        try:
            subprocess.Popen(
                build_args(exe, path, line),
                shell=False,
                close_fds=True,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                creationflags=flags,
            )
        except OSError as e:
            sublime.error_message("YAMDV: could not start {}: {}".format(exe, e))

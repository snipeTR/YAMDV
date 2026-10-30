# SPDX-License-Identifier: AGPL-3.0-or-later
"""`python tests/test_yamdv.py` — launcher helpers without Sublime Text (stub modules). Kept
in a subfolder: Sublime Text loads only top-level `.py` files as plugins."""

import ntpath
import os
import sys
import types

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))

sys.modules.setdefault("sublime", types.ModuleType("sublime"))
stub = types.ModuleType("sublime_plugin")
stub.TextCommand = object
sys.modules.setdefault("sublime_plugin", stub)
os.path = ntpath  # Windows path rules on any host
os.pathsep = ";"

import YAMDV  # noqa: E402

assert YAMDV.build_args("y.exe", "a.md") == ["y.exe", "--", "a.md"]
assert YAMDV.build_args("y.exe", "-x.md", 7) == ["y.exe", "--line", "7", "--", "-x.md"]
assert YAMDV.build_args("y.exe", "a.md", 0) == ["y.exe", "--", "a.md"]
files = {"C:\\Tools\\yamdv.exe", "C:\\L\\Programs\\YAMDV\\yamdv.exe"}
has = files.__contains__
assert YAMDV.resolve_exe("", {"PATH": 'C:\\Windows;"C:\\Tools"'}, has) == "C:\\Tools\\yamdv.exe"
assert YAMDV.resolve_exe("", {"PATH": "C:\\W", "LOCALAPPDATA": "C:\\L"}, has) == (
    "C:\\L\\Programs\\YAMDV\\yamdv.exe"
)
assert YAMDV.resolve_exe("D:\\no.exe", {}, has) is None
assert YAMDV.resolve_exe("", {}, has) is None
print("sublime launcher tests ok")

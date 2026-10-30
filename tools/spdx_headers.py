#!/usr/bin/env python3
"""Adds (default) or checks (--check) the SPDX header on every tracked Rust source file
(prompt 020 §1.3: AGPL-3.0-or-later, DEC-01 #71)."""
import subprocess, sys

HEADER = "// SPDX-License-Identifier: AGPL-3.0-or-later\n"
check = "--check" in sys.argv
files = subprocess.run(["git", "ls-files", "*.rs"], capture_output=True, text=True, check=True).stdout.split()
missing = []
for f in files:
    src = open(f, encoding="utf-8").read()
    if src.startswith(HEADER):
        continue
    missing.append(f)
    if not check:
        open(f, "w", encoding="utf-8", newline="\n").write(HEADER + src)
if check and missing:
    print("SPDX header missing:", *missing, sep="\n  ")
    sys.exit(1)
print(f"spdx: {len(files)} files, {len(missing)} {'missing' if check else 'updated'}")

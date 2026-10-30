#!/usr/bin/env python3
"""Regenerates crates/yamdv-core/src/emoji_table.rs from a local gemoji db/emoji.json (ADR-0014).
Usage: python3 tools/gen_emoji.py path/to/emoji.json COMMIT"""
import json, sys
d = json.load(open(sys.argv[1], encoding="utf-8"))
commit = sys.argv[2]
pairs = {}
for e in d:
    for a in e["aliases"]:
        pairs.setdefault(a, e["emoji"])
items = sorted(pairs.items(), key=lambda kv: kv[0].encode())
out = ["// SPDX-License-Identifier: AGPL-3.0-or-later", "//! GitHub emoji shortcodes (R-MD-13). GENERATED from the gemoji database",
       "//! (https://github.com/github/gemoji, `db/emoji.json` at commit",
       f"//! {commit}, MIT license, ADR-0014). Do not edit by hand.", "",
       "/// (shortcode without colons, emoji), sorted by shortcode bytes for binary search."]
sys.path.insert(0, __import__("os").path.dirname(__file__))
from pack_table import pack
recs = [(k, "".join(c for c in v if ord(c) >= 0x20)) for k, v in items]
src = "\n".join(out) + "\n" + pack("EMOJI", recs, all_escapes=True)
open("crates/yamdv-core/src/emoji_table.rs", "w", encoding="utf-8").write(src)

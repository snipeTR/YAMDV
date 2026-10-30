#!/usr/bin/env python3
"""Packs a sorted (key, value) string table into the compact Rust form used by
`yamdv_core::packed::Packed` (prompt 020 §3.9 / ADR-0026): one string blob plus `u16` offsets
instead of a `[(&str, &str); N]` array (32 bytes and 4 relocations per entry).

Used by tools/gen_entities.py and tools/gen_emoji.py. `python3 tools/pack_table.py <file.rs>`
re-packs an existing generated file in the old array form (one-off conversion)."""
import re, sys


def esc(s, all_escapes=False):
    out = []
    for ch in s:
        o = ord(ch)
        if all_escapes and o >= 0x80:
            out.append("\\u{%x}" % o)
        elif ch.isspace() or ch in '"\\' or o < 0x20 or o == 0x7f or 0xd800 <= o <= 0xdfff or o in (0x200b, 0x200c, 0x200d, 0x2060, 0xfeff) or 0x2000 <= o <= 0x206f or 0x80 <= o < 0xa0 or o == 0xad:
            out.append("\\u{%x}" % o)
        else:
            out.append(ch)
    return "".join(out)


def pack(name, recs, all_escapes=False):
    """Rust source for `pub static {name}: Packed` from (key, value) pairs sorted by key bytes."""
    keys = [k.encode() for k, _ in recs]
    assert keys == sorted(keys) and len(set(keys)) == len(keys), "keys must be sorted and unique"
    off, pos, parts = [0], 0, []
    for k, v in recs:
        for s in (k, v):
            pos += len(s.encode())
            off.append(pos)
            parts.append(s)
    assert pos < 65536, "blob too large for u16 offsets"
    lines, cur = [], ""
    for s in parts:
        e = esc(s, all_escapes)
        if len(cur) + len(e) > 88:
            lines.append(cur)
            cur = ""
        cur += e
    lines.append(cur)
    blob = "\\\n".join(lines)
    nums = ", ".join(str(o) for o in off)
    import textwrap
    nums = "\n".join("    " + l for l in textwrap.wrap(nums, 96))
    return (f"pub static {name}: crate::packed::Packed = crate::packed::Packed {{\n"
            f"    blob: \"{blob}\",\n    off: &OFF,\n}};\n\n"
            f"/// Start of each key and value in the blob (key i = `2i..2i+1`, value = `2i+1..2i+2`).\n"
            f"static OFF: [u16; {len(off)}] = [\n{nums}\n];\n")


def unesc(s):
    s = re.sub(r"\\u\{([0-9a-fA-F]+)\}", lambda m: chr(int(m.group(1), 16)), s)
    return s.replace('\\"', '"').replace("\\\\", "\\")


if __name__ == "__main__":
    path = sys.argv[1]
    src = open(path, encoding="utf-8").read()
    m = re.search(r"(?ms)^pub static (\w+): \[\(&str, &str\); \d+\] = \[\n(.*?)^\];\n", src)
    recs = re.findall(r'\(\s*"((?:[^"\\]|\\.)*)",\s*"((?:[^"\\]|\\.)*)",?\s*\)', m.group(2))
    recs = [(unesc(k), unesc(v)) for k, v in recs]
    allesc = "emoji" in path
    out = src[: m.start()] + pack(m.group(1), recs, allesc) + src[m.end():]
    open(path, "w", encoding="utf-8").write(out)
    print(path, len(recs))

#!/usr/bin/env python3
"""Internal helper for clifdiff.sh: normalize CLIF text so cosmetic renumbering (v-values,
blocks, sig/fn table indices, and the `#syntax_<file>_<node>` node-id suffix Beskid appends to
function names) doesn't drown out a real structural difference. Reads stdin, writes stdout."""
import re
import sys

text = sys.stdin.read()

# Strip Beskid's per-generation node-id suffix on function labels: `Name#syntax_File_bd_123` -> `Name`
text = re.sub(r"(#syntax_[A-Za-z0-9_]+)", "", text)

# Renumber v<N>, block<N>, sig<N>, fn<N>, u<N>:<N> in first-appearance order, per input, so two
# functionally-identical dumps compiled independently (different node ids, different value
# numbering) normalize to the same text.
counters = {}


def renumber(prefix):
    def repl(m):
        n = m.group(1)
        table = counters.setdefault(prefix, {})
        if n not in table:
            table[n] = len(table)
        return f"{prefix}{table[n]}"
    return repl


for prefix, pattern in [
    ("v", r"\bv(\d+)\b"),
    ("block", r"\bblock(\d+)\b"),
    ("sig", r"\bsig(\d+)\b"),
    ("fn", r"\bfn(\d+)\b"),
]:
    text = re.sub(pattern, renumber(prefix), text)

# u0:0 style module:function indices -> normalize function index only (module index is usually 0)
text = re.sub(r"\bu(\d+):(\d+)\b", lambda m: f"u{m.group(1)}:F", text)

sys.stdout.write(text)

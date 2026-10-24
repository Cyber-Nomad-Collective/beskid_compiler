#!/usr/bin/env python3
"""Codemod for 0.6.7: fields in `type` bodies end with `;` instead of being comma-separated.

    type Point { i32 x, i32 y }          ->  type Point { i32 x; i32 y; }
    type Box<T> { T value, }             ->  type Box<T> { T value; }

Usage: type_field_semicolons.py [--check] FILE...
Works on Beskid sources (.bd) and on Beskid text inside Rust test sources (.rs): it rewrites only
`type Name ... {` bodies, the field part before the first method or associated type. Enum
variants, struct values and contract bodies are untouched. Prints each changed file; with
--check, changes nothing and exits 1 when a file would change. Bodies it cannot rewrite safely
(a field directly followed by a method without a comma) are listed as "review".
"""

import re
import sys

# A Beskid type header: `type` starts the line or follows `;`/`}` (after indentation, a Rust string's
# opening quote, an escaped newline, `pub` or attributes) and the name is PascalCase.
HEADER = re.compile(r'(?m)(?:^|"|\\n|\]|;|\})[ \t]*(?:pub[ \t]+)?type[ \t]+[A-Z]\w*')
OPEN = {"(": ")", "[": "]", "<": ">", "{": "}"}
CLOSE = {")", "]", ">", "}"}


def header_brace(text, start):
    """Offset of the `{` that opens the body of the `type` header at `start`, or None."""
    i = start
    depth = 0
    while i < len(text):
        ch = text[i]
        if ch in "([<":
            depth += 1
        elif ch in ")]>":
            depth -= 1
        elif ch == "{" and depth == 0:
            return i
        elif ch in ";=\n" and depth == 0 and ch != "\n":
            return None  # `type X = Y;` (associated type) or a statement, not a body
        elif ch == "}" and depth == 0:
            return None
        i += 1
    return None


def rewrite_body(text, open_brace, rust):
    """Return (edits, review) for the body opening at `open_brace`. Edits are (offset, old_len, new)."""
    i = open_brace + 1
    depth = 0
    commas = []
    stop = None
    string = None
    while i < len(text):
        ch = text[i]
        if string:
            if ch == "\\":
                i += 2
                continue
            if ch == string:
                string = None
            i += 1
            continue
        if ch == '"':
            if rust:
                return [], False  # the Rust string around the fixture ends here: leave it alone
            string = '"'
        elif text.startswith("//", i):
            nl = text.find("\n", i)
            i = len(text) if nl < 0 else nl
            continue
        elif ch == "{" and re.match(r"\{\s*\d+\s*\}", text[i:]):
            i += re.match(r"\{\s*\d+\s*\}", text[i:]).end()  # event capacity `event{16}`
            continue
        elif ch in "([<":
            depth += 1
        elif ch in ")]>" and not text.startswith("=>", i - 1):
            depth -= 1
        elif depth == 0 and (ch == "{" or text.startswith("=>", i) or ch == ";"):
            stop = ("member", i)
            break
        elif depth == 0 and ch == "}":
            stop = ("end", i)
            break
        elif depth == 0 and ch == "," :
            commas.append(i)
        elif depth == 0 and re.match(r"type\s", text[i:i + 5]) and re.match(r"[\s,{]", text[i - 1]):
            stop = ("member", i)
            break
        i += 1
    if stop is None:
        return [], False
    kind, at = stop
    edits = [(c, 1, ";") for c in commas]
    tail_start = commas[-1] + 1 if commas else open_brace + 1
    tail = text[tail_start:at]
    if kind == "end":
        # In Rust sources a fixture's line breaks are `\\n` escapes; they count as whitespace.
        trailing = re.compile(r"(?:\s|\\n)*$" if rust else r"\s*$")
        if trailing.sub("", tail).strip():
            end = tail_start + len(trailing.sub("", tail))
            edits.append((end, 0, ";"))
        return edits, False
    # A member follows. Its signature sits in the tail; anything before it on earlier lines would
    # be a field without a trailing comma.
    lines = [line for line in tail.strip().splitlines() if line.strip() and not line.strip().startswith("//")]
    review = len(lines) > 1 and not commas
    return edits, review


def transform(text, rust=False):
    edits = []
    reviews = []
    for match in HEADER.finditer(text):
        brace = header_brace(text, match.end())
        if brace is None:
            continue
        body_edits, review = rewrite_body(text, brace, rust)
        edits.extend(body_edits)
        if review:
            reviews.append(text.count("\n", 0, match.start()) + 1)
    for offset, old_len, new in sorted(set(edits), reverse=True):
        text = text[:offset] + new + text[offset + old_len:]
    return text, reviews


def main(argv):
    check = "--check" in argv
    files = [arg for arg in argv if arg != "--check"]
    changed = False
    for path in files:
        with open(path, encoding="utf-8") as handle:
            original = handle.read()
        updated, reviews = transform(original, rust=path.endswith(".rs"))
        for line in reviews:
            print(f"review {path}:{line}")
        if updated != original:
            changed = True
            print(path)
            if not check:
                with open(path, "w", encoding="utf-8") as handle:
                    handle.write(updated)
    return 1 if check and changed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

#!/usr/bin/env python3
"""Beskid 0.6.7 codemod: `type` body fields end with `;` instead of `,`.

    type Point { i32 x, i32 y }   ->   type Point { i32 x; i32 y; }

Only fields (value, `inject` and `event` fields) of `type` declarations are
rewritten. Enum bodies, struct literals, call arguments, parameter lists,
methods and associated-type bindings are never touched. Beskid code fenced as
`beskid` or `bd` inside Markdown/MDX files is rewritten with the same logic.

Usage: python3 -I v067_field_semicolons.py [--check] [--dry-run] PATH...
"""
import argparse
import difflib
import os
import re
import sys

SKIP_DIRS = {"obj", "target", ".beskid", "node_modules", ".git"}

# --------------------------------------------------------------------------
# Tokenizer


class Tok:
    __slots__ = ("kind", "text", "start", "end")

    def __init__(self, kind, text, start, end):
        self.kind, self.text, self.start, self.end = kind, text, start, end


def tokenize(src):
    """Significant tokens only (comments, whitespace dropped). Strings and
    char literals are single tokens, so commas inside them are invisible."""
    toks = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c in " \t\r\n":
            i += 1
        elif src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
        elif src.startswith("/*", i):
            j = src.find("*/", i + 2)
            i = n if j < 0 else j + 2
        elif c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            j = min(j + 1, n)
            toks.append(Tok("str", src[i:j], i, j))
            i = j
        elif c == "'":
            m = re.match(r"'(?:\\[^\n']{1,10}|\\'|[^'\\\n])'", src[i:i + 14])
            if m:
                toks.append(Tok("chr", m.group(0), i, i + len(m.group(0))))
                i += len(m.group(0))
            else:
                toks.append(Tok("p", c, i, i + 1))
                i += 1
        elif c.isalpha() or c == "_":
            j = i + 1
            while j < n and (src[j].isalnum() or src[j] == "_"):
                j += 1
            toks.append(Tok("id", src[i:j], i, j))
            i = j
        elif c.isdigit():
            j = i + 1
            while j < n and (src[j].isalnum() or src[j] == "_"):
                j += 1
            toks.append(Tok("num", src[i:j], i, j))
            i = j
        elif src.startswith("=>", i):
            toks.append(Tok("p", "=>", i, i + 2))
            i += 2
        elif src.startswith("::", i):
            toks.append(Tok("p", "::", i, i + 2))
            i += 2
        else:
            toks.append(Tok("p", c, i, i + 1))
            i += 1
    return toks


OPEN, CLOSE = {"(": ")", "[": "]", "{": "}"}, {")", "]", "}"}


class Unclassified(Exception):
    pass


# --------------------------------------------------------------------------
# Type-body rewriting


def skip_balanced(toks, i):
    """toks[i] is an opener of ( [ {; return index after its matching closer."""
    stack = [OPEN[toks[i].text]]
    i += 1
    while i < len(toks) and stack:
        t = toks[i]
        if t.kind == "p":
            if t.text in OPEN:
                stack.append(OPEN[t.text])
            elif t.text in CLOSE:
                if stack[-1] != t.text:
                    raise Unclassified("mismatched bracket")
                stack.pop()
        i += 1
    if stack:
        raise Unclassified("unterminated bracket")
    return i


def skip_angles(toks, i):
    """toks[i] is `<`; return index after the matching `>`."""
    depth = 0
    while i < len(toks):
        t = toks[i]
        if t.kind == "p":
            if t.text == "<":
                depth += 1
            elif t.text == ">":
                depth -= 1
                if depth == 0:
                    return i + 1
            elif t.text in OPEN:
                i = skip_balanced(toks, i)
                continue
            elif t.text in (";", "{", "}"):
                raise Unclassified("unterminated generics")
        i += 1
    raise Unclassified("unterminated generics")


def type_header_end(toks, i):
    """toks[i] is the `type` keyword. Return index of the body `{`, or None if
    this is not a type definition."""
    j = i + 1
    if j >= len(toks) or toks[j].kind != "id":
        return None
    j += 1
    try:
        if j < len(toks) and toks[j].text == "<":
            j = skip_angles(toks, j)
        if j < len(toks) and toks[j].text == ":":
            j += 1
            while j < len(toks):
                t = toks[j]
                if t.kind == "id" or t.text in ("::", ","):
                    j += 1
                elif t.text == "<":
                    j = skip_angles(toks, j)
                else:
                    break
    except Unclassified:
        return None
    if j < len(toks) and toks[j].text == "{":
        return j
    return None


def is_p(toks, j, text):
    return j < len(toks) and toks[j].kind == "p" and toks[j].text == text


def is_id(toks, j, text=None):
    return j < len(toks) and toks[j].kind == "id" and (text is None or toks[j].text == text)


def parse_type(toks, j):
    """Consume one BeskidType starting at toks[j]; return the index after it."""
    if is_p(toks, j, "("):  # arrow function type: (A, B) => R
        j = skip_balanced(toks, j)
        if not is_p(toks, j, "=>"):
            raise Unclassified("tuple type without `=>`")
        return parse_type(toks, j + 1)
    if not is_id(toks, j):
        raise Unclassified("expected a type")
    j += 1
    while True:
        if is_p(toks, j, "<"):
            j = skip_angles(toks, j)
        if is_p(toks, j, ".") and is_id(toks, j + 1):
            j += 2
            continue
        if is_p(toks, j, "::") and is_id(toks, j + 1):
            j += 2
            continue
        break
    if is_p(toks, j, "("):  # function type: R(A, B)
        return skip_balanced(toks, j)
    while is_p(toks, j, "[") and is_p(toks, j + 1, "]"):
        j += 2
    return j


def parse_member(toks, j, edits):
    """Parse one member starting at toks[j]. Returns (next index, rewritten)."""
    n = len(toks)
    first = j
    if is_p(toks, j, "."):  # `...` placeholder in documentation snippets
        while j < n and not (toks[j].kind == "p" and toks[j].text in (",", ";", "}")):
            j += 1
        return (j + 1 if is_p(toks, j, ",") or is_p(toks, j, ";") else j), 0
    while is_p(toks, j, "["):
        j = skip_balanced(toks, j)
    if is_id(toks, j, "pub"):
        j += 1
    if is_id(toks, j, "type"):  # associated-type binding
        while j < n and not is_p(toks, j, ";"):
            if toks[j].kind == "p" and toks[j].text in OPEN:
                j = skip_balanced(toks, j)
            elif is_p(toks, j, "}"):
                raise Unclassified("binding not terminated")
            else:
                j += 1
        if j >= n:
            raise Unclassified("binding not terminated")
        return j + 1, 0
    method = False
    if is_id(toks, j, "event"):
        j += 1
        if is_p(toks, j, "{"):
            j = skip_balanced(toks, j)
        if not is_id(toks, j):
            raise Unclassified("event field without a name")
        j += 1
        if not is_p(toks, j, "("):
            raise Unclassified("event field without parameters")
        j = skip_balanced(toks, j)
    else:
        if is_id(toks, j, "inject"):
            j += 1
            if (is_id(toks, j, "global") or is_id(toks, j, "parent")) and is_p(toks, j + 1, "::"):
                j += 2
        j = parse_type(toks, j)
        if not is_id(toks, j):
            raise Unclassified("expected a member name")
        j += 1
        if is_p(toks, j, "<") or is_p(toks, j, "("):
            method = True
    if method:
        if is_p(toks, j, "<"):
            j = skip_angles(toks, j)
        if not is_p(toks, j, "("):
            raise Unclassified("method without parameters")
        j = skip_balanced(toks, j)
        while j < n and not (toks[j].kind == "p" and toks[j].text in ("{", "=>")):  # where clause
            if toks[j].kind == "p" and toks[j].text in (";", "}"):
                raise Unclassified("method without a body")
            j = skip_balanced(toks, j) if toks[j].kind == "p" and toks[j].text in OPEN else j + 1
        if j >= n:
            raise Unclassified("method without a body")
        if toks[j].text == "{":
            j = skip_balanced(toks, j)
        else:
            j += 1
            while j < n and not is_p(toks, j, ";"):
                if toks[j].kind == "p" and toks[j].text in OPEN:
                    j = skip_balanced(toks, j)
                elif is_p(toks, j, "}"):
                    raise Unclassified("expression body not terminated")
                else:
                    j += 1
            if j >= n:
                raise Unclassified("expression body not terminated")
            j += 1
            return j, 0
        if is_p(toks, j, ";"):
            j += 1
        return j, 0
    # field: terminator decides the rewrite
    if is_p(toks, j, ","):
        edits.append((toks[j].start, 1, ";"))
        return j + 1, 1
    if is_p(toks, j, ";"):
        return j + 1, 0
    edits.append((toks[j - 1].end, 0, ";"))  # last field, or a field followed by a method
    return j, 1


def rewrite_body(toks, i, edits):
    """toks[i] is the body `{`. Append (pos, old_len, new_text) edits.
    Returns (index after the closing `}`, fields rewritten)."""
    i += 1
    count = 0
    while True:
        if i >= len(toks):
            raise Unclassified("type body not closed")
        if is_p(toks, i, "}"):
            return i + 1, count
        i, c = parse_member(toks, i, edits)
        count += c


def rewrite_source(src, report=None, label=""):
    """Return (new_source, fields_rewritten)."""
    toks = tokenize(src)
    edits = []
    total = 0
    i = 0
    while i < len(toks):
        t = toks[i]
        if t.kind == "id" and t.text == "type" and not (
            i > 0 and (toks[i - 1].text in ("extend", ".", "::") or toks[i - 1].text == "(")
        ):
            b = type_header_end(toks, i)
            if b is not None:
                local = []
                try:
                    i2, c = rewrite_body(toks, b, local)
                    edits.extend(local)
                    total += c
                    i = i2
                    continue
                except Unclassified as e:
                    if report is not None:
                        line = src.count("\n", 0, t.start) + 1
                        report.append("%s:%d: type `%s`: %s" % (label, line, toks[i + 1].text, e))
                    i = b + 1
                    continue
        i += 1
    out, last = [], 0
    for pos, ln, new in sorted(edits, key=lambda e: e[0]):
        out.append(src[last:pos])
        out.append(new)
        last = pos + ln
    out.append(src[last:])
    return "".join(out), total


# --------------------------------------------------------------------------
# Markdown

FENCE = re.compile(r"^( {0,3})(`{3,}|~{3,})\s*([^\n]*)$")


def is_beskid_info(info):
    word = re.split(r"[\s,{]", info.strip(), maxsplit=1)[0].lower() if info.strip() else ""
    return word in ("beskid", "bd")


def rewrite_markdown(src, report=None, label=""):
    lines = src.split("\n")
    out, total = [], 0
    i = 0
    while i < len(lines):
        m = FENCE.match(lines[i].rstrip("\r"))
        if not m:
            out.append(lines[i])
            i += 1
            continue
        ch, ln, info = m.group(2)[0], len(m.group(2)), m.group(3)
        out.append(lines[i])
        i += 1
        body = []
        while i < len(lines):
            c = re.match(r"^ {0,3}(" + re.escape(ch) + r"{%d,})\s*$" % ln, lines[i].rstrip("\r"))
            if c:
                break
            body.append(lines[i])
            i += 1
        if is_beskid_info(info):
            new, c = rewrite_source("\n".join(body), report, "%s:%d" % (label, i - len(body) + 1))
            body = new.split("\n")
            total += c
        out.extend(body)
        if i < len(lines):
            out.append(lines[i])
            i += 1
    return "\n".join(out), total


# --------------------------------------------------------------------------
# Driver


def iter_files(paths):
    for p in paths:
        if os.path.isfile(p):
            yield p
            continue
        for root, dirs, files in os.walk(p):
            dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
            for f in sorted(files):
                yield os.path.join(root, f)


def process(path, report):
    ext = os.path.splitext(path)[1].lower()
    if ext == ".bd":
        fn = rewrite_source
    elif ext in (".md", ".mdx"):
        fn = rewrite_markdown
    else:
        return None
    with open(path, encoding="utf-8", newline="") as fh:
        old = fh.read()
    new, count = fn(old, report, path)
    return old, new, count


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--check", action="store_true", help="exit 1 if anything would change; write nothing")
    ap.add_argument("--dry-run", action="store_true", help="print a unified diff; write nothing")
    a = ap.parse_args(argv)
    scanned = changed = fields = 0
    report = []
    for path in iter_files(a.paths):
        try:
            res = process(path, report)
        except (OSError, UnicodeDecodeError) as e:
            report.append("%s: %s" % (path, e))
            continue
        if res is None:
            continue
        old, new, count = res
        scanned += 1
        if old == new:
            continue
        changed += 1
        fields += count
        if a.dry_run:
            sys.stdout.writelines(difflib.unified_diff(
                old.splitlines(True), new.splitlines(True), "a/" + path, "b/" + path))
        elif not a.check:
            with open(path, "w", encoding="utf-8", newline="") as fh:
                fh.write(new)
    print("files scanned: %d, files changed: %d, fields rewritten: %d" % (scanned, changed, fields),
          file=sys.stderr if a.dry_run else sys.stdout)
    for r in report:
        print("unclassified: " + r, file=sys.stderr)
    return 1 if (a.check and changed) else 0


if __name__ == "__main__":
    sys.exit(main())

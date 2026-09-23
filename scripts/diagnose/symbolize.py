#!/usr/bin/env python3
"""symbolize.py -- expand `path#gN:nM[ Construct@L1:C1-L2:C2]` compiler diagnostic keys into
readable form: real repo-relative file, the actual source text at the span, and a short legend
for what gN/nN mean. This is a standing user preference for compiler/ISLE traces.

Usage:
    symbolize.py < some.log
    ssh ... 'podman exec <container> tail -200 /path/to.log' | symbolize.py
    symbolize.py --repo /path/to/worktree < some.log     # override repo root for path resolution
    echo 'MissingRuleOrFact at .../Codec.bd#g1:n1538 AssignExpression@22:117-22:154' | symbolize.py

What it does:
  1. Reads the whole input, undoes this CLI's line-wrap (continuation lines start with one or
     more `|` then whitespace; they are concatenated directly onto the previous line with no
     separator, since the wrap point is arbitrary mid-token -- this is how a materialized path
     like `.../corelib_http-\n  | d374f49909110c78/src/Http/Codec.bd` gets torn in these logs).
  2. Finds every `<path>#g<N>:n<M>` occurrence, optionally followed by ` <Construct>@L:C-L:C`.
  3. Resolves <path> to a real repo file:
       - used as-is if it already exists,
       - or, if it is a Salsa-materialized dependency copy under
         `.../obj/beskid/**/deps/src/<pkgname>-<hash>/<relpath>`, strips that staging prefix and
         looks for `<repo>/corelib/packages/<guess>/<relpath>` (guess = pkgname with a leading
         `corelib_` stripped), falling back to a repo-wide search for `<relpath>`'s basename,
       - or is reported unresolved (e.g. an ephemeral `/tmp/.tmpXXXXXX/...` test fixture that no
         longer exists once the test process exits -- this is expected and reported, not an error).
  4. When resolved and a span is present, prints the exact source substring/lines at that span
     (1-based line:col, half-open at the end column) so you don't have to open the file.
  5. Deduplicates identical keys and prints one annotated block per unique occurrence, plus a
     legend explaining gN/nN.

Limitations (stated up front, not hidden):
  - gN (syntax generation id) and nN (AST node index) are Salsa-internal lookup identifiers with
    no stable meaning across edits/re-parses; this tool does not (cannot, from text alone) turn
    nN into a symbol name -- only the Construct@span already printed in the diagnostic gives you
    that, which is why step 4 resolves it to real source text instead.
  - Path resolution is heuristic for materialized/staged copies; if the guessed package directory
    is wrong, use --repo to point at the right worktree or check the printed "tried:" paths.

Configuration:
  --repo / $BESKID_DIAG_REPO_ROOT default to the repo this script lives in (two directories above
  scripts/diagnose/), so it works out of the box from any checkout without hardcoding a path.
"""
import argparse
import glob
import os
import re
import sys

KEY_RE = re.compile(
    r"(?:[A-Za-z_][A-Za-z0-9_.<>]*@)?(?P<path>[^\s()\[\]{}\"'|]+?\.(?:bd|rs))#g(?P<gen>\d+):n(?P<node>\d+)"
    r"(?:\s+(?P<construct>[A-Za-z_][A-Za-z0-9_]*)@(?P<l1>\d+):(?P<c1>\d+)-(?P<l2>\d+):(?P<c2>\d+))?"
)


def default_repo_root() -> str:
    env = os.environ.get("BESKID_DIAG_REPO_ROOT")
    if env:
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    # scripts/diagnose/symbolize.py -> repo root is two levels up.
    return os.path.abspath(os.path.join(here, "..", ".."))


def unwrap(text: str) -> str:
    out_lines = []
    for line in text.splitlines():
        m = re.match(r"^(?:\s*\|)+\s*(.*)$", line)
        if m and out_lines:
            out_lines[-1] += m.group(1)
        else:
            out_lines.append(line)
    return "\n".join(out_lines)


def resolve_path(raw_path: str, repo_root: str):
    """Return (resolved_abs_path_or_None, tried_list)."""
    tried = []
    if os.path.isfile(raw_path):
        return raw_path, tried

    # Salsa-materialized dependency staging copy:
    #   .../obj/beskid/<...>/deps/src/<pkgname>-<hash>/<relpath>
    m = re.search(r"/deps/src/([A-Za-z0-9_]+)-[0-9a-f]{6,}/(.+)$", raw_path)
    if m:
        pkgname, relpath = m.group(1), m.group(2)
        guess = pkgname[len("corelib_"):] if pkgname.startswith("corelib_") else pkgname
        candidate = os.path.join(repo_root, "corelib", "packages", guess, relpath)
        tried.append(candidate)
        if os.path.isfile(candidate):
            return candidate, tried
        # fall back: search corelib/packages/*/<relpath>
        for hit in glob.glob(os.path.join(repo_root, "corelib", "packages", "*", relpath)):
            tried.append(hit)
            if os.path.isfile(hit):
                return hit, tried
        # last resort: search repo-wide for the basename under corelib/
        base = os.path.basename(relpath)
        for hit in glob.glob(os.path.join(repo_root, "corelib", "**", base), recursive=True):
            tried.append(hit)
            if os.path.isfile(hit) and hit.endswith(relpath.replace("\\", "/")):
                return hit, tried

    if "/tmp/" in raw_path or "/.tmp" in raw_path:
        tried.append("(ephemeral test-fixture tempdir; not resolvable after the process exits)")
        return None, tried

    # generic fallback: try matching the tail of the path against files in the repo
    tail_parts = raw_path.lstrip("/").split("/")
    for depth in range(min(4, len(tail_parts)), 0, -1):
        tail = "/".join(tail_parts[-depth:])
        candidate = os.path.join(repo_root, tail)
        tried.append(candidate)
        if os.path.isfile(candidate):
            return candidate, tried

    return None, tried


def extract_span(path: str, l1: int, c1: int, l2: int, c2: int):
    """Best-effort. Columns in the diagnostic are 1-based against the source unit's syntax tree
    at the generation it was reported for; the checked-out file on disk may have drifted since
    (reformatting, a later edit) so the exact slice can be off by a few characters. We print the
    slice at the reported columns AND the full line(s), so a mismatch is visible rather than
    silently wrong."""
    try:
        lines = open(path, encoding="utf-8", errors="replace").readlines()
    except OSError as e:
        return f"(could not read {path}: {e})"
    if l1 == l2:
        if 1 <= l1 <= len(lines):
            line = lines[l1 - 1]
            slice_ = line[c1 - 1:c2 - 1] if c1 >= 1 and c2 >= c1 else line.rstrip("\n")
            full = line.strip()
            return f'"{slice_.strip()}"  (full line {l1}: {full})'
        return "(line out of range)"
    out = []
    for ln in range(l1, min(l2, len(lines)) + 1):
        if 1 <= ln <= len(lines):
            out.append(f"    {ln}: {lines[ln - 1].rstrip()}")
    return "\n" + "\n".join(out) if out else "(lines out of range)"


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=default_repo_root(),
                     help="repo root to resolve real source paths against "
                          "(default: $BESKID_DIAG_REPO_ROOT, else this script's own repo)")
    args = ap.parse_args()

    raw = sys.stdin.read()
    text = unwrap(raw)

    seen = {}
    order = []
    for m in KEY_RE.finditer(text):
        dedup_key = (m.group("gen"), m.group("node"), m.group("construct"),
                     m.group("l1"), m.group("c1"), m.group("l2"), m.group("c2"))
        if dedup_key not in seen:
            full_key = (m.group("path"),) + dedup_key
            seen[dedup_key] = (full_key, m)
            order.append(dedup_key)

    if not order:
        print("No `path#gN:nM` diagnostic keys found in input.")
        return

    for dedup_key in order:
        full_key, m = seen[dedup_key]
        raw_path, gen, node, construct, l1, c1, l2, c2 = full_key
        resolved, tried = resolve_path(raw_path, args.repo)
        print(f"=== #{gen}:n{node} ===")
        if construct:
            print(f"  construct : {construct}")
            print(f"  span      : {l1}:{c1}-{l2}:{c2}")
        else:
            print("  construct : (not printed in this diagnostic -- only the key is available)")
        if resolved:
            rel = os.path.relpath(resolved, args.repo)
            print(f"  file      : {rel}")
            if construct:
                snippet = extract_span(resolved, int(l1), int(c1), int(l2), int(c2))
                print(f"  source    : {snippet}")
        else:
            print(f"  file      : UNRESOLVED ({raw_path})")
            for t in tried[:5]:
                print(f"    tried   : {t}")
        print()

    print("Legend: gN = Salsa syntax-generation id for this source unit (bumps on re-parse);")
    print("        nN = AST node index within that generation. Both are lookup-only identifiers,")
    print("        not stable across edits -- the Construct@span (when printed) is the durable part,")
    print("        which is what this tool resolves back to real source text above.")


if __name__ == "__main__":
    main()

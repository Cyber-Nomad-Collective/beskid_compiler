#!/usr/bin/env python3
"""whyfact.py -- given a MissingRuleOrFact / "semantic query `X` is unavailable" error, print the
chain: which Rust function raised it (source site), which higher-level function calls that
function (one hop, i.e. the likely ISLE-adapter/query caller), and -- when the diagnostic carries
a `path#gN:nM Construct@span` key -- the symbolized source location (reusing symbolize.py).

Usage:
    whyfact.py <query_name>                       # e.g. whyfact.py source_expression_type
    whyfact.py < some.log                          # auto-extracts the query name(s) from stdin
    whyfact.py --repo /path/to/worktree <query>
    whyfact.py --trace /path/to/trace.log <query>  # also grep a BESKID_COMPILER_TRACE=1 /
                                                     # BESKID_DIAG_CALL=1 trace file for the query

How the mapping is derived (read-only, grep-based, re-derived every run so it can't go stale):
  1. `SemanticError::unavailable("<query>")` call sites in crates/beskid_queries/src/semantic_contract/**
     are the query's failure points. For each, the nearest enclosing `fn` above it is the Rust
     function whose contract failed.
  2. Each such function's callers, found by grepping the wider crates tree for `<fn>(`, are one
     hop up the chain -- typically the query/contract function that a codegen or ISLE adapter
     entry point calls. This is a single hop by design (fast, exact-text, no false call-graph
     precision); if the immediate caller is itself another internal helper, re-run whyfact.py
     with that name to walk further.
  3. If a `path#gN:nM[ Construct@span]` key is present in the input, it is resolved to real
     source text the same way symbolize.py does.

This does not require and does not run a build; it is pure source-tree grep against
crates/beskid_isle/src/context/**, crates/beskid_codegen/src/isle_adapter/**, and
crates/beskid_queries/src/semantic_contract/**.

Configuration:
  --repo / $BESKID_DIAG_REPO_ROOT default to the repo this script lives in (two directories above
  scripts/diagnose/), same as symbolize.py.
"""
import argparse
import os
import re
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import symbolize  # noqa: E402  (local helper, same dir)

QUERY_IN_MSG_RE = re.compile(r"semantic query `([A-Za-z_][A-Za-z0-9_]*)` is unavailable")
UNAVAILABLE_CALL_RE = re.compile(r'SemanticError::unavailable\("([A-Za-z_][A-Za-z0-9_]*)"\)')


def rg(pattern: str, paths, extra=()):
    try:
        out = subprocess.run(
            ["rg", "-n", "--no-heading", *extra, pattern, *paths],
            capture_output=True, text=True, timeout=30,
        )
        return out.stdout.splitlines()
    except FileNotFoundError:
        out = subprocess.run(
            ["grep", "-rn", pattern, *paths],
            capture_output=True, text=True, timeout=30,
        )
        return out.stdout.splitlines()


def enclosing_fn(path: str, line_no: int):
    """Scan upward from line_no (1-based) in `path` for the nearest `fn <name>` signature."""
    try:
        lines = open(path, encoding="utf-8", errors="replace").readlines()
    except OSError:
        return None
    fn_re = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")
    for i in range(min(line_no, len(lines)) - 1, -1, -1):
        m = fn_re.search(lines[i])
        if m:
            return m.group(1), i + 1
    return None


def find_query(repo: str, query: str):
    contract_dir = os.path.join(repo, "crates", "beskid_queries", "src", "semantic_contract")
    sites = rg(f'SemanticError::unavailable\\("{query}"\\)', [contract_dir])
    if not sites:
        sites = rg(f'SemanticError::unavailable\\("{query}"\\)', [os.path.join(repo, "crates", "beskid_queries")])
    fns = {}
    for line in sites:
        parts = line.split(":", 2)
        if len(parts) < 2:
            continue
        path, line_no = parts[0], parts[1]
        try:
            line_no = int(line_no)
        except ValueError:
            continue
        found = enclosing_fn(path, line_no)
        if found:
            fn_name, fn_line = found
            fns.setdefault(fn_name, {"def": (path, fn_line), "fail_sites": []})
            fns[fn_name]["fail_sites"].append((path, line_no))
    return sites, fns


def find_callers(repo: str, fn_name: str, exclude_file: str):
    search_dirs = [
        os.path.join(repo, "crates", "beskid_isle", "src"),
        os.path.join(repo, "crates", "beskid_codegen", "src", "isle_adapter"),
        os.path.join(repo, "crates", "beskid_queries", "src"),
    ]
    search_dirs = [d for d in search_dirs if os.path.isdir(d)]
    lines = rg(f"{re.escape(fn_name)}\\(", search_dirs)
    callers = []
    for line in lines:
        parts = line.split(":", 2)
        if len(parts) < 3:
            continue
        path, line_no, content = parts
        if os.path.abspath(path) == os.path.abspath(exclude_file):
            continue
        if f"fn {fn_name}(" in content or f"fn {fn_name} (" in content:
            continue  # the definition itself
        callers.append((path, line_no, content.strip()))
    return callers


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("query", nargs="?", help="query/fact name, e.g. source_expression_type")
    ap.add_argument("--repo", default=symbolize.default_repo_root())
    ap.add_argument("--trace", help="optional BESKID_COMPILER_TRACE=1 / BESKID_DIAG_CALL=1 trace file to grep")
    ap.add_argument("--max-callers", type=int, default=8)
    args = ap.parse_args()

    stdin_text = "" if sys.stdin.isatty() else sys.stdin.read()
    queries = []
    if args.query:
        queries.append(args.query)
    if stdin_text:
        queries.extend(m.group(1) for m in QUERY_IN_MSG_RE.finditer(stdin_text))
        # also symbolize any path#gN:nM keys present, for free context
        keys_text = symbolize.unwrap(stdin_text)
        node_matches = list(symbolize.KEY_RE.finditer(keys_text))
    else:
        node_matches = []

    queries = list(dict.fromkeys(queries))  # de-dup, preserve order
    if not queries:
        print("No query name given and none found in stdin (looked for `semantic query `X` is unavailable`).")
        sys.exit(1)

    for query in queries:
        print(f"########## {query} ##########")
        sites, fns = find_query(args.repo, query)
        if not fns:
            print(f"  No `SemanticError::unavailable(\"{query}\")` call site found under "
                  f"crates/beskid_queries/src/semantic_contract. It may be raised elsewhere, or the "
                  f"query name in the message doesn't match a literal string in source (check spelling).")
            continue

        for fn_name, info in fns.items():
            def_path, def_line = info["def"]
            rel_def = os.path.relpath(def_path, args.repo)
            print(f"\n  fn {fn_name}()  --  {rel_def}:{def_line}")
            print(f"    fails (returns SemanticError::unavailable) at:")
            for path, line_no in info["fail_sites"][:6]:
                rel = os.path.relpath(path, args.repo)
                try:
                    src_line = open(path, encoding="utf-8", errors="replace").readlines()[line_no - 1].strip()
                except (OSError, IndexError):
                    src_line = "(could not read line)"
                print(f"      {rel}:{line_no}  {src_line}")
            if len(info["fail_sites"]) > 6:
                print(f"      ... and {len(info['fail_sites']) - 6} more fail site(s) in this function")

            callers = find_callers(args.repo, fn_name, def_path)
            print(f"    called from ({len(callers)} site(s), one hop up; re-run whyfact.py on a caller fn to go further):")
            if not callers:
                print("      (no callers found under beskid_isle/beskid_codegen/beskid_queries -- "
                      "it may be called via a generated ISLE constructor, or from a crate not searched)")
            for path, line_no, content in callers[:args.max_callers]:
                rel = os.path.relpath(path, args.repo)
                print(f"      {rel}:{line_no}  {content}")
            if len(callers) > args.max_callers:
                print(f"      ... and {len(callers) - args.max_callers} more")

        if args.trace:
            print(f"\n  Trace file grep ({args.trace}):")
            try:
                trace_lines = [l for l in open(args.trace, encoding="utf-8", errors="replace") if query in l]
            except OSError as e:
                trace_lines = None
                print(f"    could not read trace file: {e}")
            if trace_lines is not None:
                if trace_lines:
                    for l in trace_lines[:20]:
                        print(f"    {l.rstrip()}")
                else:
                    print(f"    no lines mentioning `{query}` in the trace file")
        print()

    if node_matches:
        print("---- symbolized node(s) from the same input (see symbolize.py) ----")
        seen = set()
        for m in node_matches:
            dedup_key = (m.group("gen"), m.group("node"), m.group("construct"), m.group("l1"), m.group("c1"))
            if dedup_key in seen:
                continue
            seen.add(dedup_key)
            raw_path = m.group("path")
            resolved, tried = symbolize.resolve_path(raw_path, args.repo)
            label = f"#{m.group('gen')}:n{m.group('node')}"
            if m.group("construct"):
                label += f" {m.group('construct')}@{m.group('l1')}:{m.group('c1')}-{m.group('l2')}:{m.group('c2')}"
            print(f"  {label}")
            if resolved:
                rel = os.path.relpath(resolved, args.repo)
                print(f"    file: {rel}")
                if m.group("construct"):
                    snippet = symbolize.extract_span(
                        resolved, int(m.group("l1")), int(m.group("c1")), int(m.group("l2")), int(m.group("c2")))
                    print(f"    source: {snippet}")
            else:
                print(f"    file: UNRESOLVED ({raw_path})")


if __name__ == "__main__":
    main()

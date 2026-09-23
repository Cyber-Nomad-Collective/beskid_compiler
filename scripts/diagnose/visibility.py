#!/usr/bin/env python3
"""visibility.py <symbol> [--project <bproj dir>] [--repo <worktree>]

Explains an "unknown value `X`" diagnostic when X clearly exists in the .bd sources: this is
drift diagnosis (module path / pub / import mismatch), not a fix.

Prints, for the given symbol:
  1. Every declaration site under runtime/beskid/src and corelib/packages/*/src: file, line,
     the inferred Beskid module path (directory path relative to its package's src/, dot-joined
     to match the project's own `use A.B.C;` convention -- verified against the sibling facade
     file's `pub mod A.B.C;` lines when one exists), and whether the declaration itself is `pub`.
  2. If --project is given: every .bd file under that project's src/ that references the symbol,
     and that file's own `use ...;` lines -- so you can see directly whether the declaring
     module is imported by name.
  3. A plain verdict:
       - not `pub` anywhere it's declared -> not exported (needs `pub` at the declaration)
       - `pub`, and no consuming file has `use <declaring module>;` verbatim -> likely import
         drift (the file imports a facade module that re-exports the declaring module via
         `pub mod`, but does not `use` the declaring module directly)
       - `pub`, and some consuming file DOES `use <declaring module>;` directly and the symbol
         is still reported unknown -> this tool cannot explain it from source text alone; that
         is worth escalating as a possible resolver bug, not source drift.

This tool does not know Beskid's exact `pub mod` re-export resolution rule (whether `use Parent;`
transitively pulls in a child module's `pub` symbols) -- it states the facts and flags which
case applies rather than asserting language semantics it hasn't verified against the resolver.

Configuration:
  --repo / $BESKID_DIAG_REPO_ROOT default to the repo this script lives in (two directories above
  scripts/diagnose/).
"""
import argparse
import glob
import os
import re
import sys

DECL_RE_TEMPLATE = r"^\s*(pub\s+)?(?:const\s+{sym}\s*=|[A-Za-z_][\w<>\[\],]*(?:\[\])?\s+{sym}\s*\()"
STATEMENT_KEYWORDS = {
    "if", "while", "for", "return", "else", "match", "mut", "let", "break", "continue",
    "Assert", "raw_word_store", "raw_word_load",
}


def default_repo_root() -> str:
    env = os.environ.get("BESKID_DIAG_REPO_ROOT")
    if env:
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.abspath(os.path.join(here, "..", ".."))


def module_path_for(repo: str, file_path: str):
    """Infer the Beskid module path for a .bd file from its location under a package's src/."""
    rel = os.path.relpath(file_path, repo)
    parts = rel.split(os.sep)
    if "src" in parts:
        idx = parts.index("src")
        tail = parts[idx + 1:]
    else:
        tail = parts
    if tail and tail[-1].endswith(".bd"):
        tail[-1] = tail[-1][: -len(".bd")]
    return ".".join(tail)


def find_declarations(repo: str, symbol: str):
    pattern = re.compile(DECL_RE_TEMPLATE.format(sym=re.escape(symbol)))
    roots = [
        os.path.join(repo, "runtime", "beskid", "src"),
        *glob.glob(os.path.join(repo, "corelib", "packages", "*", "src")),
    ]
    decls = []
    for root in roots:
        if not os.path.isdir(root):
            continue
        for dirpath, _, files in os.walk(root):
            for fname in files:
                if not fname.endswith(".bd"):
                    continue
                fpath = os.path.join(dirpath, fname)
                try:
                    lines = open(fpath, encoding="utf-8", errors="replace").readlines()
                except OSError:
                    continue
                for i, line in enumerate(lines, start=1):
                    m = pattern.match(line)
                    if m:
                        leading = line.strip().split()[0] if line.strip() else ""
                        if leading == "pub":
                            leading = line.strip().split()[1] if len(line.strip().split()) > 1 else ""
                        if leading in STATEMENT_KEYWORDS:
                            continue
                        is_pub = bool(m.group(1)) or line.strip().startswith("pub ")
                        decls.append({
                            "file": fpath,
                            "line": i,
                            "pub": is_pub,
                            "module": module_path_for(repo, fpath),
                            "text": line.strip(),
                        })
    return decls


def find_facade_reexport(repo: str, module_path: str):
    """If `module_path` (e.g. Runtime.Fiber.Scheduler.Core) is re-exported by a sibling facade
    file via `pub mod <module_path>;`, return (facade_file, facade_module_path)."""
    parent = ".".join(module_path.split(".")[:-1])
    if not parent:
        return None
    # facade file is typically <parent-as-path>.bd next to the directory
    for root in [os.path.join(repo, "runtime", "beskid", "src"), *glob.glob(os.path.join(repo, "corelib", "packages", "*", "src"))]:
        candidate = os.path.join(root, *parent.split("."))
        candidate += ".bd"
        if os.path.isfile(candidate):
            text = open(candidate, encoding="utf-8", errors="replace").read()
            if re.search(rf"pub\s+mod\s+{re.escape(module_path)}\s*;", text):
                return candidate, parent
    return None


def find_usages_in_project(repo: str, project_dir: str, symbol: str):
    src = os.path.join(project_dir, "src")
    if not os.path.isdir(src):
        src = project_dir
    results = []
    for dirpath, _, files in os.walk(src):
        for fname in files:
            if not fname.endswith(".bd"):
                continue
            fpath = os.path.join(dirpath, fname)
            try:
                lines = open(fpath, encoding="utf-8", errors="replace").readlines()
            except OSError:
                continue
            uses = [l.strip() for l in lines if re.match(r"\s*use\s+[\w.]+\s*;", l)]
            hits = [(i + 1, l.strip()) for i, l in enumerate(lines) if re.search(rf"\b{re.escape(symbol)}\b", l) and not l.strip().startswith("use ")]
            if hits:
                results.append({"file": fpath, "uses": uses, "hits": hits})
    return results


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("symbol")
    ap.add_argument("--project", help="path to the .bproj directory (or its src/) reporting the unknown value")
    ap.add_argument("--repo", default=default_repo_root())
    args = ap.parse_args()

    decls = find_declarations(args.repo, args.symbol)
    if not decls:
        print(f"No declaration of `{args.symbol}` found under runtime/beskid/src or corelib/packages/*/src.")
        print("Either the symbol is misspelled, or it lives outside those two trees (check corelib packages"
              " under a different root, or the runtime/beskid/tests fixtures themselves).")
        sys.exit(1)

    print(f"Declarations of `{args.symbol}`:")
    for d in decls:
        rel = os.path.relpath(d["file"], args.repo)
        pub_str = "pub" if d["pub"] else "NOT pub"
        print(f"  {rel}:{d['line']}  [{pub_str}]  module={d['module']}")
        print(f"    {d['text']}")

    any_pub = any(d["pub"] for d in decls)
    pub_modules = sorted({d["module"] for d in decls if d["pub"]})

    print()
    if not any_pub:
        print(f"VERDICT: `{args.symbol}` is declared but never `pub` -- it is private to its declaring"
              f" module ({', '.join(sorted({d['module'] for d in decls}))}) and cannot be visible to any"
              f" other file regardless of imports. Fix: add `pub` at the declaration, if the intent is to"
              f" expose it.")
    else:
        for mp in pub_modules:
            reexport = find_facade_reexport(args.repo, mp)
            if reexport:
                facade_file, facade_mod = reexport
                rel = os.path.relpath(facade_file, args.repo)
                print(f"NOTE: `{mp}` is re-exported by facade `{facade_mod}` via `pub mod {mp};` in {rel}.")

    if args.project:
        print()
        usages = find_usages_in_project(args.repo, args.project, args.symbol)
        if not usages:
            print(f"No reference to `{args.symbol}` found under {args.project}/src -- check --project points at"
                  f" the right test project.")
        for u in usages:
            rel = os.path.relpath(u["file"], args.repo)
            print(f"\nUsed in {rel}:")
            for line_no, text in u["hits"][:3]:
                print(f"  {line_no}: {text}")
            print(f"  imports ({len(u['uses'])}):")
            for use_line in u["uses"]:
                print(f"    {use_line}")
            if any_pub:
                imported_exact = any(re.search(rf"\buse\s+{re.escape(mp)}\s*;", " ".join(u["uses"])) for mp in pub_modules)
                if imported_exact:
                    print(f"  VERDICT for this file: it DOES `use` the declaring module directly. If the symbol"
                          f"  is still reported unknown, that's unexplained by source text alone -- worth"
                          f"  escalating as a possible resolver issue rather than source drift.")
                else:
                    print(f"  VERDICT for this file: it does NOT `use` any of [{', '.join(pub_modules)}] directly"
                          f" -- likely import drift. It imports a facade instead (see 'imports' above); whether"
                          f" that facade's `pub mod` re-export flattens the symbol into scope is a resolver"
                          f" semantics question this tool does not assert either way. Try adding"
                          f" `use {pub_modules[0]};` directly to this file as the first thing to test.")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Internal-error sweep (0.6.7 ticket 15, part 2).

Runs `beskid check`, `beskid build` and `beskid test` over six corpora, then a deterministic
mutation sweep over the same corpora, and fails when any run shows an internal compiler error:
any code of the internal diagnostic family E21xx, E4041 (pass further codes with
--ice-pattern), the raw internal texts, a Rust panic, a signal death or a timeout. A user error (nonzero exit with an ordinary diagnostic) is NOT a failure.

Corpora (each is copied to a work directory first; the originals are never touched):
  corelib    <compiler>/corelib, without mods
  mods       <compiler>/corelib/mods
  fixtures   <compiler>/crates/*/fixtures (Beskid fixtures; many are deliberately broken)
  templates  every project/workspace template from `beskid new --list`, instantiated with `beskid new`
  lings      the beskid-lings exercises and solutions (--lings)
Mutation: `beskid-mutate plan` picks (file, kind) pairs from every corpus (delete a token,
duplicate a line, swap two tokens; deterministic per --seed); each mutant is written into the
work copy, `beskid check` runs on the owning project and, when check accepts it, `beskid build`.

usage: ice_sweep.py --beskid PATH --mutate PATH [--compiler DIR] [--lings DIR] [options]
  --beskid PATH          installed beskid CLI (required)
  --mutate PATH          beskid-mutate binary (cargo build -p beskid_proof) (required)
  --compiler DIR         compiler checkout (default: this script's checkout)
  --lings DIR            beskid-lings checkout (default: none, corpus skipped)
  --corpora LIST         comma list of corelib,mods,fixtures,templates,lings (default all available)
  --commands LIST        clean-run commands (default check,build,test)
  --seed N               mutation seed (default 1)
  --mutation-sample N    mutants per corpus (default 60; 0 disables the mutation sweep)
  --timeout SECONDS      per command (default 900)
  --work-dir DIR         default: fresh temporary directory (kept on failure)
  --ice-pattern REGEX    extra internal-error signature, repeatable
  --out FILE             JSON report path
Exit status: 0 no internal error, 1 internal errors found, 2 usage/setup error.
"""
from __future__ import annotations

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _proof_common as common  # noqa: E402

CLEAN_COMMANDS = ("check", "build", "test")


class Corpus:
    def __init__(self, name):
        self.name = name
        self.roots = []  # work-copy roots, in plan-index order
        self.projects = []
        self.stats = {c: {"runs": 0, "nonzero": 0, "ice": 0, "timeouts": 0} for c in CLEAN_COMMANDS}
        self.mutation = {"planned": 0, "run": 0, "check_accepted": 0, "ice": 0, "timeouts": 0, "skipped": 0}
        self.first = {}  # signature -> example
        self.new_ice = 0  # `beskid new` runs that showed an internal error or timed out


def record(corpus, signature, example):
    corpus.first.setdefault(signature, example)


def build_corpora(args, work, patterns=()):
    """Returns (corpora, missing): `missing` names explicitly requested corpora that were not found."""
    wanted = set(args.corpora.split(",")) if args.corpora else None
    compiler = Path(args.compiler)
    corpora = []
    missing = []

    def add(name, sources):
        if wanted is not None and name not in wanted:
            return
        sources = [s for s in sources if s.is_dir()]
        if not sources:
            print(f"[sweep] corpus {name}: nothing found, skipped", file=sys.stderr)
            if wanted is not None:
                missing.append(name)
            return
        corpus = Corpus(name)
        for index, source in enumerate(sources):
            target = work / name / str(index)
            common.copy_tree(source, target)
            if name == "corelib":
                shutil.rmtree(target / "mods", ignore_errors=True)
            corpus.roots.append(target)
        corpora.append(corpus)

    add("corelib", [compiler / "corelib"])
    add("mods", [compiler / "corelib" / "mods"])
    fixtures = sorted(p for p in (compiler / "crates").glob("*/fixtures") if p.is_dir())
    add("fixtures", fixtures)
    if args.lings or (wanted is not None and "lings" in wanted):
        add("lings", [Path(args.lings or "/nonexistent-lings") / "exercises", Path(args.lings or "/nonexistent-lings") / "solutions", Path(args.lings or "/nonexistent-lings") / "Runner"])
    if wanted is None or "templates" in wanted:
        corpus = Corpus("templates")
        templates = generate_templates(args, work, corpus, patterns)
        if templates or corpus.new_ice:
            corpus.roots = templates
            corpora.append(corpus)
        elif wanted is not None:
            missing.append("templates")
    return corpora, missing


def generate_templates(args, work, corpus, patterns):
    names, listing = common.discover_templates(args.beskid)
    listing_signatures = common.classify(listing.output, listing.returncode, listing.timed_out, patterns)
    if listing_signatures:
        corpus.new_ice += 1
        for signature in listing_signatures:
            record(corpus, signature, {
                "phase": "new", "command": "new --list", "project": "templates",
                "returncode": listing.returncode, "excerpt": common.excerpt(listing.output, patterns),
            })
    if not names:
        print("[sweep] corpus templates: `beskid new --list` gave no templates:\n" + listing.output[:400], file=sys.stderr)
        return []
    roots = []
    base = work / "templates"
    base.mkdir(parents=True, exist_ok=True)
    for short, _kind in names:
        out = base / short
        result = common.run(
            [args.beskid, "new", f"proof_{short.replace('-', '_')}", "--template", short, "--offline", "--no-interactive", "-o", out],
            cwd=base,
            timeout=args.timeout,
        )
        signatures = common.classify(result.output, result.returncode, result.timed_out, patterns)
        if signatures:
            corpus.new_ice += 1
            for signature in signatures:
                record(corpus, signature, {
                    "phase": "new", "command": f"new --template {short}", "project": str(out),
                    "returncode": result.returncode, "excerpt": common.excerpt(result.output, patterns),
                })
        if result.returncode == 0 and out.is_dir():
            roots.append(out)
        else:
            print(f"[sweep] template {short}: `beskid new` failed (rc={result.returncode}); skipped", file=sys.stderr)
    return roots


def run_clean(args, corpus, patterns):
    for root in corpus.roots:
        corpus.projects.extend(common.discover_projects(root))
    for project in corpus.projects:
        for command in args.commands:
            result = common.run([args.beskid, command, "--project", project], cwd=project, timeout=args.timeout)
            stats = corpus.stats[command]
            stats["runs"] += 1
            if result.timed_out:
                stats["timeouts"] += 1
            elif result.returncode != 0:
                stats["nonzero"] += 1
            signatures = common.classify(result.output, result.returncode, result.timed_out, patterns)
            if signatures:
                stats["ice"] += 1
                for signature in signatures:
                    record(corpus, signature, {
                        "phase": "clean", "command": command, "project": str(project),
                        "returncode": result.returncode, "excerpt": common.excerpt(result.output, patterns),
                    })


def run_mutation(args, corpus, patterns):
    if args.mutation_sample <= 0:
        return
    plan = common.run(
        [args.mutate, "plan", "--seed", str(args.seed), "--sample", str(args.mutation_sample), *corpus.roots],
        timeout=300,
    )
    if plan.returncode != 0:
        print(f"[sweep] {corpus.name}: beskid-mutate plan failed:\n{plan.output[:300]}", file=sys.stderr)
        corpus.mutation["skipped"] += 1
        return
    for line in plan.output.splitlines():
        if "\t" not in line:
            continue
        label, kind = line.split("\t", 1)
        index_text, relative = label.split(":", 1)
        root = corpus.roots[int(index_text)]
        target = root / relative
        corpus.mutation["planned"] += 1
        project = common.owning_project(target, root)
        if project is None or not target.is_file():
            corpus.mutation["skipped"] += 1
            continue
        mutant = common.run([args.mutate, "apply", "--seed", str(args.seed), "--kind", kind, "--label", label, target], timeout=60)
        if mutant.returncode != 0:
            corpus.mutation["skipped"] += 1
            continue
        original = target.read_bytes()
        try:
            target.write_text(mutant.output, encoding="utf-8")
            corpus.mutation["run"] += 1
            for command in ("check", "build"):
                result = common.run([args.beskid, command, "--project", project], cwd=project, timeout=args.timeout)
                signatures = common.classify(result.output, result.returncode, result.timed_out, patterns)
                if result.timed_out:
                    corpus.mutation["timeouts"] += 1
                if signatures:
                    corpus.mutation["ice"] += 1
                    for signature in signatures:
                        record(corpus, signature, {
                            "phase": "mutation", "command": command, "mutation": kind,
                            "file": f"{corpus.name}/{label}", "seed": args.seed,
                            "returncode": result.returncode, "excerpt": common.excerpt(result.output, patterns),
                        })
                if command == "check":
                    if result.returncode != 0:
                        break
                    corpus.mutation["check_accepted"] += 1
        finally:
            target.write_bytes(original)


def summarize(corpora):
    lines = []
    failures = 0
    for corpus in corpora:
        clean_ice = sum(s["ice"] + s["timeouts"] for s in corpus.stats.values())
        mutation_ice = corpus.mutation["ice"] + corpus.mutation["timeouts"]
        failures += clean_ice + mutation_ice + corpus.new_ice
        runs = ", ".join(
            f"{c}: {s['runs']} runs/{s['nonzero']} user errors/{s['ice']} internal" for c, s in corpus.stats.items() if s["runs"]
        )
        m = corpus.mutation
        lines.append(f"{corpus.name}: {len(corpus.projects)} projects; {runs or 'no clean runs'}")
        if corpus.new_ice:
            lines.append(f"  `beskid new`: {corpus.new_ice} internal-error or timeout runs")
        lines.append(
            f"  mutation: {m['planned']} planned, {m['run']} run, {m['check_accepted']} accepted by check, "
            f"{m['ice']} internal, {m['timeouts']} timeouts, {m['skipped']} skipped"
        )
        for signature, example in corpus.first.items():
            where = example.get("project") or example.get("file")
            lines.append(f"  FIRST {signature}: {example['phase']} {example['command']} {where}")
            lines.append("    " + example["excerpt"].replace("\n", "\n    "))
    lines.append(f"RESULT: {'FAIL' if failures else 'PASS'} ({failures} internal-error or timeout runs)")
    return lines, failures


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--beskid", required=True)
    parser.add_argument("--mutate", required=True)
    parser.add_argument("--compiler", default=str(Path(__file__).resolve().parents[2]))
    parser.add_argument("--lings")
    parser.add_argument("--corpora")
    parser.add_argument("--commands", default=",".join(CLEAN_COMMANDS))
    parser.add_argument("--seed", type=int, default=1)
    parser.add_argument("--mutation-sample", type=int, default=60)
    parser.add_argument("--timeout", type=int, default=900)
    parser.add_argument("--work-dir")
    parser.add_argument("--ice-pattern", action="append", default=[])
    parser.add_argument("--out")
    args = parser.parse_args(argv)
    args.commands = [c for c in args.commands.split(",") if c]
    unknown = [c for c in args.commands if c not in CLEAN_COMMANDS]
    if unknown:
        print(f"unknown command(s): {unknown}", file=sys.stderr)
        return 2

    work = Path(args.work_dir) if args.work_dir else Path(tempfile.mkdtemp(prefix="beskid-ice-sweep-"))
    work.mkdir(parents=True, exist_ok=True)
    patterns = common.ice_patterns(args.ice_pattern)
    corpora, missing = build_corpora(args, work, patterns)
    if missing:
        print(f"requested corpus not available: {', '.join(missing)}", file=sys.stderr)
        return 2
    if not corpora:
        print("no corpus available", file=sys.stderr)
        return 2
    for corpus in corpora:
        print(f"[sweep] {corpus.name}: clean runs", file=sys.stderr)
        run_clean(args, corpus, patterns)
        print(f"[sweep] {corpus.name}: mutation sweep", file=sys.stderr)
        run_mutation(args, corpus, patterns)
    lines, failures = summarize(corpora)
    print("\n".join(lines))
    if args.out:
        Path(args.out).write_text(json.dumps({
            "failures": failures,
            "seed": args.seed,
            "corpora": {c.name: {"projects": len(c.projects), "clean": c.stats, "mutation": c.mutation, "first": c.first} for c in corpora},
        }, indent=2))
    if failures == 0 and not args.work_dir:
        shutil.rmtree(work, ignore_errors=True)
    else:
        print(f"[sweep] work directory kept: {work}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())

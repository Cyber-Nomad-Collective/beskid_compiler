#!/usr/bin/env python3
"""authority.py <raw_call_or_service> [--repo <worktree>]

Explains "canonical X unavailable" / corelib-service authority errors. There are two distinct
authority mechanisms in this compiler that both produce a message shaped like "canonical X ...
unavailable", and this tool checks both:

  A. Raw-builtin corelib-service authority (crates/beskid_abi/src/runtime_source/corelib_services.rs,
     `CORELIB_SERVICES` table): a raw call like `__timer_sleep_until` is authorized ONLY when made
     from one of its listed `source_path` files. A call to the same raw builtin from any other file
     fails closed with an "unavailable" error -- this is what broke `__timer_sleep_until` when it
     was called from a file the table didn't scope it to.

  B. Scheduler-entry reachability (crates/beskid_codegen/src/module_emission.rs
     `SCHEDULER_ENTRY_HELPERS`, checked in module_emission/orchestration.rs): once ANY scheduler
     entry/return trampoline is required in a compiled module, the compiler requires the COMPLETE
     set of named items (SchedulerContext, SchedulerSetCurrentFiber, ContextSwitch,
     SchedulerCurrentFiber, FiberRecord, FiberDone, ...) to be present and reachable in that
     module's compiled item set. "canonical SchedulerSetCurrentFiber item unavailable" means that
     name wasn't found among the items actually included in THIS compilation -- usually because
     the compiled project doesn't transitively reach/declare it, not because of a source-file
     authority table.

For the given name, this tool reports which mechanism applies (or both), prints the exact table
entry or reachability requirement, and lists every real call/reference site in the repo so you can
see directly which file is making the call vs. which file(s) are authorized.

Configuration:
  --repo / $BESKID_DIAG_REPO_ROOT default to the repo this script lives in (two directories above
  scripts/diagnose/).
"""
import argparse
import os
import re
import subprocess
import sys


def default_repo_root() -> str:
    env = os.environ.get("BESKID_DIAG_REPO_ROOT")
    if env:
        return env
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.abspath(os.path.join(here, "..", ".."))


def rg(pattern, paths):
    try:
        out = subprocess.run(["rg", "-n", "--no-heading", pattern, *paths], capture_output=True, text=True, timeout=30)
        return out.stdout.splitlines()
    except FileNotFoundError:
        out = subprocess.run(["grep", "-rn", pattern, *paths], capture_output=True, text=True, timeout=30)
        return out.stdout.splitlines()


def canonical_path_map(repo: str):
    """Reuse the same parsing approach as kitcheck.sh's manifest builder: map
    CANONICAL_..._SOURCE_PATH constants to their real corelib/runtime file."""
    sources_rs = os.path.join(repo, "crates", "beskid_abi", "src", "runtime_source", "sources.rs")
    text = open(sources_rs, encoding="utf-8", errors="replace").read()
    path_consts = dict(re.findall(r'pub const (CANONICAL_\w+_SOURCE_PATH): &str = "([^"]+)";', text))
    source_real = {}
    for m in re.finditer(
        r'const (CANONICAL_\w+_SOURCE): &str =\s*include_str!\(concat!\(\s*env!\("CARGO_MANIFEST_DIR"\),\s*"(/[^"]+)"\s*\)\);',
        text, re.S):
        source_real[m.group(1)] = re.sub(r'^/(\.\./)+', '', m.group(2))
    result = {}
    for path_key, logical in path_consts.items():
        source_key = path_key[: -len("_PATH")]
        real = source_real.get(source_key)
        if real:
            # sources.rs real paths are relative to crates/beskid_abi/../.. == repo root
            result[path_key] = real
    return result


def find_corelib_service_entries(repo: str, name: str):
    services_rs = os.path.join(repo, "crates", "beskid_abi", "src", "runtime_source", "corelib_services.rs")
    text = open(services_rs, encoding="utf-8", errors="replace").read()
    entries = []
    for m in re.finditer(
        r'CorelibService\s*\{\s*name:\s*"([^"]+)"\s*,\s*symbol:\s*"([^"]+)"\s*,\s*source_path:\s*(CANONICAL_\w+_SOURCE_PATH)\s*,?\s*\}',
        text):
        if m.group(1) == name:
            entries.append({"symbol": m.group(2), "source_path_const": m.group(3)})
    return entries, services_rs


def find_scheduler_entry_helper(repo: str, name: str):
    module_emission = os.path.join(repo, "crates", "beskid_codegen", "src", "module_emission.rs")
    text = open(module_emission, encoding="utf-8", errors="replace").read()
    m = re.search(r'SCHEDULER_ENTRY_HELPERS:.*?=\s*&\[(.*?)\];', text, re.S)
    if not m:
        return False, []
    names = re.findall(r'"([^"]+)"', m.group(1))
    return name in names, names


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("name", help="raw call (e.g. __timer_sleep_until) or canonical service/item name (e.g. SchedulerSetCurrentFiber)")
    ap.add_argument("--repo", default=default_repo_root())
    args = ap.parse_args()
    repo, name = args.repo, args.name

    found_any = False

    # Mechanism A: corelib service authority table
    entries, services_rs = find_corelib_service_entries(repo, name)
    if entries:
        found_any = True
        path_map = canonical_path_map(repo)
        rel_services_rs = os.path.relpath(services_rs, repo)
        print(f"MECHANISM A: raw-builtin corelib-service authority table ({rel_services_rs})")
        print(f"  `{name}` is authorized for {len(entries)} source file(s):")
        authorized_files = []
        for e in entries:
            real = path_map.get(e["source_path_const"], "(unresolved constant)")
            authorized_files.append(real)
            print(f"    symbol={e['symbol']!r}  source_path={e['source_path_const']} -> {real}")

        print(f"\n  Actual call sites of `{name}(` in corelib/ and runtime/:")
        call_lines = rg(f"{re.escape(name)}\\(", [os.path.join(repo, "corelib"), os.path.join(repo, "runtime")])
        if not call_lines:
            print("    (none found -- check spelling, or it's called only from generated/test code)")
        for line in call_lines:
            parts = line.split(":", 2)
            if len(parts) < 2:
                continue
            path = parts[0]
            rel = os.path.relpath(path, repo)
            authorized = rel in authorized_files
            flag = "OK (authorized file)" if authorized else "** NOT in the authorized list **"
            print(f"    {rel}  [{flag}]")
        print()

    # Mechanism B: scheduler-entry reachability
    is_helper, helper_list = find_scheduler_entry_helper(repo, name)
    if is_helper:
        found_any = True
        print("MECHANISM B: scheduler-entry reachability (crates/beskid_codegen/src/module_emission.rs SCHEDULER_ENTRY_HELPERS)")
        print(f"  `{name}` is one of the required scheduler-entry items: {helper_list}")
        print("  Once ANY of these is needed by a compiled module (a spawn/context trampoline exists),")
        print("  the compiler requires ALL of them to be present in that module's reachable item set,")
        print("  or it fails closed with `canonical <name> item unavailable`")
        print("  (crates/beskid_codegen/src/module_emission/orchestration.rs, `symbol(name)` lookup")
        print("  against the module's own compiled `items`, not against a source-file authority table).")
        print()
        print(f"  Declaration sites of `{name}` in runtime/beskid/src (must be reachable from the")
        print(f"  compiled project's entry point, not just exist somewhere in the tree):")
        decl_lines = rg(rf"\b{re.escape(name)}\s*\(", [os.path.join(repo, "runtime", "beskid", "src")])
        pub_decls = [l for l in decl_lines if re.search(rf"pub\s+[\w<>\[\]]*\s*{re.escape(name)}\s*\(", l)]
        for l in pub_decls[:10]:
            parts = l.split(":", 2)
            if len(parts) >= 3:
                print(f"    {os.path.relpath(parts[0], repo)}:{parts[1]}  {parts[2].strip()}")
        if not pub_decls:
            print("    (no `pub ... name(` declaration found -- check it's actually exported)")
        print("\n  If declared and exported but still 'unavailable': the compiled project (its .bproj /")
        print("  test target entry point) likely doesn't transitively `use` the declaring module, so the")
        print("  item never enters this compilation's item set -- cross-check with visibility.py.")

    if not found_any:
        print(f"`{name}` matched neither the corelib-service authority table nor SCHEDULER_ENTRY_HELPERS.")
        print("Falling back to a plain repo grep for the raw error text pointing at it:")
        for line in rg(f'"canonical {re.escape(name)}', [os.path.join(repo, "crates")])[:10]:
            print(f"  {line}")
        print("\nIf that found nothing either, this may be a different authority mechanism this tool")
        print("doesn't yet model -- check crates/beskid_abi/src/runtime_source/capabilities.rs and")
        print("crates/beskid_codegen/src/module_emission/** by hand.")


if __name__ == "__main__":
    main()

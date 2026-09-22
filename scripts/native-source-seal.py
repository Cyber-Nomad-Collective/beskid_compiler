#!/usr/bin/env python3
"""Local sealed Git source transport. This is not a build/evidence producer.

Consumers must obtain the receipt SHA-256 and dirty-file allowlist through a
trusted channel independent of the package. A matching hash proves bytes,
not authority. Restore always validates in a fresh temporary directory.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import stat
import tempfile
import unicodedata

from native_runtime_source_closure import (
    REPOSITORIES, discover_source_closure, git_run as run, is_output_path,
    untracked_source_paths, validate_source_closure,
)

PATHS = {"superproject": ".", "compiler": "compiler", "corelib": "compiler/corelib", "bsol": "beskid_bsol"}


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True) + "\n").encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def exact(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields):
        raise RuntimeError("malformed sealed source record")


def relative(value):
    if not isinstance(value, str) or not value or any(c in value for c in '\\:<>"|?*'):
        raise RuntimeError("unsafe source path")
    parts = value.split("/")
    if any(p in ("", ".", "..") or p.casefold() == ".git" or p.endswith((".", " ")) for p in parts):
        raise RuntimeError("unsafe source path")
    if any(ord(c) < 32 for c in value):
        raise RuntimeError("unsafe source path")
    devices = {"con", "prn", "aux", "nul", "conin$", "conout$"}
    devices.update(f"{prefix}{digit}" for prefix in ("com", "lpt") for digit in "123456789¹²³")
    if any(part.split(".")[0].casefold() in devices for part in parts):
        raise RuntimeError("unsafe Windows device source path")
    return value


def portable_paths(paths):
    """Check each component, including directory aliases, on every host OS."""
    known = {}
    for path in paths:
        parts = relative(path).split("/")
        for length in range(1, len(parts) + 1):
            spelling = "/".join(parts[:length])
            key = unicodedata.normalize("NFC", spelling).casefold()
            if key in known and known[key] != spelling:
                raise RuntimeError("Windows-portable source path collision")
            known[key] = spelling


def regular(root, name, *, missing=False):
    path = root
    parts = relative(name).split("/")
    for index, part in enumerate(parts):
        path = path / part
        try:
            mode = path.lstat().st_mode
        except FileNotFoundError:
            if missing:
                return root.joinpath(*parts)
            raise RuntimeError("missing regular source file") from None
        if stat.S_ISLNK(mode) or (index < len(parts) - 1 and not stat.S_ISDIR(mode)):
            raise RuntimeError("symlink or non-regular source path")
        if index == len(parts) - 1 and not stat.S_ISREG(mode):
            raise RuntimeError("non-regular source file")
    return path


def allowlist(value):
    if not isinstance(value, dict) or not set(value) <= set(REPOSITORIES):
        raise RuntimeError("malformed allowlist")
    result = {}
    for name in REPOSITORIES:
        paths = value.get(name, [])
        if not isinstance(paths, list) or any(not isinstance(p, str) for p in paths):
            raise RuntimeError("malformed allowlist")
        normalized = [relative(p) for p in paths]
        portable_paths(normalized)
        if len(set(normalized)) != len(normalized):
            raise RuntimeError("duplicate allowlist path")
        result[name] = set(normalized)
    return result


def changes(root, name, allowed):
    changed = set(run(root, "diff", "HEAD", "--name-only", "-z", "--no-renames", "--ignore-submodules=all", "--no-ext-diff", "--no-textconv").decode().split("\0")[:-1])
    untracked = set(untracked_source_paths(root, name))
    baseline = set(run(root, "ls-tree", "-r", "--name-only", "-z", "HEAD").decode().split("\0")[:-1])
    indexed = set(run(root, "ls-files", "--cached", "-z").decode().split("\0")[:-1])
    if (changed | untracked) - allowed:
        raise RuntimeError("source change is outside the consumer allowlist")
    result = []
    # The allowlist itself drives enumeration; ignore rules never determine
    # whether an approved path is inspected or silently omitted.
    for path in sorted(allowed):
        if is_output_path(name, path):
            raise RuntimeError("output/cache path cannot be a source overlay")
        file = regular(root, path, missing=True)
        exists = file.exists()
        tracked = path in baseline or path in indexed
        if (tracked and path not in changed) or (not tracked and not exists):
            raise RuntimeError("allowlisted path has no source delta")
        result.append({"path": path, "kind": "tracked" if tracked else "untracked",
                       "sha256": sha(file.read_bytes()) if exists else None,
                       "executable": bool(file.stat().st_mode & 0o111) if exists else False})
    return result


def require_regular_tree(root, overlays=(), revision="HEAD"):
    paths = list(overlays)
    gitlinks = []
    for record in run(root, "ls-tree", "-r", "-z", revision).split(b"\0"):
        if not record:
            continue
        metadata, name = record.decode().split("\t", 1)
        mode, kind, _ = metadata.split()
        relative(name)
        paths.append(name)
        if kind == "commit":
            gitlinks.append(name)
        if kind == "blob" and mode not in ("100644", "100755"):
            raise RuntimeError("source tree contains symlink or non-regular file")
    portable_paths(paths)
    for path in overlays:
        if any(path == edge or path.startswith(edge + "/") for edge in gitlinks):
            raise RuntimeError("overlay crosses a committed source gitlink")


def artifact(root, name, data):
    (root / name).write_bytes(data)
    return {"path": name, "sha256": sha(data)}


def pack(compiler, destination, mode, allowed):
    if mode not in ("clean", "diagnostic_dirty"):
        raise RuntimeError("invalid sealed source mode")
    approved = allowlist(allowed)
    if mode == "clean" and any(approved.values()):
        raise RuntimeError("clean source cannot accept overlays")
    root, revisions = discover_source_closure(compiler, allow_dirty=mode == "diagnostic_dirty")
    destination = pathlib.Path(destination).absolute()
    if destination.exists() or destination.is_symlink():
        raise RuntimeError("package destination must not exist")
    for name in REPOSITORIES:
        repo = root / PATHS[name]
        require_regular_tree(repo, approved[name])
        if destination.is_relative_to(repo):
            raise RuntimeError("package destination must be outside source closure")
    # Preflight every overlay before creating any output.
    inventories = {name: changes(root / PATHS[name], name, approved[name]) for name in REPOSITORIES}
    with tempfile.TemporaryDirectory(prefix="beskid-seal-pack-", dir=destination.parent) as temporary:
        output = pathlib.Path(temporary) / "package"
        output.mkdir()
        records = {}
        for name in REPOSITORIES:
            repo = root / PATHS[name]
            bundle_name = name + ".bundle"
            run(repo, "bundle", "create", str(output / bundle_name), "HEAD")
            record = {"bundle": {"path": bundle_name, "sha256": sha((output / bundle_name).read_bytes())},
                      "patch": artifact(output, name + ".patch", run(repo, "diff", "HEAD", "--binary", "--full-index", "--no-renames", "--ignore-submodules=all", "--no-ext-diff", "--no-textconv")),
                      "files": inventories[name], "untracked": {}}
            for index, entry in enumerate(record["files"]):
                if entry["kind"] == "untracked":
                    record["untracked"][entry["path"]] = artifact(output, f"{name}-{index}.file", regular(repo, entry["path"]).read_bytes())
            records[name] = record
        receipt = {"schema_version": 1, "mode": mode, "revisions": revisions, "repositories": records}
        data = canonical(receipt)
        (output / "receipt.json").write_bytes(data)
        digest = sha(data)
        # Replay proves captured patches/inventories agree before publication.
        verify(output, digest, allowed, purpose="matrix" if mode == "clean" else "diagnostic")
        output.rename(destination)
    return digest


def read_package(package, digest, allowed, purpose):
    package = pathlib.Path(package)
    if package.is_symlink() or not package.is_dir():
        raise RuntimeError("package must be a regular directory")
    approved = allowlist(allowed)
    data = regular(package, "receipt.json").read_bytes()
    if not isinstance(digest, str) or not re.fullmatch("[0-9a-f]{64}", digest) or sha(data) != digest:
        raise RuntimeError("receipt digest mismatch")
    try:
        receipt = json.loads(data)
    except (ValueError, UnicodeError):
        raise RuntimeError("malformed source receipt") from None
    exact(receipt, ("schema_version", "mode", "revisions", "repositories"))
    if canonical(receipt) != data or type(receipt["schema_version"]) is not int or receipt["schema_version"] != 1:
        raise RuntimeError("noncanonical or unsupported receipt")
    if receipt["mode"] not in ("clean", "diagnostic_dirty") or purpose not in ("matrix", "diagnostic"):
        raise RuntimeError("invalid closure mode or purpose")
    if purpose == "matrix" and receipt["mode"] != "clean":
        raise RuntimeError("diagnostic dirty closure cannot produce matrix evidence")
    if receipt["mode"] == "clean" and any(approved.values()):
        raise RuntimeError("clean source cannot accept overlays")
    exact(receipt["revisions"], [f"{name}_{field}" for name in REPOSITORIES for field in ("sha", "dirty")] + ["gitlinks"])
    validate_source_closure(receipt["revisions"], allow_dirty=receipt["mode"] == "diagnostic_dirty")
    exact(receipt["repositories"], REPOSITORIES)
    inventory = {"receipt.json"}

    def check_artifact(record):
        exact(record, ("path", "sha256"))
        path = relative(record["path"])
        if "/" in path or path in inventory:
            raise RuntimeError("malformed package inventory")
        inventory.add(path)
        if sha(regular(package, path).read_bytes()) != record["sha256"]:
            raise RuntimeError("artifact digest mismatch")

    for name in REPOSITORIES:
        record = receipt["repositories"][name]
        exact(record, ("bundle", "patch", "files", "untracked"))
        check_artifact(record["bundle"])
        check_artifact(record["patch"])
        if not isinstance(record["files"], list) or not isinstance(record["untracked"], dict):
            raise RuntimeError("malformed source inventory")
        names = []
        untracked = set()
        for entry in record["files"]:
            exact(entry, ("path", "kind", "sha256", "executable"))
            path = relative(entry["path"])
            if path not in approved[name]:
                raise RuntimeError("source change is outside the consumer allowlist")
            if entry["kind"] not in ("tracked", "untracked") or type(entry["executable"]) is not bool:
                raise RuntimeError("malformed source inventory")
            if entry["sha256"] is not None and (not isinstance(entry["sha256"], str) or not re.fullmatch("[0-9a-f]{64}", entry["sha256"])):
                raise RuntimeError("malformed source digest")
            names.append(path)
            if entry["kind"] == "untracked":
                untracked.add(path)
        if names != sorted(set(names)) or set(record["untracked"]) != untracked:
            raise RuntimeError("malformed source inventory")
        if set(names) != approved[name]:
            raise RuntimeError("receipt does not represent every allowlisted path")
        if receipt["mode"] == "clean" and names:
            raise RuntimeError("clean source has dirty overlay")
        for entry in record["untracked"].values():
            check_artifact(entry)
    for item in package.iterdir():
        regular(package, item.name)
    if {p.name for p in package.iterdir()} != inventory:
        raise RuntimeError("package inventory mismatch")
    return receipt, approved


def materialize(package, receipt, approved, destination):
    destination.mkdir()
    for name in REPOSITORIES:
        record = receipt["repositories"][name]
        repo = destination / PATHS[name]
        run(destination, "clone", "--quiet", "--no-checkout", str(package / record["bundle"]["path"]), str(repo))
        require_regular_tree(repo, [entry["path"] for entry in record["files"]], receipt["revisions"][name + "_sha"])
        run(repo, "checkout", "--quiet", "--detach", receipt["revisions"][name + "_sha"])
    # Register only the three closure edges, never fetch .gitmodules URLs.
    run(destination, "submodule", "init", "--", "compiler", "beskid_bsol")
    run(destination / "compiler", "submodule", "init", "--", "corelib")
    run(destination, "submodule", "absorbgitdirs", "--", "compiler", "beskid_bsol")
    _, clean = discover_source_closure(destination / "compiler")
    expected = dict(receipt["revisions"])
    for name in REPOSITORIES:
        expected[name + "_dirty"] = False
    if clean != expected:
        raise RuntimeError("bundle commits or Gitlinks differ from receipt")
    for name in REPOSITORIES:
        record = receipt["repositories"][name]
        repo = destination / PATHS[name]
        for entry in record["files"]:
            regular(repo, entry["path"], missing=True)
        patch = package / record["patch"]["path"]
        if patch.stat().st_size:
            run(repo, "apply", "--binary", "--whitespace=nowarn", str(patch))
        for entry in record["files"]:
            if entry["kind"] == "untracked":
                target = regular(repo, entry["path"], missing=True)
                if target.exists():
                    raise RuntimeError("untracked overlay overwrites existing source")
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes((package / record["untracked"][entry["path"]]["path"]).read_bytes())
                target.chmod(0o755 if entry["executable"] else 0o644)
        if changes(repo, name, approved[name]) != record["files"]:
            raise RuntimeError("restored source inventory differs from receipt")
    _, actual = discover_source_closure(destination / "compiler", allow_dirty=receipt["mode"] == "diagnostic_dirty")
    if actual != receipt["revisions"]:
        raise RuntimeError("restored source status differs from receipt")


def verify(package, digest, allowed, *, purpose):
    package = pathlib.Path(package).absolute()
    receipt, approved = read_package(package, digest, allowed, purpose)
    with tempfile.TemporaryDirectory(prefix="beskid-seal-verify-") as temporary:
        materialize(package, receipt, approved, pathlib.Path(temporary) / "source")
    return receipt


def restore(package, digest, destination, allowed, *, purpose):
    package = pathlib.Path(package).absolute()
    destination = pathlib.Path(destination).absolute()
    if destination.exists() or destination.is_symlink():
        raise RuntimeError("restore destination must not exist")
    receipt, approved = read_package(package, digest, allowed, purpose)
    with tempfile.TemporaryDirectory(prefix="beskid-seal-restore-", dir=destination.parent) as temporary:
        source = pathlib.Path(temporary) / "source"
        materialize(package, receipt, approved, source)
        source.rename(destination)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    packing = commands.add_parser("pack")
    packing.add_argument("--compiler-root", type=pathlib.Path, required=True)
    packing.add_argument("--destination", type=pathlib.Path, required=True)
    packing.add_argument("--mode", choices=("clean", "diagnostic_dirty"), required=True)
    for command in ("verify", "restore"):
        sub = commands.add_parser(command)
        sub.add_argument("--package", type=pathlib.Path, required=True)
        sub.add_argument("--receipt-sha256", required=True)
        sub.add_argument("--purpose", choices=("matrix", "diagnostic"), required=True)
        if command == "restore":
            sub.add_argument("--destination", type=pathlib.Path, required=True)
        sub.add_argument("--allowlist", type=pathlib.Path)
    packing.add_argument("--allowlist", type=pathlib.Path)
    args = parser.parse_args()
    try:
        allowed = json.loads(args.allowlist.read_text()) if args.allowlist else {}
        if args.command == "pack":
            print(pack(args.compiler_root, args.destination, args.mode, allowed))
        elif args.command == "verify":
            verify(args.package, args.receipt_sha256, allowed, purpose=args.purpose)
        else:
            restore(args.package, args.receipt_sha256, args.destination, allowed, purpose=args.purpose)
    except (RuntimeError, OSError, ValueError) as error:
        parser.exit(1, f"sealed source error: {error}\n")


if __name__ == "__main__":
    main()

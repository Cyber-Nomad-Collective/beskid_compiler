"""Single authority for clean native runtime-kit source closure receipts."""

from __future__ import annotations

import pathlib
import os
import re
import subprocess
from typing import Any

REPOSITORIES = ("superproject", "compiler", "corelib", "bsol")
GITLINKS = {
    "compiler": "compiler",
    "compiler/corelib": "corelib",
    "beskid_bsol": "bsol",
}


def git_run(root: pathlib.Path, *args: str) -> bytes:
    """Run Git against an explicit directory without ambient repository/config state."""
    environment = {key: value for key, value in os.environ.items() if not key.upper().startswith("GIT_")}
    environment.update({
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_CONFIG_GLOBAL": os.devnull, "GIT_ATTR_NOSYSTEM": "1",
        "GIT_TERMINAL_PROMPT": "0",
    })
    command = ["git", "-c", "core.autocrlf=false", "-c", f"core.hooksPath={os.devnull}",
               "-c", f"core.attributesFile={os.devnull}", "-c", f"core.excludesFile={os.devnull}",
               "-c", "core.fsmonitor=false", "-c", "init.templateDir=", "-C", str(root)]
    # Local repository layout/config remains necessary for submodule worktrees,
    # but configured content filters must never execute while inspecting bytes.
    filters = subprocess.run(command + ["config", "--local", "--includes", "--name-only", "--get-regexp", r"^filter\."],
                             capture_output=True, env=environment)
    if filters.returncode == 0:
        drivers = {key.rsplit(".", 1)[0] for key in filters.stdout.decode().splitlines()}
        for driver in sorted(drivers):
            for key, value in (("clean", ""), ("smudge", ""), ("process", ""), ("required", "false")):
                command.extend(["-c", f"{driver}.{key}={value}"])
    elif filters.returncode != 1 and args[0] != "clone":
        raise RuntimeError("cannot inspect source Git configuration")
    result = subprocess.run(command + list(args), capture_output=True, env=environment)
    if result.returncode:
        raise RuntimeError(f"source Git operation failed: {args[0]}")
    return result.stdout


def git(root: pathlib.Path, *args: str) -> str:
    return git_run(root, *args).decode().strip()


def require_repository(root: pathlib.Path) -> None:
    # An uninitialized submodule directory inherits its parent's Git repository.
    if pathlib.Path(git(root, "rev-parse", "--show-toplevel")).resolve() != root:
        raise RuntimeError(f"source submodule is not initialized: {root.name}")


def require_clean(root: pathlib.Path, name: str) -> None:
    # Inspect direct sources; selected children are independently verified by
    # HEAD/Gitlink and their own clean check. Unselected root children are opaque.
    if source_dirty(root, name):
        raise RuntimeError(f"native runtime-kit evidence rejects a dirty {name} checkout")


def is_output_path(name: str, path: str) -> bool:
    """The compiler's conventional Cargo target tree is output, never source.

    No ignore file can expand this policy. Credentials, .beskid, and arbitrary
    ignored paths remain source candidates and therefore fail closed.
    """
    return name == "compiler" and path.startswith("target/")


def untracked_source_paths(root: pathlib.Path, name: str) -> list[str]:
    # Deliberately omit --exclude-standard: local/info and tracked ignore rules
    # cannot hide diagnostic inputs. Gitlinks remain opaque to ls-files.
    paths = git_run(root, "ls-files", "--others", "-z").decode().split("\0")[:-1]
    return [path for path in paths if not is_output_path(name, path)]


def source_dirty(root: pathlib.Path, name: str) -> bool:
    tracked = git(root, "status", "--porcelain", "--untracked-files=no", "--ignore-submodules=all")
    return bool(tracked or untracked_source_paths(root, name))


def gitlink(root: pathlib.Path, relative: str) -> str:
    record = git(root, "ls-tree", "--full-tree", "HEAD", "--", relative)
    fields = record.split()
    if len(fields) != 4 or fields[:2] != ["160000", "commit"] or fields[3] != relative:
        raise RuntimeError(f"source closure requires a committed gitlink for {relative}")
    return fields[2]


def require_selected_topology(root: pathlib.Path, name: str) -> None:
    """Only the root may contain opaque, unselected committed submodules."""
    permitted = {"corelib"} if name == "compiler" else set()
    for record in git_run(root, "ls-tree", "-r", "-z", "HEAD").split(b"\0"):
        if not record:
            continue
        metadata, path = record.decode().split("\t", 1)
        if is_output_path(name, path):
            raise RuntimeError("compiler output/cache path cannot be tracked source")
        if metadata.split()[1] == "commit" and name != "superproject" and path not in permitted:
            raise RuntimeError(f"undeclared source closure gitlink in {name}: {path}")


def validate_source_closure(value: Any, *, allow_dirty: bool = False) -> tuple[str, ...]:
    """Reject incomplete/dirty receipts; return the full comparable identity."""
    if not isinstance(value, dict):
        raise RuntimeError("malformed source closure revisions")
    revisions = []
    for name in REPOSITORIES:
        revision = value.get(f"{name}_sha")
        if not isinstance(revision, str) or not re.fullmatch(r"(?:[0-9a-f]{40}|[0-9a-f]{64})", revision):
            raise RuntimeError(f"malformed {name} source closure revision")
        dirty = value.get(f"{name}_dirty")
        if not isinstance(dirty, bool) or (dirty and not allow_dirty):
            raise RuntimeError(f"dirty or malformed {name} source closure evidence")
        revisions.append(revision)
    links = value.get("gitlinks")
    if not isinstance(links, dict) or set(links) != set(GITLINKS):
        raise RuntimeError("malformed source closure gitlinks")
    for path, name in GITLINKS.items():
        if links[path] != value[f"{name}_sha"]:
            raise RuntimeError(f"source closure gitlink/HEAD mismatch for {path}")
    return tuple(revisions)


def discover_source_closure(
    compiler_root: pathlib.Path, *, allow_dirty: bool = False
) -> tuple[pathlib.Path, dict[str, Any]]:
    """Prove the registered root/compiler/Corelib/BSOL Git relationship."""
    compiler_root = compiler_root.resolve()
    require_repository(compiler_root)
    # Keep the existing dirty compiler failure ahead of transport/layout checks.
    if not allow_dirty:
        require_clean(compiler_root, "compiler")
    enclosing = git(compiler_root, "rev-parse", "--show-superproject-working-tree")
    if not enclosing:
        raise RuntimeError("compiler checkout has no proven superproject submodule relationship")
    superproject = pathlib.Path(enclosing).resolve()
    require_repository(superproject)
    if (superproject / "compiler").resolve() != compiler_root:
        raise RuntimeError("compiler checkout does not match its superproject submodule path")
    paths = {
        "superproject": superproject,
        "compiler": compiler_root,
        "corelib": compiler_root / "corelib",
        "bsol": superproject / "beskid_bsol",
    }
    revisions: dict[str, Any] = {}
    for name, root in paths.items():
        require_repository(root)
        require_selected_topology(root, name)
        if not allow_dirty:
            require_clean(root, name)
        revisions[f"{name}_sha"] = git(root, "rev-parse", "HEAD")
        revisions[f"{name}_dirty"] = source_dirty(root, name)
    revisions["gitlinks"] = {
        "compiler": gitlink(superproject, "compiler"),
        "compiler/corelib": gitlink(compiler_root, "corelib"),
        "beskid_bsol": gitlink(superproject, "beskid_bsol"),
    }
    validate_source_closure(revisions, allow_dirty=allow_dirty)
    return superproject, revisions

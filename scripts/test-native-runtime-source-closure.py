#!/usr/bin/env python3
"""Exercise evidence initialization against real, isolated Git submodules."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from native_runtime_source_closure import validate_source_closure

SCRIPT = pathlib.Path(__file__).with_name("native-runtime-kit-evidence.py")
SPEC = importlib.util.spec_from_file_location("producer_evidence", SCRIPT)
assert SPEC and SPEC.loader
producer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(producer)


def git(root: pathlib.Path, *args: str) -> str:
    return subprocess.run(
        ["git", "-c", "protocol.file.allow=always", "-c", "user.name=Closure Test",
         "-c", "user.email=closure-test@example.invalid", "-c", "commit.gpgsign=false",
         "-C", str(root), *args],
        check=True, capture_output=True, text=True,
    ).stdout.strip()


def commit(root: pathlib.Path) -> None:
    git(root, "add", "--all")
    git(root, "commit", "--quiet", "-m", "isolated provenance fixture")


class SourceClosureTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = pathlib.Path(self.temporary.name).resolve()
        seeds = self.base / "seeds"
        for name in ("corelib", "beskid_bsol", "compiler", "root"):
            repo = seeds / name
            repo.mkdir(parents=True)
            git(repo, "init", "--quiet")
            (repo / "source.txt").write_text(name, encoding="utf-8")
            commit(repo)
        compiler = seeds / "compiler"
        git(compiler, "submodule", "add", "--quiet", str(seeds / "corelib"), "corelib")
        (compiler / "runtime_manifest.bsol").write_text("fixture", encoding="utf-8")
        commit(compiler)
        root = seeds / "root"
        for name in ("compiler", "beskid_bsol"):
            git(root, "submodule", "add", "--quiet", str(seeds / name), name)
        commit(root)
        self.root = self.base / "checkout"
        git(self.base, "clone", "--quiet", "--recurse-submodules", str(root), str(self.root))
        self.compiler = self.root / "compiler"
        self.repositories = {
            "superproject": self.root,
            "compiler": self.compiler,
            "corelib": self.compiler / "corelib",
            "bsol": self.root / "beskid_bsol",
        }

    def initialize(self, compiler: pathlib.Path | None = None) -> dict:
        output = self.base / "evidence"
        with patch.dict(os.environ, {"BESKID_RUNTIME_KIT_EVIDENCE_DIR": str(output)}):
            producer.command_init(argparse.Namespace(
                compiler_root=str(compiler or self.compiler),
                target="x86_64-unknown-linux-gnu", symbol_tool=sys.executable,
            ))
        return json.loads((output / "producer.json").read_text(encoding="utf-8"))

    def test_producer_records_entire_clean_closure(self) -> None:
        record = self.initialize()
        self.assertEqual(record["schema_version"], 4)
        revisions = record["revisions"]
        for name, repo in self.repositories.items():
            self.assertEqual(revisions.get(f"{name}_sha"), git(repo, "rev-parse", "HEAD"), name)
            self.assertIs(revisions.get(f"{name}_dirty"), False, name)
        self.assertEqual(revisions.get("gitlinks"), {
            "compiler": git(self.compiler, "rev-parse", "HEAD"),
            "compiler/corelib": git(self.compiler / "corelib", "rev-parse", "HEAD"),
            "beskid_bsol": git(self.root / "beskid_bsol", "rev-parse", "HEAD"),
        })

    def test_producer_ignores_ambient_git_repository_selection(self) -> None:
        with patch.dict(os.environ, {
            "GIT_DIR": str(self.base / "missing.git"),
            "GIT_WORK_TREE": str(self.base / "wrong-worktree"),
            "GIT_INDEX_FILE": str(self.base / "wrong-index"),
        }):
            record = self.initialize()
        self.assertEqual(record["revisions"]["compiler_sha"], git(self.compiler, "rev-parse", "HEAD"))

    def test_producer_rejects_locally_excluded_source(self) -> None:
        exclude = pathlib.Path(git(self.compiler, "rev-parse", "--path-format=absolute", "--git-path", "info/exclude"))
        exclude.write_text("hidden.bd\n", encoding="utf-8")
        (self.compiler / "hidden.bd").write_text("source", encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.initialize()

    def test_producer_permits_only_declared_compiler_output_root(self) -> None:
        (self.compiler / "target").mkdir()
        (self.compiler / "target/cache").write_text("output", encoding="utf-8")
        record = self.initialize()
        self.assertFalse(record["revisions"]["compiler_dirty"])
        (self.compiler / ".beskid").mkdir()
        (self.compiler / ".beskid/state").write_text("unapproved state", encoding="utf-8")
        exclude = pathlib.Path(git(self.compiler, "rev-parse", "--path-format=absolute", "--git-path", "info/exclude"))
        exclude.write_text(".beskid/\n", encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "dirty"):
            self.initialize()

    def test_producer_selects_build_closure_without_unrelated_root_submodule_state(self) -> None:
        git(self.root, "submodule", "add", "--quiet", str(self.base / "seeds/corelib"), "unrelated")
        commit(self.root)
        (self.root / "unrelated/source.txt").write_text("unselected dirty source", encoding="utf-8")
        record = self.initialize()
        self.assertFalse(record["revisions"]["superproject_dirty"])
        self.assertEqual(set(record["revisions"]["gitlinks"]), {"compiler", "compiler/corelib", "beskid_bsol"})

    def test_producer_rejects_dirty_source_in_each_repository(self) -> None:
        for name, repo in self.repositories.items():
            with self.subTest(repository=name):
                source = repo / "source.txt"
                original = source.read_text(encoding="utf-8")
                source.write_text("dirty", encoding="utf-8")
                try:
                    with self.assertRaisesRegex(RuntimeError, "dirty"):
                        self.initialize()
                finally:
                    source.write_text(original, encoding="utf-8")

    def test_producer_rejects_untracked_sources(self) -> None:
        for name, repo in self.repositories.items():
            with self.subTest(repository=name):
                source = repo / "new-source.txt"
                source.write_text("untracked", encoding="utf-8")
                try:
                    with self.assertRaisesRegex(RuntimeError, "dirty"):
                        self.initialize()
                finally:
                    source.unlink()

    def test_producer_rejects_unpinned_child_commits_even_when_ignored(self) -> None:
        for name in ("compiler", "corelib", "bsol"):
            with self.subTest(repository=name):
                repo = self.repositories[name]
                old_head = git(repo, "rev-parse", "HEAD")
                for parent, child in ((self.root, "compiler"), (self.root, "beskid_bsol"), (self.compiler, "corelib")):
                    git(parent, "config", f"submodule.{child}.ignore", "all")
                git(repo, "commit", "--quiet", "--allow-empty", "-m", "unpinned fixture")
                try:
                    with self.assertRaisesRegex(RuntimeError, "dirty|gitlink"):
                        self.initialize()
                finally:
                    git(repo, "checkout", "--quiet", "--detach", old_head)

    def test_producer_rejects_uninitialized_corelib(self) -> None:
        git(self.compiler, "submodule", "deinit", "--force", "corelib")
        with self.assertRaisesRegex(RuntimeError, "corelib|submodule"):
            self.initialize()

    def test_producer_rejects_uninitialized_bsol(self) -> None:
        git(self.root, "submodule", "deinit", "--force", "beskid_bsol")
        with self.assertRaisesRegex(RuntimeError, "beskid_bsol|submodule"):
            self.initialize()

    def test_producer_rejects_nested_compiler_worktree(self) -> None:
        nested = self.root / ".worktrees" / "compiler"
        git(self.compiler, "worktree", "add", "--quiet", "--detach", str(nested))
        with self.assertRaisesRegex(RuntimeError, "superproject|submodule"):
            self.initialize(nested)

    def test_producer_rejects_standalone_clone(self) -> None:
        standalone = self.base / "standalone"
        git(self.base, "clone", "--quiet", "--recurse-submodules", str(self.compiler), str(standalone))
        with self.assertRaisesRegex(RuntimeError, "superproject|submodule"):
            self.initialize(standalone)


class ClosureReceiptTests(unittest.TestCase):
    def setUp(self) -> None:
        self.receipt = {
            "superproject_sha": "1" * 40,
            "compiler_sha": "2" * 40,
            "corelib_sha": "3" * 40,
            "bsol_sha": "4" * 40,
            "superproject_dirty": False,
            "compiler_dirty": False,
            "corelib_dirty": False,
            "bsol_dirty": False,
            "gitlinks": {
                "compiler": "2" * 40,
                "compiler/corelib": "3" * 40,
                "beskid_bsol": "4" * 40,
            },
        }

    def test_requires_an_object(self) -> None:
        for value in (None, [], "closure", False):
            with self.subTest(value=value), self.assertRaisesRegex(RuntimeError, "malformed"):
                validate_source_closure(value)

    def test_requires_complete_object_ids(self) -> None:
        for name in ("superproject", "compiler", "corelib", "bsol"):
            for value in (None, [], 12, "", "a" * 39, "g" * 40, "A" * 40):
                with self.subTest(repository=name, value=value):
                    receipt = dict(self.receipt, **{f"{name}_sha": value})
                    with self.assertRaisesRegex(RuntimeError, f"malformed {name}"):
                        validate_source_closure(receipt)

    def test_requires_explicit_false_clean_facts(self) -> None:
        for name in ("superproject", "compiler", "corelib", "bsol"):
            for value in (None, True, "false", 0, []):
                with self.subTest(repository=name, value=value):
                    receipt = dict(self.receipt, **{f"{name}_dirty": value})
                    with self.assertRaisesRegex(RuntimeError, f"{name} source closure"):
                        validate_source_closure(receipt)

    def test_requires_every_revision_and_clean_fact(self) -> None:
        for name in ("superproject", "compiler", "corelib", "bsol"):
            for suffix in ("sha", "dirty"):
                with self.subTest(repository=name, missing=suffix):
                    receipt = dict(self.receipt)
                    del receipt[f"{name}_{suffix}"]
                    with self.assertRaisesRegex(RuntimeError, name):
                        validate_source_closure(receipt)

    def test_requires_exact_gitlink_set(self) -> None:
        for value in (None, [], {}, {"compiler": "2" * 40}, dict(self.receipt["gitlinks"], extra="5" * 40)):
            with self.subTest(gitlinks=value), self.assertRaisesRegex(RuntimeError, "gitlinks"):
                validate_source_closure(dict(self.receipt, gitlinks=value))

    def test_requires_gitlink_head_agreement(self) -> None:
        for path in ("compiler", "compiler/corelib", "beskid_bsol"):
            with self.subTest(path=path):
                links = dict(self.receipt["gitlinks"], **{path: "a" * 40})
                with self.assertRaisesRegex(RuntimeError, "gitlink/HEAD mismatch"):
                    validate_source_closure(dict(self.receipt, gitlinks=links))


if __name__ == "__main__":
    unittest.main()

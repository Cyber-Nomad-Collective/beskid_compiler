#!/usr/bin/env python3
"""Sealed transport acceptance tests using real disposable submodules."""
import hashlib
import importlib.util
import json
import os
import pathlib
import unittest
from unittest.mock import patch

from native_runtime_source_closure import discover_source_closure

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("closure_fixtures", HERE / "test-native-runtime-source-closure.py")
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)


class SealTests(unittest.TestCase):
    setUp = fixture.SourceClosureTests.setUp

    def api(self):
        spec = importlib.util.spec_from_file_location("seal", HERE / "native-source-seal.py")
        self.assertTrue(spec.origin and pathlib.Path(spec.origin).exists(), "sealed transport implementation missing")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def pack(self, mode="clean", allow=None):
        seal = self.api()
        destination = self.base / "sealed"
        digest = seal.pack(self.compiler, destination, mode, allow or {})
        return seal, destination, digest

    def test_clean_roundtrip_proves_original_gitlinks(self):
        seal, package, digest = self.pack()
        destination = self.base / "restored"
        seal.restore(package, digest, destination, {}, purpose="matrix")
        _, actual = discover_source_closure(destination / "compiler")
        _, expected = discover_source_closure(self.compiler)
        self.assertEqual(actual, expected)
        self.assertEqual((destination / "compiler/corelib/source.txt").read_text(), "corelib")

    def test_receipt_is_deterministic_for_unchanged_source(self):
        seal, package, digest = self.pack()
        second = self.base / "sealed-again"
        self.assertEqual(seal.pack(self.compiler, second, "clean", {}), digest)

    def test_hostile_git_environment_cannot_redirect_pack_or_discovery(self):
        hostile = {
            "GIT_DIR": str(self.compiler / ".git"),
            "GIT_WORK_TREE": str(self.base / "wrong-worktree"),
            "GIT_INDEX_FILE": str(self.base / "wrong-index"),
            "GIT_CONFIG_COUNT": "1", "GIT_CONFIG_KEY_0": "core.bare", "GIT_CONFIG_VALUE_0": "true",
        }
        with patch.dict(os.environ, hostile):
            seal, package, digest = self.pack()
            destination = self.base / "restored"
            seal.restore(package, digest, destination, {}, purpose="matrix")
            discovered, _ = discover_source_closure(destination / "compiler")
        self.assertEqual(discovered, destination)
        self.assertFalse((self.base / "wrong-index").exists())

    def test_ambient_git_filters_cannot_execute_or_rewrite_source(self):
        attributes = self.base / "ambient-attributes"
        attributes.write_text("* filter=hostile\n")
        config = self.base / "ambient-config"
        config.write_text(f'[core]\nattributesFile = {attributes}\n[filter "hostile"]\nsmudge = printf CORRUPT\nclean = printf CORRUPT\nrequired = true\n')
        with patch.dict(os.environ, {"GIT_CONFIG_GLOBAL": str(config), "GIT_CONFIG_SYSTEM": str(config)}):
            seal, package, digest = self.pack()
            destination = self.base / "restored"
            seal.restore(package, digest, destination, {}, purpose="matrix")
        self.assertEqual((destination / "compiler/source.txt").read_text(), "compiler")

    def test_failed_pack_does_not_publish_destination(self):
        # A staged-only change has no effective worktree patch, so exact replay
        # must fail its dirty-status comparison after bundle production.
        source = self.compiler / "source.txt"
        source.write_text("staged")
        fixture.git(self.compiler, "add", "source.txt")
        source.write_text("compiler")
        with self.assertRaisesRegex(RuntimeError, "status"):
            self.pack("diagnostic_dirty")
        self.assertFalse((self.base / "sealed").exists())

    def hide_local_source(self):
        excluded = pathlib.Path(fixture.git(self.compiler, "rev-parse", "--path-format=absolute", "--git-path", "info/exclude"))
        excluded.write_text("hidden.bd\n")
        (self.compiler / "hidden.bd").write_bytes(b"pub i64 Hidden() { return 42; }\n")

    def test_locally_excluded_allowlisted_source_is_restored(self):
        self.hide_local_source()
        allow = {"compiler": ["hidden.bd"]}
        seal, package, digest = self.pack("diagnostic_dirty", allow)
        destination = self.base / "restored"
        seal.restore(package, digest, destination, allow, purpose="diagnostic")
        self.assertEqual((destination / "compiler/hidden.bd").read_bytes(),
                         b"pub i64 Hidden() { return 42; }\n")

    def test_locally_excluded_unapproved_source_is_rejected(self):
        self.hide_local_source()
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            self.pack("diagnostic_dirty")

    def test_clean_mode_rejects_even_unchanged_approved_overlays(self):
        with self.assertRaisesRegex(RuntimeError, "clean.*overlay"):
            self.pack("clean", {"compiler": ["source.txt"]})

    def test_unused_diagnostic_allowlist_path_fails_closed(self):
        for name in ("missing.bd", "source.txt"):
            with self.subTest(path=name), self.assertRaisesRegex(RuntimeError, "allowlist|delta"):
                self.pack("diagnostic_dirty", {"compiler": [name]})

    def test_ignored_compiler_target_output_is_not_transported(self):
        (self.compiler / "target").mkdir()
        (self.compiler / "target/build-cache").write_bytes(b"cache")
        seal, package, digest = self.pack()
        destination = self.base / "restored"
        seal.restore(package, digest, destination, {}, purpose="matrix")
        self.assertFalse((destination / "compiler/target").exists())

    def test_output_cache_cannot_be_approved_as_source(self):
        (self.compiler / "target").mkdir()
        (self.compiler / "target/generated.bd").write_text("output")
        with self.assertRaisesRegex(RuntimeError, "output|cache"):
            self.pack("diagnostic_dirty", {"compiler": ["target/generated.bd"]})

    def test_windows_reserved_path_spellings_are_rejected(self):
        seal = self.api()
        for value in ('a<b', 'a>b', 'a"b', 'a|b', 'a?b', 'a*b', 'CON', 'con.txt',
                      'AUX.rs', 'NUL', 'PRN', 'COM1.log', 'lpt9.txt', 'COM¹', 'LPT².x', 'name.', 'name '):
            with self.subTest(path=value), self.assertRaisesRegex(RuntimeError, "unsafe|portable"):
                seal.relative(value)

    def test_tree_casefold_collision_is_rejected(self):
        blob = fixture.git(self.root, "rev-parse", "HEAD:source.txt")
        fixture.git(self.root, "update-index", "--add", "--cacheinfo", "100644", blob, "SOURCE.txt")
        fixture.git(self.root, "commit", "--quiet", "-m", "case collision fixture")
        with self.assertRaisesRegex(RuntimeError, "collision"):
            self.pack("diagnostic_dirty", {"superproject": ["SOURCE.txt"]})

    def test_normalized_directory_aliases_are_rejected(self):
        seal = self.api()
        for paths in (["A/one", "a/two"], ["caf\u00e9/one", "cafe\u0301/two"]):
            with self.subTest(paths=paths), self.assertRaisesRegex(RuntimeError, "collision"):
                seal.allowlist({"compiler": paths})

    def test_overlay_cannot_alias_tracked_path(self):
        seal, package, digest = self.pack()
        receipt = json.loads((package / "receipt.json").read_bytes())
        receipt["mode"] = "diagnostic_dirty"
        receipt["repositories"]["compiler"]["files"] = [{"path": "SOURCE.txt", "kind": "tracked", "sha256": None, "executable": False}]
        # Validation must detect the alias against the bundle tree before any
        # overlay is applied, even when the receipt is freshly authenticated.
        data = (json.dumps(receipt, sort_keys=True, separators=(",", ":")) + "\n").encode()
        (package / "receipt.json").write_bytes(data)
        with self.assertRaisesRegex(RuntimeError, "collision"):
            seal.verify(package, hashlib.sha256(data).hexdigest(), {"compiler": ["SOURCE.txt"]}, purpose="diagnostic")

    def test_undeclared_gitlink_is_rejected(self):
        target = fixture.git(self.compiler, "rev-parse", "HEAD")
        fixture.git(self.compiler, "update-index", "--add", "--cacheinfo", "160000", target, "undeclared")
        fixture.git(self.compiler, "commit", "--quiet", "-m", "undeclared gitlink fixture")
        fixture.commit(self.root)
        with self.assertRaisesRegex(RuntimeError, "gitlink|topology"):
            self.pack()

    def test_unrelated_root_gitlink_is_preserved_without_checkout(self):
        fixture.git(self.root, "submodule", "add", "--quiet", str(self.base / "seeds/corelib"), "unrelated")
        fixture.commit(self.root)
        (self.root / "unrelated/source.txt").write_text("unselected dirty data")
        seal, package, digest = self.pack()
        destination = self.base / "restored"
        seal.restore(package, digest, destination, {}, purpose="matrix")
        self.assertFalse((destination / "unrelated/.git").exists())
        self.assertEqual(fixture.git(destination, "ls-tree", "HEAD", "unrelated"),
                         fixture.git(self.root, "ls-tree", "HEAD", "unrelated"))

    def test_tracked_deletion_roundtrip(self):
        (self.compiler / "source.txt").unlink()
        allow = {"compiler": ["source.txt"]}
        seal, package, digest = self.pack("diagnostic_dirty", allow)
        destination = self.base / "restored"
        seal.restore(package, digest, destination, allow, purpose="diagnostic")
        self.assertFalse((destination / "compiler/source.txt").exists())

    def test_dirty_roundtrip_preserves_binary_and_untracked_bytes(self):
        (self.compiler / "source.txt").write_bytes(b"changed\x00binary\xff")
        (self.compiler / "new.txt").write_text("new")
        allow = {"compiler": ["source.txt", "new.txt"]}
        seal, package, digest = self.pack("diagnostic_dirty", allow)
        destination = self.base / "restored"
        seal.restore(package, digest, destination, allow, purpose="diagnostic")
        self.assertEqual((destination / "compiler/source.txt").read_bytes(), b"changed\x00binary\xff")
        self.assertEqual((destination / "compiler/new.txt").read_text(), "new")
        with self.assertRaisesRegex(RuntimeError, "diagnostic|dirty"):
            discover_source_closure(destination / "compiler")

    def test_dirty_cannot_be_matrix_evidence(self):
        seal, package, digest = self.pack("diagnostic_dirty")
        with self.assertRaisesRegex(RuntimeError, "matrix"):
            seal.verify(package, digest, {}, purpose="matrix")

    def test_unapproved_worktree_change_rejected(self):
        (self.compiler / "source.txt").write_text("changed")
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            self.pack("diagnostic_dirty")

    def test_symlink_overlay_rejected(self):
        (self.compiler / "new.txt").symlink_to(self.compiler / "source.txt")
        with self.assertRaisesRegex(RuntimeError, "regular|symlink"):
            self.pack("diagnostic_dirty", {"compiler": ["new.txt"]})

    def test_changed_bundle_digest_rejected(self):
        seal, package, digest = self.pack()
        bundle = next(package.glob("*.bundle"))
        bundle.write_bytes(bundle.read_bytes() + b"tamper")
        with self.assertRaisesRegex(RuntimeError, "digest"):
            seal.verify(package, digest, {}, purpose="matrix")

    def test_receipt_digest_rejected(self):
        seal, package, digest = self.pack()
        receipt = package / "receipt.json"
        receipt.write_bytes(receipt.read_bytes() + b" ")
        with self.assertRaisesRegex(RuntimeError, "digest"):
            seal.verify(package, digest, {}, purpose="matrix")

    def test_consumer_allowlist_is_independent(self):
        (self.compiler / "source.txt").write_text("changed")
        seal, package, digest = self.pack("diagnostic_dirty", {"compiler": ["source.txt"]})
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            seal.verify(package, digest, {}, purpose="diagnostic")

    def test_malformed_records_and_gitlinks_rejected_even_with_fresh_digest(self):
        seal, package, digest = self.pack()
        receipt = package / "receipt.json"
        original = receipt.read_bytes()
        for mutation in ("gitlink", "extra", "path"):
            value = json.loads(original)
            if mutation == "gitlink":
                value["revisions"]["gitlinks"]["compiler"] = "f" * 40
            elif mutation == "extra":
                value["surprise"] = True
            else:
                value["repositories"]["compiler"]["bundle"]["path"] = "../escape"
            receipt.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n")
            forged_digest = hashlib.sha256(receipt.read_bytes()).hexdigest()
            with self.subTest(mutation=mutation), self.assertRaises(RuntimeError):
                seal.verify(package, forged_digest, {}, purpose="matrix")

    def test_unlisted_package_files_and_symlinks_rejected(self):
        seal, package, digest = self.pack()
        extra = package / "unlisted"
        extra.write_text("unexpected")
        with self.assertRaisesRegex(RuntimeError, "inventory"):
            seal.verify(package, digest, {}, purpose="matrix")
        extra.unlink()
        extra.symlink_to(package / "receipt.json")
        with self.assertRaisesRegex(RuntimeError, "regular|symlink"):
            seal.verify(package, digest, {}, purpose="matrix")

    def test_nested_untracked_file_roundtrip(self):
        (self.compiler / "new-dir").mkdir()
        (self.compiler / "new-dir/source.txt").write_text("nested")
        allow = {"compiler": ["new-dir/source.txt"]}
        seal, package, digest = self.pack("diagnostic_dirty", allow)
        destination = self.base / "restored"
        seal.restore(package, digest, destination, allow, purpose="diagnostic")
        self.assertEqual((destination / "compiler/new-dir/source.txt").read_text(), "nested")

    def test_committed_symlink_is_rejected(self):
        (self.root / "unsafe").symlink_to("source.txt")
        fixture.commit(self.root)
        with self.assertRaisesRegex(RuntimeError, "symlink|regular"):
            self.pack()

    def test_rehashed_patch_cannot_modify_unapproved_file(self):
        (self.compiler / "source.txt").write_text("allowed")
        seal, package, digest = self.pack("diagnostic_dirty", {"compiler": ["source.txt"]})
        (self.compiler / "runtime_manifest.bsol").write_text("unapproved")
        patch = fixture.git(self.compiler, "diff", "HEAD", "--binary", "--full-index", "--no-renames") + "\n"
        receipt_file = package / "receipt.json"
        receipt = json.loads(receipt_file.read_bytes())
        record = receipt["repositories"]["compiler"]["patch"]
        (package / record["path"]).write_text(patch)
        record["sha256"] = hashlib.sha256(patch.encode()).hexdigest()
        receipt_file.write_text(json.dumps(receipt, sort_keys=True, separators=(",", ":")) + "\n")
        digest = hashlib.sha256(receipt_file.read_bytes()).hexdigest()
        with self.assertRaisesRegex(RuntimeError, "allowlist"):
            seal.restore(package, digest, self.base / "rejected", {"compiler": ["source.txt"]}, purpose="diagnostic")
        self.assertFalse((self.base / "rejected").exists())


if __name__ == "__main__":
    unittest.main()

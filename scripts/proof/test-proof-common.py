#!/usr/bin/env python3
"""Unit tests for scripts/proof/_proof_common.py. Run: python3 -I scripts/proof/test-proof-common.py"""
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _proof_common as common  # noqa: E402


class Classify(unittest.TestCase):
    def setUp(self):
        self.patterns = common.ice_patterns()

    def test_clean_output_has_no_signature(self):
        self.assertEqual(common.classify("error[E1101]: unknown name `x`", 1, False, self.patterns), [])

    def test_internal_codes_and_texts_are_found(self):
        self.assertIn("E2101", common.classify("error[E2101]: semantic fact unavailable", 1, False, self.patterns))
        self.assertIn("E2102", common.classify("E2102 lowering rule missing", 1, False, self.patterns))
        self.assertIn("E4041", common.classify("E4041 Invalid build request", 1, False, self.patterns))
        self.assertIn("MissingRuleOrFact", common.classify("MissingRuleOrFact at X", 1, False, self.patterns))
        self.assertIn("rust panic", common.classify("thread 'main' panicked at src/x.rs:1", 101, False, self.patterns))

    def test_timeout_and_signal(self):
        self.assertEqual(common.classify("", None, True, self.patterns), ["timeout"])
        self.assertIn("killed by signal 11", common.classify("", -11, False, self.patterns))

    def test_custom_pattern(self):
        patterns = common.ice_patterns(["E9999"])
        self.assertTrue(any(s.startswith("custom:") for s in common.classify("E9999 internal", 1, False, patterns)))

    def test_excerpt_centres_on_the_match(self):
        text = "a\nb\nerror E2101 here\nd\ne\nf"
        self.assertIn("E2101", common.excerpt(text, self.patterns))


class Discovery(unittest.TestCase):
    def test_projects_skip_build_output_and_templates(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "a").mkdir()
            (root / "a" / "a.bproj").write_text("")
            (root / "a" / "obj" / "x").mkdir(parents=True)
            (root / "a" / "obj" / "x" / "x.bproj").write_text("")
            (root / "t").mkdir()
            (root / "t" / "{{name}}.bproj").write_text("")
            (root / "a" / "src").mkdir()
            (root / "a" / "src" / "Main.bd").write_text("")
            self.assertEqual(common.discover_projects(root), [root / "a"])
            self.assertEqual(common.owning_project(root / "a" / "src" / "Main.bd", root), root / "a")
            self.assertIsNone(common.owning_project(root / "t" / "Main.bd", root))

    def test_template_list_parsing(self):
        text = (
            "Installed templates:\n"
            "  app — Application (Project) [Bundled] —\n"
            "  lib — Library (Project) [Installed] pkg@1.0.0\n"
            "  ws — Workspace (Workspace) [Bundled]\n"
            "  item — Contract (Item) [Bundled]\n"
            "noise\n"
        )
        self.assertEqual(common.parse_template_list(text), [("app", "Project"), ("lib", "Project"), ("ws", "Workspace")])


if __name__ == "__main__":
    unittest.main()

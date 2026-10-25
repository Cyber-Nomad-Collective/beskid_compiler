#!/usr/bin/env python3
"""Unit tests for scripts/proof/_proof_common.py. Run: python3 -I scripts/proof/test-proof-common.py"""
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import _proof_common as common  # noqa: E402
import ice_sweep  # noqa: E402
import onboarding_check  # noqa: E402


class Classify(unittest.TestCase):
    def setUp(self):
        self.patterns = common.ice_patterns()

    def test_clean_output_has_no_signature(self):
        self.assertEqual(common.classify("error[E1101]: unknown name `x`", 1, False, self.patterns), [])

    def test_internal_codes_and_texts_are_found(self):
        self.assertIn("E21xx internal diagnostic", common.classify("error[E2101]: semantic fact unavailable", 1, False, self.patterns))
        self.assertIn("E21xx internal diagnostic", common.classify("E2102 lowering rule missing", 1, False, self.patterns))
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


class InternalFamily(unittest.TestCase):
    def test_whole_e21xx_family_is_internal(self):
        patterns = common.ice_patterns()
        for code in ("E2103", "E2106", "E2107"):
            self.assertIn("E21xx internal diagnostic", common.classify(f"error[{code}]: x", 1, False, patterns), code)
        self.assertEqual(common.classify("error[E2201]: x and E21 and E21000", 1, False, patterns), [])


class ProcessGroups(unittest.TestCase):
    def test_timeout_kills_grandchildren(self):
        with tempfile.TemporaryDirectory() as tmp:
            marker = Path(tmp) / "alive"
            script = f"(sleep 2; touch {marker}) & sleep 30"
            result = common.run(["sh", "-c", script], timeout=1)
            self.assertTrue(result.timed_out)
            time.sleep(3)
            self.assertFalse(marker.exists(), "grandchild survived the timeout")

    def test_output_is_capped(self):
        result = common.run(["sh", "-c", "head -c 9000000 /dev/zero | tr '\\0' a"], timeout=60)
        self.assertLess(len(result.output), common.MAX_OUTPUT_BYTES + 100)
        self.assertIn("[output truncated]", result.output)

    def test_missing_binary_is_127(self):
        self.assertEqual(common.run(["/nonexistent-binary-xyz"]).returncode, 127)


class NoTestsDetection(unittest.TestCase):
    def test_ten_tests_failing_is_not_skipped(self):
        patterns = common.ice_patterns()
        failing = common.RunResult(["t"], 1, "ran 10 tests, 3 failed", 0.1, False)
        self.assertEqual(onboarding_check.step_status("test", failing, patterns)[0], "FAIL")
        zero = common.RunResult(["t"], 1, "0 tests", 0.1, False)
        self.assertEqual(onboarding_check.step_status("test", zero, patterns)[0], "skipped (no tests)")

    def test_restricted_path_keeps_bash_dir(self):
        path = onboarding_check.restricted_path(Path("/p"))
        self.assertTrue(path.startswith("/p/bin"))
        import shutil
        self.assertIn(str(Path(shutil.which("bash")).parent), path.split(os.pathsep))


class SweepSetup(unittest.TestCase):
    def test_explicitly_requested_missing_corpus_exits_2(self):
        with tempfile.TemporaryDirectory() as tmp:
            code = ice_sweep.main(["--beskid", "/bin/true", "--mutate", "/bin/true", "--compiler", tmp,
                                   "--corpora", "lings", "--work-dir", tmp + "/w"])
            self.assertEqual(code, 2)

    def test_new_failure_is_classified(self):
        class Args:
            beskid = "/nonexistent"
            timeout = 5
        corpus = ice_sweep.Corpus("templates")
        original = common.discover_templates
        common.discover_templates = lambda beskid, env=None: ([("app", "Project")], common.RunResult(["l"], 0, "", 0, False))
        orig_run = common.run
        common.run = lambda cmd, **kw: common.RunResult(cmd, 101, "thread 'main' panicked at x", 0.1, False)
        try:
            with tempfile.TemporaryDirectory() as tmp:
                ice_sweep.generate_templates(Args, Path(tmp), corpus, common.ice_patterns())
        finally:
            common.discover_templates = original
            common.run = orig_run
        self.assertEqual(corpus.new_ice, 1)
        self.assertIn("rust panic", corpus.first)


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

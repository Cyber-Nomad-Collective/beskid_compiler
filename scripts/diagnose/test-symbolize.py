#!/usr/bin/env python3
"""Unit tests for symbolize.py's pure-parsing parts: node-key extraction (KEY_RE) and the
line-unwrapping this CLI's log wrap requires (unwrap()). No builder/repo access needed."""
import importlib.util
import pathlib
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("symbolize", HERE / "symbolize.py")
symbolize = importlib.util.module_from_spec(spec)
spec.loader.exec_module(symbolize)


class UnwrapTests(unittest.TestCase):
    def test_no_continuation_lines_unchanged(self):
        text = "line one\nline two\n"
        self.assertEqual(symbolize.unwrap(text), "line one\nline two")

    def test_single_pipe_continuation_joins_with_previous_line(self):
        text = "corelib_http-\n  | d374f49909110c78/src/Http/Codec.bd\n"
        self.assertEqual(
            symbolize.unwrap(text),
            "corelib_http-d374f49909110c78/src/Http/Codec.bd",
        )

    def test_multiple_pipes_still_join(self):
        text = "start\n| middle\n|| end\n"
        self.assertEqual(symbolize.unwrap(text), "startmiddleend")

    def test_continuation_with_no_preceding_line_is_kept_as_its_own_line(self):
        # A `|`-prefixed line with nothing before it (e.g. truncated input) has nothing to join
        # onto, so it is kept rather than silently dropped.
        text = "| orphan continuation\n"
        self.assertEqual(symbolize.unwrap(text), "| orphan continuation")


class KeyRegexTests(unittest.TestCase):
    def test_matches_key_with_construct_and_span(self):
        text = "MissingRuleOrFact at .../Codec.bd#g1:n1538 AssignExpression@22:117-22:154"
        matches = list(symbolize.KEY_RE.finditer(text))
        self.assertEqual(len(matches), 1)
        m = matches[0]
        self.assertEqual(m.group("path"), ".../Codec.bd")
        self.assertEqual(m.group("gen"), "1")
        self.assertEqual(m.group("node"), "1538")
        self.assertEqual(m.group("construct"), "AssignExpression")
        self.assertEqual((m.group("l1"), m.group("c1"), m.group("l2"), m.group("c2")),
                          ("22", "117", "22", "154"))

    def test_matches_key_without_construct(self):
        text = "some/path/File.bd#g3:n42"
        matches = list(symbolize.KEY_RE.finditer(text))
        self.assertEqual(len(matches), 1)
        self.assertIsNone(matches[0].group("construct"))

    def test_matches_rs_extension_too(self):
        text = "crates/beskid_isle/src/context/foo.rs#g2:n9"
        matches = list(symbolize.KEY_RE.finditer(text))
        self.assertEqual(len(matches), 1)
        self.assertTrue(matches[0].group("path").endswith("foo.rs"))

    def test_no_match_on_plain_text(self):
        text = "this line has no diagnostic key in it at all"
        self.assertEqual(list(symbolize.KEY_RE.finditer(text)), [])

    def test_dedup_key_ignores_raw_path_prefix_differences(self):
        # Two occurrences of the same gen/node/construct/span should be treated as duplicates by
        # main()'s dedup logic even if surrounding text differs -- verify the regex captures the
        # same group values so that logic (tested at the module boundary) has something to work
        # with; this test locks the groups the dedup key is built from.
        text_a = "foo.bd#g1:n1 X@1:1-1:2"
        text_b = "totally/different/foo.bd#g1:n1 X@1:1-1:2"
        ma = next(symbolize.KEY_RE.finditer(text_a))
        mb = next(symbolize.KEY_RE.finditer(text_b))
        key_a = (ma.group("gen"), ma.group("node"), ma.group("construct"), ma.group("l1"), ma.group("c1"), ma.group("l2"), ma.group("c2"))
        key_b = (mb.group("gen"), mb.group("node"), mb.group("construct"), mb.group("l1"), mb.group("c1"), mb.group("l2"), mb.group("c2"))
        self.assertEqual(key_a, key_b)


class DefaultRepoRootTests(unittest.TestCase):
    def test_default_repo_root_is_two_levels_above_this_script(self):
        # scripts/diagnose/symbolize.py -> repo root is scripts/diagnose/../..
        expected = (HERE / ".." / "..").resolve()
        self.assertEqual(pathlib.Path(symbolize.default_repo_root()).resolve(), expected)

    def test_env_var_override_wins(self):
        import os
        from unittest.mock import patch
        with patch.dict(os.environ, {"BESKID_DIAG_REPO_ROOT": "/tmp/some-other-repo"}):
            self.assertEqual(symbolize.default_repo_root(), "/tmp/some-other-repo")


if __name__ == "__main__":
    unittest.main()

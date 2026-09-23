#!/usr/bin/env python3
"""Unit tests for whyfact.py's pure-parsing parts: extracting the unavailable query name from a
diagnostic message (QUERY_IN_MSG_RE), extracting the query name from a
`SemanticError::unavailable("...")` call site (UNAVAILABLE_CALL_RE), and locating the nearest
enclosing `fn` above a failure line (enclosing_fn). No builder/repo access needed -- enclosing_fn
is exercised against a temp file."""
import importlib.util
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("whyfact", HERE / "whyfact.py")
whyfact = importlib.util.module_from_spec(spec)
spec.loader.exec_module(whyfact)


class QueryInMessageTests(unittest.TestCase):
    def test_extracts_query_name_from_unavailable_message(self):
        text = "error: semantic query `source_expression_type` is unavailable for this node"
        matches = whyfact.QUERY_IN_MSG_RE.findall(text)
        self.assertEqual(matches, ["source_expression_type"])

    def test_extracts_multiple_distinct_queries(self):
        text = (
            "semantic query `alpha_fact` is unavailable\n"
            "semantic query `beta_fact` is unavailable\n"
        )
        matches = whyfact.QUERY_IN_MSG_RE.findall(text)
        self.assertEqual(matches, ["alpha_fact", "beta_fact"])

    def test_no_match_on_unrelated_text(self):
        text = "semantic query succeeded"
        self.assertEqual(whyfact.QUERY_IN_MSG_RE.findall(text), [])

    def test_requires_backticks_around_name(self):
        text = "semantic query source_expression_type is unavailable"
        self.assertEqual(whyfact.QUERY_IN_MSG_RE.findall(text), [])


class UnavailableCallSiteTests(unittest.TestCase):
    def test_extracts_name_from_call_site(self):
        line = '        return Err(SemanticError::unavailable("collection_owner"));'
        matches = whyfact.UNAVAILABLE_CALL_RE.findall(line)
        self.assertEqual(matches, ["collection_owner"])

    def test_no_match_without_the_exact_call(self):
        line = "        return Err(SemanticError::other(\"collection_owner\"));"
        self.assertEqual(whyfact.UNAVAILABLE_CALL_RE.findall(line), [])


class EnclosingFnTests(unittest.TestCase):
    def test_finds_nearest_fn_above_line(self):
        with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as f:
            f.write(
                "fn outer() {\n"
                "    fn inner_helper(x: u32) -> u32 {\n"
                "        if x == 0 {\n"
                "            return Err(SemanticError::unavailable(\"q\"));\n"
                "        }\n"
                "        x\n"
                "    }\n"
                "}\n"
            )
            path = f.name
        found = whyfact.enclosing_fn(path, 4)
        self.assertIsNotNone(found)
        fn_name, fn_line = found
        self.assertEqual(fn_name, "inner_helper")
        self.assertEqual(fn_line, 2)

    def test_returns_none_when_no_fn_above(self):
        with tempfile.NamedTemporaryFile("w", suffix=".rs", delete=False) as f:
            f.write("const X: u32 = 1;\nconst Y: u32 = 2;\n")
            path = f.name
        self.assertIsNone(whyfact.enclosing_fn(path, 2))

    def test_missing_file_returns_none(self):
        self.assertIsNone(whyfact.enclosing_fn("/nonexistent/path/does/not/exist.rs", 5))


if __name__ == "__main__":
    unittest.main()

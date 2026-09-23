#!/usr/bin/env python3
"""Unit tests for visibility.py's declaration/pub detection against a synthetic
runtime/beskid/src tree -- no real repo or builder access needed."""
import importlib.util
import pathlib
import tempfile
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("visibility", HERE / "visibility.py")
visibility = importlib.util.module_from_spec(spec)
spec.loader.exec_module(visibility)


class ModulePathForTests(unittest.TestCase):
    def test_infers_dot_joined_module_path_under_src(self):
        repo = "/repo"
        file_path = "/repo/runtime/beskid/src/Runtime/Fiber/Scheduler.bd"
        self.assertEqual(visibility.module_path_for(repo, file_path), "Runtime.Fiber.Scheduler")

    def test_infers_module_path_without_src_segment(self):
        repo = "/repo"
        file_path = "/repo/somewhere/Foo/Bar.bd"
        self.assertEqual(visibility.module_path_for(repo, file_path), "somewhere.Foo.Bar")


class FindDeclarationsTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = pathlib.Path(self.tmp.name)
        self.runtime_src = self.repo / "runtime" / "beskid" / "src"
        self.runtime_src.mkdir(parents=True)

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, rel_path: str, content: str):
        p = self.runtime_src / rel_path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)
        return p

    def test_pub_function_declaration_is_detected_as_pub(self):
        self.write("Runtime/Fiber/Scheduler.bd", "pub u32 CurrentFiber() {\n    0\n}\n")
        decls = visibility.find_declarations(str(self.repo), "CurrentFiber")
        self.assertEqual(len(decls), 1)
        self.assertTrue(decls[0]["pub"])
        self.assertEqual(decls[0]["module"], "Runtime.Fiber.Scheduler")

    def test_private_function_declaration_is_detected_as_not_pub(self):
        self.write("Runtime/Fiber/Scheduler.bd", "u32 InternalHelper() {\n    0\n}\n")
        decls = visibility.find_declarations(str(self.repo), "InternalHelper")
        self.assertEqual(len(decls), 1)
        self.assertFalse(decls[0]["pub"])

    def test_pub_const_declaration_is_detected(self):
        self.write("Runtime/Consts.bd", "pub const MaxFibers = 64;\n")
        decls = visibility.find_declarations(str(self.repo), "MaxFibers")
        self.assertEqual(len(decls), 1)
        self.assertTrue(decls[0]["pub"])

    def test_statement_keyword_using_symbol_name_is_not_a_declaration(self):
        # e.g. a symbol named "for" or a call that happens to start with a statement keyword must
        # not be mistaken for a declaration of that keyword.
        self.write("Runtime/Loop.bd", "for (mut i = 0; i < 10; i = i + 1) {\n    DoWork(i);\n}\n")
        decls = visibility.find_declarations(str(self.repo), "DoWork")
        self.assertEqual(decls, [])

    def test_no_declaration_found_returns_empty_list(self):
        self.write("Runtime/Other.bd", "pub u32 SomethingElse() {\n    0\n}\n")
        decls = visibility.find_declarations(str(self.repo), "DoesNotExist")
        self.assertEqual(decls, [])

    def test_multiple_declarations_across_files_all_collected(self):
        self.write("A.bd", "pub u32 Shared() {\n    1\n}\n")
        self.write("B.bd", "u32 Shared() {\n    2\n}\n")
        decls = visibility.find_declarations(str(self.repo), "Shared")
        self.assertEqual(len(decls), 2)
        pub_states = sorted(d["pub"] for d in decls)
        self.assertEqual(pub_states, [False, True])


if __name__ == "__main__":
    unittest.main()

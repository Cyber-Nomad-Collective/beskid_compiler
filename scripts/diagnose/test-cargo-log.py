#!/usr/bin/env python3
"""Tests for cargo-log.py."""

import importlib.util
import pathlib
import unittest


SCRIPT = pathlib.Path(__file__).with_name("cargo-log.py")
SPEC = importlib.util.spec_from_file_location("cargo_log", SCRIPT)
cargo_log = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(cargo_log)


class CargoLogTests(unittest.TestCase):
    def test_complete_and_active_binaries(self):
        log = """     Running tests\\first.rs (target\\debug\\deps\\first.exe)
running 2 tests
test one ... ok
test two ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
     Running tests\\second.rs (target\\debug\\deps\\second.exe)
running 1 test
test stuck has been running for over 60 seconds
"""
        result = cargo_log.summarize(log)
        self.assertEqual(result["complete"], [("first.rs", 2, 0)])
        self.assertEqual(result["active"], "second.rs")
        self.assertEqual(result["failures"], [])

    def test_failure_and_ansi(self):
        log = """\x1b[31m     Running tests\\broken.rs (target\\debug\\deps\\broken.exe)\x1b[0m
test one ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
error: test failed, to rerun pass `-p beskid_engine --test broken`
"""
        result = cargo_log.summarize(log)
        self.assertEqual(result["complete"], [("broken.rs", 0, 1)])
        self.assertEqual(result["failures"], ["test one ... FAILED"])
        self.assertIsNone(result["active"])


if __name__ == "__main__":
    unittest.main()

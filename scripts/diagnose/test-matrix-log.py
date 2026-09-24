#!/usr/bin/env python3
"""Tests for the corelib/runtime matrix log summarizer."""

import importlib.util
import pathlib
import unittest


HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("matrix_log", HERE / "matrix-log.py")
matrix_log = importlib.util.module_from_spec(spec)
spec.loader.exec_module(matrix_log)


class MatrixSummaryTests(unittest.TestCase):
    def test_finished_failed_matrix_reports_counts_and_first_error(self):
        log = (
            "Running NetworkTcpTests... Generate CLIF\n"
            "  x unknown value `__panic`\n"
            "\x1b[32mPASS (6.9s)\x1b[0m\n"
            "matrix: 0/80 passed, 80/80 failed\n"
            "release eligible: false\n"
        )
        summary = matrix_log.summarize(log)
        self.assertEqual(summary["matrix"], (0, 80))
        self.assertFalse(summary["release_eligible"])
        self.assertEqual(summary["last_target"], "NetworkTcpTests")
        self.assertEqual(summary["diagnostics"], ["unknown value `__panic`"])
        self.assertEqual(summary["pass_markers"], 1)

    def test_in_progress_matrix_never_invents_a_final_count(self):
        log = "Running LifecycleTests... Generate CLIF\nRunning GcTests... Generate CLIF\n"
        summary = matrix_log.summarize(log)
        self.assertIsNone(summary["matrix"])
        self.assertIsNone(summary["release_eligible"])
        self.assertEqual(summary["last_target"], "GcTests")
        self.assertEqual(summary["pass_markers"], 0)

    def test_repeated_diagnostics_are_deduplicated_without_losing_order(self):
        log = "  x unknown value `__panic`\n  x unknown value `__panic`\nerror: kit mismatch\n"
        self.assertEqual(matrix_log.summarize(log)["diagnostics"], ["unknown value `__panic`", "kit mismatch"])

    def test_warning_headline_is_not_called_an_error(self):
        log = "  x unused import `Core.Syscall`\nmatrix: 80/80 passed, 0/80 failed\n"
        summary = matrix_log.summarize(log)
        self.assertEqual(summary["matrix"], (80, 80))
        self.assertEqual(summary["diagnostics"], ["unused import `Core.Syscall`"])


if __name__ == "__main__":
    unittest.main()

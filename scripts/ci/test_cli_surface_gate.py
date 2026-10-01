import importlib.util
import base64
import os
import sys
import tempfile
from pathlib import Path
import unittest


MODULE = Path(__file__).with_name("cli_surface_gate.py")


class CliSurfaceGateTests(unittest.TestCase):
    def load_gate(self):
        spec = importlib.util.spec_from_file_location("cli_surface_gate", MODULE)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module

    def test_command_names_only_come_from_clap_commands_section(self):
        gate = self.load_gate()
        help_text = """Beskid CLI tool

Usage: beskid <COMMAND>

Commands:
  dev    Developer tools
  graph  Graph output
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
"""
        self.assertEqual(gate.command_names(help_text), ["dev", "graph"])

    def test_unclassified_advertised_leaf_fails_closed(self):
        gate = self.load_gate()
        self.assertEqual(gate.classify(("graph",)), "smoke")
        self.assertEqual(gate.classify(("lock",)), "smoke")
        self.assertEqual(gate.classify(("up", "list")), "smoke")
        self.assertEqual(gate.classify(("up", "check")), "smoke")
        self.assertEqual(gate.classify(("pckg", "upload")), "setup_skip")
        self.assertEqual(gate.classify(("dev", "package", "registry", "upload")), "setup_skip")
        self.assertEqual(gate.classify(("validate-bsol",)), "smoke")
        self.assertEqual(gate.classify(("build",)), "smoke")
        self.assertEqual(gate.classify(("dev", "build", "test")), "smoke")
        self.assertEqual(gate.classify(("pckg", "pack")), "smoke")
        self.assertEqual(gate.classify(("pckg", "list")), "smoke")
        self.assertEqual(gate.classify(("dev", "package", "registry", "search")), "smoke")
        self.assertEqual(gate.classify(("future-command",)), "uncovered")

    def test_uncovered_leaf_is_release_failure(self):
        gate = self.load_gate()
        self.assertEqual(gate.release_failures([{"path": "future-command", "status": "uncovered"}]),
                         ["future-command:uncovered"])

    def test_source_provenance_is_explicitly_unverified(self):
        gate = self.load_gate()
        provenance = gate.source_provenance()
        self.assertEqual(provenance["status"], "unverified")
        self.assertIsNone(provenance["commit"])
        self.assertTrue(provenance["external_receipt_required"])

    def test_alias_artifacts_have_distinct_paths(self):
        gate = self.load_gate()
        root = Path("/tmp/cli-gate-fixture")
        common = (root / "Src/Smoke.bd", root / "Smoke.bproj", root / "Test.bproj",
                  root / "Migration.bproj", root, "http://127.0.0.1:1")
        direct, _, _ = gate.smoke_args(("build",), *common)
        alias, _, _ = gate.smoke_args(("dev", "build", "compile"), *common)
        self.assertNotEqual(direct[-2], alias[-2])

    def test_output_evidence_preserves_cr_and_exact_bytes(self):
        gate = self.load_gate()
        evidence = gate.output_evidence(b"one\r\ntwo\n", b"warning\n")
        self.assertEqual(evidence["control_bytes"], [13])
        self.assertEqual(base64.b64decode(evidence["stdout_base64"]), b"one\r\ntwo\n")
        self.assertEqual(base64.b64decode(evidence["stderr_base64"]), b"warning\n")

    def test_graph_tui_accepts_terminal_box_render_without_alt_screen(self):
        gate = self.load_gate()
        self.assertTrue(gate.graph_tui_rendered("┌─────┐\r\n│Smoke│".encode()))
        self.assertFalse(gate.graph_tui_rendered(b"flowchart TD\nSmoke"))
        self.assertFalse(gate.graph_tui_rendered(b"Smoke\x1b[?1049h"))

    def test_ordinary_pty_rejects_terminal_controls(self):
        gate = self.load_gate()
        self.assertTrue(gate.ordinary_pty_clean(b"Analyze complete\r\n"))
        self.assertFalse(gate.ordinary_pty_clean(b"\x1b[?1049hAnalyze complete"))
        self.assertFalse(gate.ordinary_pty_clean(b"Analyze complete\x07"))

    @unittest.skipUnless(os.name == "posix", "PTY is POSIX-only")
    def test_pty_transcript_records_render_and_quit(self):
        gate = self.load_gate()
        with tempfile.TemporaryDirectory() as root:
            transcript = gate.run_pty(sys.executable, ["-c", "import sys; print('\\x1b[?1049hSmoke', flush=True); sys.stdin.read(1); print('\\x1b[?1049l', flush=True)"],
                                      dict(os.environ, TERM="xterm-256color"), root)
        self.assertEqual(transcript["exit"], 0)
        self.assertFalse(transcript["timed_out"])
        self.assertIn(b"\x1b[?1049hSmoke", base64.b64decode(transcript["transcript_base64"]))


if __name__ == "__main__":
    unittest.main()

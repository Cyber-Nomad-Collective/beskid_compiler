import importlib.util
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
        self.assertEqual(gate.classify(("pckg", "upload")), "setup_skip")
        self.assertEqual(gate.classify(("dev", "package", "registry", "upload")), "setup_skip")
        self.assertEqual(gate.classify(("validate-bsol",)), "smoke")
        self.assertEqual(gate.classify(("future-command",)), "uncovered")


if __name__ == "__main__":
    unittest.main()

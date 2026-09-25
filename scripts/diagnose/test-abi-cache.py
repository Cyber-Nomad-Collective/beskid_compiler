#!/usr/bin/env python3
"""Contract-variant detection without a real Cargo target directory."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("abi-cache.py")


def contract(parameter_count: int) -> str:
    source = {"intrinsics": [{"name": "network_report_leak", "params": [{}] * parameter_count}]}
    return f'pub const ABI_V5_SOURCE_JSON: &str = r#"{json.dumps(source)}"#;\n'


class AbiCacheTests(unittest.TestCase):
    def test_reports_different_cached_signature_without_claiming_active_linkage(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tracked = root / "crates/beskid_abi/src/generated/abi_v5_contract.rs"
            tracked.parent.mkdir(parents=True)
            tracked.write_text(contract(7))
            cached = root / "target/debug/build/beskid_abi-old/out/abi_v5_contract.rs"
            cached.parent.mkdir(parents=True)
            cached.write_text(contract(4))
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--target-dir", str(root / "target")],
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("tracked:", result.stdout)
            self.assertIn("network_report_leak_params=7", result.stdout)
            self.assertIn("different:", result.stdout)
            self.assertIn("network_report_leak_params=4", result.stdout)
            self.assertIn("does not identify which one a test binary linked", result.stdout)


if __name__ == "__main__":
    unittest.main()

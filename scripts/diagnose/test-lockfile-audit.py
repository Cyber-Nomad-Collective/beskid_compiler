#!/usr/bin/env python3
"""Regression tests for the read-only tracked-lock inventory."""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("lockfile-audit.py")


class LockfileAuditTests(unittest.TestCase):
    def test_classifies_tracked_locks_and_rejects_v1_on_request(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            source = root / "App"
            generated = source / "obj/beskid/deps/src/Lib"
            orphan = root / "Orphan"
            for project in (source, generated, orphan):
                project.mkdir(parents=True)
            (source / "App.bproj").write_text("App {}\n", encoding="utf-8")
            (generated / "Lib.bproj").write_text("Lib {}\n", encoding="utf-8")
            (source / "Project.lock").write_text("# Project.lock v1\n", encoding="utf-8")
            (generated / "Project.lock").write_text("# Project.lock v2\n", encoding="utf-8")
            (orphan / "Project.lock").write_text("# Project.lock v1\n", encoding="utf-8")
            (root / "Untracked").mkdir()
            (root / "Untracked/Project.lock").write_text("# Project.lock v1\n", encoding="utf-8")
            subprocess.run(["git", "-C", str(root), "add", "App", "Orphan"], check=True)

            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--json", "--require-v2"],
                text=True,
                capture_output=True,
                check=False,
            )

            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads(result.stdout)
            self.assertEqual(report["counts"], {"v1": 2, "v2": 1, "other": 0})
            self.assertEqual(report["categories"], {"source": 1, "generated": 1, "orphan": 1})
            self.assertEqual(
                [(entry["path"], entry["category"]) for entry in report["entries"]],
                [
                    ("App/Project.lock", "source"),
                    ("App/obj/beskid/deps/src/Lib/Project.lock", "generated"),
                    ("Orphan/Project.lock", "orphan"),
                ],
            )

            (source / "Project.lock").write_text("# Project.lock v2\n", encoding="utf-8")
            (orphan / "Project.lock").write_text("# Project.lock v2\n", encoding="utf-8")
            migrated = subprocess.run(
                [sys.executable, str(SCRIPT), "--repo", str(root), "--json", "--require-v2"],
                text=True,
                capture_output=True,
                check=False,
            )
            self.assertEqual(migrated.returncode, 0, migrated.stderr)
            self.assertEqual(json.loads(migrated.stdout)["counts"], {"v1": 0, "v2": 3, "other": 0})


if __name__ == "__main__":
    unittest.main()

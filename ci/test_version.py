"""Unit tests for CLI version resolution (``python -m ci.test_version``)."""

from __future__ import annotations

import unittest
from unittest.mock import patch

from ci import version as ver


class ResolveVersionTests(unittest.TestCase):
    def test_tag_push_strips_v_prefix(self) -> None:
        self.assertEqual(
            ver.resolve_version(
                github_ref="refs/tags/v0.2.0",
                github_ref_name="v0.2.0",
            ),
            "0.2.0",
        )

    def test_main_with_semver_tag_and_commits(self) -> None:
        with (
            patch.object(ver, "read_package_version", return_value="0.1.0"),
            patch.object(ver, "_latest_semver_tag", return_value="v0.1.0"),
            patch.object(ver, "_commits_since_tag", return_value=3),
        ):
            self.assertEqual(
                ver.resolve_version(github_ref="refs/heads/main"),
                "0.1.3",
            )

    def test_main_on_tag_commit_returns_tag_version(self) -> None:
        with (
            patch.object(ver, "read_package_version", return_value="0.1.0"),
            patch.object(ver, "_latest_semver_tag", return_value="v0.1.0"),
            patch.object(ver, "_commits_since_tag", return_value=0),
        ):
            self.assertEqual(
                ver.resolve_version(github_ref="refs/heads/main"),
                "0.1.0",
            )

    def test_main_without_tags_uses_run_number(self) -> None:
        with (
            patch.object(ver, "read_package_version", return_value="0.1.0"),
            patch.object(ver, "_latest_semver_tag", return_value=None),
        ):
            self.assertEqual(
                ver.resolve_version(
                    github_ref="refs/heads/main",
                    github_run_number="12",
                ),
                "0.1.12",
            )

    def test_main_without_tags_or_run_number_uses_cargo_base(self) -> None:
        with (
            patch.object(ver, "read_package_version", return_value="0.1.0"),
            patch.object(ver, "_latest_semver_tag", return_value=None),
        ):
            self.assertEqual(
                ver.resolve_version(github_ref="refs/heads/main"),
                "0.1.0",
            )


if __name__ == "__main__":
    unittest.main()

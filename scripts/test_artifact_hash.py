#!/usr/bin/env python3
"""Focused non-build regression tests for evidence artifact hashing."""

from __future__ import annotations

import hashlib
import os
import tempfile
import unittest
from pathlib import Path

from artifact_hash import artifact_hash_scheme, artifact_sha256, file_sha256


class ArtifactHashTests(unittest.TestCase):
    def test_regular_file_keeps_conventional_sha256(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            artifact = Path(temporary) / "evidence.tsv"
            content = b"app\tresult\nfreya-app\tpassed\n"
            artifact.write_bytes(content)
            self.assertEqual(file_sha256(artifact), hashlib.sha256(content).hexdigest())
            self.assertEqual(artifact_sha256(artifact), hashlib.sha256(content).hexdigest())
            self.assertEqual(artifact_hash_scheme(artifact), "file-sha256")

    def test_directory_hash_ignores_order_and_filesystem_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            first = Path(temporary) / "first.app"
            second = Path(temporary) / "second.app"

            (first / "Contents/MacOS").mkdir(parents=True)
            (first / "Contents/Resources/empty").mkdir(parents=True)
            (first / "Contents/Info.plist").write_bytes(b"plist")
            (first / "Contents/MacOS/demo").write_bytes(b"binary")
            (first / "Current").symlink_to("Contents")

            (second / "Contents/Resources/empty").mkdir(parents=True)
            (second / "Contents/MacOS").mkdir(parents=True)
            (second / "Contents/MacOS/demo").write_bytes(b"binary")
            (second / "Contents/Info.plist").write_bytes(b"plist")
            (second / "Current").symlink_to("Contents")

            os.chmod(first / "Contents/MacOS/demo", 0o755)
            os.chmod(second / "Contents/MacOS/demo", 0o600)
            os.utime(first / "Contents/Info.plist", (1, 1))
            os.utime(second / "Contents/Info.plist", (2, 2))

            self.assertEqual(artifact_sha256(first), artifact_sha256(second))
            self.assertEqual(artifact_hash_scheme(first), "tree-sha256-v1")

    def test_directory_hash_binds_content_path_and_type(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            baseline = Path(temporary) / "baseline.app"
            changed_content = Path(temporary) / "changed-content.app"
            changed_path = Path(temporary) / "changed-path.app"
            changed_type = Path(temporary) / "changed-type.app"
            for tree in (baseline, changed_content, changed_path, changed_type):
                tree.mkdir()

            (baseline / "entry").mkdir()
            (baseline / "payload").write_bytes(b"one")
            (changed_content / "entry").mkdir()
            (changed_content / "payload").write_bytes(b"two")
            (changed_path / "entry").mkdir()
            (changed_path / "renamed-payload").write_bytes(b"one")
            (changed_type / "entry").write_bytes(b"")
            (changed_type / "payload").write_bytes(b"one")

            digest = artifact_sha256(baseline)
            self.assertNotEqual(digest, artifact_sha256(changed_content))
            self.assertNotEqual(digest, artifact_sha256(changed_path))
            self.assertNotEqual(digest, artifact_sha256(changed_type))


if __name__ == "__main__":
    unittest.main()

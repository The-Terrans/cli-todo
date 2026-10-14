"""Offline tests for the binary release and Nix-flake packager."""
import base64
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package_release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
TAG = f"v{VERSION}"
REPOSITORY = "The-Terrans/cli-todo"


def fake_artifacts(directory, executable=True):
    directory.mkdir()
    for target in release.TARGETS.values():
        path = directory / f"cli-todo-{TAG}-{target}.tar.gz"
        payload = b"#!/bin/sh\nexit 0\n"
        info = tarfile.TarInfo("cli-todo")
        info.size = len(payload)
        info.mode = 0o755 if executable else 0o644
        with tarfile.open(path, "w:gz") as archive:
            archive.addfile(info, io.BytesIO(payload))


class ReleaseTests(unittest.TestCase):
    def test_release_tag_matches_cargo_and_repository_is_valid(self):
        self.assertEqual(release.validate_release(TAG, REPOSITORY), VERSION)
        for tag, repository in [(TAG + "-unexpected", REPOSITORY), (TAG, "../bad/repo"), (TAG, "../bad"), (TAG, "owner/..")]:
            with self.assertRaises(ValueError):
                release.validate_release(tag, repository)

    def test_packages_every_platform_with_correct_hashes_and_pinned_nixpkgs(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts, output = root / "artifacts", root / "dist"
            fake_artifacts(artifacts)
            flake = release.package_release(TAG, REPOSITORY, artifacts, output)
            metadata = json.loads((flake / "release.json").read_text())
            self.assertEqual(metadata["tag"], TAG)
            self.assertEqual(metadata["version"], VERSION)
            self.assertEqual(set(metadata["artifacts"]), set(release.TARGETS))
            for artifact in metadata["artifacts"].values():
                data = (output / artifact["file"]).read_bytes()
                expected = "sha256-" + base64.b64encode(hashlib.sha256(data).digest()).decode()
                self.assertEqual(artifact["hash"], expected)
            self.assertEqual((flake / "flake.lock").read_bytes(), (ROOT / ".nix/release/flake.lock").read_bytes())
            self.assertEqual((flake / "flake.nix").read_bytes(), (ROOT / ".nix/release/flake.nix").read_bytes())
            with tarfile.open(output / "cli-todo-flake.tar.gz") as archive:
                self.assertEqual(set(archive.getnames()), {"cli-todo-flake", "cli-todo-flake/flake.nix", "cli-todo-flake/flake.lock", "cli-todo-flake/release.json"})
            checksums = (output / "SHA256SUMS").read_text().splitlines()
            self.assertEqual(len(checksums), 5)
            for checksum in checksums:
                digest, name = checksum.split("  ")
                self.assertEqual(digest, hashlib.sha256((output / name).read_bytes()).hexdigest())
            # The workflow downloads archives straight into the output directory.
            release.package_release(TAG, REPOSITORY, output, output)

    def test_missing_architecture_fails_before_creating_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts, output = root / "artifacts", root / "dist"
            fake_artifacts(artifacts)
            next(artifacts.iterdir()).unlink()
            with self.assertRaises(FileNotFoundError):
                release.package_release(TAG, REPOSITORY, artifacts, output)
            self.assertFalse(output.exists())

    def test_nonexecutable_archive_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            artifacts, output = root / "artifacts", root / "dist"
            fake_artifacts(artifacts, executable=False)
            with self.assertRaises(ValueError):
                release.package_release(TAG, REPOSITORY, artifacts, output)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()

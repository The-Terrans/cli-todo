"""Offline installer tests: mock GitHub downloads, use real tar and hashing tools."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.bin = self.root / "tools"
        self.bin.mkdir()
        self.fixture = self.root / "release"
        self.fixture.mkdir()
        self.dest = self.home / ".local/bin/cli-todo"
        self.env = {**os.environ, "HOME": str(self.home), "PATH": f"{self.bin}:{os.environ['PATH']}",
                    "FIXTURE": str(self.fixture), "FAKE_OS": "Linux", "FAKE_ARCH": "x86_64",
                    "FAKE_TAG": "v0.1.0"}
        self.env.pop("CLI_TODO_INSTALL_DIR", None)
        self.tool("uname", 'import os, sys\nprint(os.environ["FAKE_OS" if sys.argv[1] == "-s" else "FAKE_ARCH"])\n')
        self.tool("curl", '''import os, pathlib, shutil, sys
args = sys.argv[1:]
url = args[-1]
base = "https://github.com/The-Terrans/cli-todo/releases"
root = pathlib.Path(os.environ["FIXTURE"])
with (root / "requests").open("a") as log:
    log.write(url + "\\n")
if url == base + "/latest":
    print(base + "/tag/" + os.environ["FAKE_TAG"], end="")
else:
    if os.environ.get("FAIL_DOWNLOAD") == "1":
        sys.exit(22)
    prefix = base + "/download/" + os.environ["FAKE_TAG"] + "/"
    if not url.startswith(prefix):
        sys.exit(22)
    shutil.copyfile(root / url[len(prefix):], args[args.index("--output") + 1])
''')

    def tool(self, name, body):
        path = self.bin / name
        path.write_text(f"#!{sys.executable}\n{body}")
        path.chmod(0o755)

    def release(self, payload=b"new binary", tag="v0.1.0", target="x86_64-unknown-linux-musl", name="cli-todo", symlink=False):
        self.env["FAKE_TAG"] = tag
        asset = f"cli-todo-{tag}-{target}.tar.gz"
        path = self.fixture / asset
        with tarfile.open(path, "w:gz") as archive:
            info = tarfile.TarInfo(name)
            info.mode = 0o755
            if symlink:
                info.type = tarfile.SYMTYPE
                info.linkname = "/etc/passwd"
                archive.addfile(info)
            else:
                info.size = len(payload)
                archive.addfile(info, io.BytesIO(payload))
        checksum = hashlib.sha256(path.read_bytes()).hexdigest()
        (self.fixture / "SHA256SUMS").write_text(f"{checksum}  ./{asset}\n")
        return path

    def run_installer(self, *args, success=True):
        result = subprocess.run(["sh", str(ROOT / "install.sh"), *args], env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def existing(self):
        self.dest.parent.mkdir(parents=True)
        self.dest.write_bytes(b"old binary")

    def test_install_and_update_preserve_task_data(self):
        data = self.home / ".local/share/cli-todo/tasks.sqlite3"
        data.parent.mkdir(parents=True)
        data.write_bytes(b"existing task data")
        self.release()
        self.run_installer()
        self.assertEqual(self.dest.read_bytes(), b"new binary")
        self.assertTrue(os.access(self.dest, os.X_OK))
        self.release(b"updated binary", tag="0.2.0")
        self.run_installer()
        self.assertEqual(self.dest.read_bytes(), b"updated binary")
        self.assertEqual(data.read_bytes(), b"existing task data")
        self.assertEqual(list(self.dest.parent.glob(".cli-todo.*")), [])

    def test_all_platforms_and_explicit_tag_and_custom_directory(self):
        for system, arch, target in [("Linux", "x86_64", "x86_64-unknown-linux-musl"),
                                     ("Linux", "aarch64", "aarch64-unknown-linux-musl"),
                                     ("Darwin", "x86_64", "x86_64-apple-darwin"),
                                     ("Darwin", "arm64", "aarch64-apple-darwin")]:
            with self.subTest(target=target):
                self.env.update(FAKE_OS=system, FAKE_ARCH=arch)
                self.release(target=target)
                directory = self.home / "path with spaces" / target
                result = self.run_installer("--version", "v0.1.0", "--dir", str(directory))
                self.assertEqual((directory / "cli-todo").read_bytes(), b"new binary")
                self.assertIn("PATH", result.stdout)
        self.assertNotIn("/latest", (self.fixture / "requests").read_text())

    def test_checksum_failure_keeps_old_installation(self):
        self.existing()
        archive = self.release()
        archive.write_bytes(b"corrupt download")
        self.assertIn("Checksum mismatch", self.run_installer(success=False).stderr)
        self.assertEqual(self.dest.read_bytes(), b"old binary")

    def test_download_failure_keeps_old_installation(self):
        self.existing()
        self.release()
        self.env["FAIL_DOWNLOAD"] = "1"
        self.run_installer(success=False)
        self.assertEqual(self.dest.read_bytes(), b"old binary")

    def test_missing_and_duplicate_checksums_are_rejected(self):
        self.existing()
        self.release()
        path = self.fixture / "SHA256SUMS"
        checksum = path.read_text()
        for content in ["", checksum * 2]:
            path.write_text(content)
            self.run_installer(success=False)
            self.assertEqual(self.dest.read_bytes(), b"old binary")

    def test_unsafe_archive_and_empty_binary_are_rejected(self):
        self.existing()
        for name, link, payload in [("../outside", False, b"bad"), ("cli-todo", True, b"bad"), ("cli-todo", False, b"")]:
            self.release(name=name, symlink=link, payload=payload)
            self.run_installer(success=False)
            self.assertEqual(self.dest.read_bytes(), b"old binary")
            self.assertEqual(list(self.dest.parent.glob(".cli-todo.*")), [])

    def test_failed_replacement_cleans_staging_and_keeps_old_binary(self):
        self.existing()
        self.release()
        self.tool("mv", "import sys\nsys.exit(1)\n")
        self.run_installer(success=False)
        self.assertEqual(self.dest.read_bytes(), b"old binary")
        self.assertEqual(list(self.dest.parent.glob(".cli-todo.*")), [])

    def test_environment_directory_is_used_on_install_and_update(self):
        directory = self.home / "custom bin"
        self.env["CLI_TODO_INSTALL_DIR"] = str(directory)
        self.release()
        self.run_installer()
        self.release(b"updated", tag="0.2.0")
        self.run_installer()
        self.assertEqual((directory / "cli-todo").read_bytes(), b"updated")
        self.assertFalse(self.dest.exists())

    def test_managed_symlink_is_not_replaced(self):
        self.dest.parent.mkdir(parents=True)
        other = self.root / "managed-binary"
        other.write_bytes(b"managed")
        self.dest.symlink_to(other)
        self.run_installer(success=False)
        self.assertTrue(self.dest.is_symlink())
        self.assertEqual(other.read_bytes(), b"managed")
        self.assertFalse((self.fixture / "requests").exists())

    def test_help_and_invalid_options_do_not_download(self):
        self.assertIn("update", self.run_installer("--help").stdout)
        for args in [("--version",), ("--dir", "relative"), ("--version", "../../bad"), ("--bad",)]:
            self.run_installer(*args, success=False)
        self.env["FAKE_ARCH"] = "riscv64"
        self.run_installer(success=False)
        self.assertFalse((self.fixture / "requests").exists())


if __name__ == "__main__":
    unittest.main()

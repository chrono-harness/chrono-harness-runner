import fcntl
import hashlib
import importlib.util
import json
import zlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("native_check", Path(__file__).with_name("native-check.py"))
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)


class NativeDirectoryTest(unittest.TestCase):
    def setUp(self):
        workspace = tempfile.TemporaryDirectory()
        self.addCleanup(workspace.cleanup)
        self.root = Path(workspace.name).resolve()
        root = patch.object(native, "ROOT", self.root)
        root.start()
        self.addCleanup(root.stop)
        self.business = native.business_root()
        self.business.mkdir(parents=True)

    def member(self, digit):
        path = self.business / ("check-" + digit * 32)
        path.mkdir()
        (path / "output").write_text("native data")
        os.utime(path, (1, 1))
        return path

    def test_pack_replays_one_stream_in_plan_order(self):
        stream = b"compiler original\n"
        (self.root / "stdout").write_bytes(stream)
        process = {"cwd": str(self.root), "stdout": {"path": "stdout", "size": len(stream),
                    "sha256": hashlib.sha256(stream).hexdigest()}}
        receipt = self.root / "receipt.json"
        receipt.write_text(json.dumps({"process": process}))
        ref = {"path": str(receipt), "length": receipt.stat().st_size,
               "sha256": hashlib.sha256(receipt.read_bytes()).hexdigest()}
        records = self.root / "roots"
        def collect(paths):
            records.write_text("".join(json.dumps({"receipt": ref, "target": {
                "source": str(self.root / path)}}) + "\n" for path in paths))
        collect(["tests.rs", "main.rs"])
        result = native.pack_cargo_roots(records, ["main.rs", "tests.rs"])
        self.assertEqual([row["path"] for row in result["roots"]], ["main.rs", "tests.rs"])
        self.assertEqual(zlib.decompress(bytes.fromhex(result["cargo_stdout_zlib_hex"]["hex"])), stream)
        with self.assertRaisesRegex(ValueError, "selection differs"):
            native.pack_cargo_roots(records, ["main.rs"])
        collect(["main.rs", "main.rs"])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            native.pack_cargo_roots(records, ["main.rs"])
        collect(["main.rs"])
        (self.root / "stdout").write_bytes(stream[::-1])
        with self.assertRaisesRegex(ValueError, "stdout changed"):
            native.pack_cargo_roots(records, ["main.rs"])
        receipt.write_text("{}")
        with self.assertRaisesRegex(ValueError, "receipt changed"):
            native.pack_cargo_roots(records, ["main.rs"])

    @patch.object(subprocess, "check_output")
    def test_only_expired_idle_untracked_directories(self, git):
        idle = self.member("0")
        live = self.member("1")
        tracked = self.member("2")
        new = self.member("3")
        os.utime(new, None)
        shared = self.business / ("check-" + "5" * 16)
        shared.mkdir()
        os.utime(shared, (1, 1))
        foreign = self.business / "external"
        foreign.mkdir()
        link = self.business / ("check-" + "4" * 32)
        link.symlink_to(foreign)
        sample = self.root / "outside"
        sample.write_text("persist")
        (idle / "external-link").symlink_to(sample)
        os.utime(idle, (1, 1))
        git.return_value = os.fsencode(tracked.relative_to(self.root) / "output") + b"\0"
        with native.directory(live) as lease:
            fcntl.flock(lease, fcntl.LOCK_SH)
            native.maintain(86400)
        self.assertFalse(idle.exists())
        self.assertTrue(all(path.exists() for path in (live, tracked, new, foreign, link, shared)))
        native.maintain(86400)
        self.assertFalse(live.exists())
        self.assertEqual(sample.read_text(), "persist")
        git.assert_called_with(
            ["git", "ls-files", "--cached", "--with-tree=HEAD", "-z", "--", "target/native-checks"],
            cwd=self.root,
        )

if __name__ == "__main__":
    unittest.main()

import fcntl
import importlib.util
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
        self.business = native.business_root(True)

    def member(self, digit):
        path = self.business / ("check-" + digit * 32)
        path.mkdir()
        (path / "output").write_text("native data")
        os.utime(path, (1, 1))
        return path

    @patch.object(subprocess, "check_output")
    def test_only_expired_idle_untracked_directories(self, git):
        idle = self.member("0")
        live = self.member("1")
        tracked = self.member("2")
        new = self.member("3")
        os.utime(new, None)
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
        self.assertTrue(all(path.exists() for path in (live, tracked, new, foreign, link)))
        native.maintain(86400)
        self.assertFalse(live.exists())
        self.assertEqual(sample.read_text(), "persist")
        git.assert_called_with(
            ["git", "ls-files", "--cached", "--with-tree=HEAD", "-z", "--", "target/native-checks"],
            cwd=self.root,
        )

    @patch.object(subprocess, "check_output", return_value=b"")
    @patch.object(subprocess, "run")
    def test_business_status_and_directory_lifetime(self, child, git):
        def check(argv, *, cwd, pass_fds):
            self.assertEqual(argv[:2], ["/bin/bash", ".chrono-harness/native-check.sh"])
            self.assertEqual(cwd, self.root)
            self.assertTrue(os.path.samestat(os.fstat(pass_fds[0]), Path(argv[2]).stat()))
            return subprocess.CompletedProcess(argv, status)

        child.side_effect = check
        for status in (0, 101, -9):
            self.assertEqual(native.run(), status if status >= 0 else 137)
            scratch = Path(child.call_args.args[0][2])
            self.assertEqual(scratch.exists(), status != 0)


if __name__ == "__main__":
    unittest.main()

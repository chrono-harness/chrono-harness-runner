"""Own the disposable directories used by this repository's native check."""
import argparse
from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
BUSINESS = Path("target/native-checks")
RUN_NAME = re.compile(r"check-[0-9a-f]{32}")


@contextmanager
def directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        yield fd
    finally:
        os.close(fd)


def business_root(create):
    path = ROOT / BUSINESS
    if path.resolve() != path:
        raise ValueError(f"native check requires a physical business directory: {path}")
    if create:
        path.mkdir(mode=0o700, parents=True, exist_ok=True)
    return path


def tracked_runs():
    raw = subprocess.check_output(
        ["git", "ls-files", "--cached", "--with-tree=HEAD", "-z", "--", str(BUSINESS)],
        cwd=ROOT,
    )
    protected = set()
    for name in raw.split(b"\0"):
        if name:
            relative = Path(os.fsdecode(name)).relative_to(BUSINESS)
            if relative.parts:
                protected.add(relative.parts[0])
    return protected


def remove_if_idle(path, cutoff, protected):
    if path.name in protected:
        return False
    with directory(path) as lease:
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return False
        stat = os.fstat(lease)
        if stat.st_mtime > cutoff or not os.path.samestat(stat, path.lstat()):
            return False
        shutil.rmtree(path)
        return True


def maintain(seconds):
    root = business_root(False)
    if not root.exists():
        return
    with directory(root) as gate:
        fcntl.flock(gate, fcntl.LOCK_EX)
        protected = tracked_runs()
        cutoff = time.time() - seconds
        for path in root.iterdir():
            if RUN_NAME.fullmatch(path.name) and not path.is_symlink() and path.is_dir():
                if remove_if_idle(path, cutoff, protected):
                    print(f"removed {path}", flush=True)


def run():
    root = business_root(True)
    scratch = root / ("check-" + uuid.uuid4().hex)
    with directory(root) as gate:
        fcntl.flock(gate, fcntl.LOCK_EX)
        scratch.mkdir(mode=0o700)
        with directory(scratch) as lease:
            fcntl.flock(lease, fcntl.LOCK_SH)
            fcntl.flock(gate, fcntl.LOCK_UN)
            print(f"native check directory: {scratch}", file=sys.stderr, flush=True)
            result = subprocess.run(
                ["/bin/bash", ".chrono-harness/native-check.sh", str(scratch)],
                cwd=ROOT, pass_fds=(lease,),
            )
            if result.returncode:
                os.utime(lease)
                return result.returncode if result.returncode > 0 else 128 - result.returncode
        fcntl.flock(gate, fcntl.LOCK_EX)
        if scratch.exists():
            remove_if_idle(scratch, float("inf"), tracked_runs())
    return 0


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest="action", required=True)
    actions.add_parser("run")
    cleanup = actions.add_parser("maintain")
    cleanup.add_argument("--keep-seconds", type=int, default=86400)
    args = parser.parse_args()
    if args.action == "run":
        sys.exit(run())
    if args.keep_seconds < 1:
        parser.error("--keep-seconds must be positive")
    maintain(args.keep_seconds)

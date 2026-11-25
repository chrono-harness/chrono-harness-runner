"""Finish the 32-hex runs allocated by the former runner-local producer."""
import argparse
from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import re
import shutil
import subprocess
import time

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


def business_root():
    path = ROOT / BUSINESS
    if path.resolve() != path:
        raise ValueError(f"native check requires a physical business directory: {path}")
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
    root = business_root()
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("seconds", type=int)
    args = parser.parse_args()
    if args.seconds < 1:
        parser.error("retention must be positive")
    maintain(args.seconds)

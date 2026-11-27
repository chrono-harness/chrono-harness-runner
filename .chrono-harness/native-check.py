"""Collect native-check Cargo originals and maintain previously allocated check directories."""
import json
import argparse
from contextlib import contextmanager
import fcntl
import os
from pathlib import Path
import re
import shutil
import sys
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


def pack_cargo_roots(records, requested):
    """Keep a shared Cargo stdout original once, with each selected target's dep-info."""
    import hashlib
    import zlib
    rows = {}
    receipt_ref = None
    receipt = None
    for line in records.read_bytes().splitlines():
        row = json.loads(line)
        observed = row.pop("receipt")
        if receipt_ref is None:
            receipt_ref = observed
            raw = Path(observed["path"]).read_bytes()
            if len(raw) != observed["length"] or hashlib.sha256(raw).hexdigest() != observed["sha256"]:
                raise ValueError("Cargo build receipt changed after collection")
            receipt = json.loads(raw)
        elif receipt_ref != observed:
            raise ValueError("Cargo proof roots were collected from different builds")
        cwd = Path(receipt["process"]["cwd"])
        path = str(Path(row["target"]["source"]).resolve().relative_to(cwd))
        if path in rows:
            raise ValueError("duplicate collected Cargo source root")
        rows[path] = row
    if len(set(requested)) != len(requested) or set(requested) != set(rows) or receipt is None:
        raise ValueError("Cargo proof root selection differs from its registration")
    original = receipt["process"]["stdout"]
    raw = (cwd / original["path"]).read_bytes()
    if len(raw) != original["size"] or hashlib.sha256(raw).hexdigest() != original["sha256"]:
        raise ValueError("Cargo compiler stdout changed after collection")
    compressed = zlib.compress(raw)
    encoded = {"hex": compressed.hex(), "length": len(compressed), "sha256": hashlib.sha256(compressed).hexdigest()}
    return {"receipt": receipt, "cargo_stdout_zlib_hex": encoded,
            "roots": [{"path": path, "original": rows[path]} for path in requested]}


if __name__ == "__main__":
    if len(sys.argv) >= 4 and sys.argv[1] == "--pack-cargo-roots":
        json.dump(pack_cargo_roots(Path(sys.argv[2]), sys.argv[3:]), sys.stdout, separators=(",", ":"))
        sys.stdout.write("\n")
        sys.exit(0)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("seconds", type=int)
    args = parser.parse_args()
    if args.seconds < 1:
        parser.error("retention must be positive")
    maintain(args.seconds)

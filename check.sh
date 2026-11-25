#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# The cron entry has no mise environment. Bind the installed executable before
# the first new allocation so the same entry can still perform the final sweep.
bootstrap=target/native-check-bootstrap
if [[ $# == 0 ]]; then
  installed=$(command -v chrono-host-bootstrap)
  if [[ $installed != /* ]]; then
    echo "expected the installed chrono-host-bootstrap on PATH" >&2
    exit 1
  fi
  if [[ -L target || ( -e "$bootstrap" && ! -L "$bootstrap" ) ]]; then
    echo "expected a physical target directory and a managed bootstrap link" >&2
    exit 1
  fi
  mkdir -p target
  if [[ $(readlink "$bootstrap" || true) != "$installed" ]]; then
    ln -sfn "$installed" "$bootstrap"
  fi
  exec "$bootstrap" native-scratch run /bin/bash .chrono-harness/native-check.sh
fi

if [[ $1 == --maintain && ( $# == 1 || ( $# == 3 && $2 == --keep-seconds ) ) ]]; then
  keep=${3:-86400}
  python3 -B .chrono-harness/native-check.py "$keep"
  if [[ -e "$bootstrap" || -L "$bootstrap" ]]; then
    exec "$bootstrap" native-scratch maintain "$keep"
  fi
  exit 0
fi

echo "usage: ./check.sh [--maintain [--keep-seconds N]]" >&2
exit 2

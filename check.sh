#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
if [[ $# == 0 ]]; then
  exec python3 .chrono-harness/native-check.py run
fi
if [[ $1 == --maintain ]]; then
  shift
  exec python3 .chrono-harness/native-check.py maintain "$@"
fi
echo "usage: ./check.sh [--maintain [--keep-seconds N]]" >&2
exit 2

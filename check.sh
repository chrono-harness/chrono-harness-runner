#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

# Maintenance can run outside mise. Bind the installed executable before
# the first new allocation so the same entry can still perform the final sweep.
if [[ $# == 0 ]]; then
  exec python3 -B .chrono-harness/native-check.py --development
fi

bootstrap=target/native-check-bootstrap
if [[ ( $# == 1 && $1 == --native ) || ( $# -ge 2 && $1 == --cargo-verus-roots ) ]]; then
  if [[ $1 == --native ]]; then
    shift
  fi
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
  exec "$bootstrap" native-scratch run /bin/bash .chrono-harness/native-check.sh "$@"
fi

# Scoped CI starts the route checker with a cleared environment. Put the selected
# binaries together so its nested calls can use the same runner and collector.
if [[ $# == 4 && $1 == --scoped ]]; then
  runner=$2
  if [[ -z ${VERUS:-} ]]; then
    VERUS=$(command -v verus)
    export VERUS
  fi
  base=$3
  candidate=$4
  mkdir -p target
  tools=$(mktemp -d "$PWD/target/scoped-tools.XXXXXXXX")
  trap 'rm -rf -- "$tools"' EXIT
  for name in chrono-judge-ci chrono-judge-filemap chrono-judge-projects \
    chrono-judge-registration chrono-judge-routes chrono-diagnostics chrono-host-bootstrap; do
    ln -s "$(command -v "$name")" "$tools/$name"
  done
  ln -s "$runner" "$tools/chrono-harness"
  # Keep the toolchain inputs needed by the check and its compiler receipts.
  check_env=("HOME=$HOME" "PATH=$tools:$PATH" "VERUS=$VERUS")
  for name in TMPDIR CARGO_HOME RUSTUP_HOME CARGO_TARGET_DIR; do
    if [[ ${!name+x} ]]; then
      check_env+=("$name=${!name}")
    fi
  done
  env -i "${check_env[@]}" "$tools/chrono-harness" check --config .chrono-harness/check.json \
    --base "$base" --candidate "$candidate"
  exit $?
fi

if [[ $1 == --maintain && ( $# == 1 || ( $# == 3 && $2 == --keep-seconds ) ) ]]; then
  keep=${3:-86400}
  python3 -B .chrono-harness/native-check.py "$keep"
  if [[ -e "$bootstrap" || -L "$bootstrap" ]]; then
    exec "$bootstrap" native-scratch maintain "$keep"
  fi
  exit 0
fi

echo "usage: ./check.sh [--native | --cargo-verus-roots ROOT ... | --scoped RUNNER BASE CANDIDATE | --maintain [--keep-seconds N]]" >&2
exit 2

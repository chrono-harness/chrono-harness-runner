#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
scratch=$(mktemp -d)
trap 'rm -rf -- "$scratch"' EXIT
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$scratch/cargo}"
cargo fmt --check --manifest-path crates/test-process/Cargo.toml
cargo fmt --check --manifest-path crates/runner/Cargo.toml
cargo fmt --check --manifest-path crates/runner-tests/Cargo.toml
cargo build --locked --manifest-path crates/runner/Cargo.toml
CHRONO_TEST_GIT=$(command -v chrono-test-git) || { echo "chrono-test-git is required by runner tests" >&2; exit 1; }
export CHRONO_TEST_GIT
cargo test --locked --manifest-path crates/runner-tests/Cargo.toml -- --test-threads=2

proof_bindings=()
prove() {
  local label=$1 root=$2
  local output="$scratch/$label.rlib"
  "${VERUS:-verus}" --crate-type=lib --no-cheating --no-external-by-default --num-threads 2 \
    "${proof_bindings[@]}" --output-json --compile "--emit=link=$output,dep-info=$output.d" "$root" -o "$output" \
    -Zchecksum-hash-algorithm=sha256 > "$scratch/proof.json" || { cat "$scratch/proof.json" >&2; return 1; }
  jq '."verification-results"' "$scratch/proof.json"
  jq -e '.verus.version == "0.2026.04.24.f8e1704" and
    .verus.commit == "f8e17045c4ac17a02e566b8997070f2eecc255be" and
    (."verification-results" | .success == true and ."is-verifying-entire-crate" == true and
    ."encountered-error" == false and ."encountered-vir-error" == false and
    .errors == 0 and .verified > 0)' "$scratch/proof.json" >/dev/null
  test -s "$output" && test -s "$output.d"
}

proof_bindings=(--crate-name chrono_identifiers --export "$scratch/chrono_identifiers.vir")
prove libchrono_identifiers crates/runner/src/verified_identifiers.rs
proof_bindings=(--extern "chrono_identifiers=$scratch/libchrono_identifiers.rlib"
                --import "chrono_identifiers=$scratch/chrono_identifiers.vir")
prove verify.runner-paths crates/runner/src/verified_paths.rs
prove verify.runner-status crates/runner/src/verified_status.rs
prove verify.runner-limits crates/runner/src/verified_limits.rs
prove verify.verus-result crates/runner/src/verified_verifier.rs
prove verify.retention-release crates/runner/src/retained_artifacts/verified_release.rs
prove verify.runner-git-binding crates/runner/src/verified_git_binding.rs
prove verify.runner-cli crates/runner/src/verified_cli.rs
prove verify.runner-paths-tests crates/runner-tests/tests/verified_paths.rs
prove verify.runner-fixture-tests crates/runner-tests/tests/verified_fixture.rs
prove verify.runner-status-tests crates/runner-tests/tests/verified_status.rs
prove verify.runner-limits-tests crates/runner-tests/tests/verified_limits.rs
prove verify.verus-result-tests crates/runner-tests/tests/verified_verifier.rs
prove verify.retention-release-tests crates/runner-tests/tests/verified_release.rs
prove verify.runner-git-binding-tests crates/runner-tests/tests/verified_git_binding.rs
prove verify.runner-cli-tests crates/runner-tests/tests/verified_cli.rs
prove verify.identifiers-tests crates/runner-tests/tests/verified_identifiers.rs

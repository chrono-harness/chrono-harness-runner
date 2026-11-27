#!/usr/bin/env bash
set -euo pipefail
scratch=${CHRONO_NATIVE_SCRATCH:?missing leased native check directory}
# A registered output contract receives the original Cargo bundle on stdout.
# The developer entry keeps its ordinary output when called without a contract.
cargo_roots=()
if [[ ${1:-} == --cargo-verus-roots ]]; then
  shift
  cargo_roots=("$@")
  exec 3>&1
  exec >&2
fi
export TMPDIR="$scratch"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$scratch/cargo}"
python3 -B .chrono-harness/test-native-check.py
cargo fmt --check --all
cargo build --locked --message-format=json-render-diagnostics --package chrono-harness > "$scratch/native.jsonl"
CHRONO_TEST_GIT=$(command -v chrono-test-git) || { echo "chrono-test-git is required by runner tests" >&2; exit 1; }
CHRONO_TEST_GIT_PROFILE_BIN=$(command -v chrono-test-git-profile) || { echo "chrono-test-git-profile is required by runner tests" >&2; exit 1; }
export CHRONO_TEST_GIT CHRONO_TEST_GIT_PROFILE_BIN
cargo build --locked --package chrono-harness-tests --bin chrono-test-diagnostics
mkdir "$scratch/mock-bin"
ln -s "$CHRONO_TEST_GIT" "$scratch/mock-bin/git"
ln -s "$CARGO_TARGET_DIR/debug/chrono-test-diagnostics" "$scratch/mock-bin/chrono-diagnostics"
export PATH="$scratch/mock-bin:$PATH"
cargo test --locked --package chrono-harness-tests -- --test-threads=2

# Use the actual library metadata selected by the native Cargo invocation.
check_native() {
  local package=$1
  shift
  local metadata stem filename
  metadata=$(jq -ers --arg manifest "$(pwd -P)/$package/Cargo.toml" '
    [.[] | select(.reason == "compiler-artifact" and .manifest_path == $manifest
                  and .target.kind == ["lib"]) | .filenames[] | select(endswith(".rmeta"))]
    | if length == 1 then .[0] else error("expected one native library") end
  ' "$scratch/native.jsonl")
  filename=${metadata##*/}
  stem=${filename#lib}
  chrono-host-bootstrap check-compiler-sources native "${metadata%/*}/${stem%.rmeta}" "$@"
}

# rustc records each workspace member's sources relative to the workspace root.
check_native crates/proofs/identifiers crates/proofs/identifiers/src/lib.rs
verifier_src=crates/proofs/verifier/src
check_native crates/proofs/verifier "$verifier_src/lib.rs" "$verifier_src/verified_sources.rs" \
  "$verifier_src/verified_continuity.rs" "$verifier_src/verified_source_pipeline.rs"
src=crates/runner/src
check_native crates/runner "$src/lib.rs" "$src/artifact_disposal.rs" "$src/source_endpoints.rs" "$src/source_coverage.rs" "$src/route_evidence.rs" \
  "$src/verified_cli.rs" "$src/trace_source.rs" "$src/diagnostic_session.rs" "$src/process_error.rs" "$src/short_console.rs" "$src/verified_git_binding.rs" "$src/verified_shallow_reads.rs" "$src/verified_limits.rs" "$src/verified_paths.rs" "$src/verified_status.rs" \
  "$src/command_stream.rs" "$src/observed_command.rs" "$src/process_child.rs" "$src/process_fds.rs" "$src/retained_artifacts/session.rs" "$src/retained_artifacts/session/descriptors.rs" \
  "$src/facts.rs" "$src/facts_binding.rs" "$src/git_facts_session.rs" "$src/publication.rs" "$src/retained_artifacts/verified_release.rs" "$src/retained_artifacts/retention.rs" "$src/retained_artifacts/session/publication.rs" "$src/retained_artifacts/manifest.rs"

verifier=$(command -v "${VERUS:-verus}")
cargo_verus=$(dirname "$verifier")/cargo-verus
workspace=Cargo.toml
chrono-host-bootstrap run-cargo-proofs "$scratch" runner-proofs "$cargo_verus" "$workspace" \
  chrono-identifiers chrono-verifier chrono-runner-proof-expectations

collect() {
  local package=$1 manifest=$2
  shift 2
  printf '%s (%s)\n' "$package" "$1" >&2
  local requested
  for requested in "${cargo_roots[@]:+${cargo_roots[@]}}"; do
    if [[ $requested == "$1" ]]; then
      chrono-host-bootstrap collect-cargo-proof-original "$scratch" runner-proofs "$cargo_verus" "$workspace" \
        "$package" 0.2026.04.24.f8e1704 f8e17045c4ac17a02e566b8997070f2eecc255be "$manifest" "$@" >> "$scratch/cargo-roots.jsonl"
      printf '\n' >> "$scratch/cargo-roots.jsonl"
      return
    fi
  done
  chrono-host-bootstrap collect-cargo-proof "$scratch" runner-proofs "$cargo_verus" "$workspace" \
    "$package" 0.2026.04.24.f8e1704 f8e17045c4ac17a02e566b8997070f2eecc255be "$manifest" "$@"
}

collect_root() {
  collect chrono-runner-proof-expectations crates/proofs/expectations/Cargo.toml "$@"
}

tests=crates/runner-tests/tests
collect chrono-identifiers crates/proofs/identifiers/Cargo.toml crates/proofs/identifiers/src/lib.rs
collect chrono-verifier crates/proofs/verifier/Cargo.toml "$verifier_src/lib.rs" \
  "$verifier_src/verified_sources.rs" "$verifier_src/verified_continuity.rs" "$verifier_src/verified_source_pipeline.rs"

limits=("$src/verified_limits.rs" "$src/command_stream.rs" "$src/observed_command.rs" "$src/process_child.rs" "$src/process_fds.rs" "$src/retained_artifacts/session.rs" "$src/retained_artifacts/session/descriptors.rs")
paths=("$src/verified_paths.rs")
status=("$src/verified_status.rs" "$src/observation.rs" "$src/facts.rs" "$src/facts_binding.rs" "$src/verified_paths.rs"
        "$src/verified_scope.rs" "$src/verified_export.rs" "$src/lib.rs" "$src/ownership.rs")
maintenance=("$src/maintenance_signal.rs" "$src/artifact_disposal.rs")
release=("$src/retained_artifacts/verified_release.rs" "$src/retained_artifacts/retention.rs"
         "$src/retained_artifacts/manifest.rs" "$src/retained_artifacts/partition.rs"
         "$src/retained_artifacts/retention_command.rs" "$src/scoped_report.rs"
         "$src/prepared.rs" "$src/prepared_artifacts.rs" "$src/prepared_binding.rs" "$src/retained_artifacts.rs"
         "$src/retained_artifacts/session/directory.rs" "$src/retained_artifacts/session/inventory.rs"
         "$src/retained_artifacts/session/publication.rs")
git_binding=("$src/verified_git_binding.rs" "$src/verified_paths.rs" "$src/facts_configs.rs" "$src/invocation_query.rs")
cli=("$src/verified_cli.rs" "$src/trace_source.rs" "$src/diagnostic_session.rs" "$src/process_error.rs" "$src/publication.rs" "$src/short_console.rs")

collect_root "${paths[@]}"
collect_root "${status[@]}"
collect_root "${limits[@]}"
collect_root "$src/verified_shallow_reads.rs"
collect_root "${maintenance[@]}"
collect_root "${release[@]}"
collect_root "${git_binding[@]}"
collect_root "${cli[@]}"

collect_root "$tests/verified_paths.rs" "${paths[@]}"
collect_root "$tests/verified_fixture.rs" "$tests/support/cli_child.rs" "$tests/support/isolated_capability.rs" \
  crates/test-process/src/lib.rs "$tests/protocol_v1.rs" "$tests/retention.rs"
collect_root "$tests/verified_status.rs" "${status[@]}" "$tests/cli_contract.rs"
collect_root "$tests/verified_limits.rs" "${limits[@]}"
collect_root "$tests/verified_shallow_reads.rs" "$src/verified_shallow_reads.rs" crates/proofs/identifiers/src/lib.rs
collect_root "$tests/maintenance_signal.rs" "${maintenance[@]}"
collect_root "$tests/verified_verifier.rs" "$tests/support/verified_continuity.rs" "$tests/support/source_pipeline.rs"
collect_root "$tests/verified_release.rs" "${release[@]}" "$tests/support/retention_frontier.rs"
collect_root "$tests/verified_git_binding.rs" "${git_binding[@]}"
collect_root "$tests/verified_cli.rs" "${cli[@]}"
collect_root "$tests/verified_identifiers.rs"


if [[ ${#cargo_roots[@]} -gt 0 ]]; then
  python3 -B .chrono-harness/native-check.py --pack-cargo-roots "$scratch/cargo-roots.jsonl" "${cargo_roots[@]}" >&3
fi

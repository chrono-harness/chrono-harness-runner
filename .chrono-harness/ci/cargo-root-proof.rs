//! Source, output and installed-tool checks for Cargo evidence replay.
#[path = "../../crates/runner/src/route_evidence.rs"]
mod route_evidence;
#[path = "../../crates/runner/src/git_facts_output.rs"]
mod git_facts_output;
#[path = "../../crates/runner/src/installed_query.rs"]
mod installed_query;

use vstd::prelude::*;
verus! {
fn process_and_candidate_boundaries() {
    let passed = route_evidence::successful_group(0, false, false, false);
    let failed = route_evidence::successful_group(1, false, false, false);
    let direct = route_evidence::successful_group(0, false, true, false);
    let complete = git_facts_output::complete_candidate(2, 2);
    let missing = git_facts_output::complete_candidate(2, 1);
    let empty = git_facts_output::complete_candidate(0, 0);
    let inherited = installed_query::selected_search(true);
    let sibling = installed_query::selected_search(false);
    assert(passed && !failed && !direct && complete && !missing && !empty);
    assert(inherited is Environment);
    assert(sibling is InvokedDirectory);
}

use route_evidence::{CargoBin, CargoBinRow, cargo_bin, cargo_bin_match};

broadcast use {vstd::string::axiom_str_literal_len, vstd::string::axiom_str_literal_get_char};

fn first_bin_matches_the_host_contracts() {
    let wrong_manifest = CargoBinRow {
        reason: "compiler-artifact", manifest: Some("/other/Cargo.toml"), singleton_kind: "bin",
        source: Some("/pkg/src/main.rs"), executable: Some("/ignore"),
    };
    let wrong_kind = CargoBinRow {
        reason: "compiler-artifact", manifest: Some("/pkg/Cargo.toml"), singleton_kind: "lib",
        source: Some("/pkg/src/main.rs"), executable: Some("/ignore"),
    };
    let wrong_reason = CargoBinRow {
        reason: "compiler-message", manifest: Some("/pkg/Cargo.toml"), singleton_kind: "bin",
        source: Some("/pkg/src/main.rs"), executable: Some("/ignore"),
    };
    let bin = CargoBinRow {
        reason: "compiler-artifact", manifest: Some("/pkg/Cargo.toml"), singleton_kind: "bin",
        source: Some("/pkg/src/main.rs"), executable: Some("/custom target/debug/bin"),
    };
    let manifest = "/pkg/Cargo.toml";
    let rows = [wrong_manifest, wrong_kind, wrong_reason, bin];
    proof {
        reveal_strlit("/other/Cargo.toml");
        reveal_strlit("/pkg/Cargo.toml");
        reveal_strlit("lib");
        reveal_strlit("bin");
        reveal_strlit("compiler-message");
        reveal_strlit("compiler-artifact");
        reveal_strlit("/pkg/src/main.rs");
        reveal_strlit("/pkg/./src/main.rs");
    }
    assert(rows@[0].manifest.unwrap()@.len() != manifest@.len());
    assert(rows@[1].singleton_kind@[0] != "bin"@[0]);
    assert(rows@[2].reason@.len() != "compiler-artifact"@.len());
    assert(rows@[3].source.unwrap()@.len() != "/pkg/./src/main.rs"@.len());
    assert(cargo_bin_match(&rows@[3], manifest, None));
    assert(cargo_bin_match(&rows@[3], manifest, Some("/pkg/src/main.rs")));
    assert forall|i: int| 0 <= i < rows@.len() && i != 3 implies
        !#[trigger] cargo_bin_match(&rows@[i], manifest, None)
        && !#[trigger] cargo_bin_match(&rows@[i], manifest, Some("/pkg/src/main.rs"))
    by {
        assert(i == 0 || i == 1 || i == 2);
    }
    assert forall|i: int| 0 <= i < rows@.len() implies
        !#[trigger] cargo_bin_match(&rows@[i], manifest, Some("/pkg/./src/main.rs"))
    by {
        assert(i == 0 || i == 1 || i == 2 || i == 3);
    }
    let bootstrap = cargo_bin(&rows, manifest, None);
    let release = cargo_bin(&rows, manifest, Some("/pkg/src/main.rs"));
    let wrong_source = cargo_bin(&rows, manifest, Some("/pkg/./src/main.rs"));
    assert(bootstrap is Executable);
    assert(bootstrap->Executable_0@ == "/custom target/debug/bin"@);
    assert(release is Executable);
    assert(release->Executable_0@ == "/custom target/debug/bin"@);
    assert(wrong_source is Missing);
}

fn uniqueness_precedes_executable_admission() {
    let missing = cargo_bin(&[], "/pkg/Cargo.toml", None);
    assert(missing is Missing);
    let null = CargoBinRow {
        reason: "compiler-artifact", manifest: Some("/pkg/Cargo.toml"), singleton_kind: "bin",
        source: None, executable: None,
    };
    let bin = CargoBinRow {
        reason: "compiler-artifact", manifest: Some("/pkg/Cargo.toml"), singleton_kind: "bin",
        source: None, executable: Some("/custom target/debug/bin"),
    };
    let single = [null];
    assert(cargo_bin_match(&single@[0], "/pkg/Cargo.toml", None));
    let no_executable = cargo_bin(&single, "/pkg/Cargo.toml", None);
    assert(no_executable is NoExecutable);
    let both = [null, bin];
    assert(cargo_bin_match(&both@[0], "/pkg/Cargo.toml", None));
    assert(cargo_bin_match(&both@[1], "/pkg/Cargo.toml", None));
    let duplicate = cargo_bin(&both, "/pkg/Cargo.toml", None);
    assert(duplicate is Duplicate);
}
}

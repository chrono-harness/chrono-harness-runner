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
}

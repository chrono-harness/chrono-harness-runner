//! Independent cases for the release workflow’s executable acceptance decision.
#![allow(unexpected_cfgs)]
#[path = "../examples/release_observation.rs"]
mod release_observation;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, vpanic as panic};

verus! {
fn expect(value: bool) requires value {
    if !value { panic!(); }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn release_requires_the_version_and_the_help_marker() {
    expect(release_observation::accepts(true, true));
    expect(!release_observation::accepts(false, true));
    expect(!release_observation::accepts(true, false));
    expect(!release_observation::accepts(false, false));
    let marker = release_observation::help_marker();
    proof { assert(marker@ == "chrono-harness check"@); }
}
}

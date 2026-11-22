//! 命令替身的封闭派发域；IO 行为仍由独立 Rust 边界预期核验。
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "support/cli_child.rs"]
mod cli_child;
#[cfg(verus_keep_ghost)]
extern crate chrono_identifiers as identifiers;
#[path = "support/isolated_capability.rs"]
mod isolation;
#[cfg(verus_keep_ghost)]
#[path = "protocol_v1.rs"]
mod protocol;
#[cfg(verus_keep_ghost)]
#[path = "retention.rs"]
mod retention;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;
verus! {

#[cfg(verus_keep_ghost)]
fn bounded_fixture_recovery_preserves_the_original_eight_round_limit() {
    expect(retention::fixture_recovery_has_room(0));
    expect(retention::fixture_recovery_has_room(7));
    expect(!retention::fixture_recovery_has_room(8));
    expect(!retention::fixture_recovery_has_room(usize::MAX));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn custody_release_requires_original_child_result_and_owner_retirement() {
    expect(isolation::release_custody(true, true));
    expect(!isolation::release_custody(false, true));
    expect(!isolation::release_custody(true, false));
    expect(!isolation::release_custody(false, false));
}
#[cfg_attr(not(verus_keep_ghost), test)]
fn isolation_requires_complete_test_identity() {
    proof {
        reveal_strlit("case"); reveal_strlit("case-extra"); reveal_strlit("");
        assert("case"@.len() != "case-extra"@.len());
        assert("case"@.len() != ""@.len());
    }
    expect(isolation::same_test("case",Some("case")));
    expect(!isolation::same_test("case",Some("case-extra")));
    expect(!isolation::same_test("case",Some("")));
    expect(!isolation::same_test("case",None));
}
fn expect(actual:bool) requires actual, { if !actual {panic!();} }
}

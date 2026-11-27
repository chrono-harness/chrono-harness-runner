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
use vstd::string::{StringSliceAdditionalSpecFns, is_ascii};
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

#[cfg_attr(not(verus_keep_ghost), test)]
fn isolation_requires_a_successful_exit_and_one_passed_test() {
    let passed = "test result: ok. 1 passed; 0 failed;";
    let zero = "test result: ok. 0 passed; 0 failed;";
    let embedded = " test result: ok. 1 passed; 0 failed;";
    proof {
        reveal_strlit("test result: ok. 1 passed; 0 failed;");
        reveal_strlit("test result: ok. 0 passed; 0 failed;");
        reveal_strlit(" test result: ok. 1 passed; 0 failed;");
        reveal(isolation::LIBTEST_PASS_PREFIX);
        broadcast use vstd::string::is_ascii_spec_bytes;
        assert(is_ascii(passed) && is_ascii(zero) && is_ascii(embedded));
        let prefix_len = passed.spec_bytes().len() as int;
        assert(passed.spec_bytes().take(prefix_len) =~= passed.spec_bytes());
        assert(isolation::libtest_pass_at(passed.spec_bytes(), 0));
        assert(zero.spec_bytes()[17] != passed.spec_bytes()[17]);
        assert(embedded.spec_bytes()[0] != passed.spec_bytes()[0]);
        assert(forall|start: int| !#[trigger] isolation::libtest_pass_at(zero.spec_bytes(), start)) by {
            assert(zero.spec_bytes().take(prefix_len)[17] == zero.spec_bytes()[17]);
        }
        assert(forall|start: int| !#[trigger] isolation::libtest_pass_at(embedded.spec_bytes(), start)) by {
            assert(embedded.spec_bytes().take(prefix_len)[0] == embedded.spec_bytes()[0]);
            assert(embedded.spec_bytes()[0] != '\n' as u8);
        }
    }
    expect(isolation::isolated_test_passed(true, passed.as_bytes()));
    expect(!isolation::isolated_test_passed(false, passed.as_bytes()));
    expect(!isolation::isolated_test_passed(true, zero.as_bytes()));
    expect(!isolation::isolated_test_passed(true, embedded.as_bytes()));
}

fn expect(actual:bool) requires actual, { if !actual {panic!();} }
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn isolation_preserves_libtest_line_and_byte_boundaries() {
    let cases: &[(&[u8], bool)] = &[
        (
            b"\xff\r\ntest result: ok. 1 passed; 0 failed; 0 ignored\xff\r\nmore output",
            true,
        ),
        (
            b"running 1 test\ntest result: ok. 1 passed; 0 failed;",
            true,
        ),
        (b"\xfftest result: ok. 1 passed; 0 failed;", false),
        (b"noise\rtest result: ok. 1 passed; 0 failed;", false),
        (b"test result: ok. 1 passed; 0 failed", false),
        (b"", false),
    ];
    for &(stdout, expected) in cases {
        assert_eq!(
            isolation::isolated_test_passed(true, stdout),
            expected,
            "stdout: {stdout:?}"
        );
    }
}

//! 验证实际浅历史状态、原回复和各次读取的顺序。
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/verified_shallow_reads.rs"]
mod policy;
#[cfg(not(verus_keep_ghost))]
use chrono_harness::verified_shallow_reads as policy;
#[cfg(verus_keep_ghost)]
#[path = "../../proofs/identifiers/src/lib.rs"]
mod identifiers;
#[cfg(not(verus_keep_ghost))]
use chrono_harness::verified_identifiers as identifiers;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {

fn expect_true(value: bool)
    requires value,
{ if !value { panic!(); } }

fn expect_state(actual: &str, expected: &str)
    requires actual == expected,
{
    expect_true(identifiers::equal_text(actual, expected));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn shallow_status_and_reply_distinguish_present_empty_absent_and_read_error() {
    let (state, result) = policy::shallow_file_reply(Ok::<u8,(bool,u8)>(19), 0);
    expect_state(state, "present");
    match result { Ok(rows) => expect_true(rows == 19), Err(_) => panic!() }
    let (state, result) = policy::shallow_file_reply(Ok::<u8,(bool,u8)>(0), 0);
    expect_state(state, "present");
    match result { Ok(rows) => expect_true(rows == 0), Err(_) => panic!() }
    let (state, result) = policy::shallow_file_reply(Err((true, 43u8)), 0u8);
    expect_state(state, "absent");
    match result { Ok(rows) => expect_true(rows == 0), Err(_) => panic!() }
    let (state, result) = policy::shallow_file_reply(Err((false, 43u8)), 0u8);
    expect_state(state, "error");
    match result { Err(code) => expect_true(code == 43), Ok(_) => panic!() }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn only_bound_shallow_reads_appear_in_their_original_order() {
    let mut rows: Vec<u8> = Vec::new();
    expect_true(policy::observed_shallow_reads(&rows).is_none());
    policy::retain_shallow_read(&mut rows, false, || -> u8 { 11 });
    expect_true(policy::observed_shallow_reads(&rows).is_none());
    policy::retain_shallow_read(&mut rows, true, || -> (result: u8)
        ensures result == 7u8
    { 7 });
    policy::retain_shallow_read(&mut rows, false, || -> u8 { 19 });
    policy::retain_shallow_read(&mut rows, true, || -> (result: u8)
        ensures result == 3u8
    { 3 });
    match policy::observed_shallow_reads(&rows) {
        Some(actual) => {
            expect_true(actual.len() == 2);
            expect_true(actual[0] == 7 && actual[1] == 3);
        },
        None => panic!(),
    }
}

}

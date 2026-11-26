//! Compare the two authenticated endpoint identities before any coverage work.
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/source_coverage.rs"]
mod policy;
#[cfg(not(verus_keep_ghost))]
use chrono_harness::source_coverage as policy;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, vpanic as panic};

verus! {
fn expect(value: bool) requires value, { if !value { panic!(); } }

fn compare(previous: Option<&String>, candidate: &String, base: &String, head: &String) {
    let reply = policy::bind_request_endpoints(previous, candidate, base, head, || -> (value: u8) ensures value == 73u8 { 73u8 });
    let ok = matches!(reply, Ok(73));
    match previous {
        None => expect(!ok),
        Some(p) => expect(ok == (*p == *base && *head == *candidate)),
    }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn both_identities_are_required_before_reading() {
    let base = "a".to_string();
    let head = "b".to_string();
    compare(None, &head, &base, &head);
    compare(Some(&head), &head, &base, &head);
    compare(Some(&base), &base, &base, &head);
    compare(Some(&head), &base, &base, &head);
    compare(Some(&base), &head, &base, &head);
}
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn a_rejected_request_never_invokes_the_reader() {
    let base = String::from("base");
    let head = String::from("head");
    for (previous, candidate) in [
        (None, &head),
        (Some(&head), &head),
        (Some(&base), &base),
        (Some(&head), &base),
    ] {
        let result = policy::bind_request_endpoints(previous, candidate, &base, &head, || {
            panic!("rejected endpoints reached the reader")
        });
        assert_eq!(
            result,
            Err::<(), _>("E_GIT_FACTS: source coverage endpoints differ from request")
        );
    }
    let mut calls = 0;
    let result = policy::bind_request_endpoints(Some(&base), &head, &base, &head, || {
        calls += 1;
        Err::<(), _>("original reader failure")
    });
    assert_eq!(calls, 1);
    assert_eq!(result, Ok(Err("original reader failure")));
}

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

#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/canonical_request.rs"]
mod canonical_request;
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/wire.rs"]
mod wire_contract;

#[cfg(verus_keep_ghost)]
verus! {
fn v1_projection_edges_and_retained_identity(recorded: &String, matching: String, other: String, error: String)
    requires recorded@ == matching@, recorded@ != other@,
{
    let lower = canonical_request::direct_integer(-9_007_199_254_740_992);
    let upper = canonical_request::direct_integer(9_007_199_254_740_992);
    let below = canonical_request::direct_integer(-9_007_199_254_740_993);
    let above = canonical_request::direct_integer(9_007_199_254_740_993);
    assert(lower && upper && !below && !above);
    let same = wire_contract::check_request_identity(recorded, Ok(matching));
    assert(same.is_ok());
    let different = wire_contract::check_request_identity(recorded, Ok(other));
    assert(different.is_err());
    let ghost expected = error@;
    let failure = wire_contract::check_request_identity(recorded, Err(error));
    match failure { Err(text) => assert(text@ == expected), Ok(()) => assert(false) }
}

fn empty_paths_skip_the_whole_read() {
    let paths = Vec::<String>::new();
    let never = || -> Result<u8, u8> requires false, { panic!() };
    let empty = policy::source_endpoints::with_selected_paths(&paths, 41, never);
    assert(empty == Ok::<u8, u8>(41));

    let paths = vec!["src/lib.rs".to_string()];
    let fail = || -> (result: Result<u8, u8>) ensures result == Err::<u8, u8>(73), { Err(73) };
    let error = policy::source_endpoints::with_selected_paths(&paths, 41, fail);
    assert(error == Err::<u8, u8>(73));
}

fn reads_keep_the_previous_and_candidate_observations() {
    let read = |id: u8| -> (result: Result<u8, u8>)
        requires id == 1 || id == 2,
        ensures result == Ok::<u8, u8>(if id == 1 { 17 } else { 42 }),
    { Ok(if id == 1 { 17 } else { 42 }) };
    let both = policy::source_endpoints::read_endpoints(Some(1), 2, read);
    assert(both == Ok::<(Option<u8>, u8), u8>((Some(17), 42)));

    let only_candidate = |id: u8| -> (result: Result<u8, u8>)
        requires id == 2,
        ensures result == Ok::<u8, u8>(42),
    { Ok(42) };
    let initial = policy::source_endpoints::read_endpoints(None, 2, only_candidate);
    assert(initial == Ok::<(Option<u8>, u8), u8>((None, 42)));
}

fn earlier_read_failure_skips_the_candidate_and_each_error_is_forwarded() {
    let previous_fails = |id: u8| -> (result: Result<u8, u8>)
        requires id == 1,
        ensures result == Err::<u8, u8>(53),
    { Err(53) };
    let first = policy::source_endpoints::read_endpoints(Some(1), 2, previous_fails);
    assert(first == Err::<(Option<u8>, u8), u8>(53));

    let candidate_fails = |id: u8| -> (result: Result<u8, u8>)
        requires id == 1 || id == 2,
        ensures result == if id == 1 { Ok::<u8, u8>(17) } else { Err::<u8, u8>(67) },
    { if id == 1 { Ok(17) } else { Err(67) } };
    let second = policy::source_endpoints::read_endpoints(Some(1), 2, candidate_fails);
    assert(second == Err::<(Option<u8>, u8), u8>(67));
}

fn ineligible_prefetch_skips_acquisition_and_short_circuits_reuse() {
    proof {
        broadcast use {vstd::string::axiom_str_literal_len, vstd::string::axiom_str_literal_get_char};
        reveal_strlit("a"); reveal_strlit("b");
        assert("a"@[0] != "b"@[0]);
        assert("a"@ != "b"@);
    }
    let absent = policy::source_endpoints::prefetch_if_reusable(None, "b", never_reuse, never_prefetch);
    assert(absent == Ok::<(), u8>(()));

    let previous_miss = |oid: &str| -> (yes: bool)
        requires oid@ == "a"@,
        ensures !yes,
    { false };
    let previous = policy::source_endpoints::prefetch_if_reusable(
        Some(("a", 17)), "b", previous_miss, never_prefetch,
    );
    assert(previous == Ok::<(), u8>(()));

    let candidate_miss = |oid: &str| -> (yes: bool)
        ensures yes == (oid@ == "a"@),
    { oid.to_owned() == "a".to_owned() };
    let candidate = policy::source_endpoints::prefetch_if_reusable(
        Some(("a", 17)), "b", candidate_miss, never_prefetch,
    );
    assert(candidate == Ok::<(), u8>(()));
}

fn never_reuse(_oid: &str) -> bool
    requires false,
{
    panic!()
}

fn never_prefetch(_base: u8) -> Result<(), u8>
    requires false,
{
    panic!()
}

fn eligible_prefetch_uses_the_observed_previous_endpoint_and_preserves_the_error() {
    let reuse = |_oid: &str| -> (yes: bool) ensures yes, { true };
    let original_failure = |base: u8| -> (out: Result<(), u8>)
        requires base == 17,
        ensures out == Err::<(), u8>(89),
    { Err(89) };
    let result = policy::source_endpoints::prefetch_if_reusable(
        Some(("a", 17)), "b", reuse, original_failure,
    );
    assert(result == Err::<(), u8>(89));
}
}

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

#[cfg(not(verus_keep_ghost))]
#[test]
fn coverage_authenticates_persisted_wire_bytes_before_reading_git() {
    use chrono_harness::wire;
    use serde_json::{Value, json, value::RawValue};

    // This request ID was computed independently using RFC 8785's ECMAScript number
    // spelling and UTF-16 key order. Its observations retain the original JSON text.
    let stored = include_str!("fixtures/sealed-wire-request.json");
    let request: wire::Request = serde_json::from_str(stored).unwrap();
    request.validate().unwrap();
    let raw: &RawValue = serde_json::from_str(stored).unwrap();
    let canonical = request.canonical().unwrap();
    assert_eq!(wire::canonical(&raw).unwrap(), canonical);
    assert_eq!(wire::canonical(&request).unwrap(), canonical);

    let outer = json!({
        "previous": null,
        "candidate": {"commit": "b".repeat(40), "filemap": {}, "projects": {}, "artifacts": []},
        "affected": [], "scopes": [], "invocations": [],
        "git": {"mode": "request", "request": serde_json::from_str::<Value>(stored).unwrap()}
    });
    // With no outer base, the endpoint check must reject the authenticated request
    // before Git can be read. An incompatible request ID fails earlier in validation.
    let bytes = serde_json::to_vec(&outer).unwrap();
    assert_eq!(
        policy::serve(std::path::Path::new("."), bytes.as_slice(), Vec::new()).unwrap_err(),
        "E_GIT_FACTS: source coverage endpoints differ from request"
    );
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn raw_observation_objects_retain_their_wire_type() {
    use chrono_harness::wire;
    use serde_json::value::RawValue;

    let object: &RawValue =
        serde_json::from_str(r#"{"$serde_json::private::Number":"4.50"}"#).unwrap();
    let number: &RawValue = serde_json::from_str("4.50").unwrap();
    assert_eq!(
        wire::canonical(&[object]).unwrap(),
        br#"[{"$serde_json::private::Number":"4.50"}]"#
    );
    assert_eq!(wire::canonical(&[number]).unwrap(), b"[4.5]");
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn generic_identity_preserves_typed_numbers_keys_and_errors() {
    use chrono_harness::wire::canonical;
    use serde::Serialize;
    use std::collections::BTreeMap;

    #[derive(Serialize)]
    enum Event {
        Limit(u128),
    }
    assert_eq!(
        canonical(&BTreeMap::from([(u64::MAX, Event::Limit(u128::MAX))])).unwrap(),
        br#"{"18446744073709552000":{"Limit":3.402823669209385e+38}}"#
    );
    assert_eq!(
        canonical(&[0.1_f32, f32::NAN]).unwrap(),
        b"[0.10000000149011612,null]"
    );
    assert!(canonical(&f64::INFINITY).is_err());
    assert!(canonical(&i128::MAX).is_err());
    assert_eq!(
        canonical(&Some(i128::MIN)).unwrap(),
        b"-1.7014118346046923e+38"
    );
}

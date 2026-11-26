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

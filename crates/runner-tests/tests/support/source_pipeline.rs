use super::identifiers::equal_text;
use super::policy::source_pipeline as policy;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, vpanic as panic};

verus! {
fn expect_continuity_bool(actual: bool)
    requires actual,
{ if !actual { panic!(); } }

#[cfg_attr(not(verus_keep_ghost), test)]
fn route_coverage_preserves_the_inventory_error_and_the_selected_roots_result() {
    let bad_inventory = policy::after_inventory(Err(1u8), || -> (r: Result<(), u8>)
        ensures r == Err::<(), u8>(2u8),
    { Err(2) });
    expect_continuity_bool(matches!(bad_inventory, Err(1)));
    let missing_root = policy::after_inventory(Ok(()), || -> (r: Result<(), u8>)
        ensures r == Err::<(), u8>(2u8),
    { Err(2) });
    expect_continuity_bool(matches!(missing_root, Err(2)));
    let complete = policy::after_inventory(Ok(()), || -> (r: Result<(), u8>)
        ensures r.is_ok(),
    { Ok(()) });
    expect_continuity_bool(complete.is_ok());
}

struct InventoryRow { path: String }

impl policy::InventoryPath for InventoryRow {
    closed spec fn path_view(&self) -> Seq<char> { self.path@ }
    fn path(&self) -> (out: &str) { self.path.as_str() }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn path_selection_keeps_inventory_order_and_repetitions() {
    let rows = vec![
        InventoryRow { path: "b".to_string() }, InventoryRow { path: "x".to_string() },
        InventoryRow { path: "a".to_string() }, InventoryRow { path: "b".to_string() },
    ];
    let nodes = vec!["file:a".to_string(), "project:x".to_string(), "file:b".to_string()];
    proof {
        reveal_strlit("file:"); reveal_strlit("file:a"); reveal_strlit("file:b");
        reveal_strlit("file:x"); reveal_strlit("project:x"); reveal_strlit("a"); reveal_strlit("b"); reveal_strlit("x");
        assert("file:a"@ =~= "file:"@ + "a"@);
        assert("file:b"@ =~= "file:"@ + "b"@);
        assert("file:x"@ =~= "file:"@ + "x"@);
        assert("file:a"@[5] != "file:x"@[5]);
        assert("file:b"@[5] != "file:x"@[5]);
        assert("project:x"@.len() != "file:x"@.len());
        assert forall|j: int| 0 <= j < nodes.len() implies (#[trigger] nodes[j])@ != "file:x"@ by {
            if j == 0 {} else if j == 1 {} else { assert(j == 2); }
        }
        assert(nodes[0]@ == "file:"@ + "a"@);
        assert(nodes[2]@ == "file:"@ + "b"@);
        assert(exists|j: int| 0 <= j < nodes.len() && (#[trigger] nodes[j])@ == "file:"@ + "a"@);
        assert(exists|j: int| 0 <= j < nodes.len() && (#[trigger] nodes[j])@ == "file:"@ + "b"@);
        reveal_with_fuel(policy::selected, 6);
        assert(policy::selected(rows@, nodes@, 1) =~= seq!["b"@]);
        assert(policy::selected(rows@, nodes@, 2) =~= seq!["b"@]);
        assert(policy::selected(rows@, nodes@, 3) =~= seq!["b"@, "a"@]);
        assert(policy::selected(rows@, nodes@, 4) =~= seq!["b"@, "a"@, "b"@]);
    }
    let chosen = policy::continuity_paths(&rows, Some(true), &nodes);
    expect_continuity_bool(chosen.len() == 3);
    expect_continuity_bool(equal_text(chosen[0].as_str(), "b"));
    expect_continuity_bool(equal_text(chosen[1].as_str(), "a"));
    expect_continuity_bool(equal_text(chosen[2].as_str(), "b"));
    let disabled = policy::continuity_paths(&rows, Some(false), &nodes);
    let absent = policy::continuity_paths(&rows, None, &nodes);
    expect_continuity_bool(disabled.is_empty() && absent.is_empty());
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn continuity_pipeline_preserves_both_endpoints_paths_and_observed_evidence() {
    let (trace, result) = policy::with_source_continuity(
        Vec::<u8>::new(), 11u8, 22u8, 33u8,
        |mut state: Vec<u8>, previous: u8, candidate: u8, paths: u8| -> (out: (Vec<u8>, Result<(u8,u8,u8), String>))
            ensures out.0@ == state@.push(1u8).push(previous).push(candidate).push(paths),
                out.1 == Ok::<(u8,u8,u8), String>((previous,candidate,paths)),
        {
            state.push(1); state.push(previous); state.push(candidate); state.push(paths);
            (state, Ok((previous,candidate,paths)))
        },
        |mut state: Vec<u8>, evidence: (u8,u8,u8)| -> (out: (Vec<u8>, Result<(u8,u8,u8), String>))
            ensures out.0@ == state@.push(2u8).push(evidence.0).push(evidence.1).push(evidence.2),
                out.1 == Ok::<(u8,u8,u8), String>(evidence),
        {
            state.push(2); state.push(evidence.0); state.push(evidence.1); state.push(evidence.2);
            (state, Ok(evidence))
        },
    );
    expect_continuity_bool(trace.len() == 8);
    expect_continuity_bool(trace[0] == 1 && trace[1] == 11 && trace[2] == 22 && trace[3] == 33);
    expect_continuity_bool(trace[4] == 2 && trace[5] == 11 && trace[6] == 22 && trace[7] == 33);
    let exact = match result { Ok(value) => value.0 == 11 && value.1 == 22 && value.2 == 33, Err(_) => false };
    expect_continuity_bool(exact);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn continuity_observation_failure_keeps_the_first_error_and_skips_validation() {
    let (trace, result) = policy::with_source_continuity(
        Vec::<u8>::new(), (), (), (),
        |mut state: Vec<u8>, _previous: (), _candidate: (), _paths: ()| -> (out: (Vec<u8>, Result<u8, String>))
            ensures out.0@ == state@.push(1u8),
                match out.1 { Err(error) => error@ == "原读取错误\0é"@, Ok(_) => false },
        { state.push(1); (state, Err("原读取错误\0é".to_string())) },
        |mut state: Vec<u8>, evidence: u8| -> (out: (Vec<u8>, Result<u8, String>))
            ensures out.0@ == state@.push(9u8), out.1 == Ok::<u8,String>(evidence),
        { state.push(9); (state, Ok(evidence)) },
    );
    expect_continuity_bool(trace.len() == 1 && trace[0] == 1);
    let exact = match result { Err(error) => equal_text(error.as_str(), "原读取错误\0é"), Ok(_) => false };
    expect_continuity_bool(exact);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn continuity_validation_failure_keeps_its_full_original_error() {
    let (trace, result) = policy::with_source_continuity(
        Vec::<u8>::new(), (), (), 7u8,
        |mut state: Vec<u8>, _previous: (), _candidate: (), paths: u8| -> (out: (Vec<u8>, Result<u8, String>))
            ensures out.0@ == state@.push(1u8), out.1 == Ok::<u8,String>(paths),
        { state.push(1); (state, Ok(paths)) },
        |mut state: Vec<u8>, evidence: u8| -> (out: (Vec<u8>, Result<u8, String>))
            ensures out.0@ == state@.push(evidence),
                match out.1 { Err(error) => error@ == "原判定错误\0é"@, Ok(_) => false },
        { state.push(evidence); (state, Err("原判定错误\0é".to_string())) },
    );
    expect_continuity_bool(trace.len() == 2 && trace[0] == 1 && trace[1] == 7);
    let exact = match result { Err(error) => equal_text(error.as_str(), "原判定错误\0é"), Ok(_) => false };
    expect_continuity_bool(exact);
}
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn source_plan_checks_the_concrete_inventory_before_the_affected_file() {
    use chrono_harness::verified_verifier::sources::{
        SourceFile, continuity::Continuity, source_registry,
    };
    use policy::validate_source_plan;
    let inventory = source_registry(
        vec![SourceFile {
            path: "x".into(),
            verification: None,
        }],
        Some(true),
    );
    let nodes = vec!["file:x".to_owned()];
    let evidence = Continuity::empty();
    let check = |previous| {
        validate_source_plan(
            &inventory,
            previous,
            &nodes,
            &Vec::new(),
            &Vec::new(),
            &evidence,
        )
    };
    assert_eq!(
        check(Err("原基线错误\0é")).unwrap_err().path,
        "原基线错误\0é"
    );
    let missing = check(Ok(Some(&inventory))).unwrap_err();
    assert_eq!(
        missing.message,
        "E_VERUS_COVERAGE: affected file lacks explicit verification classification: "
    );
    assert_eq!(missing.path, "x");
    let empty = source_registry(Vec::new(), Some(true));
    assert!(
        validate_source_plan(
            &empty,
            Ok(None),
            &Vec::new(),
            &Vec::new(),
            &Vec::new(),
            &evidence
        )
        .is_ok()
    );
}

//! Git object identities shared by checkout and retained registry blobs.
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/checkout.rs"]
mod checkout;
#[cfg(verus_keep_ghost)]
#[path = "support/cli_child.rs"]
mod cli_child;
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/facts_binding.rs"]
mod facts_binding;
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/git_facts_session.rs"]
mod git_facts_session;
#[cfg(verus_keep_ghost)]
#[path = "../../proofs/identifiers/src/lib.rs"]
mod identifiers;
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/initial.rs"]
mod initial;
#[cfg(verus_keep_ghost)]
#[path = "protocol_v1.rs"]
mod protocol;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, vpanic as panic};

#[cfg(verus_keep_ghost)]
verus! {
fn require_observed(value: bool) requires value, { if !value { panic!(); } }

fn session_reads_exact_payload_and_checks_current_checkout() {
    use git_facts_session::{complete_frame, require_checkout_config, require_checkout_head};
    require_observed(git_facts_session::FRAME_LIMIT == 67_108_864);
    require_observed(matches!(complete_frame(Vec::new(), 0), Ok(None)));
    let newline = vec![10u8];
    match complete_frame(newline, 0) {
        Ok(Some(line)) => require_observed(line.len() == 0),
        _ => vstd::vpanic!(),
    }
    let line = vec![0u8, 13u8, 10u8];
    match complete_frame(line, 2) {
        Ok(Some(payload)) => {
            require_observed(payload.len() == 2);
            require_observed(payload[0] == 0 && payload[1] == 13);
        },
        _ => vstd::vpanic!(),
    }
    require_observed(matches!(complete_frame(vec![0u8, 13u8, 10u8], 1), Err(_)));
    require_observed(matches!(complete_frame(vec![0u8, 13u8], 2), Err(_)));
    proof { reveal_strlit("same"); reveal_strlit("sand"); assert("same"@[2] != "sand"@[2]); }
    require_observed(require_checkout_head("same", "same").is_ok());
    require_observed(require_checkout_head("same", "sand").is_err());
    let same = [0u8, 255u8];
    let changed = [0u8, 254u8];
    proof { assert(same@[1] != changed@[1]); }
    require_observed(require_checkout_config(same.as_slice(), same.as_slice()).is_ok());
    require_observed(require_checkout_config(same.as_slice(), changed.as_slice()).is_err());
}

fn initial_inventory_keeps_original_profile_and_requires_first_commit() {
    let bytes = [0u8, 128u8];
    let changed = [0u8, 129u8];
    proof { assert(bytes@[1] != changed@[1]); }
    require_observed(initial::check_initial_profile(bytes.as_slice(), bytes.as_slice()).is_ok());
    require_observed(initial::check_initial_profile(bytes.as_slice(), changed.as_slice()).is_err());
    require_observed(initial::require_parentless(0).is_ok());
    require_observed(initial::require_parentless(1).is_err());
    require_observed(initial::require_parentless(2).is_err());
    require_observed(identifiers::equal_text(initial::inventory_status(0), "complete"));
    require_observed(identifiers::equal_text(initial::inventory_status(1), "failed"));
    require_observed(identifiers::equal_text(initial::inventory_status(2), "error"));
}

fn empty_and_leading_zero_bytes_keep_the_full_lowercase_width() {
    let empty: [u8; 0] = [];
    let no_text = checkout::object_id_ascii(empty.as_slice());
    require_observed(no_text.len() == 0);
    let sample = [0u8, 15u8, 160u8, 255u8];
    let bytes = sample.as_slice();
    let text = checkout::object_id_ascii(bytes);
    proof {
        assert(bytes@[0] == 0u8);
        assert(bytes@[1] == 15u8);
        assert(bytes@[2] == 160u8);
        assert(bytes@[3] == 255u8);
    }
    require_observed(text.len() == 8);
    require_observed(text[0] == 48 && text[1] == 48);
    require_observed(text[2] == 48 && text[3] == 102);
    require_observed(text[4] == 97 && text[5] == 48);
    require_observed(text[6] == 102 && text[7] == 102);
}
}

#[cfg(not(verus_keep_ghost))]
#[test]
fn checkout_matches_known_git_blob_ids_in_both_object_formats() {
    use chrono_harness::facts::{checkout_changes, parse_tree};

    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("sample"), b"git blob\0\xff\n").unwrap();
    for oid in [
        "332dba9b9f76e45de37dda68eaa0b4edb077b1ce",
        "b772e763f617257c0bf0a8bddbcc072ac46adebfd7dc14afb242db31cd0bdc03",
    ] {
        let tree = parse_tree(format!("100644 blob {oid}\tsample\0").as_bytes()).unwrap();
        let index = format!("100644 {oid} 0\tsample\0");
        assert!(
            checkout_changes(root.path(), &tree, index.as_bytes())
                .unwrap()
                .is_empty()
        );
    }
}

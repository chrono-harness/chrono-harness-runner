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
#[path = "../../proofs/identifiers/src/lib.rs"]
mod identifiers;
#[cfg(verus_keep_ghost)]
#[path = "protocol_v1.rs"]
mod protocol;
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, vpanic as panic};

#[cfg(verus_keep_ghost)]
verus! {
fn require_observed(value: bool) requires value, { if !value { panic!(); } }

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

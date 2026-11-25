//! Independent admission examples for sealed member disposal.
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/retained_artifacts/manifest.rs"]
mod accounting;
#[cfg(not(verus_keep_ghost))]
use chrono_harness::retained_artifacts::retention as accounting;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Non-capacity failures keep their original text, including the capacity diagnostics.
#[cfg(verus_keep_ghost)]
fn overflow_requires_a_tree_and_a_local_capacity_error(message: String) {
    use accounting::SealError;
    let directory = SealError::DirectoryCapacity;
    let entries = SealError::TreeCapacity;
    let other = SealError::Observation(message);
    let ordinary_directory = directory.permits_overflow(false);
    let ordinary_entries = entries.permits_overflow(false);
    let ordinary_other = other.permits_overflow(false);
    let tree_directory = directory.permits_overflow(true);
    let tree_entries = entries.permits_overflow(true);
    let tree_other = other.permits_overflow(true);
    assert(!ordinary_directory && !ordinary_entries && !ordinary_other);
    assert(tree_directory && tree_entries && !tree_other);
    let reported_directory = directory.into_message();
    let reported_entries = entries.into_message();
    let reported_other = other.into_message();
    assert(reported_directory@ == "directory identity capacity; preserve original"@);
    assert(reported_entries@ == "tree identity manifest capacity; preserve original"@);
    assert(reported_other@ == message@);
}

/// Raw observed identities and timestamp bytes are the only inputs to eligibility.
fn sealed_member_boundaries() -> (accepted: bool)
    ensures accepted,
{
    let f: &[u8] = &[49, 58, 102, 105, 108, 101]; // 1:file
    let replaced: &[u8] = &[50, 58, 102, 105, 108, 101]; // 2:file
    let dir: &[u8] = &[49, 58, 100, 105, 114]; // 1:dir
    let link: &[u8] = &[49, 58, 108, 105, 110, 107]; // 1:link
    let recorded: &[u8] = &[57, 58, 50, 58, 51, 58, 52, 58, 53]; // 9:2:3:4:5
    let ctime: &[u8] = &[57, 58, 50, 58, 51, 58, 54, 58, 55]; // 9:2:3:6:7
    let data: &[u8] = &[57, 58, 50, 58, 52, 58, 52, 58, 53]; // 9:2:4:4:5
    let malformed: &[u8] = &[57, 58, 50]; // 9:2
    let other: &[u8] = &[57, 58, 56]; // 9:8
    let equal = accounting::sealed_member_matches(f, f, false, recorded, recorded, false);
    let directory = accounting::sealed_member_matches(dir, dir, true, recorded, data, false);
    let new_file = accounting::sealed_member_matches(f, replaced, false, recorded, recorded, true);
    let wrong_type = accounting::sealed_member_matches(dir, f, true, recorded, recorded, true);
    let single = accounting::sealed_member_matches(f, f, false, recorded, ctime, false);
    let batch = accounting::sealed_member_matches(f, f, false, recorded, ctime, true);
    let modified = accounting::sealed_member_matches(f, f, false, recorded, data, true);
    let symlink = accounting::sealed_member_matches(link, link, false, recorded, ctime, true);
    let incomplete = accounting::sealed_member_matches(f, f, false, malformed, other, true);
    let equal_malformed = accounting::sealed_member_matches(f, f, false, malformed, malformed, true);
    proof {
        reveal_with_fuel(accounting::previous_colon, 12);
        assert(equal);
        assert(directory);
        assert(!new_file);
        assert(!wrong_type);
        assert(!single);
        assert(batch);
        assert(recorded@.take(5)[4] != data@.take(5)[4]);
        assert(!modified);
        assert(!symlink);
        assert(!incomplete);
        assert(equal_malformed);
    }
    equal && directory && !new_file && !wrong_type && !single && batch && !modified
        && !symlink && !incomplete && equal_malformed
}

}

#[cfg(not(verus_keep_ghost))]
#[test]
fn sealed_tree_decisions_use_the_observed_bytes() {
    assert!(sealed_member_boundaries());
}

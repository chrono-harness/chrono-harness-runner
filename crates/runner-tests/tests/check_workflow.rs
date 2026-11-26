//! Host contract for the deployed runner self-check workflow.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
fn deployed_workflow() -> &'static str { include_str!("../../../.github/workflows/check.yml") }

fn is_hex(byte: u8) -> (valid: bool)
    ensures valid == ((48 <= byte <= 57) || (97 <= byte <= 102)),
{
    (48u8 <= byte && byte <= 57u8) || (97u8 <= byte && byte <= 102u8)
}

fn full_commit(pin: &[u8]) -> (valid: bool)
    ensures valid == (pin.len() == 40
        && (forall|i: int| 0 <= i < pin.len() ==> (48 <= #[trigger] pin@[i] <= 57 || 97 <= pin@[i] <= 102))
        && (exists|i: int| 0 <= i < pin.len() && #[trigger] pin@[i] != 48)),
{
    if pin.len() != 40 { return false; }
    let mut i = 0usize;
    let mut nonzero = false;
    while i < pin.len()
        invariant i <= pin.len(), pin.len() == 40,
            forall|j: int| 0 <= j < i ==> (48 <= #[trigger] pin@[j] <= 57 || 97 <= pin@[j] <= 102),
            nonzero == (exists|j: int| 0 <= j < i && #[trigger] pin@[j] != 48),
        decreases pin.len() - i,
    {
        if !is_hex(pin[i]) { return false; }
        nonzero = nonzero || pin[i] != 48u8;
        i += 1;
    }
    nonzero
}

// SHA-256 of the reviewed YAML and shell with each action's commit replaced by @COMMIT.
fn matches_deployed_contract(digest: &String) -> (matches: bool)
    ensures matches == (digest@ == "012a89a7d8024b091b96a293e00347eac50cada9704eef2abc531d83f8a82a04"@),
{
    *digest == "012a89a7d8024b091b96a293e00347eac50cada9704eef2abc531d83f8a82a04".to_string()
}
}

#[cfg(not(verus_keep_ghost))]
mod native {
    use super::*;
    use sha2::{Digest, Sha256};

    fn accepts(workflow: &str) -> bool {
        let mut hash = Sha256::new();
        for line in workflow.split_inclusive('\n') {
            if let Some(reference) = line.strip_prefix("      - uses: ") {
                let Some((name, pin)) = reference.trim_end_matches('\n').split_once('@') else {
                    return false;
                };
                if !line.ends_with('\n') || !full_commit(pin.as_bytes()) {
                    return false;
                }
                hash.update(b"      - uses: ");
                hash.update(name.as_bytes());
                hash.update(b"@COMMIT\n");
            } else {
                hash.update(line.as_bytes());
            }
        }
        let digest = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        matches_deployed_contract(&digest)
    }

    #[test]
    fn deployed_workflow_allows_only_full_commits_on_its_selected_actions() {
        let workflow = deployed_workflow();
        assert!(accepts(workflow));
        for line in workflow
            .lines()
            .filter(|line| line.starts_with("      - uses: "))
        {
            let (source, _) = line.split_once('@').unwrap();
            for (pin, expected) in [
                ("0123456789abcdef0123456789abcdef01234567", true),
                ("0123456", false),
                ("v4", false),
                ("0000000000000000000000000000000000000000", false),
                ("g123456789abcdef0123456789abcdef01234567", false),
            ] {
                assert_eq!(
                    accepts(&workflow.replacen(line, &format!("{source}@{pin}"), 1)),
                    expected
                );
            }
        }
        assert!(!accepts(
            &workflow.replace("actions/checkout@", "other/checkout@")
        ));
    }

    #[test]
    fn deployed_workflow_binds_event_revisions_check_order_and_receipts() {
        let workflow = deployed_workflow();
        for (before, after) in [
            ("candidate=$(git rev-parse HEAD)", "candidate=$PR_HEAD"),
            ("base=$(git rev-parse HEAD^1)", "base=$PR_HEAD"),
            ("test \"$(git rev-parse HEAD^2)\" = \"$PR_HEAD\"", "true"),
            ("base=$BEFORE", "base=HEAD~1"),
            (
                "check_change \"$released\"",
                "check_change \"$target/debug/chrono-harness\"",
            ),
            (
                ".response.status == \"passed\" and",
                ".response.status == \"failed\" or",
            ),
            (".id == \"runner.verify\"", ".id == \"different\""),
            ("check_change \"$target/debug/chrono-harness\"", "true"),
            ("! git diff --quiet", "git diff --quiet"),
            (
                ".id == \"verify.sealed-member\"",
                ".id == \"different-proof\"",
            ),
            (
                "failure() || env.RUNNER_CHECK_RECEIPTS",
                "success() || env.RUNNER_CHECK_RECEIPTS",
            ),
            (
                "            .chrono-harness/state/*.operation-*/stderr\n",
                "",
            ),
        ] {
            let changed = workflow.replacen(before, after, 1);
            assert_ne!(changed, workflow, "{before}");
            assert!(!accepts(&changed), "{before}");
        }
    }
}

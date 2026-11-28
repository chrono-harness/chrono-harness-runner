//! Deployment checks for the files selected by this host.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// A mise setup installs tools first and refreshes the rules only when selected.
pub fn setup_sequence(commands: &[String], instructions: bool) -> (accepted: bool)
    ensures accepted == (commands.len() == (if instructions { 2usize } else { 1usize })
        && commands@[0]@ == "sh .chrono-harness/install-tools.sh"@
        && (!instructions || commands@[1]@ == "mise exec -- chrono-instructions sync-rules --host-root ."@)),
{
    if commands.len() != (if instructions { 2usize } else { 1usize }) { return false; }
    commands[0] == "sh .chrono-harness/install-tools.sh".to_string() && (!instructions
        || commands[1] == "mise exec -- chrono-instructions sync-rules --host-root .".to_string())
}

pub fn mise_manifest() -> (s: &'static str) {
    include_str!("../../.config/mise/conf.d/chrono-harness.toml")
}

/// The template renders these same rows into both mise platform tables.
pub fn deployed_tool_installer() -> (source: &'static str) {
    include_str!("../install-tools.sh")
}

pub fn installer_digest_matches(digest: &String) -> (accepted: bool)
    ensures accepted == (digest@ == "3b3476a09d8a7e8062bbcd527d18578897da63b409f4cba58e1ad3942709145b"@),
{
    *digest == "3b3476a09d8a7e8062bbcd527d18578897da63b409f4cba58e1ad3942709145b".to_string()
}

/// Both deployed files are inputs to this proof compilation and its native test.
pub fn deployed_roadmap() -> (&'static str, &'static str) {
    (include_str!("../../.github/roadmap.cjs"), include_str!("../../.github/workflows/issue-roadmap.yml"))
}

pub fn deployed_roadmap_lock() -> &'static str {
    include_str!("../../.github/workflows/actions.lock")
}

pub fn roadmap_repository(checkout: bool, owner_id: u64, repo_id: u64) -> (accepted: bool)
    ensures accepted == (owner_id == 44036562 && ((checkout && repo_id == 197814629)
        || (!checkout && repo_id == 205262760))),
{
    owner_id == 44036562 && repo_id == (if checkout { 197814629 } else { 205262760 })
}
/// The native caller hashes the actual UTF-8 bytes embedded above.
pub fn roadmap_matches(script: &String, workflow: &String) -> (accepted: bool)
    ensures accepted == (script@ == "86e97ab0ed63be36d37d32e7d164da4e226c6f616685ad4ce1dd75f814095f8a"@ && workflow@ == "b305ce0fec7db541582192bcacf82f82049e6628762a408174b9d757d9f36337"@),
{
    *script == "86e97ab0ed63be36d37d32e7d164da4e226c6f616685ad4ce1dd75f814095f8a".to_string() && *workflow == "b305ce0fec7db541582192bcacf82f82049e6628762a408174b9d757d9f36337".to_string()
}
}

#[cfg(not(verus_keep_ghost))]
mod native {
    use super::*;
    use serde_yaml::Value;

    #[test]
    fn deployed_tool_installer_matches_the_selected_template() {
        use sha2::{Digest, Sha256};
        let digest = Sha256::digest(deployed_tool_installer().as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(
            installer_digest_matches(&digest),
            "install-tools.sh differs from the selected template"
        );
    }

    fn roadmap_digest(source: &str) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(source.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn locked_roadmap_reference<'a>(
        lock: &Value,
        workflow: &'a [Value],
        repo: &str,
        checkout: bool,
    ) -> Option<&'a str> {
        let mut matched = None;
        for item in workflow {
            let name = item.as_str()?;
            let (source, reference) = name.split_once('@')?;
            if !source.eq_ignore_ascii_case(repo) {
                continue;
            }
            if source != repo || matched.is_some() {
                return None;
            }
            let entry = lock["dependencies"].get(name)?;
            let sha = entry["commit"].as_str()?.strip_prefix("sha1-")?;
            if reference.is_empty()
                || !reference
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
                || entry["ref"].as_str() != Some(reference)
                || entry
                    .get("hostname")
                    .is_some_and(|v| v.as_str() != Some("github.com"))
                || sha.len() != 40
                || !sha
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || sha.bytes().all(|b| b == b'0')
                || !roadmap_repository(
                    checkout,
                    entry["owner_id"].as_u64()?,
                    entry["repo_id"].as_u64()?,
                )
            {
                return None;
            }
            matched = Some(name);
        }
        matched
    }

    fn roadmap_structure(workflow: &str, lock: &str) -> Option<String> {
        let lock: Value = serde_yaml::from_str(lock).ok()?;
        if lock["version"].as_str()? != "v0.0.3" {
            return None;
        }
        let mapping = lock["workflows"][".github/workflows/issue-roadmap.yml"].as_sequence()?;
        let checkout = locked_roadmap_reference(&lock, mapping, "actions/checkout", true)?;
        let script = locked_roadmap_reference(&lock, mapping, "actions/github-script", false)?;
        let mut projected = String::with_capacity(workflow.len());
        for line in workflow.split_inclusive('\n') {
            if let Some(value) = line.strip_prefix("      - uses: ") {
                let repository = match value.strip_suffix('\n')? {
                    v if v == checkout => "actions/checkout",
                    v if v == script => "actions/github-script",
                    _ => return None,
                };
                projected.push_str("      - uses: ");
                projected.push_str(repository);
                projected.push_str("@LOCKED\n");
            } else {
                projected.push_str(line);
            }
        }
        Some(projected)
    }

    #[test]
    fn locked_roadmap_rejects_changes_to_its_deployment_contract() {
        let (script, workflow) = deployed_roadmap();
        let lock = deployed_roadmap_lock();
        let script_hash = roadmap_digest(script);
        let accepted = |candidate: &str, lock: &str| {
            roadmap_structure(candidate, lock)
                .is_some_and(|structure| roadmap_matches(&script_hash, &roadmap_digest(&structure)))
        };
        for (expected, changed) in [
            ("actions/checkout@", "other/checkout@"),
            ("contents: read", "contents: write"),
            ("    steps:\n", "    steps:\n      - run: echo extra\n"),
            (".releaseIssue(", ".different("),
        ] {
            let edit = workflow.replacen(expected, changed, 1);
            assert_ne!(edit, workflow);
            assert!(!accepted(&edit, lock));
        }
        for expected in [
            ".github/workflows/issue-roadmap.yml",
            "197814629",
            "205262760",
        ] {
            let edit = lock.replace(expected, "absent");
            assert_ne!(edit, lock);
            assert!(!accepted(workflow, &edit));
        }
        assert!(!accepted(workflow, ""));
    }

    #[test]
    fn deployed_roadmap_matches_the_selected_template() {
        let (script, workflow) = deployed_roadmap();
        let script_hash = roadmap_digest(script);
        let structure = roadmap_structure(workflow, deployed_roadmap_lock())
            .expect("the roadmap action references must belong to their GitHub workflow lock");
        let workflow_hash = roadmap_digest(&structure);
        assert!(
            roadmap_matches(&script_hash, &workflow_hash),
            "roadmap.cjs or issue-roadmap.yml differs from the selected template"
        );
        assert!(!roadmap_matches(&workflow_hash, &workflow_hash));
        assert!(!roadmap_matches(&script_hash, &script_hash));
    }
    fn setup_matches(source: &str, instructions: bool) -> bool {
        let Ok(doc) = source.parse::<toml::Table>() else {
            return false;
        };
        let instruction_tool = doc
            .get("tools")
            .and_then(toml::Value::as_table)
            .is_some_and(|tools| tools.contains_key("http:chrono-instructions"));
        let Some(setup) = doc
            .get("tasks")
            .and_then(|tasks| tasks.get("harness-setup"))
            .and_then(toml::Value::as_table)
        else {
            return false;
        };
        if instruction_tool != instructions
            || setup.keys().any(|key| key != "run" && key != "description")
        {
            return false;
        }
        let commands: Vec<String> = match setup.get("run") {
            Some(toml::Value::String(one)) => vec![one.clone()],
            Some(toml::Value::Array(steps)) => {
                let Some(all) = steps
                    .iter()
                    .map(|value| value.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                else {
                    return false;
                };
                all
            }
            _ => return false,
        };
        setup_sequence(&commands, instructions)
    }

    #[test]
    fn mise_installs_the_selected_tools_before_refreshing_instructions() {
        let original = mise_manifest();
        let selected = true;
        assert!(setup_matches(original, selected));
        assert!(!setup_matches(
            &original.replace(
                "sh .chrono-harness/install-tools.sh",
                "sh .chrono-harness/install-tools.sh --dry-run"
            ),
            selected
        ));
        assert!(!setup_matches(
            &original.replace(
                "[tasks.harness-setup]",
                "[tasks.harness-setup]\ndepends = ['other']"
            ),
            selected
        ));
        let install_only =
            "[tools]\n[tasks.harness-setup]\nrun = 'sh .chrono-harness/install-tools.sh'";
        assert!(setup_matches(install_only, false));
        assert!(!setup_matches(install_only, true));
        let reverse = "[tools.'http:chrono-instructions']\n[tasks.harness-setup]\nrun=['mise exec -- chrono-instructions sync-rules --host-root .','sh .chrono-harness/install-tools.sh']";
        assert!(!setup_matches(reverse, true));
        let extra = "[tools]\n[tasks.harness-setup]\nrun = ['sh .chrono-harness/install-tools.sh', 'another-command']";
        assert!(!setup_matches(extra, false));
    }
}

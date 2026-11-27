//! Factual process/tool observations, without expected-version policy or work selection.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// The original process must succeed before we compare its bytes, text and environment.
/// The digest operation can return its own error after the two streams match.
pub fn retained_process_matches<D, T, E>(
    exit_code: i32, failed: bool, digests: D, texts: T, environment: E,
) -> (out: Result<bool, String>)
    where D: FnOnce() -> bool, T: FnOnce() -> bool, E: FnOnce() -> Result<bool, String>,
    requires digests.requires(()), texts.requires(()), environment.requires(()),
    ensures if exit_code != 0 || failed {
        out == Ok::<bool, String>(false)
    } else {
        exists|hashes: bool| #[trigger] digests.ensures((), hashes)
            && if !hashes { out == Ok::<bool, String>(false) } else {
                exists|readable: bool| #[trigger] texts.ensures((), readable)
                    && if !readable { out == Ok::<bool, String>(false) } else { environment.ensures((), out) }
            }
    },
{
    if exit_code != 0 || failed || !digests() || !texts() { return Ok(false); }
    environment()
}
}

#[cfg(not(verus_keep_ghost))]
use crate::{CommandSpec, ProcessResult, resolve_program, run_process_observed, sha256};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tool {
    pub program: String,
    pub path: PathBuf,
    pub basename: String,
    pub sha256: String,
    pub version: ProcessResult,
}
#[cfg(not(verus_keep_ghost))]
pub fn tool(
    root: &Path,
    program: &str,
    version_argv: &[String],
    env: &BTreeMap<String, String>,
    timeout: u64,
    limit: usize,
) -> Result<Tool, String> {
    if !program.contains('/') && !env.contains_key("PATH") {
        return Err("tool resolution requires declared PATH".into());
    }
    // Empty PATH is explicit. resolve_program must not consult ambient PATH here.
    let path = resolve_program(
        root,
        program,
        Some(env.get("PATH").map(String::as_str).unwrap_or("")),
    )?;
    let hash = sha256(&fs::read(&path).map_err(|e| e.to_string())?);
    let spec = CommandSpec {
        program: path.to_str().ok_or("tool path UTF-8")?.into(),
        args: version_argv.to_vec(),
        env: env.clone(),
        timeout_seconds: timeout,
        output_limit_bytes: limit,
    };
    let version = run_process_observed(root, &spec, &[], &hash)?;
    Ok(Tool {
        program: program.into(),
        basename: path
            .file_name()
            .ok_or("tool basename")?
            .to_str()
            .ok_or("tool basename UTF-8")?
            .into(),
        path,
        sha256: hash,
        version,
    })
}

/// Pure success and byte correspondence; no invocation or expected-version policy.
#[cfg(not(verus_keep_ghost))]
pub fn process_success(p: &ProcessResult) -> Result<(), String> {
    if !retained_process_matches(
        p.exit_code,
        p.failure.is_some(),
        || p.stdout_sha256 == sha256(&p.stdout_bytes) && p.stderr_sha256 == sha256(&p.stderr_bytes),
        || {
            p.stdout == String::from_utf8_lossy(&p.stdout_bytes)
                && p.stderr == String::from_utf8_lossy(&p.stderr_bytes)
        },
        || Ok(p.environment_digest == crate::wire::digest(&p.environment)?),
    )? {
        return Err("E_PROCESS_EVIDENCE: unsuccessful or inconsistent retained process".into());
    }
    Ok(())
}

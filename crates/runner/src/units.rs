//! Typed scoped check selection and explicit report inputs. No test discovery.
#[cfg(verus_keep_ghost)]
use super::identifiers;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub const PROFILE: &'static str = "chrono-ci-check/v3";

/// The outer option is field presence. The inner option is JSON string decoding.
pub fn publication_mode(schema: &str, field: Option<Option<&str>>)
    -> (result: Result<bool, &'static str>)
    ensures
        match field {
            None => result == Ok::<bool, &str>(false),
            Some(Some(mode)) if schema@ == "chrono-ci-check/v3"@ && mode@ == "retained-reference/v2"@
                => result == Ok::<bool, &str>(true),
            _ => result == Err::<bool, &str>("report_publication requires scoped v3 retained-reference/v2"),
        },
{
    match field {
        None => Ok(false),
        Some(Some(mode)) if identifiers::equal_text(schema, PROFILE)
            && identifiers::equal_text(mode, "retained-reference/v2") => Ok(true),
        _ => Err("report_publication requires scoped v3 retained-reference/v2"),
    }
}
}

#[cfg(not(verus_keep_ghost))]
use crate::{CheckConfig, relative_path};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(not(verus_keep_ghost))]
pub const PROTOCOL: &str = "chrono-ci-judge/v2";
#[cfg(not(verus_keep_ghost))]
pub const REPORT_REFERENCE: &str = "chrono-check-reference/v1";

/// A fixed publication slot names one immutable original; it carries no verdict.
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportReference {
    pub schema: String,
    pub original: crate::prepared::Original,
}

#[cfg(not(verus_keep_ghost))]
pub fn reference_publication(config: &CheckConfig) -> Result<bool, String> {
    publication_mode(
        &config.schema,
        config
            .policy
            .get("report_publication")
            .map(serde_json::Value::as_str),
    )
    .map_err(String::from)
}

#[cfg(not(verus_keep_ghost))]
pub use crate::verified_status::scopes::Scope;

#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Unit {
    pub tests: Vec<String>,
    pub report_path: String,
}

#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub reports: Vec<ReportInput>,
}

#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportInput {
    pub unit: String,
    pub path: String,
    pub sha256: String,
    pub runner_sha256: String,
    pub judge_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<crate::prepared::ArtifactTransport>,
}

/// Full collection derives executable pins from the bound candidate registry,
/// never from caller claims copied out of a report.
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullManifest {
    pub schema: String,
    pub reports: Vec<FullReportInput>,
}
#[cfg(not(verus_keep_ghost))]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FullReportInput {
    pub unit: String,
    pub path: String,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifacts: Option<crate::prepared::ArtifactTransport>,
}

#[cfg(not(verus_keep_ghost))]
pub fn required(units: &BTreeMap<String, Unit>, selected: &BTreeSet<String>) -> Vec<String> {
    units
        .iter()
        .filter(|(_, definition)| definition.tests.iter().any(|test| selected.contains(test)))
        .map(|(id, _)| id.clone())
        .collect()
}

/// Read the opt-in full-v3 execution unit block.  Older full registrations do
/// not acquire selector semantics merely because a caller supplied `--unit`.
#[cfg(not(verus_keep_ghost))]
pub fn overlap(a: &str, b: &str) -> bool {
    let (a, b) = (std::path::Path::new(a), std::path::Path::new(b));
    a.starts_with(b) || b.starts_with(a)
}
/// Validate a caller-supplied collection manifest against every registered
/// publication destination. This is the single path owner used by the scoped
/// adapter, full runner and offline collector.
#[cfg(not(verus_keep_ghost))]
pub fn validate_collection_paths(
    manifest: &str,
    report_path: &str,
    unit_paths: &[String],
) -> Result<(), String> {
    artifact_path(manifest)?;
    artifact_path(report_path)?;
    let mut paths = vec![manifest.to_owned(), report_path.to_owned()];
    paths.extend(unit_paths.iter().cloned());
    for (index, path) in paths.iter().enumerate() {
        if paths[..index].iter().any(|prior| overlap(path, prior)) {
            return Err("collection manifest overlaps publication path".into());
        }
    }
    Ok(())
}
#[cfg(not(verus_keep_ghost))]
pub fn report_paths(profile: &serde_json::Value, evidence: Option<&str>) -> Result<(), String> {
    let Some(block) = profile.get("execution_units") else {
        return Ok(());
    };
    let mut reserved = vec![
        ".chrono-harness/state/report.json".to_owned(),
        ".chrono-harness/state/integration.json".to_owned(),
        ".chrono-harness/state/context.json".to_owned(),
        ".chrono-harness/state/inputs.json".to_owned(),
    ];
    if let Some(e) = evidence {
        reserved.push(e.into());
    }
    if let Some(argv) = profile["canonical_check"]["argv"].as_array() {
        for pair in argv.windows(2) {
            if pair[0] == "--context" {
                if let Some(path) = pair[1].as_str() {
                    reserved.push(path.into());
                }
            }
        }
    }
    let mut paths = vec![
        block["report_path"]
            .as_str()
            .ok_or("collection report path")?,
    ];
    for unit in block["units"]
        .as_object()
        .ok_or("unit definitions")?
        .values()
    {
        paths.push(unit["report_path"].as_str().ok_or("unit report path")?);
    }
    for (i, path) in paths.iter().enumerate() {
        if reserved.iter().any(|r| overlap(path, r))
            || paths[..i].iter().any(|p| overlap(path, p))
            || path.starts_with(".chrono-harness/state/run-")
            || path.starts_with(".chrono-harness/state/import-")
            || path.starts_with(".chrono-harness/state/base-")
        {
            return Err(
                "execution report paths overlap reserved evidence or another report".into(),
            );
        }
    }
    Ok(())
}
#[cfg(not(verus_keep_ghost))]
pub fn full_execution_units(
    profile: &serde_json::Value,
) -> Result<Option<serde_json::Value>, String> {
    if !matches!(
        profile
            .get("schema_version")
            .and_then(serde_json::Value::as_u64),
        Some(3 | 4)
    ) {
        return Ok(None);
    }
    let Some(block) = profile.get("execution_units") else {
        return Ok(None);
    };
    report_paths(profile, None)?;
    let object = block
        .as_object()
        .ok_or("execution_units must be an object")?;
    let units_value = object.get("units").ok_or("execution_units.units missing")?;
    let units: BTreeMap<String, Unit> = serde_json::from_value(units_value.clone())
        .map_err(|e| format!("invalid execution_units.units: {e}"))?;
    if units.is_empty() {
        return Err("execution_units.units must be nonempty".into());
    }
    let shared: BTreeMap<String, Vec<String>> = serde_json::from_value(
        object
            .get("shared_operations")
            .cloned()
            .ok_or("execution_units.shared_operations missing")?,
    )
    .map_err(|e| format!("invalid execution_units.shared_operations: {e}"))?;
    let report_path = object
        .get("report_path")
        .and_then(serde_json::Value::as_str)
        .ok_or("execution_units.report_path missing")?;
    artifact_path(report_path)?;
    let collection_limits = object
        .get("collection_limits")
        .cloned()
        .ok_or("execution_units.collection_limits missing")?;
    let limits: BTreeMap<String, u64> = serde_json::from_value(collection_limits.clone())
        .map_err(|e| format!("invalid execution_units.collection_limits: {e}"))?;
    for key in ["manifest_bytes", "report_bytes"] {
        if limits
            .get(key)
            .copied()
            .is_none_or(|n| n == 0 || n > 64 * 1024 * 1024)
        {
            return Err(format!("invalid execution_units limit {key}"));
        }
    }
    for (unit_id, unit) in &units {
        id(unit_id)?;
        if unit.tests.is_empty() {
            return Err(format!("execution unit has no tests: {unit_id}"));
        }
        artifact_path(&unit.report_path)?;
    }
    Ok(Some(serde_json::json!({
        "units": units,
        "shared_operations": shared,
        "collection_limits": collection_limits,
        "report_path": report_path,
    })))
}

#[cfg(not(verus_keep_ghost))]
pub fn id(s: &str) -> Result<(), String> {
    if !crate::verified_identifiers::valid_id(s.as_bytes()) {
        return Err(format!("invalid CI unit ID: {s:?}"));
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
pub fn artifact_path(path: &str) -> Result<(), String> {
    relative_path(path)?;
    if !path.starts_with(".chrono-harness/state/") {
        return Err(
            "unit reports and collection manifests must reside in .chrono-harness/state/".into(),
        );
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
impl Scope {
    pub fn contract_argv(&self, profile: &serde_json::Value) -> Vec<String> {
        if profile["schema_version"] == 4 {
            self.short_argv()
        } else {
            self.argv().to_vec()
        }
    }

    pub fn report_path(&self, c: &CheckConfig) -> Result<String, String> {
        if c.schema != PROFILE {
            return Err("unit/collect selection requires chrono-ci-check/v3".into());
        }
        match self {
            Self::Unit { unit } => {
                id(unit)?;
                let units: BTreeMap<String, Unit> =
                    serde_json::from_value(c.policy["units"].clone()).map_err(|e| e.to_string())?;
                let path = &units.get(unit).ok_or("unregistered CI unit")?.report_path;
                artifact_path(path)?;
                Ok(path.clone())
            }
            Self::Collect { manifest } => {
                let units: Vec<String> = c.policy["units"]
                    .as_object()
                    .ok_or("unit definitions")?
                    .values()
                    .map(|unit| {
                        unit["report_path"]
                            .as_str()
                            .ok_or("unit report path")
                            .map(str::to_owned)
                    })
                    .collect::<Result<_, _>>()?;
                validate_collection_paths(manifest, &c.report_path, &units)?;
                Ok(c.report_path.clone())
            }
        }
    }
}

/// Explicit bounded state input reader shared by registration bootstrap and projects admission.
#[cfg(not(verus_keep_ghost))]
pub fn read_bounded(root: &std::path::Path, path: &str, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    artifact_path(path)?;
    let _input = crate::retained_artifacts::retention::read_guard(root, path)?;
    let resolved = crate::no_symlink_parents(root, path)?;
    let meta = std::fs::symlink_metadata(&resolved).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > limit {
        return Err(format!("bounded collection input rejected: {path}"));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(resolved)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!("bounded collection input exceeds limit: {path}"));
    }
    Ok(bytes)
}

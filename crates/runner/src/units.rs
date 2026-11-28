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
use std::collections::{BTreeMap, BTreeSet};
#[cfg(verus_keep_ghost)]
use vstd::std_specs::cmp::OrdSpec;

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

#[cfg_attr(
    not(verus_keep_ghost),
    derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)
)]
#[cfg_attr(not(verus_keep_ghost), serde(deny_unknown_fields))]
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

verus! {
pub open spec fn intersects(tests: Seq<String>, selected: Set<String>) -> bool {
    exists|i: int| 0 <= i < tests.len() && selected.contains(#[trigger] tests[i])
}

fn unit_is_required(tests: &[String], selected: &BTreeSet<String>) -> (found: bool)
    requires vstd::laws_cmp::obeys_cmp::<String>(),
    ensures found == intersects(tests@, selected@),
{
    broadcast use vstd::std_specs::btree::group_btree_axioms;
    let mut i = 0;
    while i < tests.len()
        invariant i <= tests.len(), vstd::laws_cmp::obeys_cmp::<String>(),
            forall|j: int| 0 <= j < i ==> !selected@.contains(#[trigger] tests@[j]),
        decreases tests.len() - i,
    {
        if selected.contains::<String>(&tests[i]) { return true; }
        i += 1;
    }
    false
}

/// BTreeMap supplies the registered order. A shared selected test can require several units.
pub fn required(units: &BTreeMap<String, Unit>, selected: &BTreeSet<String>) -> (out: Vec<String>)
    requires vstd::laws_cmp::obeys_cmp::<String>(),
    ensures
        forall|id: String| #[trigger] out@.contains(id) == (units@.contains_key(id)
            && intersects(units@[id].tests@, selected@)),
        forall|i: int, j: int| #![trigger out@[i], out@[j]] 0 <= i < j < out.len()
            ==> (out@[i]).cmp_spec(&(out@[j])) is Less,
{
    broadcast use vstd::std_specs::btree::group_btree_axioms;
    let mut iter = units.iter();
    let ghost rows = iter@.1;
    let ghost keys = rows.map(|i: int, row: (String, Unit)| row.0);
    proof { vstd::std_specs::btree::axiom_increasing_seq_meaning(keys); }
    let mut out: Vec<String> = Vec::new();
    let ghost mut positions: Seq<int> = Seq::empty();
    loop
        invariant
            vstd::laws_cmp::obeys_cmp::<String>(), iter@.1 == rows, 0 <= iter@.0 <= rows.len(),
            rows.to_set() == units@.kv_pairs(), keys == rows.map(|i: int, row: (String, Unit)| row.0),
            forall|i: int, j: int| #![trigger keys[i], keys[j]] 0 <= i < j < rows.len() ==> (keys[i]).cmp_spec(&(keys[j])) is Less,
            positions.len() == out.len(),
            forall|j: int| 0 <= j < out.len() ==> 0 <= #[trigger] positions[j] < iter@.0
                && out@[j] == rows[positions[j]].0 && intersects(rows[positions[j]].1.tests@,selected@),
            forall|i: int, j: int| #![trigger positions[i], positions[j]] 0 <= i < j < positions.len() ==> positions[i] < positions[j],
            forall|i: int| 0 <= i < iter@.0 && intersects((#[trigger] rows[i]).1.tests@, selected@)
                ==> out@.contains(rows[i].0),
        decreases rows.len() - iter@.0,
    {
        let ghost pos = iter@.0;
        match iter.next() {
            Some((id, unit)) => {
                let ghost earlier = out@;
                if unit_is_required(&unit.tests, selected) {
                    let ghost before = positions;

                    out.push(id.clone());
                    proof {
                        positions = positions.push(pos);
                        assert(*id == rows[pos].0 && *unit == rows[pos].1);
                        assert forall|j: int| 0 <= j < out.len() implies
                            0 <= #[trigger] positions[j] < iter@.0 && out@[j] == rows[positions[j]].0
                            && intersects(rows[positions[j]].1.tests@,selected@) by {
                            if j < before.len() { assert(positions[j] == before[j] && out@[j] == earlier[j]); }
                        }
                        assert forall|i: int, j: int| #![trigger positions[i], positions[j]]
                            0 <= i < j < positions.len() implies positions[i] < positions[j] by {
                            assert(positions[i] == before[i]);
                            if j < before.len() { assert(positions[j] == before[j]); }
                        }
                    }
                }
                assert forall|i:int| 0 <= i < iter@.0 && intersects((#[trigger] rows[i]).1.tests@, selected@)
                    implies out@.contains(rows[i].0) by {
                    if i < pos {
                        assert(earlier.contains(rows[i].0));
                        let at = choose|at:int| 0 <= at < earlier.len() && earlier[at] == rows[i].0;
                        assert(out@[at] == earlier[at]);
                    } else {
                        assert(i == pos && rows[i].0 == *id && rows[i].1 == *unit);
                        assert(intersects(unit.tests@, selected@));
                        assert(earlier.len() < out.len() && out@[earlier.len() as int] == rows[i].0);
                    }
                }
            }
            None => {
                assert forall|id: String| #[trigger] out@.contains(id) ==
                    (units@.contains_key(id) && intersects(units@[id].tests@, selected@)) by {
                    if out@.contains(id) {
                        let j = choose|j: int| 0 <= j < out.len() && out@[j] == id;
                        let row = rows[positions[j]];
                        assert(rows.to_set().contains(row));
                    } else if units@.contains_key(id) {
                        let row = (id, units@[id]);
                        assert(rows.to_set().contains(row));
                        let j = choose|j: int| 0 <= j < rows.len() && rows[j] == row;
                    }
                }
                assert forall|i: int, j: int| #![trigger out@[i], out@[j]] 0 <= i < j < out.len() implies
                    (out@[i]).cmp_spec(&(out@[j])) is Less by {
                    assert(out@[i] == keys[positions[i]]);
                    assert(out@[j] == keys[positions[j]]);
                }
                return out;
            }
        }
    }
}

pub fn collection_limit(value: Option<u64>) -> (valid: bool)
    ensures valid == (value is Some && 1 <= value->Some_0 <= 67108864),
{
    match value { Some(bytes) => bytes > 0 && bytes <= 64 * 1024 * 1024, None => false }
}

/// Both the configured collection and each registered unit must have at least one member.
pub fn nonempty_registration(count: usize) -> (ok: bool)
    ensures ok == (count > 0),
{ count > 0 }

/// The standard library supplies normalized path components. A common textual
/// prefix inside one component does not establish directory ancestry.
pub open spec fn component_overlap(a: Seq<&[u8]>, b: Seq<&[u8]>) -> bool {
    forall|i: int| 0 <= i < a.len() && i < b.len() ==> (#[trigger] a[i])@ == (#[trigger] b[i])@
}

pub fn overlapping_components(a: &[&[u8]], b: &[&[u8]]) -> (overlap: bool)
    ensures overlap == component_overlap(a@, b@),
{
    let mut i = 0;
    while i < a.len() && i < b.len()
        invariant i <= a.len(), i <= b.len(),
            forall|j: int| 0 <= j < i ==> (#[trigger] a@[j])@ == (#[trigger] b@[j])@,
        decreases a.len() - i,
    {
        if !identifiers::equal_bytes(a[i], b[i]) { return false; }
        i += 1;
    }
    true
}

pub open spec fn disjoint_before(path: Seq<&[u8]>, others: Seq<Vec<&[u8]>>, end: int) -> bool {
    forall|i: int| 0 <= i < end ==> !component_overlap(path, (#[trigger] others[i])@)
}

fn disjoint_from(path: &[&[u8]], others: &[Vec<&[u8]>], end: usize) -> (ok: bool)
    requires end <= others.len(),
    ensures ok == disjoint_before(path@, others@, end as int),
{
    let mut i = 0;
    while i < end
        invariant i <= end, end <= others.len(), disjoint_before(path@, others@, i as int),
        decreases end - i,
    {
        if overlapping_components(path, &others[i]) { return false; }
        i += 1;
    }
    true
}

pub fn disjoint_publications(outputs: &[Vec<&[u8]>], reserved: &[Vec<&[u8]>]) -> (ok: bool)
    ensures ok == (forall|i: int| 0 <= i < outputs.len() ==>
        disjoint_before((#[trigger] outputs@[i])@, outputs@, i)
        && disjoint_before(outputs@[i]@, reserved@, reserved.len() as int)),
{
    let mut i = 0;
    while i < outputs.len()
        invariant i <= outputs.len(),
            forall|j: int| 0 <= j < i ==> disjoint_before((#[trigger] outputs@[j])@, outputs@, j)
                && disjoint_before(outputs@[j]@, reserved@, reserved.len() as int),
        decreases outputs.len() - i,
    {
        if !disjoint_from(&outputs[i], reserved, reserved.len())
            || !disjoint_from(&outputs[i], outputs, i) { return false; }
        i += 1;
    }
    true
}

pub open spec fn byte_prefix(path: Seq<u8>, prefix: Seq<u8>) -> bool {
    prefix.len() <= path.len() && path.take(prefix.len() as int) == prefix
}

fn literal_prefix(path: &[u8], prefix: &[u8]) -> (matches: bool)
    ensures matches == byte_prefix(path@, prefix@),
{
    if path.len() < prefix.len() { return false; }
    let (start, _) = path.split_at(prefix.len());
    identifiers::equal_bytes(start, prefix)
}

pub fn reserved_family(path: &[u8]) -> (reserved: bool)
    ensures reserved == (byte_prefix(path@, vstd::utf8::encode_utf8(".chrono-harness/state/run-"@))
        || byte_prefix(path@, vstd::utf8::encode_utf8(".chrono-harness/state/import-"@))
        || byte_prefix(path@, vstd::utf8::encode_utf8(".chrono-harness/state/base-"@))),
{
    literal_prefix(path, ".chrono-harness/state/run-".as_bytes())
        || literal_prefix(path, ".chrono-harness/state/import-".as_bytes())
        || literal_prefix(path, ".chrono-harness/state/base-".as_bytes())
}

}

#[cfg(not(verus_keep_ghost))]
fn components(path: &str) -> Vec<&[u8]> {
    std::path::Path::new(path)
        .components()
        .map(|part| part.as_os_str().as_encoded_bytes())
        .collect()
}

#[cfg(not(verus_keep_ghost))]
pub fn overlap(a: &str, b: &str) -> bool {
    overlapping_components(&components(a), &components(b))
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
    let mut paths = vec![components(manifest), components(report_path)];
    paths.extend(unit_paths.iter().map(|path| components(path)));
    if !disjoint_publications(&paths, &[]) {
        return Err("collection manifest overlaps publication path".into());
    }
    Ok(())
}
#[cfg(not(verus_keep_ghost))]
pub fn report_paths(profile: &serde_json::Value, evidence: Option<&str>) -> Result<(), String> {
    let Some(block) = profile.get("execution_units") else {
        return Ok(());
    };
    let mut reserved = vec![
        ".chrono-harness/state/report.json",
        ".chrono-harness/state/integration.json",
        ".chrono-harness/state/context.json",
        ".chrono-harness/state/inputs.json",
    ];
    if let Some(e) = evidence {
        reserved.push(e);
    }
    if let Some(argv) = profile["canonical_check"]["argv"].as_array() {
        for pair in argv.windows(2) {
            if pair[0] == "--context" {
                if let Some(path) = pair[1].as_str() {
                    reserved.push(path);
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
    let reserved: Vec<_> = reserved.iter().map(|path| components(path)).collect();
    let outputs: Vec<_> = paths.iter().map(|path| components(path)).collect();
    if paths.iter().any(|path| reserved_family(path.as_bytes()))
        || !disjoint_publications(&outputs, &reserved)
    {
        return Err("execution report paths overlap reserved evidence or another report".into());
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
    if !nonempty_registration(units.len()) {
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
        if !collection_limit(limits.get(key).copied()) {
            return Err(format!("invalid execution_units limit {key}"));
        }
    }
    for (unit_id, unit) in &units {
        id(unit_id)?;
        if !nonempty_registration(unit.tests.len()) {
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
        return Err(invalid_id(s));
    }
    Ok(())
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn invalid_id(s: &str) -> String {
    format!("invalid CI unit ID: {s:?}")
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

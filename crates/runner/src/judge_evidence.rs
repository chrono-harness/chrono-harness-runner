//! Read and verify the runner's judge wire evidence without opening any host retention policy.
#[cfg(verus_keep_ghost)]
use super::{identifiers, units::Unit};
#[cfg(not(verus_keep_ghost))]
use crate::{units::Unit, verified_identifiers as identifiers};
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(not(verus_keep_ghost))]
use std::io::{BufRead, Read, Write};
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {

pub(crate) enum UnitTestsError {
    InvalidId,
    Unregistered,
}

pub(crate) fn unit_tests<'a>(
    unit: &String, units: &'a BTreeMap<String, Unit>, selected: &BTreeSet<String>,
) -> (result: Result<BTreeSet<&'a String>, UnitTestsError>)
    requires vstd::laws_cmp::obeys_cmp::<String>(),
    ensures
        if !identifiers::identifier(vstd::utf8::encode_utf8(unit@)) {
            result is Err && result->Err_0 is InvalidId
        } else if !units@.contains_key(*unit) {
            result is Err && result->Err_0 is Unregistered
        } else {
            result is Ok && result->Ok_0@ == Set::new(|test: &String|
                units@[*unit].tests@.contains(*test) && selected@.contains(*test))
        },
{
    broadcast use vstd::std_specs::btree::group_btree_axioms;
    if !identifiers::valid_id(unit.as_str().as_bytes()) {
        return Err(UnitTestsError::InvalidId);
    }
    let definition = match units.get::<String>(unit) {
        Some(definition) => definition,
        None => return Err(UnitTestsError::Unregistered),
    };
    let tests = &definition.tests;
    // Serialization uses the set's sorted order and borrows the registered names.
    let mut matches = BTreeSet::new();
    let mut index = 0;
    assert(matches@ =~= Set::new(|test: &String|
        tests@.take(0).contains(*test) && selected@.contains(*test)));
    while index < tests.len()
        invariant
            vstd::laws_cmp::obeys_cmp::<String>(),
            index <= tests.len(),
            matches@ == Set::new(|test: &String|
                tests@.take(index as int).contains(*test) && selected@.contains(*test)),
        decreases tests.len() - index,
    {
        let test = &tests[index];
        let ghost previous = matches@;
        let chosen = selected.contains::<String>(test);
        if chosen {
            matches.insert(test);
        }
        proof {
            broadcast use vstd::seq_lib::lemma_seq_contains_after_push;
            let prefix = tests@.take(index as int);
            let next = tests@.take(index as int + 1);
            assert(next =~= prefix.push(*test));
            let expected = Set::new(|name: &String|
                next.contains(*name) && selected@.contains(*name));
            assert forall|name: &String| matches@.contains(name) == expected.contains(name) by {
                assert(next.contains(*name) == (prefix.contains(*name) || *test == *name));
                assert(previous.contains(name) == (prefix.contains(*name) && selected@.contains(*name)));
            }
            assert(matches@ =~= expected);
        }
        index += 1;
    }
    assert(tests@.take(index as int) =~= tests@);
    Ok(matches)
}

}

#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    ScopedReportPath {
        config: crate::CheckConfig,
        scope: crate::units::Scope,
    },
    ScopedReferencePublication {
        config: crate::CheckConfig,
    },
    CheckoutIndexIssue {
        flags: Vec<crate::verified_status::IndexFlag>,
    },
    DecodeJson {
        text: String,
    },
    ValidateRequest {
        request: Box<crate::wire::Request>,
    },
    ValidateInitialRequest {
        request: Box<crate::initial::Request>,
    },
    ValidateInitialProfile {
        profile: crate::initial::Profile,
    },
    RequiredUnits {
        units: BTreeMap<String, crate::units::Unit>,
        selected: std::collections::BTreeSet<String>,
    },
    ExpandProcess {
        process: Value,
    },
    ExpandRecord {
        record: Value,
    },
    ObserveRecord {
        record: Value,
    },
    CheckProcess {
        process: Box<crate::ProcessResult>,
    },
    ReportTransportBytes {
        report: Value,
        metadata_bytes: u64,
    },
    RetainedJudges {
        request: Box<crate::wire::Request>,
        bindings: Vec<crate::wire::Binding>,
        records: Vec<Value>,
        judge: String,
    },
    UnitTests {
        unit: String,
        units: BTreeMap<String, Unit>,
        selected: BTreeSet<String>,
    },
    CollectionPaths {
        manifest: String,
        report: String,
        units: Vec<String>,
    },
    FullExecutionUnits {
        profile: Value,
    },
    BoundJudgePath {
        binding: crate::wire::Binding,
    },
    RegistryIdentity {
        entry: String,
        documents: BTreeMap<String, Value>,
    },
    ReportPaths {
        profile: Value,
        evidence: Option<String>,
    },
    ValidateGitDeclaration {
        config: Value,
    },
    ValidatePreparationDeclaration {
        config: Value,
    },
    CredentialEnvironment {
        config: Value,
    },
    ValidateSchedule {
        bindings: Vec<crate::wire::Binding>,
    },
    ArtifactDirectories {
        config: Value,
    },
    RetainedEntry {
        request: Box<crate::wire::Request>,
        config: Value,
    },
}
#[cfg(not(verus_keep_ghost))]
impl Operation {
    fn evaluate(self) -> Result<Value, String> {
        match self {
            Operation::ScopedReportPath { config, scope } => Ok(json!(scope.report_path(&config)?)),
            Operation::ScopedReferencePublication { config } => {
                Ok(json!(crate::units::reference_publication(&config)?))
            }
            Operation::CheckoutIndexIssue { flags } => Ok(json!(
                crate::verified_status::first_unsupported_index_flag(&flags)
                    .map(|(entry, reason)| json!({"path":entry.path,"reason":reason.message()}))
            )),

            Operation::DecodeJson { text } => crate::json(text.as_bytes()),
            Operation::ValidateRequest { request } => {
                request.validate()?;
                Ok(Value::Null)
            }
            Operation::ValidateInitialRequest { request } => {
                request.validate()?;
                Ok(Value::Null)
            }
            Operation::ValidateInitialProfile { profile } => {
                profile.validate()?;
                Ok(Value::Null)
            }
            Operation::RequiredUnits { units, selected } => {
                Ok(json!(crate::units::required(&units, &selected)))
            }
            Operation::ExpandProcess { process } => {
                let process: crate::ProcessResult =
                    serde_json::from_value(crate::full::expand_process(&process)?)
                        .map_err(|e| e.to_string())?;
                Ok(
                    json!({"stdout_bytes":process.stdout_bytes,"stderr_bytes":process.stderr_bytes,"failure":process.failure,"exit_code":process.exit_code}),
                )
            }
            Operation::ExpandRecord { record } => crate::full::expand_record(&record),
            Operation::ObserveRecord { record } => {
                let record = crate::full::expand_record(&record)?;
                crate::wire::Response::deserialize(&record["response"])
                    .map_err(|error| format!("E_RECORD_SHAPE: {error}"))?;
                let process = crate::ProcessResult::deserialize(&record["process"])
                    .map_err(|error| format!("E_RECORD_SHAPE: {error}"))?;
                crate::observation::process_success(&process)?;
                let matches = crate::json(&process.stdout_bytes)? == record["response"];
                Ok(json!({"record":record,"stdout_matches":matches}))
            }
            Operation::CheckProcess { process } => {
                crate::observation::process_success(&process)?;
                Ok(Value::Null)
            }
            Operation::ReportTransportBytes {
                report,
                metadata_bytes,
            } => Ok(json!(crate::full::report_transport_bytes(
                &report,
                metadata_bytes
            )?)),
            Operation::RetainedJudges {
                request,
                bindings,
                records,
                judge,
            } => {
                request.validate()?;
                let parsed = crate::full::retained_judges(&request, &bindings, &records)?;
                let status = parsed
                    .values()
                    .fold(crate::wire::Status::Pass, |status, (_, _, response, _)| {
                        status.maximum(response.status.clone())
                    });
                let original = parsed.get(&judge).ok_or("producer workflow input")?;
                Ok(json!({"request":original.0,"status":status}))
            }
            Operation::UnitTests {
                unit,
                units,
                selected,
            } => match unit_tests(&unit, &units, &selected) {
                Ok(tests) => Ok(json!(tests)),
                Err(UnitTestsError::InvalidId) => Err(crate::units::invalid_id(&unit)),
                Err(UnitTestsError::Unregistered) => Err("unregistered CI unit".into()),
            },
            Operation::CollectionPaths {
                manifest,
                report,
                units,
            } => {
                crate::units::validate_collection_paths(&manifest, &report, &units)?;
                Ok(Value::Null)
            }
            Operation::FullExecutionUnits { profile } => {
                Ok(json!(crate::units::full_execution_units(&profile)?))
            }
            Operation::BoundJudgePath { binding } => {
                let root = std::env::current_dir().map_err(|error| error.to_string())?;
                let (path, _) = crate::wire::judge_executable(&root, &binding)
                    .map_err(|error| error.to_string())?;
                crate::executable_digest(&path, Some(binding.required_digest()?))
                    .map_err(|error| error.to_string())?;
                Ok(json!(path.to_str().ok_or("non UTF-8 executable")?))
            }
            Operation::RegistryIdentity { entry, documents } => {
                Ok(json!(crate::facts::registry_identity(&documents, &entry)?))
            }
            Operation::ReportPaths { profile, evidence } => {
                crate::units::report_paths(&profile, evidence.as_deref())?;
                Ok(Value::Null)
            }
            Operation::ValidateGitDeclaration { config } => {
                crate::facts::git_declaration(&config)?;
                Ok(Value::Null)
            }
            Operation::ValidatePreparationDeclaration { config } => {
                crate::prepared::declaration(&config)?;
                Ok(Value::Null)
            }
            Operation::CredentialEnvironment { config } => {
                Ok(json!(crate::prepared::credential_environment(&config)?))
            }
            Operation::ValidateSchedule { bindings } => {
                crate::full::schedule(&bindings)?;
                Ok(Value::Null)
            }
            Operation::ArtifactDirectories { config } => {
                Ok(json!(crate::facts::artifact_directories(&config)?))
            }
            Operation::RetainedEntry { request, config } => {
                request.validate()?;
                let scope = request.scope.as_ref().ok_or("original unit scope")?;
                let mut argv: Vec<String> =
                    serde_json::from_value(config["canonical_check"]["argv"].clone())
                        .map_err(|e| e.to_string())?;
                argv.extend(scope.contract_argv(&config));
                crate::validate_invocation_observation(
                    &request.observations["entry"],
                    &argv,
                    &json!({"base":request.base.commit,"candidate":request.candidate.commit}),
                )?;
                Ok(Value::Null)
            }
        }
    }
}

#[cfg(not(verus_keep_ghost))]
pub fn serve(mut input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    loop {
        let mut line = Vec::new();
        (&mut input)
            .take(128 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Ok(());
        }
        if line.len() > 128 * 1024 * 1024 || line.pop() != Some(b'\n') {
            return Err("E_PROTOCOL: invalid judge evidence frame".into());
        }
        let result = crate::decode::<Operation>(&line).and_then(Operation::evaluate);
        serde_json::to_writer(&mut output, &result).map_err(|e| e.to_string())?;
        output
            .write_all(b"\n")
            .and_then(|()| output.flush())
            .map_err(|e| e.to_string())?;
    }
}

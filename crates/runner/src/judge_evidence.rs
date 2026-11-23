//! Read and verify the runner's judge wire evidence without opening any host retention policy.
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, Read, Write},
};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    DecodeJson {
        text: String,
    },
    ValidateRequest {
        request: Box<crate::wire::Request>,
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
}
impl Operation {
    fn evaluate(self) -> Result<Value, String> {
        match self {
            Operation::DecodeJson { text } => crate::json(text.as_bytes()),
            Operation::ValidateRequest { request } => {
                request.validate()?;
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
        }
    }
}

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

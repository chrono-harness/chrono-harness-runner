//! Original tool observation and compiler output admission for installed route planners.
use crate::{observation, verified_verifier as verifier};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observe {
    root: PathBuf,
    program: String,
    version_argv: Vec<String>,
    environment: BTreeMap<String, String>,
    timeout_seconds: u64,
    output_limit_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, tag = "operation", rename_all = "kebab-case")]
enum Compiler {
    Bind {
        argv: Vec<String>,
        directory: String,
    },
    Check {
        contract: Option<verifier::OutputContract>,
        stdout: Vec<u8>,
        argv: Vec<String>,
        exit: i32,
        process_failed: bool,
        dependency: Option<Result<Vec<u8>, String>>,
        limit: usize,
        artifact: Option<Result<verifier::CompiledArtifact, String>>,
    },
}

pub fn serve(operation: &str, input: impl Read, output: impl Write) -> Result<(), String> {
    match operation {
        "observed-tool-v1" => {
            let q: Observe = serde_json::from_reader(input).map_err(|e| e.to_string())?;
            let original = observation::tool(
                &q.root,
                &q.program,
                &q.version_argv,
                &q.environment,
                q.timeout_seconds,
                q.output_limit_bytes,
            )?;
            serde_json::to_writer(output, &original).map_err(|e| e.to_string())
        }
        "compiler-output-v1" => {
            let q: Compiler = serde_json::from_reader(input).map_err(|e| e.to_string())?;
            match q {
                Compiler::Bind {
                    mut argv,
                    directory,
                } => {
                    verifier::bind_compiler_directory(&mut argv, &directory);
                    serde_json::to_writer(output, &argv).map_err(|e| e.to_string())
                }
                Compiler::Check {
                    contract,
                    stdout,
                    argv,
                    exit,
                    process_failed,
                    dependency,
                    limit,
                    artifact,
                } => {
                    let decoded = contract
                        .as_ref()
                        .and_then(|_| serde_json::from_slice(&stdout).ok());
                    verifier::admit_observed_output(
                        &contract,
                        &decoded,
                        &argv,
                        exit,
                        process_failed,
                        &dependency,
                        limit,
                        &artifact,
                    )?;
                    serde_json::to_writer(output, &()).map_err(|e| e.to_string())
                }
            }
        }
        _ => Err("unsupported route evidence operation".into()),
    }
}

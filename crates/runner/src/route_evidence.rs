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
    ValidateContract {
        contract: verifier::OutputContract,
    },
    ProofRows {
        contract: verifier::OutputContract,
        rows: Vec<String>,
    },
    DependencyPaths {
        dependency: Vec<u8>,
        stem: Vec<u8>,
    },
    DependencyMatches {
        dependency: Vec<u8>,
        targets: Vec<Vec<u8>>,
        sources: Vec<BoundSource>,
    },
    Bind {
        argv: Vec<String>,
        directory: String,
    },
    Observe {
        contract: verifier::OutputContract,
        argv: Vec<String>,
        exit: i32,
        process_failed: bool,
        limit: usize,
    },
    Canonical {
        argv: Vec<String>,
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundSource {
    path: Vec<u8>,
    sha256: Vec<u8>,
    length: u64,
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
                Compiler::ValidateContract { contract } => {
                    verifier::validate_contract(&Some(contract))?;
                    serde_json::to_writer(output, &()).map_err(|e| e.to_string())
                }
                Compiler::ProofRows { contract, rows } => {
                    let normalized: Vec<Option<bool>> = rows
                        .into_iter()
                        .map(|row| {
                            serde_json::from_str::<verifier::CompilerReport>(&row)
                                .ok()
                                .map(|report| verifier::accepts_report(&report, &contract))
                        })
                        .collect();
                    serde_json::to_writer(output, &normalized).map_err(|e| e.to_string())
                }
                Compiler::DependencyPaths { dependency, stem } => {
                    let target = verifier::sources::encode_path(&stem);
                    let paths = verifier::sources::compiler_dependency_paths(&dependency, &target);
                    serde_json::to_writer(output, &paths).map_err(|e| e.to_string())
                }
                Compiler::DependencyMatches {
                    dependency,
                    targets,
                    sources,
                } => {
                    let bindings = sources
                        .into_iter()
                        .map(|source| verifier::sources::SourceBinding {
                            path: source.path,
                            sha256: source.sha256,
                            length: source.length,
                        })
                        .collect();
                    let matched = verifier::sources::matches_cargo_dependency(
                        &dependency,
                        &targets,
                        &bindings,
                    );
                    serde_json::to_writer(output, &matched).map_err(|e| e.to_string())
                }
                Compiler::Bind {
                    mut argv,
                    directory,
                } => {
                    verifier::bind_compiler_directory(&mut argv, &directory);
                    serde_json::to_writer(output, &argv).map_err(|e| e.to_string())
                }
                Compiler::Observe {
                    contract,
                    argv,
                    exit,
                    process_failed,
                    limit,
                } => {
                    let root = std::env::current_dir().map_err(|e| e.to_string())?;
                    let files = CompilerFiles(&root);
                    let contract = Some(contract);
                    let dependency = verifier::sources::observe_dependency(
                        &contract,
                        &argv,
                        exit,
                        process_failed,
                        limit,
                        &files,
                    );
                    let artifact =
                        verifier::observe_artifact(&contract, &argv, exit, process_failed, &files);
                    serde_json::to_writer(
                        output,
                        &serde_json::json!({"dependency":dependency,"artifact":artifact}),
                    )
                    .map_err(|e| e.to_string())
                }
                Compiler::Canonical { argv } => {
                    serde_json::to_writer(output, &verifier::canonical_verifier_arguments(&argv))
                        .map_err(|e| e.to_string())
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

struct CompilerFiles<'a>(&'a std::path::Path);
impl verifier::sources::DependencyRead for CompilerFiles<'_> {
    fn read(&self, path: &[u8], limit: u64) -> Result<Vec<u8>, String> {
        std::str::from_utf8(path)
            .map_err(|e| e.to_string())
            .and_then(|path| std::fs::File::open(self.0.join(path)).map_err(|e| e.to_string()))
            .and_then(|file| {
                let mut bytes = Vec::new();
                file.take(limit)
                    .read_to_end(&mut bytes)
                    .map(|_| bytes)
                    .map_err(|e| e.to_string())
            })
    }
}

impl verifier::ArtifactRead for CompilerFiles<'_> {
    fn identity(&self, path: &[u8]) -> Result<(Vec<u8>, u64), String> {
        std::str::from_utf8(path)
            .map_err(|e| e.to_string())
            .and_then(|path| crate::no_symlink_parents(self.0, path))
            .and_then(|path| crate::file_identity(&path))
            .map(|(digest, length)| (digest.into_bytes(), length))
    }
}

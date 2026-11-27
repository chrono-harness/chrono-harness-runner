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
    SourceDeclarations {
        files: Vec<verifier::sources::SourceFile>,
        operations: Vec<String>,
        reference_paths: Vec<String>,
    },
    ProofRows {
        contract: verifier::OutputContract,
        rows: Vec<String>,
    },
    DependencyHeader {
        dependency: Vec<u8>,
    },
    DependencyPaths {
        dependency: Vec<u8>,
        stem: Vec<u8>,
    },
    DependencyMatches {
        dependency: Vec<u8>,
        targets: Vec<Vec<u8>>,
        sources: Vec<verifier::sources::SourceBinding>,
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

pub(crate) fn cargo_roots(args: &[String]) -> Option<&[String]> {
    args.get(1)
        .filter(|arg| arg.as_str() == "--cargo-verus-roots")
        .map(|_| &args[2..])
}

type Bindings = BTreeMap<String, BTreeMap<String, (String, usize)>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CargoArchive {
    receipt: serde_json::Value,
    cargo_stdout_zlib_hex: String,
    roots: Vec<ArchiveRoot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveRoot {
    path: String,
    original: serde_json::Value,
}

pub(crate) fn validate_cargo(
    stdout: &[u8],
    roots: &[String],
    contract: &verifier::OutputContract,
    limit: usize,
    candidate: Option<&Bindings>,
    root: &std::path::Path,
) -> Result<(), String> {
    if roots.is_empty()
        || stdout.len() > limit
        || roots
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != roots.len()
        || !roots
            .iter()
            .all(|r| verifier::sources::canonical_source_name(r))
    {
        return Err("invalid registered Cargo proof roots or output bound".into());
    }
    let archive: CargoArchive = serde_json::from_slice(stdout).map_err(|e| e.to_string())?;
    if !archive
        .roots
        .iter()
        .map(|row| row.path.as_str())
        .eq(roots.iter().map(String::as_str))
    {
        return Err("Cargo proof roots differ from the registered method".into());
    }
    let size = archive.receipt["process"]["stdout"]["size"]
        .as_u64()
        .filter(|n| *n <= 8_388_608)
        .ok_or("Cargo original exceeds the collector bound")? as usize;
    let compressed = crate::full::decode_hex(&archive.cargo_stdout_zlib_hex)?;
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&compressed, size)
        .map_err(|_| "invalid compressed Cargo original")?;
    if raw.len() != size {
        return Err("Cargo original length differs".into());
    }
    let mut selected = Vec::new();
    let mut expected = BTreeMap::new();
    for row in archive.roots {
        let path = &row.path;
        let report = row.original["report_sha256"]
            .as_str()
            .ok_or("missing original Verus result")?;
        expected.insert(path.clone(), report.to_owned());
        let sources = if let Some(all) = candidate {
            serde_json::to_value(
                all.get(path)
                    .ok_or("Cargo root missing candidate source selection")?,
            )
            .map_err(|e| e.to_string())?
        } else {
            let source = row.original["target"]["source"]
                .as_str()
                .ok_or("missing original Cargo root")?;
            let id = archive.receipt["files"]
                .get(source)
                .ok_or("missing compiled Cargo root identity")?;
            serde_json::json!({path:id})
        };
        selected.push(serde_json::json!({"path":path,"original":row.original,"sources":sources}));
    }
    let input = serde_json::json!({"receipt":archive.receipt,"stdout":std::str::from_utf8(&raw).map_err(|e|e.to_string())?,"contract":contract,"roots":selected});
    let result = crate::installed_query::inherited(
        root,
        "chrono-host-bootstrap",
        "validate-cargo-proof-originals",
        &input,
    )?;
    let verified: BTreeMap<String, String> =
        serde_json::from_slice(&result).map_err(|e| e.to_string())?;
    if expected != verified {
        return Err("Cargo collector reply differs from requested root reports".into());
    }
    Ok(())
}

pub(crate) struct CompilerReportReader;

impl verifier::CompilerReportRead for CompilerReportReader {
    fn decode(&self, bytes: &[u8]) -> Option<verifier::CompilerReport> {
        serde_json::from_slice(bytes).ok()
    }
}

impl verifier::CompilerRowRead for CompilerReportReader {
    fn decode(&self, text: &str) -> Option<verifier::CompilerReport> {
        verifier::CompilerReportRead::decode(self, text.as_bytes())
    }
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
                Compiler::SourceDeclarations {
                    files,
                    operations,
                    reference_paths,
                } => {
                    let canonical = verifier::sources::source_declarations(
                        &files,
                        &operations,
                        &reference_paths,
                    )
                    .map_err(|(path, error)| format!("{path}: {error}"))?;
                    serde_json::to_writer(output, &canonical).map_err(|error| error.to_string())
                }
                Compiler::ProofRows { contract, rows } => {
                    let normalized = verifier::proof_rows(&rows, &contract, &CompilerReportReader);
                    serde_json::to_writer(output, &normalized).map_err(|e| e.to_string())
                }
                Compiler::DependencyHeader { dependency } => {
                    let header = verifier::sources::compiler_dependency_header(&dependency);
                    serde_json::to_writer(output, &header).map_err(|e| e.to_string())
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
                    let matched = verifier::sources::matches_cargo_dependency(
                        &dependency,
                        &targets,
                        &sources,
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
                    if cargo_roots(&argv).is_some() {
                        return serde_json::to_writer(
                            output,
                            &serde_json::json!({"dependency":null,"artifact":null}),
                        )
                        .map_err(|e| e.to_string());
                    }
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
                    if let Some(roots) = cargo_roots(&argv) {
                        if exit != 0 || process_failed || dependency.is_some() || artifact.is_some()
                        {
                            return Err(
                                "Cargo proof operation did not return a successful group".into()
                            );
                        }
                        let root = std::env::current_dir().map_err(|e| e.to_string())?;
                        validate_cargo(
                            &stdout,
                            roots,
                            contract.as_ref().ok_or("missing Cargo proof contract")?,
                            limit,
                            None,
                            &root,
                        )?;
                        return serde_json::to_writer(output, &()).map_err(|e| e.to_string());
                    }
                    verifier::admit_original_output(
                        &contract,
                        &stdout,
                        &CompilerReportReader,
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

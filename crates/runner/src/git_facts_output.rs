//! Bind CI operation receipts to the registered plan and this reader's fixed candidate.
use crate::{facts, verified_verifier as verifier};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, path::Path};
use verifier::sources::{CandidateSourceRead, SourceRegistry};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Method {
    operation: String,
    argv: Vec<String>,
    output_contract: Option<verifier::OutputContract>,
    output_limit_bytes: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Original {
    operation: String,
    argv: Vec<String>,
    stdout: Vec<u8>,
    exit: i32,
    failed: bool,
    dependency: Option<Result<Vec<u8>, String>>,
    artifact: Option<Result<verifier::CompiledArtifact, String>>,
}

pub(super) struct Binding {
    methods: Vec<Method>,
    registry: Result<SourceRegistry, String>,
}

impl Binding {
    pub fn new(methods: Vec<Method>, registry: Value) -> Self {
        Self {
            methods,
            registry: serde_json::from_value(registry).map_err(|e| e.to_string()),
        }
    }

    pub fn check(
        &self,
        state: &str,
        operation: &str,
        original: Option<Original>,
        input: &Candidate<'_>,
    ) -> Result<(), String> {
        let methods = self
            .methods
            .iter()
            .map(|m| verifier::ProofInvocation {
                operation: &m.operation,
                argv: &m.argv,
                output_contract: &m.output_contract,
                output_limit_bytes: m.output_limit_bytes,
            })
            .collect();
        if state == "passed" {
            let mut matching = self
                .methods
                .iter()
                .filter(|method| method.operation == operation);
            if let (Some(method), None) = (matching.next(), matching.next()) {
                if let Some(roots) = crate::route_evidence::cargo_roots(&method.argv) {
                    let receipt = original.as_ref().ok_or("missing Cargo proof operation")?;
                    let contract = method
                        .output_contract
                        .as_ref()
                        .ok_or("missing Cargo proof contract")?;
                    if receipt.operation != operation
                        || receipt.exit != 0
                        || receipt.failed
                        || receipt.argv.get(1..) != Some(method.argv.as_slice())
                        || receipt.dependency.is_some()
                        || receipt.artifact.is_some()
                    {
                        return Err("Cargo proof operation does not match its plan".into());
                    }
                    let files = &self.registry.as_ref().map_err(|e| e.clone())?.files.entries;
                    let mut paths_by_root: BTreeMap<&String, std::collections::BTreeSet<String>> =
                        roots
                            .iter()
                            .map(|root| (root, Default::default()))
                            .collect();
                    for file in files {
                        if let Some(declaration) = &file.verification {
                            for root in declaration
                                .roots
                                .iter()
                                .filter(|root| root.operation == operation)
                            {
                                paths_by_root
                                    .get_mut(&root.path)
                                    .ok_or(
                                        "FILEMAP Cargo proof root absent from the planned method",
                                    )?
                                    .insert(file.path.clone());
                            }
                        }
                    }
                    let mut declared = BTreeMap::new();
                    for (root, mut paths) in paths_by_root {
                        if paths.is_empty() {
                            return Err("planned Cargo proof root absent from FILEMAP".into());
                        }
                        paths.insert(root.clone());
                        let paths: Vec<_> = paths.into_iter().collect();
                        if paths
                            .iter()
                            .any(|p| !matches!(input.mode(p).as_deref(), Some("100644" | "100755")))
                        {
                            return Err("Cargo proof source is not a fixed regular Git file".into());
                        }
                        let contents = input.blobs(&paths)?;
                        if contents.len() != paths.len() {
                            return Err("Cargo proof candidate Git inputs are incomplete".into());
                        }
                        let bindings = paths
                            .into_iter()
                            .zip(contents)
                            .map(|(path, bytes)| (path, (crate::sha256(&bytes), bytes.len())))
                            .collect();
                        declared.insert(root.clone(), bindings);
                    }
                    return crate::route_evidence::validate_cargo(
                        &receipt.stdout,
                        roots,
                        contract,
                        method.output_limit_bytes,
                        Some(&declared),
                        input.root,
                    );
                }
            }
        }
        let process = original.as_ref().map(|r| verifier::ProofProcess {
            operation: &r.operation,
            argv: &r.argv,
            stdout: &r.stdout,
            exit: r.exit,
            failed: r.failed,
            dependency: &r.dependency,
            artifact: &r.artifact,
        });
        verifier::bind_operation_output(
            state,
            operation,
            &methods,
            process,
            &self.registry,
            input,
            |message, path| format!("{message}{path}"),
        )
        .map(|_| ())
    }
}

pub(super) struct Candidate<'a> {
    pub reader: &'a facts::Reader,
    pub root: &'a Path,
    pub commit: &'a str,
    pub modes: &'a BTreeMap<String, String>,
}
impl CandidateSourceRead for Candidate<'_> {
    fn text(&self, bytes: &[u8]) -> Result<String, String> {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|e| e.to_string())
    }
    fn mode(&self, path: &str) -> Option<String> {
        self.modes.get(path).cloned()
    }
    fn blobs(&self, paths: &Vec<String>) -> Result<Vec<Vec<u8>>, String> {
        self.reader.blobs(self.root, self.commit, paths)
    }
    fn digest(&self, bytes: &[u8]) -> Vec<u8> {
        crate::sha256(bytes).into_bytes()
    }
}
impl verifier::CompilerReportRead for Candidate<'_> {
    fn decode(&self, bytes: &[u8]) -> Option<verifier::CompilerReport> {
        verifier::CompilerReportRead::decode(&crate::route_evidence::CompilerReportReader, bytes)
    }
}

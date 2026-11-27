//! Invocation-scoped JSON lines connection to one policy-bound Git reader.
#[cfg(verus_keep_ghost)]
use crate::identifiers;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub(super) const FRAME_LIMIT: usize = 64 * 1024 * 1024;

/// Strip the terminating LF only after admitting the complete bounded line.
pub(super) fn complete_frame(mut bytes: Vec<u8>, limit: usize)
    -> (out: Result<Option<Vec<u8>>, &'static str>)
    ensures match out {
        Ok(None) => bytes@.len() == 0,
        Ok(Some(payload)) => bytes@.len() > 0 && bytes@.last() == 10
            && bytes@.len() - 1 <= limit && payload@ == bytes@.drop_last(),
        Err(error) => bytes@.len() > 0 && (bytes@.last() != 10 || bytes@.len() - 1 > limit)
            && error@ == "E_PROTOCOL: invalid or oversized Git facts frame"@,
    },
{
    match bytes.pop() {
        None => Ok(None),
        Some(10) if bytes.len() <= limit => Ok(Some(bytes)),
        _ => Err("E_PROTOCOL: invalid or oversized Git facts frame"),
    }
}

pub(super) fn require_checkout_head(observed: &str, endpoint: &str) -> (out: Result<(), &'static str>)
    ensures out == if observed@ == endpoint@ { Ok::<(), &str>(()) }
        else { Err::<(), &str>("E_SNAPSHOT_IDENTITY: capture requires the explicit checked-out commit") },
{
    if identifiers::equal_text(observed, endpoint) { Ok(()) }
    else { Err("E_SNAPSHOT_IDENTITY: capture requires the explicit checked-out commit") }
}

pub(super) fn require_checkout_config(current: &[u8], committed: &[u8]) -> (out: Result<(), &'static str>)
    ensures out == if current@ == committed@ { Ok::<(), &str>(()) }
        else { Err::<(), &str>("E_SNAPSHOT_IDENTITY: config differs from the fixed commit") },
{
    if identifiers::equal_bytes(current, committed) { Ok(()) }
    else { Err("E_SNAPSHOT_IDENTITY: config differs from the fixed commit") }
}
}

#[cfg(not(verus_keep_ghost))]
#[path = "git_facts_output.rs"]
mod output;
#[cfg(not(verus_keep_ghost))]
use crate::{facts, verified_shallow_reads, wire};
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Open {
    History,
    Scoped {
        config: Option<String>,
        base: Option<String>,
        candidate: String,
    },
    PreparedCollection {
        config: String,
        base: String,
        candidate: String,
        context: PathBuf,
        scope: crate::units::Scope,
        judge: String,
    },
    Config {
        config: String,
        #[serde(default)]
        observe_failure: bool,
        base: String,
        candidate: String,
    },
    Observed {
        config: String,
        base: Option<String>,
        candidate: String,
        observations: Value,
    },
    Request {
        request: Box<wire::Request>,
    },
}
#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Endpoint {
    Base,
    Candidate,
}
#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    PrepareOutputBinding {
        methods: Vec<output::Method>,
        registry: Value,
    },
    BindOperationOutput {
        state: String,
        operation: String,
        original: Option<output::Original>,
    },
    VerifyTree {
        endpoint: Endpoint,
    },
    VerifyCommit {
        commit: String,
    },
    CheckBranchName {
        branch: String,
    },
    Ancestors {
        commit: String,
    },
    ShallowBoundaries,
    Delta,
    CheckoutRoot,
    CandidateInventory,
    TreeEntries {
        endpoint: Endpoint,
        #[serde(default)]
        retain_output_modes: bool,
    },
    RegistryAt {
        endpoint: Endpoint,
        config: String,
    },
    BlobBatch {
        endpoint: Endpoint,
        paths: Vec<String>,
    },
    RegistrySnapshot {
        endpoint: Endpoint,
    },
    CheckedOutSnapshot {
        endpoint: Endpoint,
    },
    CheckoutExcluding {
        artifacts: Vec<String>,
    },
    JsonBlob {
        endpoint: Endpoint,
        path: String,
    },
    Observation,
    VerifyConfig {
        candidate: String,
    },
    Head,
    CommitAvailable {
        commit: String,
    },
    FetchCommit {
        commit: String,
        shallow: bool,
    },
    ResolveCommit {
        commit: String,
    },
    CheckReference {
        reference: String,
    },
    RemoteReference {
        reference: String,
    },
    ChangedPaths {
        base: Option<String>,
        candidate: String,
    },
    Parents {
        commit: String,
    },
    ShowText {
        commit: String,
        path: String,
    },
    BlobBytes {
        commit: String,
        path: String,
    },
}
#[cfg(not(verus_keep_ghost))]
struct Session {
    root: PathBuf,
    config: String,
    base: Option<String>,
    candidate: String,
    reader: facts::Reader,
    shallow_reads: Vec<Value>,
    candidate_modes: Option<BTreeMap<String, String>>,
    output_binding: Option<output::Binding>,
    collection: Option<(
        crate::retained_artifacts::retention::ReadScope,
        Option<tempfile::TempDir>,
    )>,
}
#[cfg(not(verus_keep_ghost))]
impl Session {
    fn open(cwd: &Path, open: Open) -> Result<(Self, Value), String> {
        let mut header = json!({});
        let mut collection = None;
        let (root, config, base, candidate, reader) = match open {
            Open::History => (
                cwd.to_owned(),
                String::new(),
                None,
                String::new(),
                facts::Reader::legacy(),
            ),
            Open::Config {
                config,
                observe_failure,
                base,
                candidate,
            } => {
                facts::full_oid(&base)?;
                facts::full_oid(&candidate)?;
                let reader = if observe_failure {
                    facts::Reader::for_config_observed(cwd, &config).map_err(|e| {
                        json!({"message":e.message,"git_facts":e.observation}).to_string()
                    })?
                } else {
                    facts::Reader::for_config(cwd, &config)?
                };
                (cwd.to_owned(), config, Some(base), candidate, reader)
            }
            Open::Scoped {
                config,
                base,
                candidate,
            } => {
                if let Some(base) = &base {
                    facts::full_oid(base)?;
                }
                facts::full_oid(&candidate)?;
                let reader = match &config {
                    Some(path) => facts::Reader::for_config_observed(cwd, path).map_err(|e| {
                        json!({"message":e.message,"git_facts":e.observation}).to_string()
                    })?,
                    None => facts::Reader::legacy(),
                };
                (
                    cwd.to_owned(),
                    config.unwrap_or_default(),
                    base,
                    candidate,
                    reader,
                )
            }
            Open::PreparedCollection {
                config,
                base,
                candidate,
                context,
                scope,
                judge,
            } => {
                if !matches!(scope, crate::units::Scope::Collect { .. }) {
                    return Err("collection scope required".into());
                }
                facts::full_oid(&base)?;
                facts::full_oid(&candidate)?;
                crate::units::id(&judge)?;
                let lease = crate::retained_artifacts::retention::read_scope(cwd)?;
                let mut temporary = None;
                let (mut request, _, reader, profile) = crate::full::prepare_request_with_snapshot(
                    cwd,
                    &config,
                    &base,
                    &candidate,
                    &context,
                    Value::Null,
                    Some(scope),
                    None,
                    Some(&mut temporary),
                )?;
                collection = Some((lease, temporary));
                request.judge_id = judge;
                request.seal()?;
                header["request"] = json!(request);
                header["config"] = profile;
                (cwd.to_owned(), config, Some(base), candidate, reader)
            }
            Open::Observed {
                config,
                base,
                candidate,
                observations,
            } => {
                if let Some(base) = &base {
                    facts::full_oid(base)?;
                }
                facts::full_oid(&candidate)?;
                let reader =
                    facts::Reader::from_observations(cwd, &config, &candidate, &observations)?;
                (cwd.to_owned(), config, base, candidate, reader)
            }
            Open::Request { request } => {
                let reader = facts::Reader::for_request(&request)?;
                (
                    request.candidate.root,
                    request.config_path,
                    Some(request.base.commit),
                    request.candidate.commit,
                    reader,
                )
            }
        };
        header["bound"] = json!(reader.is_bound());
        Ok((
            Self {
                root,
                config,
                base,
                candidate,
                reader,
                shallow_reads: Vec::new(),
                candidate_modes: None,
                output_binding: None,
                collection,
            },
            header,
        ))
    }
    fn commit(&self, endpoint: Endpoint) -> Result<&str, String> {
        match endpoint {
            Endpoint::Base => self
                .base
                .as_deref()
                .ok_or_else(|| "E_PROTOCOL: no base endpoint".into()),
            Endpoint::Candidate => Ok(&self.candidate),
        }
    }
    fn snapshot(&self, endpoint: Endpoint, checked_out: bool) -> Result<Value, String> {
        let commit = self.commit(endpoint)?;
        if checked_out {
            self.reader.verify_config(&self.root, commit)?;
            self.reader.verify_oid(&self.root, commit)?;
            let head = facts::utf8(self.reader.git(&self.root, &["rev-parse", "HEAD"])?)?;
            require_checkout_head(head.trim(), commit)?;
            let path = crate::no_symlink_parents(&self.root, &self.config)?;
            let bytes = self.reader.blob(&self.root, commit, &self.config)?;
            let current = fs::read(path).map_err(|error| error.to_string())?;
            require_checkout_config(&current, &bytes)?;
        }
        self.registry_at(commit, &self.config)
    }
    fn registry_at(&self, commit: &str, config: &str) -> Result<Value, String> {
        crate::relative_path(config)?;
        let snap = self.reader.registry_snapshot(&self.root, commit, config)?;
        let originals: BTreeMap<_, _> = snap
            .bytes
            .into_iter()
            .map(|(path, bytes)| String::from_utf8(bytes).map(|text| (path, text)))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"values":snap.values,"originals":originals,"effective_path":snap.effective_path,"selection":snap.selection}),
        )
    }
    fn execute(&mut self, op: Operation) -> Result<Value, String> {
        if self.candidate.is_empty()
            && !matches!(
                op,
                Operation::VerifyCommit { .. }
                    | Operation::BlobBytes { .. }
                    | Operation::Ancestors { .. }
                    | Operation::ShallowBoundaries
                    | Operation::CheckBranchName { .. }
                    | Operation::Observation
            )
        {
            return Err("E_PROTOCOL: no configured history endpoints".into());
        }
        match op {
            Operation::PrepareOutputBinding { methods, registry } => {
                self.output_binding = Some(output::Binding::new(methods, registry));
                Ok(Value::Null)
            }
            Operation::BindOperationOutput {
                state,
                operation,
                original,
            } => {
                let binding = self
                    .output_binding
                    .as_ref()
                    .ok_or("E_PROTOCOL: no registered CI output binding")?;
                let modes = self
                    .candidate_modes
                    .as_ref()
                    .ok_or("E_PROTOCOL: output binding requires candidate tree-entries")?;
                binding.check(
                    &state,
                    &operation,
                    original,
                    &output::Candidate {
                        reader: &self.reader,
                        root: &self.root,
                        commit: &self.candidate,
                        modes,
                    },
                )?;
                Ok(Value::Null)
            }
            Operation::TreeEntries {
                endpoint,
                retain_output_modes,
            } => {
                if retain_output_modes && !matches!(endpoint, Endpoint::Candidate) {
                    return Err("E_PROTOCOL: output binding requires the candidate endpoint".into());
                }
                let tree = self.reader.tree(&self.root, self.commit(endpoint)?)?;
                if retain_output_modes {
                    self.candidate_modes = Some(
                        tree.iter()
                            .map(|(path, entry)| (path.text.clone(), entry.mode.clone()))
                            .collect(),
                    );
                }
                let entries: BTreeMap<_, _> = tree
                    .into_iter()
                    .map(|(path, entry)| {
                        (
                            path.text,
                            json!({"mode":entry.mode,"kind":entry.kind,"oid":entry.oid}),
                        )
                    })
                    .collect();
                Ok(json!(entries))
            }
            Operation::RegistryAt { endpoint, config } => {
                self.registry_at(self.commit(endpoint)?, &config)
            }
            Operation::BlobBatch { endpoint, paths } => {
                for path in &paths {
                    crate::relative_path(path)?;
                }
                Ok(json!(self.reader.blobs(
                    &self.root,
                    self.commit(endpoint)?,
                    &paths
                )?))
            }
            Operation::RegistrySnapshot { endpoint } => self.snapshot(endpoint, false),
            Operation::CheckedOutSnapshot { endpoint } => self.snapshot(endpoint, true),
            Operation::CheckoutExcluding { artifacts } => {
                let paths: Vec<_> = artifacts.iter().map(String::as_str).collect();
                let checkout =
                    self.reader
                        .checkout_excluding(&self.root, &self.candidate, &paths)?;
                serde_json::to_value(checkout).map_err(|e| e.to_string())
            }
            Operation::VerifyTree { endpoint } => Ok(self
                .reader
                .verify_oid(&self.root, self.commit(endpoint)?)?
                .into()),
            Operation::VerifyCommit { commit } => {
                Ok(self.reader.verify_oid(&self.root, &commit)?.into())
            }
            Operation::CheckBranchName { branch } => {
                let reference = format!("refs/heads/{branch}");
                self.reader
                    .git(&self.root, &["check-ref-format", &reference])?;
                Ok(Value::Null)
            }
            Operation::Ancestors { commit } => {
                facts::full_oid(&commit)?;
                let list = facts::utf8(self.reader.git(&self.root, &["rev-list", &commit])?)?;
                Ok(json!(list.lines().collect::<Vec<_>>()))
            }
            Operation::ShallowBoundaries => {
                let path = facts::utf8(
                    self.reader
                        .git(&self.root, &["rev-parse", "--git-path", "shallow"])?,
                )?;
                let path = self.root.join(path.trim());
                let read = fs::read_to_string(&path)
                    .map(|text| json!(text.lines().collect::<Vec<_>>()))
                    .map_err(|error| (error.kind() == std::io::ErrorKind::NotFound, error));
                let (state, response) = verified_shallow_reads::shallow_file_reply(read, json!([]));
                let response = response.map_err(|error| format!("E_HISTORY_MISSING: {error}"));
                verified_shallow_reads::retain_shallow_read(
                    &mut self.shallow_reads,
                    self.reader.is_bound(),
                    || json!({"path": path, "state": state, "response": response}),
                );
                response
            }
            Operation::CheckoutRoot => {
                let top = facts::utf8(
                    self.reader
                        .git(&self.root, &["rev-parse", "--show-toplevel"])?,
                )?;
                let path = fs::canonicalize(top.trim()).map_err(|error| error.to_string())?;
                serde_json::to_value(path).map_err(|error| error.to_string())
            }
            Operation::CandidateInventory => {
                let before = self
                    .base
                    .as_ref()
                    .map(|oid| self.reader.tree(&self.root, oid))
                    .transpose()?;
                let after = self.reader.tree(&self.root, &self.candidate)?;
                let delta = before.map(|tree| facts::delta(&tree, &after));
                let entries: BTreeMap<_, _> = after
                    .into_iter()
                    .map(|(path, entry)| (path.text, json!({"mode":entry.mode,"kind":entry.kind})))
                    .collect();
                Ok(json!({"entries":entries,"delta":delta}))
            }
            Operation::Delta => {
                let before = self.reader.tree(&self.root, self.commit(Endpoint::Base)?)?;
                let after = self.reader.tree(&self.root, &self.candidate)?;
                Ok(json!(facts::delta(&before, &after)))
            }
            Operation::JsonBlob { endpoint, path } => {
                crate::relative_path(&path)?;
                let bytes = self
                    .reader
                    .blob(&self.root, self.commit(endpoint)?, &path)?;
                crate::json(&bytes).map_err(|error| format!("E_SEMANTIC_INPUT: {path}: {error}"))
            }
            Operation::Observation => {
                let mut observed = self.reader.observation();
                if let Some(records) =
                    verified_shallow_reads::observed_shallow_reads(&self.shallow_reads)
                {
                    observed["shallow_reads"] = json!(records);
                }
                Ok(observed)
            }
            Operation::VerifyConfig { candidate } => {
                facts::full_oid(&candidate)?;
                self.reader.verify_config(&self.root, &candidate)?;
                Ok(Value::Null)
            }
            Operation::Head => Ok(json!(facts::utf8(
                self.reader.git(&self.root, &["rev-parse", "HEAD"])?
            )?)),
            Operation::CommitAvailable { commit } => {
                Ok(json!(self.reader.commit_available(&self.root, &commit)?))
            }
            Operation::FetchCommit { commit, shallow } => {
                facts::full_oid(&commit)?;
                let mut args = vec!["fetch", "--no-tags"];
                if shallow {
                    args.extend(["--depth=2", "--filter=blob:none"]);
                }
                args.extend(["origin", &commit]);
                Ok(json!(facts::utf8(self.reader.git(&self.root, &args)?)?))
            }
            Operation::ResolveCommit { commit } => {
                facts::full_oid(&commit)?;
                Ok(json!(facts::utf8(self.reader.git(
                    &self.root,
                    &["rev-parse", &format!("{commit}^{{commit}}")]
                )?)?))
            }
            Operation::CheckReference { reference } => {
                self.reader
                    .git(&self.root, &["check-ref-format", &reference])?;
                Ok(Value::Null)
            }
            Operation::RemoteReference { reference } => Ok(json!(facts::utf8(
                self.reader
                    .git(&self.root, &["ls-remote", "--refs", "origin", &reference])?
            )?)),
            Operation::ChangedPaths { base, candidate } => {
                let after = self.reader.tree(&self.root, &candidate)?;
                let paths: Vec<_> = if let Some(base) = base {
                    facts::delta(&self.reader.tree(&self.root, &base)?, &after)
                        .into_iter()
                        .map(|d| d.path)
                        .collect()
                } else {
                    after.keys().map(|p| p.text.clone()).collect()
                };
                Ok(json!(paths))
            }
            Operation::Parents { commit } => Ok(json!(self.reader.parents(&self.root, &commit)?)),
            Operation::ShowText { commit, path } => {
                facts::full_oid(&commit)?;
                crate::relative_path(&path)?;
                let bytes = self
                    .reader
                    .git(&self.root, &["show", &format!("{commit}:{path}")])?;
                Ok(json!(
                    String::from_utf8(bytes).map_err(|_| "git output not UTF-8")?
                ))
            }
            Operation::BlobBytes { commit, path } => {
                facts::full_oid(&commit)?;
                crate::relative_path(&path)?;
                Ok(json!(self.reader.blob(&self.root, &commit, &path)?))
            }
        }
    }
}

#[cfg(not(verus_keep_ghost))]
fn frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut data = Vec::new();
    reader
        .take(FRAME_LIMIT as u64 + 2)
        .read_until(b'\n', &mut data)
        .map_err(|e| e.to_string())?;
    complete_frame(data, FRAME_LIMIT).map_err(Into::into)
}
#[cfg(not(verus_keep_ghost))]
fn reply(output: &mut impl Write, result: Result<Value, String>) -> Result<(), String> {
    serde_json::to_writer(&mut *output, &result).map_err(|e| e.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
#[cfg(not(verus_keep_ghost))]
pub fn serve(cwd: &Path, mut input: impl BufRead, mut out: impl Write) -> Result<(), String> {
    let first = frame(&mut input)?.ok_or("E_PROTOCOL: missing Git facts session header")?;
    let (mut session, header) =
        match crate::decode(&first).and_then(|open| Session::open(cwd, open)) {
            Ok(session) => session,
            Err(error) => return reply(&mut out, Err(error)),
        };
    reply(&mut out, Ok(header))?;
    while let Some(line) = frame(&mut input)? {
        let result = crate::decode(&line).and_then(|op| session.execute(op));
        reply(&mut out, result)?;
    }
    if let Some((_lease, Some(temporary))) = session.collection {
        temporary.close().map_err(|error| error.to_string())?;
    }
    Ok(())
}

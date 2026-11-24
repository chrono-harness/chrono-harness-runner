//! Invocation-scoped JSON lines connection to one policy-bound Git reader.
use crate::{facts, wire};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Open {
    History,
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
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum Endpoint {
    Base,
    Candidate,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
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
struct Session {
    root: PathBuf,
    config: String,
    base: Option<String>,
    candidate: String,
    reader: facts::Reader,
}
impl Session {
    fn open(cwd: &Path, open: Open) -> Result<Self, String> {
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
        Ok(Self {
            root,
            config,
            base,
            candidate,
            reader,
        })
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
            if facts::utf8(self.reader.git(&self.root, &["rev-parse", "HEAD"])?)?.trim() != commit {
                return Err(
                    "E_SNAPSHOT_IDENTITY: capture requires the explicit checked-out commit".into(),
                );
            }
            let path = crate::no_symlink_parents(&self.root, &self.config)?;
            let bytes = self.reader.blob(&self.root, commit, &self.config)?;
            if fs::read(path).map_err(|error| error.to_string())? != bytes {
                return Err("E_SNAPSHOT_IDENTITY: config differs from the fixed commit".into());
            }
        }
        let snap = self
            .reader
            .registry_snapshot(&self.root, commit, &self.config)?;
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
    fn execute(&self, op: Operation) -> Result<Value, String> {
        if self.config.is_empty()
            && !matches!(
                op,
                Operation::VerifyCommit { .. }
                    | Operation::Ancestors { .. }
                    | Operation::ShallowBoundaries
                    | Operation::CheckBranchName { .. }
                    | Operation::Observation
            )
        {
            return Err("E_PROTOCOL: no configured history endpoints".into());
        }
        match op {
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
                let boundaries = match fs::read_to_string(self.root.join(path.trim())) {
                    Ok(text) => text,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                    Err(error) => return Err(format!("E_HISTORY_MISSING: {error}")),
                };
                Ok(json!(boundaries.lines().collect::<Vec<_>>()))
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
            Operation::Observation => Ok(self.reader.observation()),
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

fn frame(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut data = Vec::new();
    const LIMIT: u64 = 64 * 1024 * 1024;
    let size = reader
        .take(LIMIT + 2)
        .read_until(b'\n', &mut data)
        .map_err(|e| e.to_string())?;
    if size == 0 {
        return Ok(None);
    }
    if data.pop() != Some(b'\n') || data.len() as u64 > LIMIT {
        return Err("E_PROTOCOL: invalid or oversized Git facts frame".into());
    }
    Ok(Some(data))
}
fn reply(output: &mut impl Write, result: Result<Value, String>) -> Result<(), String> {
    serde_json::to_writer(&mut *output, &result).map_err(|e| e.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
pub fn serve(cwd: &Path, mut input: impl BufRead, mut out: impl Write) -> Result<(), String> {
    let first = frame(&mut input)?.ok_or("E_PROTOCOL: missing Git facts session header")?;
    let session = match crate::decode(&first).and_then(|open| Session::open(cwd, open)) {
        Ok(session) => session,
        Err(error) => return reply(&mut out, Err(error)),
    };
    reply(&mut out, Ok(json!({"bound":session.reader.is_bound()})))?;
    while let Some(line) = frame(&mut input)? {
        let result = crate::decode(&line).and_then(|op| session.execute(op));
        reply(&mut out, result)?;
    }
    Ok(())
}

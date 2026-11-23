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
    Config {
        config: String,
        base: String,
        candidate: String,
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
    VerifyTree { endpoint: Endpoint },
    Delta,
    RegistrySnapshot { endpoint: Endpoint },
    CheckedOutSnapshot { endpoint: Endpoint },
    CheckoutExcluding { artifacts: Vec<String> },
    JsonBlob { endpoint: Endpoint, path: String },
    Observation,
}
struct Session {
    root: PathBuf,
    config: String,
    base: String,
    candidate: String,
    reader: facts::Reader,
}
impl Session {
    fn open(cwd: &Path, open: Open) -> Result<Self, String> {
        let (root, config, base, candidate, reader) = match open {
            Open::Config {
                config,
                base,
                candidate,
            } => {
                facts::full_oid(&base)?;
                facts::full_oid(&candidate)?;
                let reader = facts::Reader::for_config(cwd, &config)?;
                (cwd.to_owned(), config, base, candidate, reader)
            }
            Open::Request { request } => {
                let reader = facts::Reader::for_request(&request)?;
                (
                    request.candidate.root,
                    request.config_path,
                    request.base.commit,
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
    fn commit(&self, endpoint: Endpoint) -> &str {
        match endpoint {
            Endpoint::Base => &self.base,
            Endpoint::Candidate => &self.candidate,
        }
    }
    fn snapshot(&self, endpoint: Endpoint, checked_out: bool) -> Result<Value, String> {
        let commit = self.commit(endpoint);
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
                .verify_oid(&self.root, self.commit(endpoint))?
                .into()),
            Operation::Delta => {
                let before = self.reader.tree(&self.root, &self.base)?;
                let after = self.reader.tree(&self.root, &self.candidate)?;
                Ok(json!(facts::delta(&before, &after)))
            }
            Operation::JsonBlob { endpoint, path } => {
                crate::relative_path(&path)?;
                let bytes = self.reader.blob(&self.root, self.commit(endpoint), &path)?;
                crate::json(&bytes).map_err(|error| format!("E_SEMANTIC_INPUT: {path}: {error}"))
            }
            Operation::Observation => Ok(self.reader.observation()),
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

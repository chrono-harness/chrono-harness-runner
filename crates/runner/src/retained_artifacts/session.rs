//! Retain adopted artifact capabilities for the lifetime of an external producer.
use crate::{
    decode, file_identity, no_symlink_parents,
    prepared::{ArtifactTransport, Original},
    retained_artifacts::{
        self,
        retention::{self, Consumer, Inventory, OriginalIdentity, Publication},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    CheckedPath {
        path: String,
    },
    Accepts {
        producer: String,
        path: String,
    },
    RetainRead {
        path: String,
    },
    PublishFile {
        producer: String,
        path: String,
        source: PathBuf,
        sha256: String,
        length: u64,
        outcome: Value,
    },
    PublishText {
        producer: String,
        path: String,
        text: String,
        outcome: Value,
    },
    Dependencies {
        path: String,
        originals: Vec<String>,
    },
    NewStage {
        producer: String,
        directory: String,
        prefix: String,
        suffix: String,
        length: u64,
        outcome: Value,
    },
    SealStage {
        path: String,
        outcome: Value,
    },
    ProtectConsumer {
        producer: String,
        path: String,
        consumer: String,
        purpose: String,
    },
    ReadOriginal {
        original: Original,
        transport: Option<ArtifactTransport>,
        artifacts: Option<Value>,
    },
    CopyOriginal {
        path: String,
        transport: Option<ArtifactTransport>,
        artifacts: Value,
        target: String,
        length: u64,
    },
    ResolveOriginal {
        original: Original,
        transport: Option<ArtifactTransport>,
    },
}

struct Session<'a> {
    root: &'a Path,
    inventory: Option<Inventory>,
    stages: BTreeMap<PathBuf, Stage>,
}

struct Stage {
    publication: Option<Publication>,
    file: fs::File,
    length: u64,
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        use std::os::unix::fs::MetadataExt;
        for (path, stage) in &self.stages {
            if stage.publication.is_none()
                && stage
                    .file
                    .metadata()
                    .ok()
                    .zip(fs::symlink_metadata(path).ok())
                    .is_some_and(|(old, new)| old.dev() == new.dev() && old.ino() == new.ino())
            {
                let _ = fs::remove_file(path);
            }
        }
    }
}
impl Session<'_> {
    fn apply(&mut self, op: Operation) -> Result<Value, String> {
        let root = self.root;
        match op {
            Operation::CheckedPath { path } => Ok(json!(no_symlink_parents(root, &path)?)),
            Operation::Accepts { producer, path } => Ok(json!(
                self.inventory
                    .as_ref()
                    .is_some_and(|i| i.accepts(&producer, &path))
            )),
            Operation::RetainRead { path } => {
                // The scope acquired before serving requests retains the exact lease.
                retention::read_guard(root, &path)?;
                Ok(Value::Null)
            }
            Operation::PublishFile {
                producer,
                path,
                source,
                sha256,
                length,
                outcome,
            } => Ok(json!(retention::publish_owned_file(
                root, &producer, &path, &source, &sha256, length, outcome
            )?)),
            Operation::PublishText {
                producer,
                path,
                text,
                outcome,
            } => Ok(json!(retention::publish_owned_original(
                root,
                &producer,
                &path,
                text.as_bytes(),
                outcome
            )?)),
            Operation::Dependencies { path, originals } => {
                if let Some(i) = &self.inventory {
                    i.dependencies_paths(&path, &originals)?;
                }
                Ok(Value::Null)
            }
            Operation::NewStage {
                producer,
                directory,
                prefix,
                suffix,
                length,
                outcome,
            } => {
                let path = retention::unique_path(
                    &no_symlink_parents(root, &directory)?,
                    &prefix,
                    &suffix,
                )?;
                let publication = retention::create_adopted_with_limit(
                    root,
                    &producer,
                    &path,
                    false,
                    outcome,
                    Some(length.max(1)),
                )?;
                let managed = publication.is_some();
                let file = fs::OpenOptions::new()
                    .write(true)
                    .open(&path)
                    .map_err(|error| error.to_string())?;
                self.stages.insert(
                    path.clone(),
                    Stage {
                        publication,
                        file,
                        length,
                    },
                );
                Ok(json!({"path":path,"managed":managed}))
            }
            Operation::SealStage { path, outcome } => {
                let stage = self
                    .stages
                    .remove(Path::new(&path))
                    .ok_or("unknown staged publication")?;
                if let Some(publication) = stage.publication {
                    publication.complete_sealed(outcome)?;
                }
                Ok(Value::Null)
            }
            Operation::ProtectConsumer {
                producer,
                path,
                consumer,
                purpose,
            } => {
                let i = self
                    .inventory
                    .as_ref()
                    .ok_or("caller handoff requires adopted retention")?;
                let status = i.producer_status(&producer, Some(&path))?;
                let [record] = status["records"]
                    .as_array()
                    .ok_or("producer status records")?
                    .as_slice()
                else {
                    return Err("existing unknown output requires its original custodian".into());
                };
                let original = OriginalIdentity {
                    id: record["id"].as_str().ok_or("output id")?.into(),
                    path: path.clone(),
                    identity: record["identity"].as_str().ok_or("output identity")?.into(),
                };
                let (digest, length) = file_identity(&no_symlink_parents(root, &path)?)?;
                if !i.published_original(&path, &digest, length)? {
                    return Err("output original is not sealed".into());
                }
                i.protect_consumer(&consumer, std::slice::from_ref(&original))?;
                serde_json::to_value(Consumer {
                    schema: "chrono-retention-consumer/v1".into(),
                    consumer,
                    purpose,
                    originals: vec![original],
                })
                .map_err(|e| e.to_string())
            }
            Operation::ReadOriginal {
                original,
                transport,
                artifacts,
            } => {
                let bytes = if let Some(a) = artifacts {
                    retained_artifacts::bytes(root, &a, &original.path, transport.as_ref())?
                } else {
                    crate::prepared::read_original(root, &original, transport.as_ref())?
                };
                Ok(Value::String(
                    String::from_utf8(bytes).map_err(|e| e.to_string())?,
                ))
            }
            Operation::ResolveOriginal {
                original,
                transport,
            } => Ok(Value::String(crate::prepared::original_path(
                &original,
                transport.as_ref(),
            )?)),
            Operation::CopyOriginal {
                path,
                transport,
                artifacts,
                target,
                length,
            } => {
                let target = Path::new(&target);
                let stage = self
                    .stages
                    .get_mut(target)
                    .ok_or("copy destination is not an active staged publication")?;
                if stage.length != length {
                    return Err("copy length differs from the staged allocation".into());
                }
                let destination = &mut stage.file;
                let mut bounded = Bounded {
                    file: destination,
                    remaining: length,
                };
                let (digest, copied) = retained_artifacts::copy_to(
                    root,
                    &artifacts,
                    &path,
                    transport.as_ref(),
                    &mut bounded,
                )?;
                if copied != length || bounded.remaining != 0 {
                    return Err("staged source length changed".into());
                }
                destination.sync_all().map_err(|e| e.to_string())?;
                Ok(json!({"sha256":digest,"length":copied}))
            }
        }
    }
}
struct Bounded<'a> {
    file: &'a mut fs::File,
    remaining: u64,
}
impl Write for Bounded<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("source exceeds declared length"));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
pub fn serve(root: &Path, mut input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    let _scope = retention::read_scope(root)?;
    let mut session = Session {
        root,
        inventory: Inventory::adopted(root)?,
        stages: BTreeMap::new(),
    };
    loop {
        let mut line = Vec::new();
        (&mut input)
            .take(128 * 1024 * 1024 + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Ok(());
        }
        if line.len() > 128 * 1024 * 1024 || !line.ends_with(b"\n") {
            return Err("invalid retained operation frame".into());
        }
        let result = decode(&line).and_then(|op| session.apply(op));
        serde_json::to_writer(&mut output, &result).map_err(|e| e.to_string())?;
        output
            .write_all(b"\n")
            .and_then(|()| output.flush())
            .map_err(|e| e.to_string())?;
    }
}

pub(crate) fn command() -> crate::CliOutput {
    let result = std::env::current_dir()
        .map_err(|e| e.to_string())
        .and_then(|root| serve(&root, std::io::stdin().lock(), std::io::stdout().lock()));
    match result {
        Ok(()) => crate::CliOutput {
            exit_code: 0,
            stdout: String::new(),
            stderr: String::new(),
        },
        Err(error) => crate::CliOutput {
            exit_code: 2,
            stdout: String::new(),
            stderr: format!("{error}\n"),
        },
    }
}

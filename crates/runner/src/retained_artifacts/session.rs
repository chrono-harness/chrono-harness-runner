//! Retain adopted artifact capabilities for the lifetime of an external producer.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub enum AcquisitionError<E> { AlreadyOpen, Owner(E) }

/// The current removal scope stays owned when a second open is rejected.
pub fn acquire_removal<T, E, F>(already_open: bool, acquire: F)
    -> (out: Result<Option<T>, AcquisitionError<E>>)
    where F: FnOnce() -> Result<Option<T>, E>,
    requires !already_open ==> acquire.requires(()),
    ensures if already_open { out == Err::<Option<T>, AcquisitionError<E>>(AcquisitionError::AlreadyOpen) } else {
        exists|original: Result<Option<T>, E>| #[trigger] acquire.ensures((), original)
            && out == match original {
                Ok(scope) => Ok::<Option<T>, AcquisitionError<E>>(scope),
                Err(e) => Err(AcquisitionError::Owner(e)),
            }
    },
{
    if already_open { return Err(AcquisitionError::AlreadyOpen); }
    match acquire() {
        Ok(scope) => Ok(scope),
        Err(error) => Err(AcquisitionError::Owner(error)),
    }
}
}

#[cfg(not(verus_keep_ghost))]
#[path = "session/descriptors.rs"]
pub(crate) mod descriptors;
#[cfg(not(verus_keep_ghost))]
#[path = "session/directory.rs"]
pub(crate) mod directory;
#[cfg(not(verus_keep_ghost))]
#[path = "session/inventory.rs"]
pub(crate) mod inventory;
#[cfg(not(verus_keep_ghost))]
#[path = "session/publication.rs"]
pub(crate) mod publication;
#[cfg(not(verus_keep_ghost))]
use crate::{
    decode, file_identity, no_symlink_parents,
    prepared::{ArtifactTransport, Original},
    retained_artifacts::{
        self,
        retention::{self, Consumer, Inventory, OriginalIdentity, Publication},
    },
};
#[cfg(not(verus_keep_ghost))]
use directory::Directory;
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
#[serde(tag = "operation", rename_all = "kebab-case", deny_unknown_fields)]
enum Operation {
    Inventory {
        fact: inventory::Fact,
    },
    ObserveMaintenance,
    ExplicitReadScopes,
    ValidateLaunch {
        original: Original,
    },
    CreateDirectoryMember {
        path: String,
        name: String,
    },
    ReserveDirectoryFile {
        path: String,
        name: String,
        length: u64,
    },
    SealDirectory {
        path: String,
        outcome: Value,
    },
    OpenRemoval {
        paths: Vec<String>,
    },
    CheckRemoval,
    ScopedDescriptors {
        offset: usize,
    },
    Publication {
        action: publication::Action,
    },
    BeginDirectory {
        producer: String,
        path: String,
        reference: String,
        intent: Value,
        #[serde(default)]
        rooted: bool,
    },
    ProcessCompletion {
        exit_code: i32,
        signal: Option<i32>,
        owned_work_joined: bool,
    },
    CompactProcess {
        process: Value,
    },
    DirectoryEvidence {
        path: String,
        process: Box<crate::ProcessResult>,
    },
    DisposeCompilerOriginals {
        path: String,
        library_sha256: String,
        library_length: u64,
        dependency_sha256: String,
        dependency_length: u64,
    },
    CheckArtifactPath {
        path: String,
    },
    WriteDirectory {
        path: String,
        name: String,
        value: Value,
    },
    CheckDirectoryWrite {
        path: String,
    },
    ExecuteRetained {
        path: String,
        command: crate::CommandSpec,
        expected_sha256: String,
        #[serde(default)]
        member: Option<String>,
    },
    FinishDirectory {
        path: String,
        terminal: bool,
        outcome: Value,
        inputs: Vec<String>,
    },
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
    PublishStream {
        producer: String,
        path: String,
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
    ResolveMap {
        artifacts: Value,
        closure: Option<Value>,
    },
    TransportMap {
        artifacts: Value,
        transport: Option<ArtifactTransport>,
    },
    RestageMap {
        artifacts: Value,
        transport: Option<ArtifactTransport>,
        directory: String,
    },
    CheckPreparation {
        binding: Value,
        transport: Option<ArtifactTransport>,
        artifacts: Option<Value>,
    },
    ArtifactIdentity {
        path: String,
        artifacts: Value,
        transport: Option<ArtifactTransport>,
    },
    ReadArtifactText {
        path: String,
        artifacts: Value,
        transport: Option<ArtifactTransport>,
    },
    BoundedFileSha256 {
        path: String,
        limit: u64,
    },
    ReadBoundedText {
        path: String,
        limit: u64,
    },
    RetainOriginalText {
        directory: String,
        prefix: String,
        text: String,
    },
    IdentifyOriginal {
        path: String,
    },
    VerifyOriginal {
        original: Original,
    },
    ScopedCheckProfile {
        path: String,
    },
    ValidatePreparationRequest {
        request: Box<crate::prepared::InputRequest>,
    },
    ValidatePreparationResult {
        request: Box<crate::prepared::InputRequest>,
        result: Box<crate::prepared::PreparedCheck>,
    },
    ValidatePortableBinding {
        binding: Value,
        transport: Option<ArtifactTransport>,
    },
    RestoreScopedProcess {
        process: Value,
        limit: u64,
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

#[cfg(not(verus_keep_ghost))]
struct Session<'a> {
    publications: publication::Publications,
    implicit_reads: Option<retention::ReadScope>,
    removal: Option<retention::RemovalGuard>,
    root: &'a Path,
    inventory: Option<Inventory>,
    stages: BTreeMap<PathBuf, Stage>,
    directories: BTreeMap<String, Directory>,
    consumers: Vec<crate::ownership::Lease>,
}

#[cfg(not(verus_keep_ghost))]
struct Stage {
    publication: Option<Publication>,
    file: fs::File,
    length: u64,
}
#[cfg(not(verus_keep_ghost))]
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
#[cfg(not(verus_keep_ghost))]
impl Session<'_> {
    fn apply(
        &mut self,
        op: Operation,
        input: &mut impl BufRead,
        output: &mut impl Write,
    ) -> Result<Value, String> {
        let root = self.root;
        match op {
            Operation::Inventory { fact } => self.inventory_fact(fact),
            Operation::ExplicitReadScopes => {
                self.implicit_reads.take();
                Ok(Value::Null)
            }
            Operation::ObserveMaintenance => {
                let inventory = self
                    .inventory
                    .as_ref()
                    .ok_or("adopted original inventory unavailable")?;
                inventory.maintain_explicit_observed(|record| {
                    // Each row reaches the caller before the owner advances to the next object.
                    write_frame(output, &json!({"observation": record}))
                })
            }
            Operation::ValidateLaunch { original } => {
                let path = no_symlink_parents(root, &original.path)?;
                let raw = fs::read(&path).map_err(|e| e.to_string())?;
                if crate::sha256(&raw) != original.sha256 {
                    return Err("process original digest changed".into());
                }
                crate::validate_launch_evidence(
                    &path.with_file_name("launch.json"),
                    &decode(&raw)?,
                )?;
                Ok(Value::Null)
            }
            Operation::CreateDirectoryMember { path, name } => {
                self.directories
                    .get(&path)
                    .ok_or("unknown original directory")?
                    .create_member(root, &name)?;
                Ok(Value::Null)
            }
            Operation::ReserveDirectoryFile { path, name, length } => Ok(json!(
                self.directories
                    .get(&path)
                    .ok_or("unknown original directory")?
                    .reserve_file(root, &name, length)?
            )),
            Operation::SealDirectory { path, outcome } => {
                self.directories
                    .remove(&path)
                    .ok_or("unknown original directory")?
                    .seal(outcome)?;
                Ok(Value::Null)
            }
            Operation::OpenRemoval { paths } => {
                self.removal = acquire_removal(self.removal.is_some(), || {
                    retention::external_removal_guards(root, &paths)
                })
                .map_err(|error| match error {
                    AcquisitionError::AlreadyOpen => "removal scope is already open".to_string(),
                    AcquisitionError::Owner(source) => source,
                })?;
                Ok(json!(self.removal.is_some()))
            }
            Operation::CheckRemoval => {
                self.removal
                    .as_ref()
                    .ok_or("no removal scope is open")?
                    .stable()?;
                Ok(Value::Null)
            }
            Operation::ScopedDescriptors { offset } => descriptors::send(offset).map(|n| json!(n)),
            Operation::Publication { action } => {
                self.publications
                    .apply(root, self.inventory.as_ref(), action)
            }
            Operation::BeginDirectory {
                producer,
                path,
                reference,
                intent,
                rooted,
            } => {
                if self.directories.contains_key(&path) {
                    return Err("original directory already open".into());
                }
                let directory =
                    Directory::begin(root, &producer, path.clone(), reference, intent, rooted)?;
                let response = if rooted {
                    json!({"adopted":true,"id":directory.identity()?})
                } else {
                    json!({"adopted":self.inventory.is_some()})
                };
                self.directories.insert(path, directory);
                Ok(response)
            }
            Operation::ProcessCompletion {
                exit_code,
                signal,
                owned_work_joined,
            } => Ok(json!(crate::verified_release::process_completion_known(
                exit_code,
                signal,
                owned_work_joined
            ))),
            Operation::CompactProcess { process } => crate::full::compact_process(&process),
            Operation::DirectoryEvidence { path, process } => self
                .directories
                .get(&path)
                .ok_or("unknown original directory")?
                .evidence(root, &process),
            Operation::DisposeCompilerOriginals {
                path,
                library_sha256,
                library_length,
                dependency_sha256,
                dependency_length,
            } => {
                self.directories
                    .get(&path)
                    .ok_or("unknown original directory")?
                    .dispose_compiler(
                        root,
                        (&library_sha256, library_length),
                        (&dependency_sha256, dependency_length),
                    )?;
                Ok(Value::Null)
            }
            Operation::CheckArtifactPath { path } => {
                crate::units::artifact_path(&path)?;
                Ok(Value::Null)
            }
            Operation::WriteDirectory { path, name, value } => self
                .directories
                .get(&path)
                .ok_or("unknown original directory")?
                .write(root, &name, &value),
            Operation::CheckDirectoryWrite { path } => {
                self.directories
                    .get(&path)
                    .ok_or("unknown original directory")?
                    .check_write()?;
                Ok(Value::Null)
            }
            Operation::ExecuteRetained {
                path,
                command,
                expected_sha256,
                member,
            } => {
                let directory = self
                    .directories
                    .get_mut(&path)
                    .ok_or("unknown original directory")?;
                serde_json::to_value(directory.execute(
                    root,
                    &command,
                    &expected_sha256,
                    member.as_deref(),
                )?)
                .map_err(|e| e.to_string())
            }
            Operation::FinishDirectory {
                path,
                terminal,
                outcome,
                inputs,
            } => {
                let directory = self
                    .directories
                    .remove(&path)
                    .ok_or("unknown original directory")?;
                directory.finish(self.inventory.as_ref(), terminal, outcome, &inputs)?;
                Ok(Value::Null)
            }
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
            Operation::PublishStream {
                producer,
                path,
                sha256,
                length,
                outcome,
            } => {
                write_frame(output, &json!({"receive": length}))?;
                let mut body = input.take(length);
                let result = retention::publish_streamed_original(
                    root, &producer, &path, &sha256, length, outcome, &mut body,
                );
                // Reuse the session even when the owner rejected or already owned this address.
                std::io::copy(&mut body, &mut std::io::sink())
                    .map_err(|error| error.to_string())?;
                if body.limit() != 0 {
                    return Err("incomplete original content frame".into());
                }
                result.map(|published| json!(published))
            }
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
            Operation::ResolveMap { artifacts, closure } => {
                retained_artifacts::resolve_map(&artifacts, closure.as_ref())
            }
            Operation::TransportMap {
                artifacts,
                transport,
            } => retained_artifacts::transport_map(&artifacts, transport.as_ref()),
            Operation::RestageMap {
                artifacts,
                transport,
                directory,
            } => retained_artifacts::restage(root, &artifacts, transport.as_ref(), &directory),
            Operation::CheckPreparation {
                binding,
                transport,
                artifacts,
            } => {
                crate::prepared::validate_retained_binding(
                    root,
                    &binding,
                    transport.as_ref(),
                    artifacts.as_ref(),
                )?;
                Ok(Value::Null)
            }
            Operation::ArtifactIdentity {
                path,
                artifacts,
                transport,
            } => {
                let (sha256, length) =
                    retained_artifacts::identity(root, &artifacts, &path, transport.as_ref())?;
                Ok(json!({"sha256":sha256,"length":length}))
            }
            Operation::ReadArtifactText {
                path,
                artifacts,
                transport,
            } => {
                let bytes = retained_artifacts::bytes(root, &artifacts, &path, transport.as_ref())?;
                String::from_utf8(bytes)
                    .map(Value::String)
                    .map_err(|err| err.to_string())
            }
            Operation::BoundedFileSha256 { path, limit } => {
                if limit > 64 * 1024 * 1024 {
                    return Err("collection file limit exceeds 64 MiB".into());
                }
                Ok(json!(crate::sha256(&crate::units::read_bounded(
                    root, &path, limit
                )?)))
            }
            Operation::ReadBoundedText { path, limit } => {
                // Use the same upper bound as other JSON collection originals, including on inline reads.
                if limit > 64 * 1024 * 1024 {
                    return Err("collection text limit exceeds 64 MiB".into());
                }
                let bytes = crate::units::read_bounded(root, &path, limit)?;
                String::from_utf8(bytes)
                    .map(Value::String)
                    .map_err(|err| err.to_string())
            }
            Operation::RetainOriginalText {
                directory,
                prefix,
                text,
            } => Ok(json!(crate::prepared::retain_original(
                root,
                &directory,
                &prefix,
                text.as_bytes()
            )?)),
            Operation::IdentifyOriginal { path } => {
                Ok(json!(crate::prepared::original(root, &path)?))
            }
            Operation::VerifyOriginal { original } => {
                crate::prepared::read_original(root, &original, None)?;
                Ok(Value::Null)
            }
            Operation::ScopedCheckProfile { path } => {
                let config = crate::load_config(&no_symlink_parents(root, &path)?)?;
                let retained_reference = crate::units::reference_publication(&config)?;
                Ok(
                    json!({"schema":config.schema,"judge":config.judge,"units":config.policy["units"],
                    "report_path":config.report_path,"retained_reference":retained_reference}),
                )
            }
            Operation::ValidatePreparationRequest { request } => {
                validate_request(root, &request)?;
                Ok(json!(crate::prepared::retention_directory(&request)))
            }
            Operation::ValidatePreparationResult { request, result } => {
                validate_request(root, &request)?;
                crate::prepared::validate_result(&request, &result)?;
                Ok(Value::Null)
            }
            Operation::ValidatePortableBinding { binding, transport } => {
                crate::prepared::validate_portable_binding_with_reader(
                    root,
                    &binding,
                    transport.as_ref(),
                    |original| requested_original(input, output, original, 64 * 1024 * 1024),
                )?;
                Ok(Value::Null)
            }
            Operation::RestoreScopedProcess { process, mut limit } => {
                let restored = crate::scoped_report::restore_process(&process, |original| {
                    let bytes = requested_original(input, output, original, limit)?;
                    limit -= bytes.len() as u64;
                    Ok(bytes)
                })?;
                serde_json::to_value(restored).map_err(|error| error.to_string())
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
#[cfg(not(verus_keep_ghost))]
fn validate_request(root: &Path, request: &crate::prepared::InputRequest) -> Result<(), String> {
    if request
        .host_root
        .canonicalize()
        .map_err(|e| e.to_string())?
        != root.canonicalize().map_err(|e| e.to_string())?
    {
        return Err("preparation request does not address the session checkout".into());
    }
    request.validate()
}

#[cfg(not(verus_keep_ghost))]
struct Bounded<'a> {
    file: &'a mut fs::File,
    remaining: u64,
}
#[cfg(not(verus_keep_ghost))]
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
#[cfg(not(verus_keep_ghost))]
/// The collector supplies each original under its own cumulative budget and declared paths.
fn requested_original(
    input: &mut impl BufRead,
    output: &mut impl Write,
    original: &Original,
    limit: u64,
) -> Result<Vec<u8>, String> {
    write_frame(output, &json!({"read_original": original}))?;
    let mut header = String::new();
    input
        .take(MAX_FRAME + 1)
        .read_line(&mut header)
        .map_err(|error| error.to_string())?;
    if header.len() as u64 > MAX_FRAME || !header.ends_with('\n') {
        return Err("invalid original reader header".into());
    }
    let length = serde_json::from_str::<Result<u64, String>>(&header)
        .map_err(|error| error.to_string())??;
    if length > limit {
        return Err("original exceeds collection bound".into());
    }
    write_frame(output, &json!({"send_original":true}))?;
    let mut bytes = Vec::new();
    input
        .take(length)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 != length {
        return Err("truncated collection original".into());
    }
    Ok(bytes)
}

#[cfg(not(verus_keep_ghost))]
fn write_frame(output: &mut impl Write, frame: &impl serde::Serialize) -> Result<(), String> {
    serde_json::to_writer(&mut *output, frame).map_err(|e| e.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|()| output.flush())
        .map_err(|e| e.to_string())
}

#[cfg(not(verus_keep_ghost))]
const MAX_FRAME: u64 = 128 * 1024 * 1024;

#[cfg(not(verus_keep_ghost))]
pub fn serve(root: &Path, mut input: impl BufRead, mut output: impl Write) -> Result<(), String> {
    let mut session = Session {
        implicit_reads: Some(retention::read_scope(root)?),
        publications: Default::default(),
        removal: None,
        root,
        inventory: Inventory::adopted(root)?,
        stages: BTreeMap::new(),
        directories: BTreeMap::new(),
        consumers: Vec::new(),
    };
    loop {
        let mut line = Vec::new();
        (&mut input)
            .take(MAX_FRAME + 1)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if line.is_empty() {
            return Ok(());
        }
        if line.len() as u64 > MAX_FRAME || !line.ends_with(b"\n") {
            return Err("invalid retained operation frame".into());
        }
        let result = decode(&line).and_then(|op| session.apply(op, &mut input, &mut output));
        write_frame(&mut output, &result)?;
    }
}

#[cfg(not(verus_keep_ghost))]
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

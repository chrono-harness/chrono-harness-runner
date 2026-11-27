//! Registered finding records belong to fixed Git endpoints already retained in the request.
#[cfg(verus_keep_ghost)]
use super::identifiers;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub struct BoundGitCommand<'a> {
    pub executable: &'a str,
    pub sha256: &'a str,
    pub cwd: &'a str,
    /// A batch binds its input when the requested object is read.
    pub stdin_sha256: Option<&'a str>,
}

pub struct RecordedGitCommand<'a> {
    pub executable: Option<&'a str>,
    pub sha256: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub stdin_sha256: Option<&'a str>,
    pub successful_exit: bool,
    pub failure_is_null: bool,
    pub stdout_complete: bool,
}

fn recorded_text_is(actual: Option<&str>, expected: &str) -> (ok: bool)
    ensures ok == (actual matches Some(text) && text@ == expected@),
{
    match actual {
        Some(text) => identifiers::equal_text(text, expected),
        None => false,
    }
}

impl<'a> BoundGitCommand<'a> {
    pub fn accepts(&self, argv_matches: bool, record: &RecordedGitCommand) -> (ok: bool)
        ensures ok == (argv_matches && record.successful_exit && record.failure_is_null
            && record.stdout_complete
            && (record.executable matches Some(path) && path@ == self.executable@)
            && (record.sha256 matches Some(hash) && hash@ == self.sha256@)
            && (record.cwd matches Some(dir) && dir@ == self.cwd@)
            && match self.stdin_sha256 {
                None => true,
                Some(expected) => record.stdin_sha256 matches Some(hash) && hash@ == expected@,
            }),
    {
        argv_matches && record.successful_exit && record.failure_is_null && record.stdout_complete
            && recorded_text_is(record.executable, self.executable)
            && recorded_text_is(record.sha256, self.sha256)
            && recorded_text_is(record.cwd, self.cwd)
            && match self.stdin_sha256 {
                None => true,
                Some(expected) => recorded_text_is(record.stdin_sha256, expected),
            }
    }
}
}

#[cfg(not(verus_keep_ghost))]
use crate::{facts, full};
#[cfg(not(verus_keep_ghost))]
use serde_json::Value;
#[cfg(not(verus_keep_ghost))]
use std::collections::BTreeMap;

#[cfg(not(verus_keep_ghost))]
fn component(path: &str) -> (&str, &str) {
    path.split_once('/')
        .map_or((path, ""), |(head, _)| (head, &path[head.len()..]))
}

#[cfg(not(verus_keep_ghost))]
pub(crate) struct Resolver<'a> {
    request: &'a Value,
    streams: BTreeMap<usize, Vec<u8>>,
    trees: BTreeMap<String, facts::Tree>,
    documents: BTreeMap<(String, String), Value>,
}

#[cfg(not(verus_keep_ghost))]
impl<'a> Resolver<'a> {
    pub(crate) fn new(request: &'a Value) -> Self {
        Self {
            request,
            streams: BTreeMap::new(),
            trees: BTreeMap::new(),
            documents: BTreeMap::new(),
        }
    }

    pub(crate) fn check(&mut self, reference: &str) -> Result<(), String> {
        use crate::verified_status::{FindingOwner, finding_owner};
        let root = component(reference.strip_prefix('/').ok_or("record root")?);
        let child = component(root.1.strip_prefix('/').unwrap_or(""));
        let record = component(child.1.strip_prefix('/').unwrap_or(""));
        let initial = self.request["protocol"] == crate::initial::PROTOCOL;
        let direct_member = self.request.pointer(reference).is_some();
        let (end, role, pointer) =
            match finding_owner(reference, root, child, record, initial, direct_member) {
                FindingOwner::Request(pointer) => {
                    return self
                        .request
                        .pointer(pointer)
                        .map(|_| ())
                        .ok_or_else(|| "request field unavailable".into());
                }
                FindingOwner::RegistryIdentity => {
                    return if self.request["registries"].is_object()
                        || self.request["registry_digest"].is_string()
                    {
                        Ok(())
                    } else {
                        Err("registry identity unavailable".into())
                    };
                }
                FindingOwner::Document {
                    endpoint,
                    role,
                    pointer,
                } => (endpoint, role, pointer),
                FindingOwner::Unknown => return Err("finding reference has no owner".into()),
            };
        let request = self.request;
        let oid = request[end]["commit"]
            .as_str()
            .ok_or("registry endpoint unavailable")?;
        let obs = &request["observations"];
        let binding = if initial {
            &obs["registry_binding"]
        } else {
            &obs["registry_bindings"][end]
        };
        let config = binding["effective_path"]
            .as_str()
            .ok_or("effective registry configuration unavailable")?;
        let path = if role == "config" {
            config.to_owned()
        } else {
            self.document(oid, config)?["registries"][role]
                .as_str()
                .ok_or("role absent from effective registry configuration")?
                .to_owned()
        };
        self.document(oid, &path)?
            .pointer(pointer)
            .ok_or("record absent from fixed registry")?;
        Ok(())
    }

    fn document(&mut self, oid: &str, path: &str) -> Result<&Value, String> {
        let key = (oid.to_owned(), path.to_owned());
        if !self.documents.contains_key(&key) {
            crate::relative_path(path)?;
            if !self.trees.contains_key(oid) {
                facts::full_oid(oid)?;
                let i = self
                    .processes_for(&["ls-tree", "-rz", "--full-tree", oid])?
                    .into_iter()
                    .next()
                    .ok_or("fixed Git tree original unavailable")?;
                let tree = facts::parse_tree(self.stream(i)?)?;
                self.trees.insert(oid.into(), tree);
            }
            let entry = self.trees[oid]
                .get(&facts::TreePath::from_text(path))
                .ok_or("registry file absent from fixed Git tree")?;
            if entry.kind != "blob" {
                return Err("registry file is not a blob".into());
            }
            let object = entry.oid.clone();
            let show = self.processes_for(&["show", &format!("{oid}:{path}")])?;
            let direct = show.first().copied();
            let mut contents = None;
            if let Some(i) = direct {
                let bytes = self.stream(i)?;
                if !crate::facts_binding::matches_blob_oid(&object, bytes) {
                    return Err("registry blob differs from fixed Git tree".into());
                }
                contents = Some(bytes.to_vec());
            } else {
                for i in self.processes_for(&["cat-file", "--batch"])? {
                    let stdin =
                        self.request["observations"]["git_facts"]["processes"][i]["stdin_sha256"]
                            .as_str()
                            .ok_or("Git batch input unavailable")?;
                    if let Some(bytes) =
                        crate::facts_binding::retained_batch_blob(self.stream(i)?, &object, stdin)?
                    {
                        contents = Some(bytes.to_vec());
                        break;
                    }
                }
            }
            let bytes = contents.ok_or("fixed registry blob original unavailable")?;
            self.documents.insert(key.clone(), crate::json(&bytes)?);
        }
        Ok(&self.documents[&key])
    }

    fn processes_for(&self, args: &[&str]) -> Result<Vec<usize>, String> {
        let git = &self.request["observations"]["git_facts"];
        let tool = git["binding"]["path"]
            .as_str()
            .ok_or("retained Git executable unavailable")?;
        let digest = git["binding"]["sha256"]
            .as_str()
            .filter(|s| crate::wire::is_digest(s))
            .ok_or("bound Git digest unavailable")?;
        let root = self.request["candidate"]["root"]
            .as_str()
            .ok_or("checkout root unavailable")?;
        let argv: Vec<&str> = [
            tool,
            "--no-optional-locks",
            "--no-replace-objects",
            "-C",
            root,
        ]
        .into_iter()
        .chain(args.iter().copied())
        .collect();
        let records = git["processes"]
            .as_array()
            .ok_or("Git process originals unavailable")?;
        let expected_stdin = (args != ["cat-file", "--batch"]).then(|| crate::sha256(&[]));
        let command = BoundGitCommand {
            executable: tool,
            sha256: digest,
            cwd: root,
            stdin_sha256: expected_stdin.as_deref(),
        };
        Ok(records
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                let argv_matches = p["argv"].as_array().is_some_and(|a| {
                    a.iter()
                        .map(|x| x.as_str())
                        .eq(argv.iter().map(|x| Some(*x)))
                });
                command.accepts(
                    argv_matches,
                    &RecordedGitCommand {
                        executable: p["executable"].as_str(),
                        sha256: p["sha256"].as_str(),
                        cwd: p["cwd"].as_str(),
                        stdin_sha256: p["stdin_sha256"].as_str(),
                        successful_exit: p["exit_code"] == 0,
                        failure_is_null: p["failure"].is_null(),
                        stdout_complete: p["stdout_complete"] == true,
                    },
                )
            })
            .map(|(index, _)| index)
            .collect())
    }

    fn stream(&mut self, index: usize) -> Result<&[u8], String> {
        if !self.streams.contains_key(&index) {
            let record = &self.request["observations"]["git_facts"]["processes"][index];
            let process: crate::ProcessResult =
                serde_json::from_value(full::expand_process(record)?)
                    .map_err(|error| error.to_string())?;
            full::validate_process_receipt(&process, "registry Git")?;
            self.streams.insert(index, process.stdout_bytes);
        }
        Ok(&self.streams[&index])
    }
}

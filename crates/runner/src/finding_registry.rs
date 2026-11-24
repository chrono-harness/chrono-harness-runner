//! Registered finding records belong to fixed Git endpoints already retained in the request.
use crate::{facts, full};
use serde_json::Value;
use std::collections::BTreeMap;

fn component(path: &str) -> (&str, &str) {
    path.split_once('/')
        .map_or((path, ""), |(head, _)| (head, &path[head.len()..]))
}

pub(crate) struct Resolver<'a> {
    request: &'a Value,
    streams: BTreeMap<usize, Vec<u8>>,
    trees: BTreeMap<String, facts::Tree>,
    documents: BTreeMap<(String, String), Value>,
}

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
        Ok(records
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p["argv"].as_array().is_some_and(|a| {
                    a.iter()
                        .map(|x| x.as_str())
                        .eq(argv.iter().map(|x| Some(*x)))
                }) && p["executable"] == tool
                    && p["sha256"] == digest
                    && expected_stdin
                        .as_ref()
                        .is_none_or(|d| p["stdin_sha256"].as_str() == Some(d))
                    && p["cwd"] == root
                    && p["exit_code"] == 0
                    && p["failure"].is_null()
                    && p["stdout_complete"] == true
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

//! Producer-held directories with optional host adoption. Reads and publication capabilities
//! remain in the same session as reference updates and admission.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
use super::identifiers;
#[cfg(verus_keep_ghost)]
use super::recovery_text;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
verus! {
pub enum ReportSettlement { Commit, Reject, Unmanaged }

pub fn report_settlement(adopted: bool, held_original: Option<&str>, original: &str) -> (decision: ReportSettlement)
    ensures match decision {
        ReportSettlement::Commit => recovery_text(held_original) == Some(original@),
        ReportSettlement::Unmanaged => !adopted && held_original.is_none(),
        ReportSettlement::Reject => (adopted || held_original.is_some())
            && recovery_text(held_original) != Some(original@),
    },
{
    match held_original {
        Some(held) => if identifiers::equal_text(held, original) { ReportSettlement::Commit }
                      else { ReportSettlement::Reject },
        None => if adopted { ReportSettlement::Reject } else { ReportSettlement::Unmanaged },
    }
}
}

#[cfg(not(verus_keep_ghost))]
use super::*;
#[cfg(not(verus_keep_ghost))]
use crate::ownership::Lease;
#[cfg(not(verus_keep_ghost))]
use crate::verified_release::{self, PublicationReadScope};
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Action {
    Create {
        parent: String,
        prefix: String,
        consumer_prefix: Option<String>,
        producer: String,
        outcome: Value,
    },
    CreateAt {
        path: String,
        producer: String,
        directory: bool,
        outcome: Value,
    },
    CreateReferenced {
        path: String,
        producer: String,
        reference: String,
        outcome: Value,
    },
    ResumeDirectory {
        path: String,
        producer: String,
    },
    CheckLength {
        path: String,
        length: u64,
    },
    HardLink {
        path: String,
        source: PathBuf,
        name: String,
    },
    Read {
        path: String,
        consumer: Option<String>,
    },
    Scope {
        path: Option<String>,
        inputs: bool,
    },
    EndScope {
        token: u64,
    },
    ReferenceHeld {
        slot: String,
        path: String,
        inputs: Vec<String>,
    },
    CreateMember {
        path: String,
        name: String,
    },
    Enroll {
        path: String,
        producer: String,
        outcome: Value,
    },
    RetainDiagnostic {
        path: String,
    },
    ReadSelected {
        path: String,
        slot: String,
    },
    ReserveContent {
        path: String,
        length: u64,
    },
    ReserveWrite {
        path: String,
        name: String,
        length: u64,
    },
    ReleaseRead {
        token: u64,
    },
    Copy {
        path: String,
        source: PathBuf,
        name: String,
    },
    AcceptConsumer {
        path: String,
        consumer: Consumer,
    },
    ProtectConsumer {
        path: String,
        consumer: String,
    },
    Reference {
        slot: String,
        path: String,
        inputs: Vec<String>,
    },
    Settle {
        slot: String,
        path: String,
    },
    Complete {
        path: String,
        outcome: Value,
        #[serde(default)]
        seal: bool,
    },
    Close {
        path: String,
    },
}
#[cfg(not(verus_keep_ghost))]
struct ReportReference {
    original: String,
    _guard: Lease,
    held_by_reader: bool,
}

#[cfg(not(verus_keep_ghost))]
enum ReadLease {
    Original { _guard: Lease },
    ReportSlot(String),
}

#[cfg(not(verus_keep_ghost))]
#[derive(Default)]
pub(super) struct Publications {
    open: BTreeMap<String, Publication>,
    slots: BTreeMap<String, ReportReference>,
    reads: BTreeMap<u64, ReadLease>,
    next_read: u64,
    scopes: BTreeMap<u64, retention::ReadScope>,
}
#[cfg(not(verus_keep_ghost))]
impl Publications {
    fn next_token(&mut self) -> Result<u64, String> {
        self.next_read =
            verified_release::next_observation(self.next_read).ok_or("read id exhausted")?;
        Ok(self.next_read)
    }

    fn retain_read(&mut self, guard: Option<Lease>) -> Result<Value, String> {
        if let Some(guard) = guard {
            let token = self.next_token()?;
            self.reads
                .insert(token, ReadLease::Original { _guard: guard });
            Ok(json!(token))
        } else {
            Ok(Value::Null)
        }
    }
    fn reference(
        &mut self,
        root: &Path,
        slot: String,
        original: String,
        inputs: &[String],
        held_by_reader: bool,
    ) -> Result<Value, String> {
        if self.slots.contains_key(&slot) {
            return Err("report slot already open".into());
        }
        let Some(guard) = retention::reference_report(root, &slot, &original, inputs, None)? else {
            return Ok(Value::Null);
        };
        let token = if held_by_reader {
            let token = self.next_token()?;
            self.reads
                .insert(token, ReadLease::ReportSlot(slot.clone()));
            Some(token)
        } else {
            None
        };
        self.slots.insert(
            slot,
            ReportReference {
                original,
                _guard: guard,
                held_by_reader,
            },
        );
        Ok(json!(token))
    }

    fn active(&self, path: &str) -> Result<&Publication, String> {
        self.open
            .get(path)
            .ok_or_else(|| format!("no active publication at {path}"))
    }
    pub(super) fn apply(
        &mut self,
        root: &Path,
        inventory: Option<&Inventory>,
        action: Action,
    ) -> Result<Value, String> {
        let relative = |path: &Path| {
            path.strip_prefix(root)
                .ok()
                .and_then(Path::to_str)
                .map(str::to_owned)
                .ok_or("invalid publication path".to_owned())
        };
        match action {
            Action::Create {
                parent,
                prefix,
                consumer_prefix,
                producer,
                outcome,
            } => {
                if prefix.contains('/') || prefix.contains('\0') {
                    return Err("invalid publication name prefix".into());
                }
                let directory = no_symlink_parents(root, &parent)?;
                let path = retention::unique_path(&directory, &prefix, "")?;
                let key = relative(&path)?;
                let p = match consumer_prefix {
                    Some(consumer) => retention::create_adopted_for_consumer(
                        root, &producer, &path, true, outcome, None, &consumer,
                    )?,
                    None => retention::create_adopted(root, &producer, &path, true, outcome)?,
                };
                let adopted = p.is_some();
                if let Some(p) = p {
                    self.open.insert(key.clone(), p);
                }
                Ok(json!({"path":key,"adopted":adopted}))
            }
            Action::CreateAt {
                path,
                producer,
                directory,
                outcome,
            } => {
                let target = no_symlink_parents(root, &path)?;
                if self.open.contains_key(&path) {
                    return Err("publication already open".into());
                }
                let publication =
                    retention::create_adopted(root, &producer, &target, directory, outcome)?;
                let adopted = publication.is_some();
                if let Some(publication) = publication {
                    self.open.insert(path, publication);
                }
                Ok(json!(adopted))
            }
            Action::CreateReferenced {
                path,
                producer,
                reference,
                outcome,
            } => {
                if self.open.contains_key(&path) {
                    return Err("publication already open".into());
                }
                let publication = Inventory::adopted(root)?
                    .ok_or("adopted original inventory unavailable")?
                    .plan(&producer, &path, false, outcome)?;
                publication.materialize_for_reference(&reference)?;
                self.open.insert(path, publication);
                Ok(Value::Null)
            }
            Action::ResumeDirectory { path, producer } => {
                if self.open.contains_key(&path) {
                    return Err("publication already open".into());
                }
                let publication = Inventory::adopted(root)?
                    .ok_or("adopted original inventory unavailable")?
                    .resume_unfinished_directory(&producer, &path)?;
                let adopted = publication.is_some();
                if let Some(publication) = publication {
                    self.open.insert(path, publication);
                }
                Ok(json!(adopted))
            }
            Action::CheckLength { path, length } => {
                self.active(&path)?.check_length(length)?;
                Ok(Value::Null)
            }
            Action::HardLink { path, source, name } => {
                crate::relative_path(&name)?;
                let destination = no_symlink_parents(root, &format!("{path}/{name}"))?;
                self.active(&path)?.hard_link_file(&source, &destination)?;
                Ok(Value::Null)
            }
            Action::Read { path, consumer } => {
                let guard = match consumer {
                    Some(consumer) => retention::consumer_read_guard(root, &path, &consumer)?,
                    None => retention::read_guard(root, &path)?,
                };
                self.retain_read(guard)
            }
            Action::Scope { path, inputs } => {
                let scope = match verified_release::publication_read_scope(path, inputs) {
                    PublicationReadScope::Inputs(path) => {
                        retention::publication_read_scope(self.active(&path)?)?
                    }
                    PublicationReadScope::Output(path) => {
                        retention::published_output_scope(self.active(&path)?)?
                    }
                    PublicationReadScope::RootInputs => retention::read_scope(root)?,
                    PublicationReadScope::MissingOutput => {
                        return Err("output scope requires a publication".into());
                    }
                };
                let token = self.next_token()?;
                self.scopes.insert(token, scope);
                Ok(json!(token))
            }
            Action::EndScope { token } => {
                self.scopes
                    .remove(&token)
                    .ok_or("read scope is not active")?;
                Ok(Value::Null)
            }
            Action::ReferenceHeld { slot, path, inputs } => {
                self.reference(root, slot, path, &inputs, true)
            }
            Action::CreateMember { path, name } => {
                crate::relative_path(&name)?;
                let destination = no_symlink_parents(root, &format!("{path}/{name}"))?;
                self.active(&path)?.create_directory(&destination)?;
                Ok(Value::Null)
            }
            Action::Enroll {
                path,
                producer,
                outcome,
            } => {
                if self.open.contains_key(&path) {
                    return Err("publication already open".into());
                }
                let absolute = no_symlink_parents(root, &path)?;
                let p = retention::begin_adopted(root, &producer, &absolute, outcome)?;
                let adopted = p.is_some();
                if let Some(p) = p {
                    self.open.insert(path, p);
                }
                Ok(json!(adopted))
            }
            Action::RetainDiagnostic { path } => {
                inventory
                    .ok_or("retention policy disappeared")?
                    .retain_diagnostic_path(&path)?;
                Ok(Value::Null)
            }
            Action::ReadSelected { path, slot } => {
                let guard = retention::consumer_read_guard(
                    root,
                    &path,
                    &retention::current_report_role(&slot),
                )?;
                self.retain_read(guard)
            }
            Action::ReleaseRead { token } => {
                if let ReadLease::ReportSlot(slot) = self
                    .reads
                    .remove(&token)
                    .ok_or("read capability is not active")?
                {
                    self.slots.remove(&slot);
                }
                Ok(Value::Null)
            }
            Action::ReserveContent { path, length } => {
                let target = no_symlink_parents(root, &path)?;
                self.active(&path)?.reserve_external_file(&target, length)?;
                Ok(json!(target))
            }
            Action::ReserveWrite { path, name, length } => {
                crate::relative_path(&name)?;
                let destination = no_symlink_parents(root, &format!("{path}/{name}"))?;
                self.active(&path)?
                    .reserve_external_file(&destination, length)?;
                Ok(json!(destination))
            }
            Action::Copy { path, source, name } => {
                crate::relative_path(&name)?;
                let destination = no_symlink_parents(root, &format!("{path}/{name}"))?;
                retention::copy_adopted(Some(self.active(&path)?), &source, &destination)?;
                Ok(Value::Null)
            }
            Action::AcceptConsumer { path, consumer } => {
                self.active(&path)?.accept_consumer(&consumer)?;
                Ok(Value::Null)
            }
            Action::ProtectConsumer { path, consumer } => {
                Ok(json!(self.active(&path)?.protect_consumer(&consumer)?))
            }
            Action::Reference { slot, path, inputs } => {
                self.reference(root, slot, path, &inputs, false)
            }
            Action::Settle { slot, path } => {
                let claim = self.slots.get(&slot);
                match report_settlement(inventory.is_some(), claim.map(|c| c.original.as_str()), &path) {
                    ReportSettlement::Unmanaged => return Ok(Value::Null),
                    ReportSettlement::Reject => return Err("report settlement requires the active reference for this slot and original".into()),
                    ReportSettlement::Commit => retention::settle_report(root, &slot, &path)?,
                }
                if claim.is_some_and(|c| !c.held_by_reader) {
                    self.slots.remove(&slot);
                }
                Ok(Value::Null)
            }
            Action::Complete {
                path,
                outcome,
                seal,
            } => {
                let publication = self.open.remove(&path).ok_or("no active publication")?;
                if seal {
                    publication.complete_sealed(outcome)?;
                } else {
                    publication.complete(outcome)?;
                }
                Ok(Value::Null)
            }
            Action::Close { path } => {
                self.open.remove(&path).ok_or("no active publication")?;
                Ok(Value::Null)
            }
        }
    }
}

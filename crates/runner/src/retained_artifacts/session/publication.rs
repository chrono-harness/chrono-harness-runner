//! Producer-held directories with optional host adoption. Reads and publication capabilities
//! remain in the same session as reference updates and admission.
use super::*;
use crate::ownership::Lease;
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Action {
    Create {
        parent: String,
        prefix: String,
        producer: String,
        outcome: Value,
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
    },
    Close {
        path: String,
    },
}
#[derive(Default)]
pub(super) struct Publications {
    open: BTreeMap<String, Publication>,
    slots: BTreeMap<String, Lease>,
    reads: BTreeMap<u64, Lease>,
    next_read: u64,
}
impl Publications {
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
                producer,
                outcome,
            } => {
                if prefix.contains('/') || prefix.contains('\0') {
                    return Err("invalid publication name prefix".into());
                }
                let directory = no_symlink_parents(root, &parent)?;
                let path = retention::unique_path(&directory, &prefix, "")?;
                let key = relative(&path)?;
                let p = retention::create_adopted(root, &producer, &path, true, outcome)?;
                let adopted = p.is_some();
                if let Some(p) = p {
                    self.open.insert(key.clone(), p);
                }
                Ok(json!({"path":key,"adopted":adopted}))
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
                let token = if let Some(guard) = guard {
                    self.next_read = self.next_read.checked_add(1).ok_or("read id exhausted")?;
                    self.reads.insert(self.next_read, guard);
                    Some(self.next_read)
                } else {
                    None
                };
                Ok(json!(token))
            }
            Action::ReleaseRead { token } => {
                self.reads
                    .remove(&token)
                    .ok_or("read capability is not active")?;
                Ok(Value::Null)
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
                if self.slots.contains_key(&slot) {
                    return Err("report slot already open".into());
                }
                if let Some(guard) = retention::reference_report(root, &slot, &path, &inputs, None)?
                {
                    self.slots.insert(slot, guard);
                }
                Ok(Value::Null)
            }
            Action::Settle { slot, path } => {
                retention::settle_report(root, &slot, &path)?;
                self.slots.remove(&slot);
                Ok(Value::Null)
            }
            Action::Complete { path, outcome } => {
                self.open
                    .remove(&path)
                    .ok_or("no active publication")?
                    .complete(outcome)?;
                Ok(Value::Null)
            }
            Action::Close { path } => {
                self.open.remove(&path).ok_or("no active publication")?;
                Ok(Value::Null)
            }
        }
    }
}

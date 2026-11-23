//! Address only the named artifact, producer and consumer reference.
use super::Session;
use crate::retained_artifacts::retention::{self, Inventory};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case", deny_unknown_fields)]
pub(super) enum Fact {
    Adoption {
        producers: Vec<String>,
    },
    Rooted {
        producer: String,
        role: String,
    },
    Enrolled {
        path: String,
    },
    Status {
        producer: String,
        path: String,
    },
    Maintain {
        ids: Vec<String>,
    },
    ResumeSealed {
        producer: String,
        path: String,
        outcome: Value,
    },
    Published {
        path: String,
        sha256: String,
        length: u64,
    },
    Member {
        producer: String,
        path: String,
        member: String,
        sha256: String,
        length: u64,
    },
    Consume {
        id: String,
    },
    ReleaseRole {
        role: String,
    },
    FinalOriginal {
        producer: String,
        path: String,
        text: String,
        outcome: Value,
    },
}
impl Session<'_> {
    pub(super) fn inventory_fact(&mut self, request: Fact) -> Result<Value, String> {
        if let Fact::Adoption { producers } = request {
            let Some(i) = &self.inventory else {
                return Ok(Value::Null);
            };
            return Ok(json!(
                producers
                    .iter()
                    .map(|p| (p, i.terminal_limits(p).is_some()))
                    .collect::<std::collections::BTreeMap<_, _>>()
            ));
        }
        if let Fact::FinalOriginal {
            producer,
            path,
            text,
            outcome,
        } = request
        {
            return Ok(json!(retention::publish_final_original(
                self.root,
                &producer,
                &path,
                text.as_bytes(),
                outcome,
                true
            )?));
        }
        let inv = self
            .inventory
            .as_ref()
            .ok_or("adopted original inventory unavailable")?;
        match request {
            Fact::Adoption { .. } | Fact::FinalOriginal { .. } => unreachable!(),
            Fact::Rooted { producer, role } => Ok(json!(inv.rooted_original(&producer, &role)?)),
            Fact::Enrolled { path } => Ok(json!(inv.enrolled(&path)?)),
            Fact::Status { producer, path } => inv.producer_status(&producer, Some(&path)),
            Fact::Maintain { ids } => inv.maintain_originals(&ids),
            Fact::ResumeSealed {
                producer,
                path,
                outcome,
            } => {
                let publication = Inventory::adopted(self.root)?
                    .ok_or("adopted original inventory unavailable")?
                    .resume_unfinished_directory(&producer, &path)?;
                match publication {
                    Some(p) => {
                        p.complete_sealed(outcome)?;
                        Ok(json!(true))
                    }
                    None => Ok(json!(false)),
                }
            }
            Fact::Published {
                path,
                sha256,
                length,
            } => Ok(json!(inv.published_original(&path, &sha256, length)?)),
            Fact::Member {
                producer,
                path,
                member,
                sha256,
                length,
            } => Ok(json!(inv.published_tree_member(
                &producer, &path, &member, &sha256, length
            )?)),
            Fact::Consume { id } => {
                self.consumers.push(inv.consume(&id)?);
                Ok(Value::Null)
            }
            Fact::ReleaseRole { role } => {
                inv.reference_paths(&role, &[])?;
                Ok(Value::Null)
            }
        }
    }
}

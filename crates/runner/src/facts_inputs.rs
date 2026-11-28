//! Explicit Git input guards. Never discover files or register dependencies.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// An observation records the declaration handed to the file reader and its original result.
pub trait GuardObserver<D,R,E> {
    spec fn observations(&self) -> Seq<(D,Result<R,E>)>;
    fn observe(&mut self, declaration: D) -> (out: Result<R,E>)
        ensures self.observations() == old(self).observations().push((declaration,out));
}

pub fn observe_declared_guard<D,R,E,O: GuardObserver<D,R,E>>(declaration: Option<D>, observer: &mut O)
    -> (out: Result<Option<R>,E>)
    ensures match declaration {
        None => observer.observations() == old(observer).observations() && out == Ok::<Option<R>,E>(None),
        Some(d) => match out {
            Ok(Some(r)) => observer.observations() == old(observer).observations().push((d,Ok::<R,E>(r))),
            Err(e) => observer.observations() == old(observer).observations().push((d,Err::<R,E>(e))),
            Ok(None) => false,
        },
    },
{
    match declaration {
        None => Ok(None),
        Some(d) => Ok(Some(observer.observe(d)?)),
    }
}
}

#[cfg(not(verus_keep_ghost))]
pub(crate) use native::{Guard, declaration};

#[cfg(not(verus_keep_ghost))]
mod native {
    use crate::{input_file::observe_file, relative_path, verified_git_binding};
    use serde_json::{Value, json};
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::Path,
    };

    pub(crate) fn declaration(config: &Value) -> Result<Option<Vec<Value>>, String> {
        let Some(guard) = config["facts_git"].get("guard") else {
            return Ok(None);
        };
        verified_git_binding::admit_git_guard_object(
            guard.as_object().map(|object| object.len()),
            guard["schema"].as_str(),
            guard.get("inputs").is_some(),
        )?;
        let ids = verified_git_binding::git_guard_ids(
            guard["inputs"].as_array().map(|ids| (ids, ids.len())),
        )?;
        let inputs =
            verified_git_binding::git_guard_files(config["environment"]["inputs"].as_array())?;
        let mut seen = BTreeSet::new();
        let mut selected = vec![];
        for id in ids {
            let id = verified_git_binding::git_guard_id(id.as_str())?;
            let index = verified_git_binding::git_guard_reference(
                &JsonGuardReferences(inputs),
                id,
                seen.insert(id),
            )
            .map_err(guard_error)?;
            let input = &inputs[index];
            let location = verified_git_binding::git_guard_location(input["location"].as_str())?;
            if !Path::new(location).is_absolute() {
                relative_path(location)?;
            }
            verified_git_binding::admit_git_guard_presence(
                id,
                input["presence"].as_str(),
                input.get("sha256").is_some(),
                input["sha256"].as_str(),
            )
            .map_err(guard_error)?;
            selected.push(input.clone());
        }
        Ok(Some(selected))
    }

    struct JsonGuardReferences<'a>(&'a [Value]);

    impl verified_git_binding::BindingReferenceRows for JsonGuardReferences<'_> {
        fn len(&self) -> usize {
            self.0.len()
        }
        fn id(&self, index: usize) -> Option<&str> {
            self.0[index]["id"].as_str()
        }
    }

    fn guard_error(error: verified_git_binding::GitGuardError<'_>) -> String {
        format!("{}{}{}", error.prefix, error.id, error.suffix)
    }

    pub(crate) struct Guard {
        entries: Vec<Value>,
        pub observation: Value,
    }
    impl Guard {
        pub fn from_declaration(
            root: &Path,
            declared: Option<Vec<Value>>,
        ) -> Result<Option<Self>, String> {
            super::observe_declared_guard(declared, &mut InputObserver(root))
        }
        pub fn unchanged(&self, root: &Path) -> Result<(), String> {
            if observe(root, &self.entries)? != self.observation {
                return Err("Git input observation changed".into());
            }
            Ok(())
        }
    }
    struct InputObserver<'a>(&'a Path);

    impl super::GuardObserver<Vec<Value>, Guard, String> for InputObserver<'_> {
        fn observe(&mut self, entries: Vec<Value>) -> Result<Guard, String> {
            let observation = observe(self.0, &entries)?;
            Ok(Guard {
                entries,
                observation,
            })
        }
    }

    fn observe(root: &Path, entries: &[Value]) -> Result<Value, String> {
        let mut files = BTreeMap::new();
        for input in entries {
            let id = input["id"].as_str().ok_or("Git input ID")?;
            let path = Path::new(input["location"].as_str().ok_or("Git input location")?);
            let path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            };
            let identity = observe_file(&path).map_err(|e| format!("Git input {id}: {e}"))?;
            let admitted = verified_git_binding::admit_git_guard_observation(
                id,
                input["presence"].as_str(),
                input["sha256"].as_str(),
                identity
                    .as_ref()
                    .map(|(digest, length)| (digest.as_str(), *length)),
            )
            .map_err(|(prefix, id, suffix)| format!("{prefix}{id}{suffix}"))?;
            let observation = match admitted {
                None => json!({"presence":"absent","location":path}),
                Some((digest, length)) => {
                    json!({"presence":"present","location":path,"sha256":digest,"length":length})
                }
            };
            files.insert(id, observation);
        }
        Ok(
            json!({"schema":"chrono-git-inputs-result/v1","files":files,"completeness_proven":false}),
        )
    }
}

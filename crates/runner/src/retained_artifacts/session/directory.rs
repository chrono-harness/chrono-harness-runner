//! An external producer keeps this native publication open until its process and writes finish.
use crate::retained_artifacts::retention::{self, Inventory, Publication};
use crate::{CommandSpec, ProcessEvidence, ProcessResult, no_symlink_parents, sha256};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Directory {
    path: String,
    reference: String,
    publication: Option<Publication>,
    executed: std::collections::BTreeSet<Option<String>>,
    rooted: bool,
}

impl Directory {
    pub(super) fn begin(
        root: &Path,
        producer: &str,
        path: String,
        reference: String,
        intent: Value,
        rooted: bool,
    ) -> Result<Self, String> {
        crate::units::artifact_path(&path)?;
        let absolute = no_symlink_parents(root, &path)?;
        fs::create_dir_all(
            absolute
                .parent()
                .ok_or("original directory has no parent")?,
        )
        .map_err(|e| e.to_string())?;
        no_symlink_parents(root, &path)?;
        let publication = if let Some(inventory) = Inventory::adopted(root)? {
            if !inventory.accepts(producer, &path) {
                return Err("original directory is outside the registered producer scope".into());
            }
            let publication = if rooted {
                let p = inventory.plan_rooted(producer, &path, intent, &reference)?;
                p.materialize()?;
                p
            } else {
                let p = inventory.plan(producer, &path, true, intent)?;
                p.materialize_for_reference(&reference)?;
                p
            };
            Some(publication)
        } else {
            if rooted {
                return Err("adopted original inventory unavailable".into());
            }
            fs::create_dir(&absolute).map_err(|e| {
                format!("operation state {path}: {e}; preserve originals, do not redispatch")
            })?;
            None
        };
        Ok(Self {
            path,
            reference,
            publication,
            executed: Default::default(),
            rooted,
        })
    }

    pub(super) fn identity(&self) -> Result<&str, String> {
        self.publication
            .as_ref()
            .map(Publication::id)
            .ok_or_else(|| "adopted original inventory unavailable".to_owned())
    }
    pub(super) fn create_member(&self, root: &Path, name: &str) -> Result<(), String> {
        let p = self.file(root, name)?;
        self.publication
            .as_ref()
            .ok_or("adopted original inventory unavailable")?
            .create_directory(&p)
    }
    pub(super) fn reserve_file(
        &self,
        root: &Path,
        name: &str,
        length: u64,
    ) -> Result<PathBuf, String> {
        crate::relative_path(name)?;
        let p = no_symlink_parents(root, &format!("{}/{}", self.path, name))?;
        self.publication
            .as_ref()
            .ok_or("adopted original inventory unavailable")?
            .reserve_external_file(&p, length)?;
        Ok(p)
    }
    pub(super) fn seal(self, outcome: Value) -> Result<(), String> {
        self.publication
            .ok_or("adopted original inventory unavailable")?
            .complete_sealed(outcome)
    }
    fn file(&self, root: &Path, name: &str) -> Result<PathBuf, String> {
        if Path::new(name).file_name().and_then(|p| p.to_str()) != Some(name) {
            return Err("original must name one file within its publication".into());
        }
        no_symlink_parents(root, &format!("{}/{name}", self.path))
    }

    pub(super) fn write(&self, root: &Path, name: &str, value: &Value) -> Result<Value, String> {
        let path = self.file(root, name)?;
        let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        retention::write_adopted(self.publication.as_ref(), &path, &bytes)?;
        fs::File::open(root.join(&self.path))
            .and_then(|dir| dir.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(
            json!({"path":format!("{}/{name}", self.path), "sha256":sha256(&bytes), "length":bytes.len()}),
        )
    }

    pub(super) fn execute(
        &mut self,
        root: &Path,
        command: &CommandSpec,
        digest: &str,
        member: Option<&str>,
    ) -> Result<ProcessResult, String> {
        if member.is_some() != self.rooted {
            return Err("a rooted publication runs one command per member".into());
        }
        if !self.executed.insert(member.map(str::to_owned)) {
            return Err("retained operation has already been attempted".into());
        }
        let directory = match member {
            None => no_symlink_parents(root, &self.path)?,
            Some(name) => self.file(root, name)?,
        };
        let file = |name: &str| {
            let relative = directory.join(name);
            no_symlink_parents(
                root,
                relative
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_str()
                    .ok_or("non UTF-8 original path")?,
            )
        };
        let stdout = file("stdout")?;
        let stderr = file("stderr")?;
        let launch = file("launch.json")?;
        let evidence = ProcessEvidence {
            root,
            stdout: &stdout,
            stderr: &stderr,
            launch: &launch,
            publication: self.publication.as_ref(),
        };
        crate::run_process_observed_retained(root, command, &[], digest, &evidence)
    }

    pub(super) fn check_write(&self) -> Result<(), String> {
        match &self.publication {
            Some(publication) => publication.check_length(0),
            None => Ok(()),
        }
    }

    pub(super) fn finish(
        self,
        inventory: Option<&Inventory>,
        terminal: bool,
        outcome: Value,
        inputs: &[String],
    ) -> Result<(), String> {
        if let Some(inventory) = inventory {
            inventory.dependencies_paths(&self.path, inputs)?;
        }
        if let Some(publication) = self.publication {
            publication.complete_sealed(outcome)?;
        }
        if let Some(inventory) = inventory {
            inventory.retain_diagnostic_path(&self.path)?;
            if terminal {
                inventory.reference_paths(&self.reference, &[])?;
            }
        }
        Ok(())
    }
}

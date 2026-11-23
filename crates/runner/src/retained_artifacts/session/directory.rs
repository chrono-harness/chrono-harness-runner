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
    executed: bool,
    compiler_output: Option<String>,
}

impl Directory {
    pub(super) fn begin(
        root: &Path,
        producer: &str,
        path: String,
        reference: String,
        intent: Value,
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
            let publication = inventory.plan(producer, &path, true, intent)?;
            publication.materialize_for_reference(&reference)?;
            Some(publication)
        } else {
            fs::create_dir(&absolute).map_err(|e| {
                format!("operation state {path}: {e}; preserve originals, do not redispatch")
            })?;
            None
        };
        Ok(Self {
            path,
            reference,
            publication,
            executed: false,
            compiler_output: None,
        })
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
    ) -> Result<ProcessResult, String> {
        if std::mem::replace(&mut self.executed, true) {
            return Err("retained operation has already been attempted".into());
        }
        let stdout = self.file(root, "stdout")?;
        let stderr = self.file(root, "stderr")?;
        let launch = self.file(root, "launch.json")?;
        let evidence = ProcessEvidence {
            root,
            stdout: &stdout,
            stderr: &stderr,
            launch: &launch,
            publication: self.publication.as_ref(),
        };
        let process = crate::run_process_observed_retained(root, command, &[], digest, &evidence)?;
        if process.exit_code == 0
            && process.failure.is_none()
            && process.owned_work_joined
            && crate::verified_verifier::canonical_verifier_arguments(&command.args)
        {
            self.compiler_output = Some(command.args[10].clone());
        }
        Ok(process)
    }

    pub(super) fn evidence(&self, root: &Path, process: &ProcessResult) -> Result<Value, String> {
        if !self.executed {
            return Err("original directory has no executed process".into());
        }
        let names = ["stdout", "stderr", "launch.json"].map(|name| format!("{}/{name}", self.path));
        let record = crate::scoped_report::retain_process_evidence(
            root, process, &names[0], &names[1], &names[2],
        )?;
        let length = |name| {
            fs::symlink_metadata(self.file(root, name)?)
                .map(|m| m.len())
                .map_err(|e| e.to_string())
        };
        Ok(
            json!({"record":record,"stdout_length":length("stdout")?, "stderr_length":length("stderr")?}),
        )
    }

    /// The original process chose these two compiler outputs and has joined. Verify both before retiring either.
    pub(super) fn dispose_compiler(
        &self,
        root: &Path,
        library: (&str, u64),
        dependency: (&str, u64),
    ) -> Result<(), String> {
        let output = self
            .compiler_output
            .as_ref()
            .ok_or("no returned compiler output in this execution")?;
        crate::units::artifact_path(output)?;
        let dep = format!("{output}.d");
        let entries = [(output.as_str(), library), (dep.as_str(), dependency)]
            .into_iter()
            .map(|(relative, expected)| {
                let path = no_symlink_parents(root, relative)?;
                let identity = crate::artifact_disposal::identity(&path)?;
                let (observed, length) = crate::file_identity(&path)?;
                if (observed.as_str(), length) != expected {
                    return Err(
                        "operation original differs from the actual process result".to_owned()
                    );
                }
                Ok((relative, identity))
            })
            .collect::<Result<Vec<_>, String>>()?;
        // For unmanaged publications, the durable result is the only remaining copy of the original receipt.
        if self.publication.is_none() {
            fs::File::open(self.file(root, "result.json")?)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        let parent = Path::new(output)
            .parent()
            .and_then(Path::to_str)
            .ok_or("compiler output parent")?;
        crate::artifact_disposal::source_safe(root, parent)?;
        for (relative, identity) in entries {
            crate::artifact_disposal::dispose_entry(root, relative, &identity, || {
                self.check_write()
            })?;
        }
        fs::File::open(no_symlink_parents(root, parent)?)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
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

//! Admission of a worktree request before reading its registered input files.
#[cfg(verus_keep_ghost)]
use super::identifiers;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub fn validate_local_preparation<F>(source: &str, has_artifacts: bool, prepared: bool, validate: F)
    -> (result: Result<(), String>)
    where F: FnOnce() -> Result<(), String>,
    requires (source@ == "local"@ && !has_artifacts && !prepared) ==> validate.requires(()),
    ensures
        if source@ == "local"@ && !has_artifacts && !prepared {
            validate.ensures((), result)
        } else {
            result is Err && result->Err_0@ == "expected an unprepared local input request"@
        },
{
    if identifiers::equal_text(source, "local") && !has_artifacts && !prepared {
        validate()
    } else {
        Err("expected an unprepared local input request".to_owned())
    }
}
}

#[cfg(not(verus_keep_ghost))]
use crate::wire::Request;
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Query {
    ScopedRequest {
        request: Box<crate::Request>,
        config: Value,
        admit_entry: bool,
    },
    LocalPreparation {
        request: Box<crate::prepared::InputRequest>,
    },
    Participation {
        config: Value,
    },
    Template {
        root: PathBuf,
        entry: Value,
        argv: Vec<String>,
        binding: Value,
    },
    Request {
        request: Box<Request>,
        config: Value,
    },
}
#[cfg(not(verus_keep_ghost))]
pub(super) fn serve(input: impl Read, mut output: impl Write) -> Result<(), String> {
    let query: Query = serde_json::from_reader(input).map_err(|e| e.to_string())?;
    match query {
        Query::ScopedRequest {
            request,
            config,
            admit_entry,
        } => {
            let short = config["schema_version"] == 4;
            let initial = request.base.is_none();
            if initial && !short {
                return Err("initial scoped invocation requires short configuration".into());
            }
            let mut argv: Vec<String> = if initial {
                crate::prepared::declaration(&config)?.argv
            } else {
                serde_json::from_value(config["canonical_check"]["argv"].clone())
                    .map_err(|e| e.to_string())?
            };
            if let Some(scope) = &request.scope {
                argv.extend(scope.contract_argv(&config));
            }
            if admit_entry {
                let endpoints = match &request.base {
                    Some(base) => json!({"base":base,"candidate":request.candidate}),
                    None => json!({}),
                };
                crate::resolve_invocation(
                    &request.host_root,
                    &request.observations["entry"],
                    &argv,
                    &endpoints,
                )?;
                if short {
                    crate::prepared::validate_binding(
                        &request.host_root,
                        &config,
                        &request.config_path,
                        request.base.as_deref(),
                        &request.candidate,
                        request.initial,
                        &request.scope,
                        None,
                        &request.observations["preparation"],
                    )?;
                }
            }
        }
        Query::LocalPreparation { request } => {
            validate_local_preparation(
                &request.source,
                request.native_artifacts.is_some(),
                request.prepared.is_some(),
                || request.validate(),
            )?;
        }
        Query::Participation { config } => {
            let declaration = crate::prepared::declaration(&config)?;
            return serde_json::to_writer(&mut output, &declaration.participation)
                .map_err(|error| error.to_string());
        }
        Query::Template {
            root,
            entry,
            argv,
            binding,
        } => {
            crate::resolve_invocation(&root, &entry, &argv, &binding)?;
        }
        Query::Request { request, config } => {
            request.validate()?;
            let mut argv: Vec<String> =
                serde_json::from_value(config["canonical_check"]["argv"].clone())
                    .map_err(|e| e.to_string())?;
            if let Some(scope) = &request.scope {
                argv.extend(scope.contract_argv(&config));
            }
            crate::resolve_invocation(
                &request.candidate.root,
                &request.observations["entry"],
                &argv,
                &json!({"base":request.base.commit,"candidate":request.candidate.commit}),
            )?;
            if config["schema_version"] == 4 {
                crate::prepared::validate_binding(
                    &request.candidate.root,
                    &config,
                    &request.config_path,
                    Some(&request.base.commit),
                    &request.candidate.commit,
                    false,
                    &request.scope,
                    Some(&request.context),
                    &request.observations["preparation"],
                )?;
            }
        }
    }
    output
        .write_all(b"null")
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}

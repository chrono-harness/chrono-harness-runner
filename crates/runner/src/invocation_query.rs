use crate::wire::Request;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    path::PathBuf,
};

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Query {
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
pub(super) fn serve(input: impl Read, mut output: impl Write) -> Result<(), String> {
    let query: Query = serde_json::from_reader(input).map_err(|e| e.to_string())?;
    match query {
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

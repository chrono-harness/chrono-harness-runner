//! Fixed endpoints and verifier declarations supplied by the route planner.
use crate::{
    facts,
    verified_verifier::{OutputContract, ProofInvocation, source_pipeline, sources},
    wire,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    path::Path,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    commit: String,
    filemap: Value,
    projects: Value,
    artifacts: Vec<String>,
}
impl Endpoint {
    fn view(&self) -> (&str, &Value, &Value, Vec<String>) {
        (
            &self.commit,
            &self.filemap,
            &self.projects,
            self.artifacts.clone(),
        )
    }
}

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
enum Git {
    Legacy,
    Observed { config: String, observations: Value },
    Request { request: Box<wire::Request> },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MethodScope {
    operation: String,
    owner: String,
    tool: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Invocation {
    operation: String,
    argv: Vec<String>,
    output_contract: Option<OutputContract>,
    output_limit_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    previous: Option<Endpoint>,
    candidate: Endpoint,
    affected: Vec<String>,
    scopes: Vec<MethodScope>,
    invocations: Vec<Invocation>,
    git: Git,
}

fn validate(q: &Query, cwd: &Path, observation: &mut Value) -> Result<(), String> {
    let registry: sources::SourceRegistry =
        serde_json::from_value(q.candidate.filemap.clone()).map_err(|e| e.to_string())?;
    let previous: Result<Option<sources::SourceRegistry>, String> = q
        .previous
        .as_ref()
        .map(|endpoint| serde_json::from_value(endpoint.filemap.clone()))
        .transpose()
        .map_err(|e| e.to_string());
    let paths = source_pipeline::continuity_paths(
        &registry.files.entries,
        registry.files.require_new_file_classification,
        &q.affected,
    );
    let nodes = q
        .affected
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let scopes = q
        .scopes
        .iter()
        .map(|method| sources::SourceMethodScope {
            operation: &method.operation,
            owner: &method.owner,
            tool: &method.tool,
        })
        .collect();
    let invocations = q
        .invocations
        .iter()
        .map(|method| ProofInvocation {
            operation: &method.operation,
            argv: &method.argv,
            output_contract: &method.output_contract,
            output_limit_bytes: method.output_limit_bytes,
        })
        .collect();

    source_pipeline::with_source_continuity(
        (),
        q.previous.as_ref().map(Endpoint::view),
        q.candidate.view(),
        &paths,
        |state, previous, candidate, paths| {
            let acquire = || {
                if paths.is_empty() {
                    return Ok(sources::continuity::Continuity::empty());
                }
                let (root, reader) = match &q.git {
                    Git::Legacy => (cwd, facts::Reader::legacy()),
                    Git::Observed {
                        config,
                        observations,
                    } => (
                        cwd,
                        facts::Reader::from_observations(cwd, config, candidate.0, observations)?,
                    ),
                    Git::Request { request } => (
                        request.candidate.root.as_path(),
                        facts::Reader::for_request(request)?,
                    ),
                };
                let result =
                    sources::continuity::observe(&reader, root, previous, candidate, paths);
                *observation = reader.observation();
                result
            };
            (state, acquire())
        },
        |state, continuity| {
            let result = source_pipeline::validate_source_plan(
                &registry,
                previous
                    .as_ref()
                    .map(Option::as_ref)
                    .map_err(String::as_str),
                &nodes,
                &invocations,
                &scopes,
                &continuity,
            )
            .map_err(|error| error.render(|message, path| format!("{message}{path}")));
            (state, result)
        },
    )
    .1
}

pub fn serve(cwd: &Path, input: impl Read, mut output: impl Write) -> Result<(), String> {
    let q: Query = serde_json::from_reader(input).map_err(|e| e.to_string())?;
    facts::full_oid(&q.candidate.commit)?;
    if let Some(previous) = &q.previous {
        facts::full_oid(&previous.commit)?;
    }
    if let Git::Request { request } = &q.git {
        request.validate()?;
        if request.candidate.commit != q.candidate.commit
            || q.previous.as_ref().map(|p| &p.commit) != Some(&request.base.commit)
        {
            return Err("E_GIT_FACTS: source coverage endpoints differ from request".into());
        }
    }
    let mut observation = Value::Null;
    let error = validate(&q, cwd, &mut observation).err();
    serde_json::to_writer(
        &mut output,
        &json!({
            "previous":q.previous.as_ref().map(|p| &p.commit), "candidate":q.candidate.commit,
            "git_facts":observation, "error":error
        }),
    )
    .map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

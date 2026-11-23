//! Shared chrono-judge/v1 wire types and transport. No host admissibility policy.
use crate::{
    CommandSpec, ProcessResult, decode, no_symlink_parents, no_symlink_parents_detailed,
    run_process_observed_detailed, sha256,
};
use chrono_diagnostics::error::{ErrorRecord, Failure, ReceivedError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, path::PathBuf};
pub const PROTOCOL: &str = "chrono-judge/v1";
/// 运行共用判官协议入口，具体求值由调用方负责。
pub fn judge_main(name: &str, version: &str, judge: impl FnOnce(&Request) -> Response) -> u8 {
    crate::logged_text_main(name, |args| judge_entry(name, version, args, judge))
}

pub(crate) fn judge_entry(
    name: &str,
    version: &str,
    args: &[String],
    judge: impl FnOnce(&Request) -> Response,
) -> u8 {
    use std::io::Read;
    let (state, outcome) = crate::verified_cli::run_judge_entry(
        (),
        crate::verified_cli::judge_owned_mode(args),
        |()| {
            let mut bytes = vec![];
            let result = std::io::stdin()
                .take(crate::verified_cli::JUDGE_READ_LIMIT)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            ((), result)
        },
        |(), bytes| ((), crate::decode::<Request>(&bytes)),
        |(), request| {
            let response = judge(&request);
            let exit = response.status.exit_code() as u8;
            ((), response, exit)
        },
        |(), response| ((), canonical(&response)),
        |(), bytes| {
            let result = crate::publication::write_output(&mut std::io::stdout().lock(), &bytes);
            ((), result)
        },
    );
    crate::verified_cli::finish_judge_entry(
        state,
        outcome,
        |()| println!("{name} {version}"),
        |()| {
            crate::log_cli_error("E_USAGE: --protocol chrono-judge/v1");
            eprintln!("E_USAGE: --protocol chrono-judge/v1");
        },
        |()| {
            crate::log_cli_error("E_PROTOCOL: request read/bound");
            eprintln!("E_PROTOCOL: request read/bound");
        },
        |(), error| {
            crate::log_cli_error(&format!("E_PROTOCOL: {error}"));
            eprintln!("E_PROTOCOL: {error}");
        },
    )
    .1
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepared {
    request_id: String,
    #[serde(default)]
    outputs: BTreeMap<String, Value>,
    result: Result<Value, String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    status: Status,
    findings: Vec<Finding>,
    outputs: BTreeMap<String, Value>,
}

fn protocol_limits(req: &Request) -> Result<Value, String> {
    let config = crate::load_selected_config(&req.candidate.root, &req.config_path)?;
    Ok(config["protocol"].clone())
}
fn run_data(
    req: &Request,
    limits: &Value,
    [program, operation]: [&str; 2],
    input: &[u8],
) -> Result<Vec<u8>, String> {
    let env: BTreeMap<String, String> =
        serde_json::from_value(req.observations["environment"]["effective"].clone())
            .map_err(|e| e.to_string())?;
    let timeout_seconds = limits["timeout_seconds"]
        .as_u64()
        .ok_or("invalid protocol timeout")?;
    let output_limit_bytes = limits["stdout_limit_bytes"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("invalid protocol output bound")?;
    let command = CommandSpec {
        program: program.into(),
        args: vec![operation.into()],
        env,
        timeout_seconds,
        output_limit_bytes,
    };
    let process = crate::run_process(&req.candidate.root, &command, input)?;
    if process.exit_code != 0 {
        let error = std::str::from_utf8(&process.stderr_bytes).map_err(|e| e.to_string())?;
        return Err(if error.is_empty() {
            format!("{program} {operation} exited with {}", process.exit_code)
        } else {
            error.strip_suffix('\n').unwrap_or(error).into()
        });
    }
    Ok(process.stdout_bytes)
}

fn sealed_data(req: &Request, command: [&str; 2]) -> Result<Vec<u8>, String> {
    req.validate()?;
    let limits = protocol_limits(req)?;
    let payload = serde_json::json!({"request":req,"protocol":limits});
    run_data(req, &limits, command, &canonical(&payload)?)
}

/// The installed evaluator owns its findings. The caller chooses the admission error code.
pub(crate) fn single_judge(
    req: &Request,
    command: [&str; 2],
    error_code: &str,
    mode: &str,
) -> Response {
    let mut response = req.response(Status::Pass);
    let inherited = mode == "inherited";
    let data = if mode == "admission-result" {
        let admission = req.validate();
        crate::installed_query::inherited(
            &req.candidate.root,
            command[0],
            command[1],
            &serde_json::json!({"request":req,"admission":admission}),
        )
    } else if inherited {
        req.validate().and_then(|()| {
            crate::installed_query::inherited(
                &req.candidate.root,
                command[0],
                command[1],
                &serde_json::json!({"request":req}),
            )
        })
    } else {
        sealed_data(req, command)
    };
    match data.and_then(|bytes| decode::<Decision>(&bytes)) {
        Ok(decision) => {
            response.status = decision.status;
            response.findings = decision.findings;
            response.outputs = decision.outputs;
        }
        Err(message) => {
            response.status = if inherited {
                Status::Fail
            } else {
                Status::Error
            };
            let code = if inherited {
                message.split(':').next().unwrap_or(error_code)
            } else {
                error_code
            };
            response.findings.push(Finding {
                code: code.into(),
                level: "error".into(),
                message,
                delta_refs: vec!["/request".into()],
                causes: vec![],
            });
        }
    }
    response
}

/// An installed input producer can use the same admission and bounds outside the native judge entry.
pub(crate) fn prepare_command(
    command: [&str; 2],
    input: impl std::io::Read,
    mut output: impl std::io::Write,
) -> Result<(), String> {
    use std::io::Read;
    let mut bytes = vec![];
    input
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("E_PROTOCOL: request read/bound".into());
    }
    let req: Request = decode(&bytes)?;
    let result = sealed_data(&req, command).and_then(|bytes| {
        let prepared: Prepared = decode(&bytes)?;
        if prepared.request_id != req.request_id {
            return Err("E_PROTOCOL: prepared input identity".into());
        }
        Ok(bytes)
    });
    let bytes = match result {
        Ok(bytes) => bytes,
        Err(error) => canonical(
            &serde_json::json!({"request_id":req.request_id,"outputs":{},"result":{"Err":error}}),
        )?,
    };
    output
        .write_all(&bytes)
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}
/// Two installed data commands form a judge. Request admission, both limits and response publication stay in the runner.
pub(crate) fn judge_pipeline(
    req: &Request,
    preparation: [&str; 2],
    evaluation: [&str; 2],
) -> Response {
    let mut response = req.response(Status::Pass);
    let mut evaluate = || -> Result<(), String> {
        req.validate()?;
        let limits = protocol_limits(req)?;
        let run = |command, input: &[u8]| run_data(req, &limits, command, input);
        let prepared: Prepared = decode(&run(preparation, &req.canonical()?)?)?;
        if prepared.request_id != req.request_id {
            return Err("E_PROTOCOL: prepared input identity".into());
        }
        response.outputs = prepared.outputs;
        let input = prepared.result?;
        let decision: Decision = decode(&run(evaluation, &canonical(&input)?)?)?;
        if decision
            .outputs
            .keys()
            .any(|key| response.outputs.contains_key(key))
        {
            return Err("E_PROTOCOL: duplicate judge output".into());
        }
        response.status = decision.status;
        response.findings = decision.findings;
        response.outputs.extend(decision.outputs);
        Ok(())
    };
    if let Err(message) = evaluate() {
        response.status = Status::Error;
        let code = message
            .split(':')
            .next()
            .filter(|s| s.starts_with("E_"))
            .unwrap_or("E_INPUT");
        response.findings.push(Finding {
            code: code.into(),
            level: "error".into(),
            message,
            delta_refs: vec!["/request".into()],
            causes: vec![],
        });
    }
    response
}

pub fn canonical<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_jcs::to_vec(value).map_err(|e| e.to_string())
}
pub fn digest<T: Serialize>(value: &T) -> Result<String, String> {
    Ok(sha256(&canonical(value)?))
}
pub use crate::verified_identifiers::valid_digest_text as is_digest;
pub use crate::verified_status::{
    Checkout, Delta, Evidence, Finding, IndexFlag, WireStatus as Status, append_finding,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub protocol: String,
    pub request_id: String,
    pub judge_id: String,
    pub status: Status,
    pub findings: Vec<Finding>,
    pub evidence: Vec<Evidence>,
    pub outputs: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub commit: String,
    pub tree: String,
    pub root: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Registries {
    pub base: PathBuf,
    pub candidate: PathBuf,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executable {
    pub path: String,
    pub sha256: String,
    pub version: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Optional full execution selector.  It is omitted from legacy requests so
    /// v1/v2 consumers retain their original wire shape; a selector is carried
    /// in the sealed request and therefore cannot silently become a full run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<crate::units::Scope>,
    #[serde(default)]
    pub observations: Value,
    pub protocol: String,
    pub request_id: String,
    pub judge_id: String,
    pub mode: String,
    pub base: Endpoint,
    pub candidate: Endpoint,
    pub delta: Vec<Delta>,
    pub registries: Registries,
    pub context: Context,
    pub impact: Value,
    pub prior_results: Vec<Response>,
    pub config_path: String,
    pub checkout: Checkout,
    pub runner: Executable,
}
impl Request {
    fn identity(&self) -> Result<String, String> {
        Ok(sha256(&self.canonical_projection(None)?))
    }
    /// Original canonical stdin bytes, without expanding borrowed observations
    /// into another owned JSON tree. This preserves the v1 wire representation.
    pub fn canonical(&self) -> Result<Vec<u8>, String> {
        self.canonical_projection(Some(&self.request_id))
    }
    fn canonical_projection(&self, request_id: Option<&str>) -> Result<Vec<u8>, String> {
        // Keep the large observations borrowed while encoding the typed header.
        // The exhaustive destructuring keeps
        // a new request field from silently escaping the identity contract.
        #[derive(Serialize)]
        struct Projection<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            scope: Option<&'a crate::units::Scope>,
            #[serde(skip_serializing_if = "Option::is_none")]
            request_id: Option<&'a str>,
            protocol: &'a str,
            judge_id: &'a str,
            mode: &'a str,
            base: &'a Endpoint,
            candidate: &'a Endpoint,
            delta: &'a [Delta],
            registries: &'a Registries,
            context: &'a Context,
            impact: &'a Value,
            prior_results: &'a [Response],
            config_path: &'a str,
            checkout: &'a Checkout,
            runner: &'a Executable,
        }
        let Self {
            scope,
            observations,
            protocol,
            request_id: _,
            judge_id,
            mode,
            base,
            candidate,
            delta,
            registries,
            context,
            impact,
            prior_results,
            config_path,
            checkout,
            runner,
        } = self;
        crate::canonical_request::encode(
            &Projection {
                scope: scope.as_ref(),
                request_id,
                protocol,
                judge_id,
                mode,
                base,
                candidate,
                delta,
                registries,
                context,
                impact,
                prior_results,
                config_path,
                checkout,
                runner,
            },
            observations,
        )
    }
    pub fn seal(&mut self) -> Result<(), String> {
        self.request_id = self.identity()?;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.validate_header()?;
        if self.request_id != self.identity()? {
            return Err("E_PROTOCOL: request identity/mode".into());
        }
        Ok(())
    }
    fn validate_header(&self) -> Result<(), String> {
        if !crate::verified_identifiers::valid_request_header(
            self.protocol.as_str(),
            self.mode.as_str(),
            self.judge_id.as_str(),
            PROTOCOL,
            "evaluate",
        ) {
            return Err("E_PROTOCOL: request identity/mode".into());
        }
        Ok(())
    }
    pub fn response(&self, status: Status) -> Response {
        Response {
            protocol: PROTOCOL.into(),
            request_id: self.request_id.clone(),
            judge_id: self.judge_id.clone(),
            status,
            findings: vec![],
            evidence: vec![],
            outputs: BTreeMap::new(),
        }
    }
}
/// An invocation-owned request whose seal and original stdin were produced
/// together. Neither its request nor its bytes can be mutated by the caller.
pub(crate) struct PreparedRequest {
    request: Request,
    stdin: Vec<u8>,
}
impl PreparedRequest {
    pub(crate) fn new(mut request: Request) -> Result<Self, String> {
        request.seal()?;
        let stdin = request.canonical()?;
        Ok(Self { request, stdin })
    }
    pub(crate) fn request(&self) -> &Request {
        &self.request
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub id: String,
    pub executable: String,
    pub version: String,
    pub sha256: Option<String>,
    pub argv: Vec<String>,
    pub selector: String,
    pub after: Vec<String>,
    pub modes: Vec<String>,
}

pub fn validate_response(req: &Request, r: &Response, exit: i32) -> Result<(), String> {
    validate_response_at(
        &req.candidate.root,
        PROTOCOL,
        &req.request_id,
        &req.judge_id,
        r,
        exit,
        Some(&req.delta),
    )
}
/// Shared response contract; the request protocol owns admissible finding references.
pub(crate) fn validate_response_at(
    root: &std::path::Path,
    protocol: &str,
    request_id: &str,
    judge_id: &str,
    r: &Response,
    exit: i32,
    delta: Option<&[Delta]>,
) -> Result<(), String> {
    let header = crate::verified_status::validate_response_fields(
        &r.protocol,
        &r.request_id,
        &r.judge_id,
        &r.status,
        exit,
        protocol,
        request_id,
        judge_id,
        &r.findings,
        delta,
    );
    crate::verified_status::validate_response_evidence(
        header,
        (),
        &r.evidence,
        |(), path| ((), no_symlink_parents(root, path)),
        |(), path: &PathBuf| ((), path.is_file()),
        |(), path: &PathBuf| {
            (
                (),
                fs::read(path).map_err(|error| format!("E_PROTOCOL: evidence: {error}")),
            )
        },
        |(), bytes: &[u8]| ((), sha256(bytes)),
    )
    .1
}
#[derive(Debug)]
pub struct TransportFailure {
    pub message: String,
    pub process: Option<ProcessResult>,
    error: Failure,
}
impl From<String> for TransportFailure {
    fn from(message: String) -> Self {
        let error = Failure::new("E_JUDGE_TRANSPORT", message.clone());
        Self {
            message,
            process: None,
            error,
        }
    }
}
impl From<crate::ProcessFailure> for TransportFailure {
    fn from(source: crate::ProcessFailure) -> Self {
        Self {
            message: source.message.clone(),
            process: None,
            error: Failure::wrap("E_JUDGE_TRANSPORT", source.message.clone(), source),
        }
    }
}
impl std::fmt::Display for TransportFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for TransportFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
/// Versioned runner report extension; never a judge-v1 request/response field.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportDiagnostic {
    pub schema: TransportDiagnosticSchema,
    pub error: ErrorRecord,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransportDiagnosticSchema {
    #[serde(rename = "chrono-runner-transport-failure/v1")]
    V1,
}
impl TransportFailure {
    pub(crate) fn with_process(message: String, process: ProcessResult) -> Self {
        let mut failure = Self::from(message);
        if let Some(source) = &process.failure_cause {
            failure.error = Failure::wrap(
                "E_JUDGE_TRANSPORT",
                failure.message.clone(),
                crate::SharedProcessFailure(source.clone()),
            );
        } else if let Some(record) = process.failure_record.clone() {
            failure.error = Failure::wrap(
                "E_JUDGE_TRANSPORT",
                failure.message.clone(),
                ReceivedError::from(record),
            );
        }
        failure.process = Some(process);
        failure
    }
    fn context(mut self, req: &Request) -> Self {
        self.error = self
            .error
            .at("request_id", req.request_id.clone())
            .at("judge_id", req.judge_id.clone())
            .at("operation", "judge.invoke")
            .at("operation_started", self.process.is_some());
        self
    }
    pub fn diagnostic(&self) -> TransportDiagnostic {
        TransportDiagnostic {
            schema: TransportDiagnosticSchema::V1,
            error: self.error.record(),
        }
    }
}
impl From<&str> for TransportFailure {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}
pub fn invoke(
    req: &Request,
    b: &Binding,
    env: &BTreeMap<String, String>,
    timeout: u64,
    limit: usize,
) -> Result<(Response, ProcessResult), String> {
    invoke_detailed(req, b, env, timeout, limit).map_err(|e| e.message)
}
pub fn invoke_detailed(
    req: &Request,
    b: &Binding,
    env: &BTreeMap<String, String>,
    timeout: u64,
    limit: usize,
) -> Result<(Response, ProcessResult), TransportFailure> {
    (|| {
        req.validate()?;
        invoke_bytes(req, b, env, timeout, limit, &req.canonical()?)
    })()
    .map_err(|failure: TransportFailure| failure.context(req))
}
pub(crate) fn invoke_prepared(
    req: &PreparedRequest,
    b: &Binding,
    env: &BTreeMap<String, String>,
    timeout: u64,
    limit: usize,
) -> Result<(Response, ProcessResult), TransportFailure> {
    (|| {
        req.request.validate_header()?;
        invoke_bytes(&req.request, b, env, timeout, limit, &req.stdin)
    })()
    .map_err(|failure: TransportFailure| failure.context(&req.request))
}
fn invoke_bytes(
    req: &Request,
    b: &Binding,
    env: &BTreeMap<String, String>,
    timeout: u64,
    limit: usize,
    stdin: &[u8],
) -> Result<(Response, ProcessResult), TransportFailure> {
    if b.id != req.judge_id {
        return Err("E_PROTOCOL: binding identity".into());
    }
    let hash = b
        .sha256
        .as_deref()
        .filter(|v| is_digest(v))
        .ok_or("E_PROTOCOL: missing executable digest")?;
    let path = no_symlink_parents_detailed(&req.candidate.root, &b.executable)?;
    let spec = CommandSpec {
        program: path.to_str().ok_or("non UTF-8 executable")?.into(),
        args: b
            .argv
            .iter()
            .map(|arg| match arg.as_str() {
                "{base}" => req.base.commit.clone(),
                "{candidate}" => req.candidate.commit.clone(),
                _ => arg.clone(),
            })
            .collect(),
        env: env.clone(),
        timeout_seconds: timeout,
        output_limit_bytes: limit,
    };
    let p = run_process_observed_detailed(&req.candidate.root, &spec, stdin, hash)?;
    if let Some(message) = p.failure.clone() {
        return Err(TransportFailure::with_process(message, p));
    }
    let result = decode::<Response>(&p.stdout_bytes).and_then(|response| {
        validate_response(req, &response, p.exit_code)?;
        Ok(response)
    });
    match result {
        Ok(response) => Ok((response, p)),
        Err(message) => Err(TransportFailure::with_process(
            format!(
                "E_PROTOCOL: {message}; exit={}; stderr={}",
                p.exit_code, p.stderr
            ),
            p,
        )),
    }
}

//! Execute a selected installed JSON interface at its declared coordinates.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Environment {
    Inherited,
    SearchPath,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Query {
    root: PathBuf,
    environment: Environment,
    program: String,
    operation: String,
    input: Value,
}

pub(crate) fn inherited(
    root: &Path,
    program: &str,
    operation: &str,
    input: &(impl Serialize + Sync),
) -> Result<Vec<u8>, String> {
    invoke(root, Environment::Inherited, program, operation, input)
}

fn invoke(
    root: &Path,
    environment: Environment,
    program: &str,
    operation: &str,
    input: &(impl Serialize + Sync),
) -> Result<Vec<u8>, String> {
    let search = match environment {
        Environment::Inherited => None,
        Environment::SearchPath => Some(std::env::var("PATH").map_err(|e| e.to_string())?),
    };
    invoke_selected(root, program, operation, input, search.as_deref())
}

/// A route checker may have a cleared environment and an explicit directory of host tool aliases.
pub(crate) fn host_tool(
    root: &Path,
    program: &str,
    operation: &str,
    input: &(impl Serialize + Sync),
) -> Result<Vec<u8>, String> {
    if std::env::var_os("PATH").is_some() {
        return inherited(root, program, operation, input);
    }
    let invocation = std::env::args_os()
        .next()
        .ok_or("missing runner invocation")?;
    let directory = Path::new(&invocation)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or("missing selected host tools directory")?;
    let directory = std::path::absolute(directory).map_err(|e| e.to_string())?;
    let search = directory.to_str().ok_or("non-UTF8 host tools directory")?;
    invoke_selected(root, program, operation, input, Some(search))
}

fn invoke_selected(
    root: &Path,
    program: &str,
    operation: &str,
    input: &(impl Serialize + Sync),
    search: Option<&str>,
) -> Result<Vec<u8>, String> {
    let program = crate::resolve_program(root, program, search)?;
    let digest = crate::file_identity(&program)?.0;
    let mut command = Command::new(&program);
    if let Some(search) = search {
        command.current_dir(root).env_clear().env("PATH", search);
    }
    command
        .arg(operation)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process_fds::forward_command(&mut command, &[])?;
    let mut child = command
        .spawn()
        .map_err(|e| format!("start {}: {e}", program.display()))?;
    let stdin = child.stdin.take().unwrap();
    let (output, sent) = std::thread::scope(|scope| {
        let sender = scope.spawn(|| serde_json::to_writer(stdin, input));
        (child.wait_with_output(), sender.join())
    });
    let output = output.map_err(|e| e.to_string())?;
    if !output.status.success() {
        let reason = String::from_utf8(output.stderr).map_err(|e| e.to_string())?;
        return Err(if reason.is_empty() {
            format!("{} {operation} {}", program.display(), output.status)
        } else {
            reason.strip_suffix('\n').unwrap_or(&reason).into()
        });
    }
    sent.map_err(|_| "installed query request writer panicked")?
        .map_err(|e| e.to_string())?;
    if crate::file_identity(&program)?.0 != digest {
        return Err("E_RETIREMENT: registration executable changed".into());
    }
    Ok(output.stdout)
}

pub(crate) fn serve(input: impl Read, mut output: impl Write) -> Result<(), String> {
    let q: Query = serde_json::from_reader(input).map_err(|e| e.to_string())?;
    let bytes = invoke(&q.root, q.environment, &q.program, &q.operation, &q.input)?;
    output
        .write_all(&bytes)
        .and_then(|_| output.flush())
        .map_err(|e| e.to_string())
}

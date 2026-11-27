//! Transport a host-selected command to the runner without encoding its binary streams.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub const COMMAND_HEADER_BYTES: usize = 1_048_576;
pub const COMMAND_INPUT_BYTES: usize = 67_108_864;

pub fn command_header(line: &[u8]) -> (ok: bool)
    ensures ok == (0 < line.len() <= 1_048_576 && line@[line.len() - 1] == 10u8),
{
    !line.is_empty() && line.len() <= COMMAND_HEADER_BYTES && line[line.len() - 1] == 10
}

pub fn command_input(length: usize) -> (ok: bool)
    ensures ok == (length <= 67_108_864),
{
    length <= COMMAND_INPUT_BYTES
}
}

#[cfg(not(verus_keep_ghost))]
use crate::{CommandSpec, run_process, run_process_observed};
#[cfg(not(verus_keep_ghost))]
use serde::Serialize;
#[cfg(not(verus_keep_ghost))]
use std::io::{BufRead, Read, Write};
#[cfg(not(verus_keep_ghost))]
use std::path::Path;

#[cfg(not(verus_keep_ghost))]
#[derive(Serialize)]
struct Lengths {
    exit_code: i32,
    stdout_length: usize,
    stderr_length: usize,
}

#[cfg(not(verus_keep_ghost))]
pub type Streams = (i32, Vec<u8>, Vec<u8>);

#[cfg(not(verus_keep_ghost))]
fn run(
    root: &Path,
    spec: &CommandSpec,
    bytes: &[u8],
    identity: Option<&str>,
) -> Result<Streams, String> {
    let result = match identity {
        Some(digest) => run_process_observed(root, spec, bytes, digest)?,
        None => run_process(root, spec, bytes)?,
    };
    if let Some(error) = result.failure {
        return Err(error);
    }
    Ok((result.exit_code, result.stdout_bytes, result.stderr_bytes))
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn command(root: &Path) -> Result<(), String> {
    serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        |spec, bytes, identity| run(root, spec, bytes, identity),
    )
}

#[cfg(not(verus_keep_ghost))]
fn request(mut input: impl BufRead) -> Result<(CommandSpec, Vec<u8>, Option<String>), String> {
    let mut header = String::new();
    (&mut input)
        .take(COMMAND_HEADER_BYTES as u64 + 1)
        .read_line(&mut header)
        .map_err(|error| error.to_string())?;
    if !command_header(header.as_bytes()) {
        return Err("E_PROCESS: command specification must be a JSON line of at most 1 MiB".into());
    }
    let mut fields: serde_json::Map<String, serde_json::Value> = crate::decode(header.as_bytes())?;
    let identity = fields
        .remove("expected_sha256")
        .map(serde_json::from_value::<String>)
        .transpose()
        .map_err(|e| e.to_string())?;
    let spec: CommandSpec =
        serde_json::from_value(serde_json::Value::Object(fields)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    input
        .take(COMMAND_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if !command_input(bytes.len()) {
        return Err("E_PROCESS: command input exceeds 64 MiB".into());
    }
    Ok((spec, bytes, identity))
}

#[cfg(not(verus_keep_ghost))]
pub fn serve_record(
    input: impl BufRead,
    mut output: impl Write,
    invoke: impl FnOnce(&CommandSpec, &[u8], &str) -> Result<crate::ProcessResult, String>,
) -> Result<(), String> {
    let (spec, bytes, identity) = request(input)?;
    let digest = identity
        .as_deref()
        .ok_or("E_PROCESS: record-command-v1 requires expected_sha256")?;
    let original = invoke(&spec, &bytes, digest)?;
    serde_json::to_writer(&mut output, &original).map_err(|e| e.to_string())?;
    output.flush().map_err(|e| e.to_string())
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn record(root: &Path) -> Result<(), String> {
    serve_record(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        |spec, bytes, digest| run_process_observed(root, spec, bytes, digest),
    )
}

#[cfg(not(verus_keep_ghost))]
pub fn serve(
    input: impl BufRead,
    mut output: impl Write,
    invoke: impl FnOnce(&CommandSpec, &[u8], Option<&str>) -> Result<Streams, String>,
) -> Result<(), String> {
    let (spec, bytes, identity) = request(input)?;
    let (exit_code, stdout, stderr) = invoke(&spec, &bytes, identity.as_deref())?;
    let sizes = crate::verified_limits::CapturedStreams {
        stdout_bytes: &stdout,
        stderr_bytes: &stderr,
    }
    .sizes();
    let lengths = Lengths {
        exit_code,
        stdout_length: sizes.stdout_size,
        stderr_length: sizes.stderr_size,
    };
    serde_json::to_writer(&mut output, &lengths).map_err(|error| error.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|()| output.write_all(&stdout))
        .and_then(|()| output.write_all(&stderr))
        .and_then(|()| output.flush())
        .map_err(|error| error.to_string())
}

//! Transport a host-selected command to the runner without encoding its binary streams.
use crate::{CommandSpec, run_process, run_process_observed};
use serde::Serialize;
use std::io::{BufRead, Read, Write};
use std::path::Path;

#[derive(Serialize)]
struct Lengths {
    exit_code: i32,
    stdout_length: usize,
    stderr_length: usize,
}

pub type Streams = (i32, Vec<u8>, Vec<u8>);

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

pub(crate) fn command(root: &Path) -> Result<(), String> {
    serve(
        std::io::stdin().lock(),
        std::io::stdout().lock(),
        |spec, bytes, identity| run(root, spec, bytes, identity),
    )
}

pub fn serve(
    mut input: impl BufRead,
    mut output: impl Write,
    invoke: impl FnOnce(&CommandSpec, &[u8], Option<&str>) -> Result<Streams, String>,
) -> Result<(), String> {
    let mut header = String::new();
    (&mut input)
        .take(1024 * 1024 + 1)
        .read_line(&mut header)
        .map_err(|error| error.to_string())?;
    if header.len() > 1024 * 1024 || !header.ends_with('\n') {
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
        .take(64 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("E_PROCESS: command input exceeds 64 MiB".into());
    }
    let (exit_code, stdout, stderr) = invoke(&spec, &bytes, identity.as_deref())?;
    let lengths = Lengths {
        exit_code,
        stdout_length: stdout.len(),
        stderr_length: stderr.len(),
    };
    serde_json::to_writer(&mut output, &lengths).map_err(|error| error.to_string())?;
    output
        .write_all(b"\n")
        .and_then(|()| output.write_all(&stdout))
        .and_then(|()| output.write_all(&stderr))
        .and_then(|()| output.flush())
        .map_err(|error| error.to_string())
}

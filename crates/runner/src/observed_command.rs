//! Binary stdout/stderr frames share the original bounded readers and their real acquisition deadline.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// A whole-second command deadline includes its exact endpoint.
pub fn acquisition_within(seconds: u64, nanos: u32, bound_seconds: u64) -> (ok: bool)
    requires nanos < 1_000_000_000,
    ensures ok == (seconds as int * 1_000_000_000 + nanos <= bound_seconds as int * 1_000_000_000),
{
    seconds < bound_seconds || (seconds == bound_seconds && nanos == 0)
}
}

#[cfg(not(verus_keep_ghost))]
use crate::verified_limits::{CapturedStreams, StreamSizes};
#[cfg(not(verus_keep_ghost))]
use crate::{CommandSpec, ProcessForward, ProcessResult, run_process_observed_forwarded_for};
#[cfg(not(verus_keep_ghost))]
use serde::Deserialize;
#[cfg(not(verus_keep_ghost))]
use serde_json::json;
#[cfg(not(verus_keep_ghost))]
use std::{
    io::{self, BufRead, Read, Write},
    path::Path,
    sync::Mutex,
    time::Duration,
};

#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    command: CommandSpec,
    expected_sha256: String,
    timeout: Duration,
}

#[cfg(not(verus_keep_ghost))]
fn frame(output: &mut impl Write, kind: u8, bytes: &[u8]) -> io::Result<()> {
    output.write_all(&[kind])?;
    output.write_all(&(bytes.len() as u64).to_be_bytes())?;
    output.write_all(bytes)?;
    output.flush()
}

#[cfg(not(verus_keep_ghost))]
struct Stream<'a, W> {
    output: &'a Mutex<W>,
    kind: u8,
}
#[cfg(not(verus_keep_ghost))]
impl<W: Write> Write for Stream<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !bytes.is_empty() {
            frame(
                &mut *self
                    .output
                    .lock()
                    .map_err(|_| io::Error::other("stream lock poisoned"))?,
                self.kind,
                bytes,
            )?;
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
pub fn serve(
    mut input: impl BufRead,
    output: impl Write + Send,
    invoke: impl FnOnce(
        &CommandSpec,
        &[u8],
        &str,
        Duration,
        ProcessForward<'_>,
    ) -> Result<ProcessResult, String>,
) -> Result<(), String> {
    use crate::command_stream::{
        COMMAND_HEADER_BYTES, COMMAND_INPUT_BYTES, command_header, command_input,
    };
    let mut line = vec![];
    (&mut input)
        .take(COMMAND_HEADER_BYTES as u64 + 1)
        .read_until(b'\n', &mut line)
        .map_err(|e| e.to_string())?;
    if !command_header(&line) {
        return Err("observed command requires a JSON line of at most 1 MiB".into());
    }
    let request: Request = crate::decode(&line)?;
    if !acquisition_within(
        request.timeout.as_secs(),
        request.timeout.subsec_nanos(),
        request.command.timeout_seconds,
    ) {
        return Err("acquisition timeout exceeds the declared command bound".into());
    }
    let mut bytes = vec![];
    input
        .take(COMMAND_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if !command_input(bytes.len()) {
        return Err("observed command input exceeds 64 MiB".into());
    }
    let output = Mutex::new(output);
    let observed = invoke(
        &request.command,
        &bytes,
        &request.expected_sha256,
        request.timeout,
        ProcessForward {
            stdout: &mut Stream {
                output: &output,
                kind: b'O',
            },
            stderr: &mut Stream {
                output: &output,
                kind: b'E',
            },
        },
    )?;
    let ProcessResult {
        stdout_bytes,
        stderr_bytes,
        ..
    } = &observed;
    let StreamSizes {
        stdout_size,
        stderr_size,
    } = CapturedStreams {
        stdout_bytes,
        stderr_bytes,
    }
    .sizes();
    let terminal = json!({
        "argv":observed.argv,"cwd":observed.cwd,"launcher_pid":observed.launcher_pid,"child_pid":observed.child_pid,
        "executable":observed.executable,"sha256":observed.sha256,"environment_digest":observed.environment_digest,
        "exit_signal":observed.exit_signal,"exit_code":observed.exit_code,"failure":observed.failure,
        "stdout_complete":observed.stdout_complete,"stderr_complete":observed.stderr_complete,
        "stdout_size":stdout_size,"stderr_size":stderr_size,
        "stdout_sha256":observed.stdout_sha256,"stderr_sha256":observed.stderr_sha256
    });
    frame(
        &mut output.into_inner().map_err(|_| "stream lock poisoned")?,
        b'T',
        &serde_json::to_vec(&terminal).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn command(root: &Path) -> Result<(), String> {
    serve(
        io::stdin().lock(),
        io::stdout(),
        |spec, input, digest, timeout, forward| {
            run_process_observed_forwarded_for(root, spec, input, digest, timeout, forward)
        },
    )
}

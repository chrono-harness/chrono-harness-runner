//! The installed diagnostics producer owns policy, formatting, and rolling files.
//! Runner retains its actual output handles and any native errors returned by them.
#[cfg(verus_keep_ghost)]
use super::identifiers;
#[cfg(not(verus_keep_ghost))]
use crate::verified_identifiers as identifiers;
use std::collections::VecDeque;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Only format the native message after its record kind and optional errno match.
pub fn writer_record_matches<F>(
    code: &str, source: Option<(Option<i64>, &str)>, observed: Option<i64>, original: F,
) -> (paired: bool)
    where F: FnOnce() -> String,
    requires code@ == "E_LOG_IO"@ && source.is_some() && source.unwrap().0 == observed
        ==> original.requires(()),
    ensures match source {
        Some((reported, message)) => if code@ == "E_LOG_IO"@ && reported == observed {
            exists|text: String| #[trigger] original.ensures((), text) && paired == (message@ == text@)
        } else { !paired },
        None => !paired,
    },
{
    if !identifiers::equal_text(code, "E_LOG_IO") { return false; }
    let (reported, message) = match source { Some(pair) => pair, None => return false };
    let same_number = match (reported, observed) {
        (Some(left), Some(right)) => left == right,
        (None, None) => true,
        _ => false,
    };
    same_number && identifiers::equal_text(message, original().as_str())
}

/// Consume exactly the matching head, leaving later native writer objects in order.
pub fn take_matching_front<T, F>(writers: &mut VecDeque<T>, matches: F) -> (original: Option<T>)
    where F: FnOnce(&T) -> bool,
    requires forall|v: &T| matches.requires((v,)),
    ensures if old(writers)@.len() == 0 {
        original.is_none() && writers@ == old(writers)@
    } else {
        exists|matched: bool| #[trigger] matches.ensures((&old(writers)@[0],), matched) &&
            if matched {
                original == Some(old(writers)@[0]) &&
                    writers@ == old(writers)@.subrange(1, old(writers)@.len() as int)
            } else { original.is_none() && writers@ == old(writers)@ }
    },
{
    if writers.len() != 0 && matches(&writers[0]) { writers.pop_front() } else { None }
}
}

#[cfg(not(verus_keep_ghost))]
use crate::{
    process_error::{ErrorRecord, Failure, NativeIoError, ReceivedError, RecordedError},
    trace_source::Source,
};
#[cfg(not(verus_keep_ghost))]
use serde::{Deserialize, Serialize};
#[cfg(not(verus_keep_ghost))]
use serde_json::{Value, json};
#[cfg(not(verus_keep_ghost))]
use std::{
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

#[cfg(not(verus_keep_ghost))]
fn command() -> Result<Command, io::Error> {
    let mut child = Command::new("chrono-diagnostics");
    crate::process_fds::forward_command(&mut child, &[]).map_err(io::Error::other)?;
    Ok(child)
}
#[cfg(not(verus_keep_ghost))]
#[derive(Serialize)]
struct TraceMessage {
    event: tracing_tunnel::TracingEvent,
}
#[cfg(not(verus_keep_ghost))]
fn line(writer: &mut impl Write, data: impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, &data)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
#[cfg(not(verus_keep_ghost))]
fn read<T: serde::de::DeserializeOwned>(reader: &mut impl BufRead) -> io::Result<T> {
    let mut data = String::new();
    if reader.read_line(&mut data)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "diagnostics session ended before its reply",
        ));
    }
    serde_json::from_str(&data).map_err(io::Error::other)
}

#[cfg(not(verus_keep_ghost))]
pub fn log_cli_error(message: &str) {
    tracing::error!(event_id = "cli.diagnostic", message = message);
}
#[cfg(not(verus_keep_ghost))]
fn context() -> io::Result<(PathBuf, String)> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let root = match args.windows(2).find(|p| p[0] == "--host-root") {
        Some(pair) => PathBuf::from(&pair[1]),
        None => std::env::current_dir()?,
    };
    Ok((
        root,
        args.first()
            .and_then(|a| a.to_str())
            .unwrap_or("")
            .to_owned(),
    ))
}
#[cfg(not(verus_keep_ghost))]
fn report(result: io::Result<u8>) -> u8 {
    result.unwrap_or_else(|e| {
        eprintln!("chrono-diagnostics: {e}");
        2
    })
}
#[cfg(not(verus_keep_ghost))]
pub fn logged_main(producer: &str, work: impl FnOnce() -> u8) -> u8 {
    report(context().map(|(root, operation)| logged_main_at(&root, producer, &operation, work)))
}
#[cfg(not(verus_keep_ghost))]
pub fn logged_main_at(
    root: &Path,
    producer: &str,
    operation: &str,
    work: impl FnOnce() -> u8,
) -> u8 {
    match std::fs::symlink_metadata(root.join(".chrono-harness/logging.json")) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => work(),
        _ => report(cli(root, producer, operation, None, work)),
    }
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn invalid_argument(producer: &str, index: usize, bytes: &[u8]) -> u8 {
    report(context().and_then(|(root, op)| {
        cli(
            &root,
            producer,
            &op,
            Some(json!({"invalid_argument":{"argv_index":index,"bytes":bytes}})),
            || 2,
        )
    }))
}

#[cfg(not(verus_keep_ghost))]
fn cli(
    root: &Path,
    producer: &str,
    operation: &str,
    event: Option<Value>,
    work: impl FnOnce() -> u8,
) -> io::Result<u8> {
    let mut child = command()?
        .arg("cli-events-v1")
        .arg(root)
        .arg(producer)
        .arg(operation)
        .arg(std::process::id().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut reply = BufReader::new(child.stdout.take().unwrap());
    let mut writer = BufWriter::new(child.stdin.take().unwrap());
    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> io::Result<u8> {
        let ready: Value = read(&mut reply)?;
        if ready != json!({"ready":true}) {
            return Err(io::Error::other("invalid diagnostics ready reply"));
        }
        if let Some(event) = event {
            line(&mut writer, event)?;
        }
        let state = Arc::new(Mutex::new((writer, None)));
        let to_session = state.clone();
        let subscriber = Source::new(move |event| {
            let mut s = to_session.lock().unwrap();
            if s.1.is_none() {
                s.1 = line(&mut s.0, TraceMessage { event }).err();
            }
        });
        let code = tracing::subscriber::with_default(subscriber, work);
        let mut s = state.lock().unwrap();
        if let Some(error) = s.1.take() {
            return Err(error);
        }
        line(&mut s.0, json!({"exit_code":code}))?;
        Ok(code)
    }));
    let status = child.wait()?;
    match run {
        Ok(Ok(code)) if status.code() == Some(i32::from(code)) => Ok(code),
        // The diagnostics owner has already emitted its original diagnostic to stderr.
        Ok(_) if status.code() == Some(2) => Ok(2),
        Ok(Err(error)) => Err(error),
        Ok(Ok(code)) => Err(io::Error::other(format!(
            "diagnostics exited {status} after business exit {code}"
        ))),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
#[cfg(not(verus_keep_ghost))]
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Ready,
    Accepted,
    Write(String),
    Flush,
    Finished(Vec<ErrorRecord>),
}
#[cfg(not(verus_keep_ghost))]
struct Trace<I, O, W> {
    input: I,
    output: O,
    sink: W,
    originals: Arc<Mutex<VecDeque<io::Error>>>,
}
#[cfg(not(verus_keep_ghost))]
impl<I: Write, O: BufRead, W: Write> Trace<I, O, W> {
    fn reply(&mut self) -> io::Result<Reply> {
        loop {
            let result = match read(&mut self.output)? {
                Reply::Write(bytes) => self.sink.write_all(bytes.as_bytes()),
                Reply::Flush => self.sink.flush(),
                reply => return Ok(reply),
            };
            match result {
                Ok(()) => line(&mut self.input, "ok")?,
                Err(e) => {
                    let ack = json!({"error":{"native_code":e.raw_os_error(),"kind":format!("{:?}",e.kind()),"custom_source":e.get_ref().is_some(),"message":e.to_string()}});
                    self.originals.lock().unwrap().push_back(e);
                    line(&mut self.input, ack)?;
                }
            }
        }
    }
    fn finish_errors(&mut self, records: Vec<ErrorRecord>) -> Vec<ReceivedError> {
        let mut originals = self.originals.lock().unwrap();
        records
            .into_iter()
            .map(|record| {
                let original = take_matching_front(&mut originals, |error| {
                    let source = record
                        .error
                        .source
                        .as_deref()
                        .map(|source| (source.native_code, source.message.as_str()));
                    writer_record_matches(
                        &record.error.code,
                        source,
                        error.raw_os_error().map(i64::from),
                        || error.to_string(),
                    )
                });
                match original {
                    Some(error) => ReceivedError::with_native(record, error),
                    None => ReceivedError::from(record),
                }
            })
            .collect()
    }
}
#[cfg(not(verus_keep_ghost))]
fn protocol(error: io::Error) -> ReceivedError {
    let native = NativeIoError::new(error);
    let mut record = Failure::new("E_LOG_PROTOCOL", "diagnostic publication failed").record();
    record.error.source = Some(Box::new(native.error_node()));
    ReceivedError::with_native(record, native.into_inner())
}
#[cfg(not(verus_keep_ghost))]
pub(crate) fn publish_failure(
    id: &str,
    failure: &Failure,
    sink: impl Write + Send + 'static,
) -> Vec<ReceivedError> {
    let originals = Arc::new(Mutex::new(VecDeque::new()));
    let result = (|| -> io::Result<Vec<ReceivedError>> {
        let mut child = command()?
            .args(["trace-events-v1", "--sink-only", "chrono-harness"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let stream = Trace {
            input: BufWriter::new(child.stdin.take().unwrap()),
            output: BufReader::new(child.stdout.take().unwrap()),
            sink,
            originals: originals.clone(),
        };
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut stream = stream;
            match stream.reply()? {
                Reply::Ready => (),
                Reply::Finished(records) => return Ok(stream.finish_errors(records)),
                _ => return Err(io::Error::other("invalid diagnostic trace ready")),
            }
            let state = Arc::new(Mutex::new((stream, Ok(()))));
            let transport = state.clone();
            let subscriber = Source::new(move |event| {
                let mut lock = transport.lock().unwrap();
                let (stream, sent) = &mut *lock;
                if sent.is_ok() {
                    *sent =
                        line(&mut stream.input, TraceMessage { event }).and_then(|()| match stream
                            .reply(
                        )? {
                            Reply::Accepted => Ok(()),
                            _ => Err(io::Error::other("missing diagnostic trace acknowledgment")),
                        });
                }
            });
            let record = serde_json::to_string(&failure.record())?;
            tracing::subscriber::with_default(
                subscriber,
                || tracing::error!(target:"chrono",event_id=id,message=%failure,chrono_error=record.as_str()),
            );
            let mut lock = state.lock().unwrap();
            let (stream, sent) = &mut *lock;
            std::mem::replace(sent, Ok(()))?;
            line(&mut stream.input, "finish")?;
            match stream.reply()? {
                Reply::Finished(records) => Ok(stream.finish_errors(records)),
                _ => Err(io::Error::other("missing diagnostic trace terminal")),
            }
        }));
        let status = child.wait()?;
        let mut errors = run.unwrap_or_else(|panic| std::panic::resume_unwind(panic))?;

        if status.code() != Some(if errors.is_empty() { 0 } else { 2 }) {
            errors.push(protocol(io::Error::other(format!(
                "diagnostics exited {status}"
            ))));
        }
        Ok(errors)
    })();
    let mut errors = result.unwrap_or_else(|e| vec![protocol(e)]);
    for observed in originals.lock().unwrap().drain(..) {
        errors.push(protocol(observed));
    }
    errors
}
#[cfg(not(verus_keep_ghost))]
#[derive(Debug, Deserialize, Serialize)]
pub struct ExpiredLogs {
    pub removed: usize,
    pub legacy: usize,
    pub busy: bool,
    pub errors: Vec<String>,
}
#[cfg(not(verus_keep_ghost))]
pub fn maintain_runtime_logs(root: &Path) -> Result<Option<ExpiredLogs>, Failure> {
    if std::fs::symlink_metadata(root.join(".chrono-harness/logging.json"))
        .is_err_and(|e| e.kind() == io::ErrorKind::NotFound)
    {
        return Ok(None);
    }
    let run = command()
        .and_then(|mut child| child.arg("expire-logs-v1").arg(root).output())
        .map_err(|e| Failure::wrap("E_LOG_IO", "log maintenance", NativeIoError::new(e)))?;
    if !matches!(run.status.code(), Some(0 | 1)) {
        return Err(Failure::new(
            "E_LOG_PROTOCOL",
            String::from_utf8_lossy(&run.stderr),
        ));
    }
    serde_json::from_slice(&run.stdout).map_err(|e| Failure::new("E_LOG_PROTOCOL", e.to_string()))
}

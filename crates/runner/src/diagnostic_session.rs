//! The installed diagnostics producer owns policy, formatting, and rolling files.
//! Runner retains its actual output handles and any native errors returned by them.
use crate::{
    process_error::{ErrorRecord, Failure, NativeIoError, ReceivedError},
    trace_source::Source,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{self, BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
};

fn command() -> Result<Command, io::Error> {
    let mut child = Command::new("chrono-diagnostics");
    crate::process_fds::forward_command(&mut child, &[]).map_err(io::Error::other)?;
    Ok(child)
}
#[derive(Serialize)]
struct TraceMessage {
    event: tracing_tunnel::TracingEvent,
}
fn line(writer: &mut impl Write, data: impl Serialize) -> io::Result<()> {
    serde_json::to_writer(&mut *writer, &data)?;
    writer.write_all(b"\n")?;
    writer.flush()
}
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

pub fn log_cli_error(message: &str) {
    tracing::error!(event_id = "cli.diagnostic", message = message);
}
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
fn report(result: io::Result<u8>) -> u8 {
    result.unwrap_or_else(|e| {
        eprintln!("chrono-diagnostics: {e}");
        2
    })
}
pub fn logged_main(producer: &str, work: impl FnOnce() -> u8) -> u8 {
    report(context().map(|(root, operation)| logged_main_at(&root, producer, &operation, work)))
}
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
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Reply {
    Ready,
    Accepted,
    Write(String),
    Flush,
    Finished(Vec<ErrorRecord>),
}
struct Trace<I, O, W> {
    input: I,
    output: O,
    sink: W,
    originals: VecDeque<io::Error>,
}
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
                    self.originals.push_back(e);
                    line(&mut self.input, ack)?;
                }
            }
        }
    }
    fn finish_errors(&mut self, records: Vec<ErrorRecord>) -> Vec<ReceivedError> {
        let mut errors = Vec::new();
        for record in records {
            let paired = record
                .error
                .source
                .as_deref()
                .zip(self.originals.front())
                .is_some_and(|(source, original)| {
                    record.error.code == "E_LOG_IO"
                        && source.native_code == original.raw_os_error().map(i64::from)
                        && source.message == original.to_string()
                });
            errors.push(if paired {
                ReceivedError::with_writer(record, self.originals.pop_front().unwrap())
            } else {
                ReceivedError::from(record)
            });
        }
        for cause in self.originals.drain(..) {
            let record = Failure::new(
                "E_LOG_PROTOCOL",
                "diagnostics omitted an observed output error",
            )
            .record();
            errors.push(ReceivedError::with_writer(record, cause));
        }
        errors
    }
}
fn protocol(e: io::Error) -> ReceivedError {
    ReceivedError::from(
        Failure::wrap(
            "E_LOG_PROTOCOL",
            "diagnostic publication failed",
            NativeIoError::new(e),
        )
        .record(),
    )
}
pub(crate) fn publish_failure(
    id: &str,
    failure: &Failure,
    sink: impl Write + Send + 'static,
) -> Vec<ReceivedError> {
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
            originals: VecDeque::new(),
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
    result.unwrap_or_else(|e| vec![protocol(e)])
}
#[derive(Debug, Deserialize, Serialize)]
pub struct ExpiredLogs {
    pub removed: usize,
    pub legacy: usize,
    pub busy: bool,
    pub errors: Vec<String>,
}
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

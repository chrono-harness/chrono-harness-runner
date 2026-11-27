//! The CLI boundary owns output policy; diagnostics owns records and logging.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Both channels have been attempted. Keep stdout first when both failed.
pub fn channel_failures<E, F>(stdout: Option<E>, stderr: Option<E>, join: F) -> (result: Result<(), E>)
    where F: FnOnce(E, E) -> E,
    requires stdout.is_some() && stderr.is_some() ==> join.requires((stdout.unwrap(), stderr.unwrap())),
    ensures match (stdout, stderr) {
        (Some(first), Some(second)) => exists|both: E| #[trigger] join.ensures((first, second), both)
            && result == Err::<(), E>(both),
        (Some(error), None) | (None, Some(error)) => result == Err::<(), E>(error),
        (None, None) => result.is_ok(),
    },
{
    match (stdout, stderr) {
        (Some(first), Some(second)) => Err(join(first, second)),
        (Some(error), None) | (None, Some(error)) => Err(error),
        (None, None) => Ok(()),
    }
}

pub fn needs_separator(last: Option<u8>) -> (needed: bool)
    ensures needed == (last.is_some() && last.unwrap() != 10),
{
    match last { Some(byte) => byte != 10u8, None => false }
}
}

#[cfg(not(verus_keep_ghost))]
use crate::CliOutput;
#[cfg(not(verus_keep_ghost))]
use crate::process_error::{Failure, NativeIoError};
#[cfg(not(verus_keep_ghost))]
use std::io::{self, Write};

#[cfg(not(verus_keep_ghost))]
/// Write through the supplied output handle without duplicating its descriptor.
/// Unlike Rust's standard output writers, native write failures (including EBADF)
/// reach the publication owner unchanged. This adapter has no userspace buffer.
#[cfg(unix)]
pub struct NativeOutput<W>(W);

#[cfg(not(verus_keep_ghost))]
#[cfg(unix)]
impl<W: std::os::fd::AsFd> NativeOutput<W> {
    pub fn new(writer: W) -> Self {
        Self(writer)
    }
}

#[cfg(not(verus_keep_ghost))]
#[cfg(unix)]
impl<W: std::os::fd::AsFd> Write for NativeOutput<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        use std::os::fd::AsRawFd;
        let count = bytes.len().min(isize::MAX as usize);
        // The handle keeps the borrowed descriptor live; the slice remains valid
        // for this synchronous call. No close or ownership transfer occurs here.
        let written =
            unsafe { libc::write(self.0.as_fd().as_raw_fd(), bytes.as_ptr().cast(), count) };
        if written < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(written as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        // Native stdout/stderr have no buffer to flush. This is not fsync.
        Ok(())
    }
}

#[cfg(not(verus_keep_ghost))]
/// The actual CLI uses the same native adapter for both output channels.
pub fn publish_native_output(output: &CliOutput) -> Result<u8, Failure> {
    #[cfg(unix)]
    {
        publish_output(
            output,
            &mut NativeOutput::new(io::stdout().lock()),
            NativeOutput::new(io::stderr()),
        )
    }
    #[cfg(not(unix))]
    publish_output(output, &mut io::stdout().lock(), io::stderr())
}

#[cfg(not(verus_keep_ghost))]
struct ObservedWriter<W> {
    writer: W,
    published: usize,
    last_byte: Option<u8>,
}

#[cfg(not(verus_keep_ghost))]
impl<W> ObservedWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            writer,
            published: 0,
            last_byte: None,
        }
    }
}

#[cfg(not(verus_keep_ghost))]
impl<W: Write> Write for ObservedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.writer.write(bytes)?;
        self.published += written;
        if written > 0 {
            self.last_byte = Some(bytes[written - 1]);
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(not(verus_keep_ghost))]
pub(crate) fn write_output(
    writer: &mut impl Write,
    bytes: &[u8],
) -> Result<(), (io::Error, &'static str)> {
    crate::verified_cli::publish_bytes(
        writer,
        |writer| {
            let result = writer.write_all(bytes);
            (writer, result)
        },
        |writer| {
            let result = writer.flush();
            (writer, result)
        },
    )
    .1
}

#[cfg(not(verus_keep_ghost))]
fn publish_channel<W: Write>(
    writer: &mut ObservedWriter<W>,
    stream: &'static str,
    bytes: &str,
    output: &CliOutput,
) -> Result<(), Failure> {
    let Err((source, phase)) = write_output(writer, bytes.as_bytes()) else {
        return Ok(());
    };
    Err(Failure::wrap(
        "E_OUTPUT_PUBLISH",
        "runner output publication failed",
        NativeIoError::new(source),
    )
    .at("stream", stream)
    .at("phase", phase)
    .at("operation_exit_code", output.exit_code)
    .at("operation_succeeded", output.exit_code == 0)
    .at("output_bytes", bytes.len() as u64)
    .at("published_bytes", writer.published as u64))
}

#[cfg(not(verus_keep_ghost))]
fn attempt_output<WOut: Write, WErr: Write>(
    output: &CliOutput,
    stdout: &mut ObservedWriter<WOut>,
    stderr: &mut ObservedWriter<WErr>,
) -> Result<(), Failure> {
    let stdout_failure = publish_channel(stdout, "stdout", &output.stdout, output).err();
    let stderr_failure = publish_channel(stderr, "stderr", &output.stderr, output).err();
    channel_failures(stdout_failure, stderr_failure, |stdout, stderr| {
        stdout.with_related("secondary_output_publication", stderr)
    })
}

#[cfg(not(verus_keep_ghost))]
/// Attempt both existing channels, returning original typed IO failures.
pub fn publish_output_to<WOut: Write, WErr: Write>(
    output: &CliOutput,
    stdout: &mut WOut,
    stderr: &mut WErr,
) -> Result<(), Failure> {
    attempt_output(
        output,
        &mut ObservedWriter::new(stdout),
        &mut ObservedWriter::new(stderr),
    )
}

#[cfg(not(verus_keep_ghost))]
struct BoundaryWriter<W> {
    writer: W,
    needs_newline: bool,
}
#[cfg(not(verus_keep_ghost))]
impl<W: Write> Write for BoundaryWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.needs_newline {
            self.writer.write_all(b"\n")?;
            self.needs_newline = false;
        }
        self.writer.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(not(verus_keep_ghost))]
/// Publish CLI output and one failure event on stderr. On unavailable stderr,
/// return all causes without claiming that the diagnostic was retained durably.
pub fn publish_output<WOut: Write, WErr: Write + Send + 'static>(
    output: &CliOutput,
    stdout: &mut WOut,
    stderr: WErr,
) -> Result<u8, Failure> {
    let mut stdout = ObservedWriter::new(stdout);
    let mut stderr = ObservedWriter::new(stderr);
    let Err(mut error) = attempt_output(output, &mut stdout, &mut stderr) else {
        return Ok(output.exit_code);
    };
    let writer = BoundaryWriter {
        needs_newline: needs_separator(stderr.last_byte),
        writer: stderr.writer,
    };
    for failure in crate::diagnostic_session::publish_failure(
        "runner.output_publication_failed",
        &error,
        writer,
    ) {
        error = error.with_related("diagnostic_publication", failure);
    }
    Err(error)
}

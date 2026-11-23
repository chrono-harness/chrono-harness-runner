//! Read the runner's public tracing events in an isolated test process.
use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
};

struct CheckedWriter<W>(Arc<Mutex<W>>);

impl<W: Write> Write for CheckedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap()
            .write_all(bytes)
            .expect("write original trace");
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .unwrap()
            .flush()
            .expect("flush original trace");
        Ok(())
    }
}

pub fn capture<W: Write + Send + 'static, R>(output: W, run: impl FnOnce() -> R) -> R {
    let writer = Arc::new(Mutex::new(output));
    let events = writer.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .without_time()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(move || CheckedWriter(events.clone()))
        .finish();
    let result = tracing::subscriber::with_default(subscriber, run);
    writer
        .lock()
        .unwrap()
        .flush()
        .expect("flush original trace");
    result
}

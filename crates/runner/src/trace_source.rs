//! A new diagnostics process needs the metadata of each call site it actually receives.
//! Repeated local runs can use cached metadata without another register_callsite callback.
use std::{collections::HashSet, sync::Mutex};
use tracing::{
    Event, Metadata, Subscriber,
    span::{Attributes, Id, Record},
    subscriber::Interest,
};
use tracing_tunnel::{TracingEvent, TracingEventSender};

pub(super) struct Source<F> {
    sender: TracingEventSender<F>,
    metadata: Mutex<HashSet<usize>>,
}

impl<F: Fn(TracingEvent) + 'static> Source<F> {
    pub(super) fn new(send: F) -> Self {
        Self {
            sender: TracingEventSender::new(send),
            metadata: Mutex::new(HashSet::new()),
        }
    }

    fn describe(&self, metadata: &'static Metadata<'static>) {
        // Keep metadata and its first use ordered even for concurrent producers.
        let mut seen = self.metadata.lock().unwrap();
        if seen.insert(metadata as *const _ as usize) {
            self.sender.register_callsite(metadata);
        }
    }
}

impl<F: Fn(TracingEvent) + 'static> Subscriber for Source<F> {
    fn register_callsite(&self, _: &'static Metadata<'static>) -> Interest {
        Interest::always()
    }
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, attrs: &Attributes<'_>) -> Id {
        self.describe(attrs.metadata());
        self.sender.new_span(attrs)
    }
    fn record(&self, span: &Id, values: &Record<'_>) {
        self.sender.record(span, values);
    }
    fn record_follows_from(&self, span: &Id, follows: &Id) {
        self.sender.record_follows_from(span, follows);
    }
    fn event(&self, event: &Event<'_>) {
        self.describe(event.metadata());
        self.sender.event(event);
    }
    fn enter(&self, span: &Id) {
        self.sender.enter(span);
    }
    fn exit(&self, span: &Id) {
        self.sender.exit(span);
    }
    fn clone_span(&self, span: &Id) -> Id {
        self.sender.clone_span(span)
    }
    fn try_close(&self, span: Id) -> bool {
        self.sender.try_close(span)
    }
}

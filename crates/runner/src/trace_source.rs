//! A new diagnostics process needs the metadata of each call site it actually receives.
//! Repeated local runs can use cached metadata without another register_callsite callback.
use std::collections::HashSet;
#[cfg(not(verus_keep_ghost))]
use std::sync::Mutex;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
/// Each subscriber owns its set; its caller holds the lock during this operation.
pub fn publish_new_callsite<T, F>(seen: &mut HashSet<usize>, site: usize, state: T, publish: F)
    -> (out: T)
    where F: FnOnce(T) -> T,
    requires !old(seen)@.contains(site) ==> publish.requires((state,)),
    ensures
        seen@ == old(seen)@.insert(site),
        if old(seen)@.contains(site) { out == state } else { publish.ensures((state,), out) },
{
    broadcast use {
        vstd::std_specs::hash::axiom_usize_obeys_hash_table_key_model,
        vstd::std_specs::hash::axiom_random_state_builds_valid_hashers,
    };
    if seen.insert(site) {
        publish(state)
    } else {
        state
    }
}
}

#[cfg(not(verus_keep_ghost))]
use tracing::{
    Event, Metadata, Subscriber,
    span::{Attributes, Id, Record},
    subscriber::Interest,
};
#[cfg(not(verus_keep_ghost))]
use tracing_tunnel::{TracingEvent, TracingEventSender};

#[cfg(not(verus_keep_ghost))]
pub(super) struct Source<F> {
    sender: TracingEventSender<F>,
    metadata: Mutex<HashSet<usize>>,
}

#[cfg(not(verus_keep_ghost))]
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
        publish_new_callsite(&mut seen, metadata as *const _ as usize, (), |()| {
            self.sender.register_callsite(metadata);
        });
    }
}

#[cfg(not(verus_keep_ghost))]
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

//! Direct Child status ownership, separate from advisory kernel exit events.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub enum ProbeAction<S> {
    Reaped(S),
    Join,
    Pending,
}

/// An actual status or a native error takes precedence over an advisory exit event.
pub fn probe_action<S, E>(status: Result<Option<S>, E>, observed: bool, expired: bool)
    -> (action: Result<ProbeAction<S>, E>)
    ensures action == match status {
        Err(e) => Err(e),
        Ok(Some(s)) => Ok(ProbeAction::Reaped(s)),
        Ok(None) => if observed && !expired { Ok(ProbeAction::Join) } else { Ok(ProbeAction::Pending) },
    },
{
    match status {
        Err(error) => Err(error),
        Ok(Some(status)) => Ok(ProbeAction::Reaped(status)),
        Ok(None) if observed && !expired => Ok(ProbeAction::Join),
        Ok(None) => Ok(ProbeAction::Pending),
    }
}
}

#[cfg(not(verus_keep_ghost))]
use std::{
    io,
    ops::{Deref, DerefMut},
    process::{Child, ExitStatus},
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

#[cfg(not(verus_keep_ghost))]
#[cfg(feature = "retention-test")]
static STATUS_PROBES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(not(verus_keep_ghost))]
/// Read actual monitor probes in the isolated completion-wait fixture.
#[cfg(feature = "retention-test")]
pub fn direct_child_status_probe_count() -> usize {
    STATUS_PROBES.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(not(verus_keep_ghost))]
struct ReapOnDrop {
    child: Child,
    joined: bool,
}
#[cfg(not(verus_keep_ghost))]
impl ReapOnDrop {
    fn wait(&mut self) -> io::Result<ExitStatus> {
        let status = self.child.wait()?;
        self.joined = true;
        Ok(status)
    }
}
#[cfg(not(verus_keep_ghost))]
impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        if !self.joined {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(not(verus_keep_ghost))]
struct WaitState {
    child: Option<ReapOnDrop>,
    result: Option<io::Result<ExitStatus>>,
}
#[cfg(not(verus_keep_ghost))]
struct Waiter {
    state: Arc<Mutex<WaitState>>,
    thread: JoinHandle<()>,
}

#[cfg(not(verus_keep_ghost))]
pub(crate) struct DirectChild {
    pid: u32,
    child: Option<ReapOnDrop>,
    waiter: Option<Waiter>,
    status: Option<ExitStatus>,
}

#[cfg(not(verus_keep_ghost))]
impl DirectChild {
    pub(crate) fn new(child: Child) -> Self {
        Self {
            pid: child.id(),
            child: Some(ReapOnDrop {
                child,
                joined: false,
            }),
            waiter: None,
            status: None,
        }
    }

    pub(crate) fn id(&self) -> u32 {
        self.pid
    }

    pub(crate) fn probe(
        &mut self,
        observed: bool,
        expired: bool,
        wake: impl FnOnce() + Send + 'static,
    ) -> io::Result<Option<ExitStatus>> {
        #[cfg(feature = "retention-test")]
        STATUS_PROBES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.probe_with(observed, expired, wake, Child::try_wait)
    }

    // The dedicated fixture controls only the status-probe ordering; its exit
    // observation and the final Child status still come from the real kernel.
    pub(crate) fn probe_with(
        &mut self,
        observed: bool,
        expired: bool,
        wake: impl FnOnce() + Send + 'static,
        probe: impl FnOnce(&mut Child) -> io::Result<Option<ExitStatus>>,
    ) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        if let Some(waiter) = &self.waiter {
            let result = waiter
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .result
                .take();
            return match result {
                Some(result) => {
                    self.join_waiter()?;
                    self.record(result).map(Some)
                }
                None => Ok(None),
            };
        }
        let child = self.child.as_mut().expect("direct Child owner");
        match probe_action(probe(&mut child.child), observed, expired)? {
            ProbeAction::Reaped(status) => {
                child.joined = true;
                self.status = Some(status);
                Ok(Some(status))
            }
            ProbeAction::Join => {
                // NOTE_EXIT may precede waitability. Keep deadline and cancellation on
                // the monitor while the native blocking waiter owns the child.
                self.start_join(wake)?;
                Ok(None)
            }
            ProbeAction::Pending => Ok(None),
        }
    }

    fn start_join(&mut self, wake: impl FnOnce() + Send + 'static) -> io::Result<()> {
        let builder = std::thread::Builder::new().name("chrono-child-join".into());
        let state = Arc::new(Mutex::new(WaitState {
            child: None,
            result: None,
        }));
        state.lock().unwrap_or_else(|e| e.into_inner()).child = self.child.take();
        let joining = state.clone();
        let thread = match builder.spawn(move || {
            let mut child = joining
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .child
                .take()
                .expect("transferred Child");
            let result = child.wait();
            drop(child);
            // Publish the actual wait result before notifying. A final hint
            // cannot be consumed while that result is still unpublished.
            joining.lock().unwrap_or_else(|e| e.into_inner()).result = Some(result);
            wake();
        }) {
            Ok(thread) => thread,
            Err(error) => {
                self.child = state.lock().unwrap_or_else(|e| e.into_inner()).child.take();
                return Err(error);
            }
        };
        self.waiter = Some(Waiter { state, thread });
        Ok(())
    }

    fn join_waiter(&mut self) -> io::Result<()> {
        if let Some(waiter) = self.waiter.take() {
            waiter
                .thread
                .join()
                .map_err(|_| io::Error::other("Child waiter panicked"))?;
        }
        Ok(())
    }

    fn record(&mut self, result: io::Result<ExitStatus>) -> io::Result<ExitStatus> {
        let status = result?;
        self.status = Some(status);
        Ok(status)
    }

    pub(crate) fn wait(&mut self) -> io::Result<ExitStatus> {
        if let Some(status) = self.status {
            return Ok(status);
        }
        if let Some(waiter) = self.waiter.take() {
            waiter
                .thread
                .join()
                .map_err(|_| io::Error::other("Child waiter panicked"))?;
            let result = waiter
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .result
                .take()
                .ok_or_else(|| io::Error::other("Child waiter returned no status"))?;
            return self.record(result);
        }
        let result = self
            .child
            .as_mut()
            .ok_or_else(|| io::Error::other("Child status unavailable"))?
            .wait();
        self.record(result)
    }

    pub(crate) fn kill(&mut self) -> io::Result<()> {
        if self.status.is_some() {
            return Ok(());
        }
        if let Some(child) = &mut self.child {
            return child.child.kill();
        }
        // OwnedProcess terminates the registered group before joining this
        // waiter, including on unwind. The transferred Child may already have
        // been reaped; do not add a signal using only its numeric PID here.
        Ok(())
    }
}

// Pipe extraction happens before any blocking join starts.
#[cfg(not(verus_keep_ghost))]
impl Deref for DirectChild {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child.as_ref().expect("Child pipes before join").child
    }
}
#[cfg(not(verus_keep_ghost))]
impl DerefMut for DirectChild {
    fn deref_mut(&mut self) -> &mut Child {
        &mut self.child.as_mut().expect("Child pipes before join").child
    }
}
#[cfg(not(verus_keep_ghost))]
impl Drop for DirectChild {
    fn drop(&mut self) {
        if self.status.is_none() {
            let _ = self.kill();
            let _ = self.wait();
        }
    }
}

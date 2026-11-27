//! Pass at most one bounded batch over the session's anonymous Unix input socket.
//! The receiver shares the very same open file descriptions, including their locks.
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub const DESCRIPTOR_BATCH: usize = 32;

pub enum BatchError<E> { Send(E), MarkerLength }

/// Nonempty SCM_RIGHTS sends carry one marker byte. Preserve native errors.
pub fn send_descriptor_batch<E, F>(count: usize, send: F) -> (out: Result<usize, BatchError<E>>)
    where F: FnOnce() -> Result<usize, E>,
    requires 0 < count ==> send.requires(()),
    ensures if count == 0 { out == Ok::<usize, BatchError<E>>(0usize) } else {
        exists|original: Result<usize, E>| #[trigger] send.ensures((), original)
            && out == match original {
                Ok(1usize) => Ok::<usize, BatchError<E>>(count),
                Ok(_) => Err(BatchError::MarkerLength),
                Err(e) => Err(BatchError::Send(e)),
            }
    },
{
    if count == 0 { return Ok(0); }
    match send() {
        Ok(1) => Ok(count),
        Ok(_) => Err(BatchError::MarkerLength),
        Err(error) => Err(BatchError::Send(error)),
    }
}
}

#[cfg(not(verus_keep_ghost))]
use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags, sendmsg};
#[cfg(not(verus_keep_ghost))]
use std::{io::IoSlice, mem::MaybeUninit, os::fd::AsFd};

#[cfg(not(verus_keep_ghost))]
pub(super) fn send(offset: usize) -> Result<usize, String> {
    crate::process_fds::with_scoped_descriptors(offset, DESCRIPTOR_BATCH, |fds| {
        send_descriptor_batch(fds.len(), || {
            let mut buffer =
                [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(DESCRIPTOR_BATCH))];
            let mut messages = SendAncillaryBuffer::new(&mut buffer);
            if !messages.push(SendAncillaryMessage::ScmRights(&fds)) {
                return Err("retained descriptors exceed ancillary capacity".to_string());
            }
            sendmsg(
                std::io::stdin().as_fd(),
                &[IoSlice::new(&[1])],
                &mut messages,
                SendFlags::empty(),
            )
            .map_err(|error| error.to_string())
        })
        .map_err(|error| match error {
            BatchError::Send(source) => source,
            BatchError::MarkerLength => "retained descriptor transfer was incomplete".to_string(),
        })
    })
}

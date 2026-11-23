//! Pass at most one bounded batch over the session's anonymous Unix input socket.
//! The receiver shares the very same open file descriptions, including their locks.
use rustix::net::{SendAncillaryBuffer, SendAncillaryMessage, SendFlags, sendmsg};
use std::{io::IoSlice, mem::MaybeUninit, os::fd::AsFd};

pub(super) fn send(offset: usize) -> Result<usize, String> {
    const BATCH: usize = 32;
    crate::process_fds::with_scoped_descriptors(offset, BATCH, |fds| {
        if !fds.is_empty() {
            let mut buffer = [MaybeUninit::uninit(); rustix::cmsg_space!(ScmRights(BATCH))];
            let mut messages = SendAncillaryBuffer::new(&mut buffer);
            if !messages.push(SendAncillaryMessage::ScmRights(&fds)) {
                return Err("retained descriptors exceed ancillary capacity".into());
            }
            let n = sendmsg(
                std::io::stdin().as_fd(),
                &[IoSlice::new(&[1])],
                &mut messages,
                SendFlags::empty(),
            )
            .map_err(|e| e.to_string())?;
            if n != 1 {
                return Err("retained descriptor transfer was incomplete".into());
            }
        }
        Ok(fds.len())
    })
}

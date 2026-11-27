//! 执行器、登记和路由共用的资源上限判断。
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg(verus_keep_ghost)]
#[path = "command_stream.rs"]
mod command_stream;
#[cfg(not(verus_keep_ghost))]
pub use crate::command_stream::{command_header, command_input};
#[cfg(verus_keep_ghost)]
pub use command_stream::{command_header, command_input};

#[cfg(verus_keep_ghost)]
#[path = "observed_command.rs"]
mod observed_command;
#[cfg(not(verus_keep_ghost))]
pub use crate::observed_command::acquisition_within;
#[cfg(verus_keep_ghost)]
pub use observed_command::acquisition_within;

#[cfg(verus_keep_ghost)]
#[path = "process_child.rs"]
mod process_child;
#[cfg(not(verus_keep_ghost))]
pub use crate::process_child::{ProbeAction, probe_action};
#[cfg(verus_keep_ghost)]
pub use process_child::{ProbeAction, probe_action};

#[cfg(verus_keep_ghost)]
#[path = "process_fds.rs"]
mod process_fds;
#[cfg(not(verus_keep_ghost))]
pub use crate::process_fds::{ownership_source, select_private_copy};
#[cfg(verus_keep_ghost)]
pub use process_fds::{ownership_source, select_private_copy};

#[cfg(verus_keep_ghost)]
#[path = "retained_artifacts/session/descriptors.rs"]
mod descriptor_batch;
#[cfg(not(verus_keep_ghost))]
pub use crate::retained_artifacts::session::descriptors::{
    BatchError, DESCRIPTOR_BATCH, send_descriptor_batch,
};
#[cfg(verus_keep_ghost)]
pub use descriptor_batch::{BatchError, DESCRIPTOR_BATCH, send_descriptor_batch};

verus! {

pub struct CapturedStreams<'a> {
    pub stdout_bytes: &'a [u8],
    pub stderr_bytes: &'a [u8],
}

pub struct StreamSizes {
    pub stdout_size: usize,
    pub stderr_size: usize,
}

impl<'a> CapturedStreams<'a> {
    pub fn sizes(&self) -> (sizes: StreamSizes)
        ensures
            sizes.stdout_size == self.stdout_bytes.len(),
            sizes.stderr_size == self.stderr_bytes.len(),
    {
        StreamSizes {
            stdout_size: self.stdout_bytes.len(),
            stderr_size: self.stderr_bytes.len(),
        }
    }
}

pub const MAX_OUTPUT_BYTES: usize = 67_108_864;

pub open spec fn output_limit(bytes: usize) -> bool {
    0 < bytes && bytes <= MAX_OUTPUT_BYTES
}

pub open spec fn execution_bounds(timeout_seconds: u64, output_bytes: usize) -> bool {
    0 < timeout_seconds && output_limit(output_bytes)
}

pub fn valid_output_limit(bytes: usize) -> (valid: bool)
    ensures valid == output_limit(bytes),
{
    bytes > 0 && bytes <= MAX_OUTPUT_BYTES
}

pub fn valid_execution_bounds(timeout_seconds: u64, output_bytes: usize) -> (valid: bool)
    ensures valid == execution_bounds(timeout_seconds, output_bytes),
{
    timeout_seconds > 0 && valid_output_limit(output_bytes)
}

}

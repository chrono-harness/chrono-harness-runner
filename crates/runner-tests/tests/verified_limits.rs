//! 直接调用真实实现的独立资源边界预期；相同测试体也参与整 crate 证明。
#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../runner/src/verified_limits.rs"]
mod policy;
#[cfg(not(verus_keep_ghost))]
use chrono_harness::verified_limits as policy;
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::vpanic as panic;

verus! {

// 前置条件由独立预期证明，普通 Rust 同时执行失败分支。
fn expect_limit_true(actual: bool)
    requires actual,
{
    if !actual {
        panic!();
    }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn terminal_sizes_preserve_the_identity_of_empty_and_unequal_streams() {
    let empty: [u8; 0] = [];
    let one = [255u8];
    let two = [0u8, 128];
    let both = policy::CapturedStreams { stdout_bytes: &one, stderr_bytes: &two }.sizes();
    expect_limit_true(both.stdout_size == 1 && both.stderr_size == 2);
    let only_stdout = policy::CapturedStreams { stdout_bytes: &one, stderr_bytes: &empty }.sizes();
    expect_limit_true(only_stdout.stdout_size == 1 && only_stdout.stderr_size == 0);
    let only_stderr = policy::CapturedStreams { stdout_bytes: &empty, stderr_bytes: &two }.sizes();
    expect_limit_true(only_stderr.stdout_size == 0 && only_stderr.stderr_size == 2);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn positive_timeout_accepts_the_complete_inclusive_output_boundary() {
    let minimum = policy::valid_execution_bounds(1, 1);
    let below_maximum = policy::valid_execution_bounds(30, 67_108_863);
    let maximum = policy::valid_execution_bounds(1, 67_108_864);
    let maximum_timeout = policy::valid_execution_bounds(u64::MAX, 67_108_864);
    expect_limit_true(minimum && below_maximum && maximum && maximum_timeout);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn zero_timeout_and_invalid_output_are_rejected_independently() {
    let zero_pair = policy::valid_execution_bounds(0, 0);
    let zero_timeout = policy::valid_execution_bounds(0, 1);
    let zero_timeout_at_maximum = policy::valid_execution_bounds(0, 67_108_864);
    let zero_output = policy::valid_execution_bounds(u64::MAX, 0);
    let above_maximum = policy::valid_execution_bounds(1, 67_108_865);
    let largest_output = policy::valid_execution_bounds(u64::MAX, usize::MAX);
    expect_limit_true(!zero_pair && !zero_timeout && !zero_timeout_at_maximum && !zero_output);
    expect_limit_true(!above_maximum && !largest_output);
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn transport_output_uses_the_same_inclusive_byte_boundary() {
    let zero = policy::valid_output_limit(0);
    let minimum = policy::valid_output_limit(1);
    let maximum = policy::valid_output_limit(67_108_864);
    let above_maximum = policy::valid_output_limit(67_108_865);
    let largest = policy::valid_output_limit(usize::MAX);
    expect_limit_true(!zero && minimum && maximum && !above_maximum && !largest);
}


#[cfg_attr(not(verus_keep_ghost), test)]
fn request_bounds_preserve_line_termination_and_accept_empty_input() {
    expect_limit_true(!policy::command_header(&[]));
    expect_limit_true(policy::command_header(&[10u8]));
    expect_limit_true(!policy::command_header(&[10u8, 0u8]));
    expect_limit_true(policy::command_header(&[255u8, 10u8]));
    expect_limit_true(policy::command_input(0));
    expect_limit_true(policy::command_input(67_108_864));
    expect_limit_true(!policy::command_input(67_108_865));
}

#[cfg(verus_keep_ghost)]
fn inclusive_header_limit_and_first_excess(line: &[u8])
    requires line.len() >= 1_048_576,
        line@[line.len() - 1] == 10u8,
{
    let accepted = policy::command_header(line);
    assert(accepted == (line.len() == 1_048_576));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn acquisition_deadline_keeps_the_fractional_second() {
    expect_limit_true(policy::acquisition_within(0, 0, 0));
    expect_limit_true(policy::acquisition_within(8, 999_999_999, 9));
    expect_limit_true(policy::acquisition_within(9, 0, 9));
    expect_limit_true(!policy::acquisition_within(9, 1, 9));
    expect_limit_true(!policy::acquisition_within(10, 0, 9));
    expect_limit_true(!policy::acquisition_within(u64::MAX, 1, u64::MAX));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn only_an_unexpired_exit_hint_starts_the_wait_for_an_unavailable_status() {
    use policy::ProbeAction;
    match policy::probe_action::<i32, u8>(Ok(None), true, false) {
        Ok(ProbeAction::Join) => {},
        _ => panic!(),
    }
    match policy::probe_action::<i32, u8>(Ok(None), false, false) {
        Ok(ProbeAction::Pending) => {},
        _ => panic!(),
    }
    match policy::probe_action::<i32, u8>(Ok(None), true, true) {
        Ok(ProbeAction::Pending) => {},
        _ => panic!(),
    }
    match policy::probe_action::<i32, u8>(Ok(None), false, true) {
        Ok(ProbeAction::Pending) => {},
        _ => panic!(),
    }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn direct_status_or_native_error_precedes_both_hints() {
    use policy::ProbeAction;
    match policy::probe_action::<i32, u8>(Ok(Some(-19)), true, false) {
        Ok(ProbeAction::Reaped(code)) => expect_limit_true(code == -19),
        _ => panic!(),
    }
    match policy::probe_action::<i32, u8>(Err(77), true, false) {
        Err(error) => expect_limit_true(error == 77),
        _ => panic!(),
    }
}


#[cfg_attr(not(verus_keep_ghost), test)]
fn process_capabilities_leave_standard_streams_to_the_child() {
    expect_limit_true(!policy::ownership_source(-1));
    expect_limit_true(!policy::ownership_source(2));
    expect_limit_true(policy::ownership_source(3));
    expect_limit_true(policy::ownership_source(255));
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn reserved_bash_slot_retries_once_and_preserves_second_failure() {
    let retained = policy::select_private_copy(254, 90u8, || -> (r: Result<u8, u8>)
        ensures r == Err::<u8, u8>(77)
    { Err(77) });
    match retained { Ok(id) => expect_limit_true(id == 90), Err(_) => panic!() }
    let replaced = policy::select_private_copy(255, 90u8, || -> (r: Result<u8, u8>)
        ensures r == Ok::<u8, u8>(91)
    { Ok(91) });
    match replaced { Ok(id) => expect_limit_true(id == 91), Err(_) => panic!() }
    let unavailable = policy::select_private_copy(255, 90u8, || -> (r: Result<u8, u8>)
        ensures r == Err::<u8, u8>(77)
    { Err(77) });
    match unavailable { Err(id) => expect_limit_true(id == 77), Ok(_) => panic!() }
}

#[cfg_attr(not(verus_keep_ghost), test)]
fn empty_batch_skips_the_socket_and_full_batch_requires_one_marker_byte() {
    use policy::BatchError;
    expect_limit_true(policy::DESCRIPTOR_BATCH == 32);
    let empty = policy::send_descriptor_batch(0, || -> (r: Result<usize, u8>)
        ensures r == Err::<usize, u8>(63)
    { Err(63) });
    match empty { Ok(n) => expect_limit_true(n == 0), Err(_) => panic!() }
    let full = policy::send_descriptor_batch(32, || -> (r: Result<usize, u8>)
        ensures r == Ok::<usize, u8>(1)
    { Ok(1) });
    match full { Ok(n) => expect_limit_true(n == 32), Err(_) => panic!() }
    let short = policy::send_descriptor_batch(1, || -> (r: Result<usize, u8>)
        ensures r == Ok::<usize, u8>(0)
    { Ok(0) });
    match short { Err(BatchError::MarkerLength) => {}, _ => panic!() }
    let extra = policy::send_descriptor_batch(31, || -> (r: Result<usize, u8>)
        ensures r == Ok::<usize, u8>(2)
    { Ok(2) });
    match extra { Err(BatchError::MarkerLength) => {}, _ => panic!() }
    let failed = policy::send_descriptor_batch(1, || -> (r: Result<usize, u8>)
        ensures r == Err::<usize, u8>(47)
    { Err(47) });
    match failed { Err(BatchError::Send(e)) => expect_limit_true(e == 47), _ => panic!() }
}


#[cfg_attr(not(verus_keep_ghost), test)]
fn removal_scope_rejects_reopening_and_preserves_acquisition_results() {
    use policy::AcquisitionError;
    let held = policy::acquire_removal(true, || -> (r: Result<Option<u8>, u8>)
        ensures r == Ok::<Option<u8>, u8>(Some(7u8))
    { Ok(Some(7)) });
    match held { Err(AcquisitionError::AlreadyOpen) => {}, _ => panic!() }
    let opened = policy::acquire_removal(false, || -> (r: Result<Option<u8>, u8>)
        ensures r == Ok::<Option<u8>, u8>(Some(17u8))
    { Ok(Some(17)) });
    match opened { Ok(Some(token)) => expect_limit_true(token == 17), _ => panic!() }
    let absent = policy::acquire_removal(false, || -> (r: Result<Option<u8>, u8>)
        ensures r == Ok::<Option<u8>, u8>(None)
    { Ok(None) });
    match absent { Ok(None) => {}, _ => panic!() }
    let failed = policy::acquire_removal(false, || -> (r: Result<Option<u8>, u8>)
        ensures r == Err::<Option<u8>, u8>(21u8)
    { Err(21) });
    match failed { Err(AcquisitionError::Owner(code)) => expect_limit_true(code == 21), _ => panic!() }
}

}

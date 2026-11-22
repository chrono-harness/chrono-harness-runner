#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use chrono_harness::{verified_identifiers as identifiers, verus};
#[cfg(verus_keep_ghost)]
extern crate chrono_identifiers as identifiers;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
verus! {
pub fn same_test(expected:&str, current:Option<&str>) -> (matches:bool)
    ensures matches == match current {Some(actual)=>expected@==actual@,None=>false},
{match current {Some(actual)=>identifiers::equal_text(expected,actual),None=>false}}

pub fn release_custody(verified_child: bool, retired: bool) -> (release: bool)
    ensures release == (verified_child && retired),
{ verified_child && retired }
}
// fork duplicates every thread's open descriptions until exec, even CLOEXEC
// capabilities that the child never intentionally consumes. Tests asserting an
// immediate last-close boundary need a process without unrelated fixture forks.
#[cfg(not(verus_keep_ghost))]
pub fn isolated_capability_test(name: &str) -> bool {
    isolated_test(name, "CHRONO_RETENTION_CAPABILITY_TEST", &[])
}

#[cfg(not(verus_keep_ghost))]
pub const OBSERVATION_PREFIX: &str = "CHRONO_TEST_OBSERVATION ";

/// The Git fixture needs the same native completion and single-test result
/// check, with a private child environment instead of global PATH mutation.
#[cfg(not(verus_keep_ghost))]
pub fn isolated_test(name: &str, marker: &str, environment: &[(&str, std::ffi::OsString)]) -> bool {
    run_isolated_test(name, marker, environment, false)
}

#[cfg(not(verus_keep_ghost))]
pub fn run_isolated_test(
    name: &str,
    marker: &str,
    environment: &[(&str, std::ffi::OsString)],
    forward_observation: bool,
) -> bool {
    if same_test(name, std::env::var(marker).ok().as_deref()) {
        return true;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(marker, name)
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let one_passed = stdout
        .lines()
        .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed;"));
    assert!(
        output.status.success() && one_passed,
        "isolated capability test failed: {}; stdout: {}; stderr: {}",
        output.status,
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    // Successful libtest capture normally hides nested observations. Forward only
    // explicitly emitted records into the enclosing operation's retained stderr.
    // The failure assertion above already carries both original streams.
    if forward_observation {
        use std::io::Write;
        let mut records = output
            .stderr
            .split_inclusive(|byte| *byte == b'\n')
            .filter(|line| line.starts_with(OBSERVATION_PREFIX.as_bytes()));
        let record = records
            .next()
            .expect("successful child omitted its observation");
        assert!(records.next().is_none(), "ambiguous child observation");
        std::io::stderr().lock().write_all(record).unwrap();
    }
    false
}

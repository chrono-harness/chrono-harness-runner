#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use chrono_harness::{verified_identifiers as identifiers, verus};
#[cfg(verus_keep_ghost)]
extern crate chrono_identifiers as identifiers;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::string::StringSliceAdditionalSpecFns;
verus! {
pub fn same_test(expected:&str, current:Option<&str>) -> (matches:bool)
    ensures matches == match current {Some(actual)=>expected@==actual@,None=>false},
{match current {Some(actual)=>identifiers::equal_text(expected,actual),None=>false}}

pub fn release_custody(verified_child: bool, retired: bool) -> (release: bool)
    ensures release == (verified_child && retired),
{ verified_child && retired }

pub const LIBTEST_PASS_PREFIX: &'static str = "test result: ok. 1 passed; 0 failed;";

pub open spec fn libtest_pass_at(stdout: Seq<u8>, start: int) -> bool {
    let prefix = LIBTEST_PASS_PREFIX.spec_bytes();
    let end = start + prefix.len();
    0 <= start && end <= stdout.len()
        && (start == 0 || stdout[start - 1] == '\n' as u8)
        && stdout.subrange(start, end) == prefix
}

/// The ASCII prefix and LF survive lossy UTF-8 decoding. Stripping a CR
/// before LF also has no effect on whether a line begins with this prefix.
pub fn isolated_test_passed(success: bool, stdout: &[u8]) -> (passed: bool)
    ensures passed == (success && exists|start: int| libtest_pass_at(stdout@, start)),
{
    let prefix = LIBTEST_PASS_PREFIX.as_bytes();
    proof {
        reveal_strlit("test result: ok. 1 passed; 0 failed;");
        vstd::utf8::encode_utf8_decode_utf8(LIBTEST_PASS_PREFIX@);
        assert(prefix@.len() > 0);
    }
    if !success || stdout.len() < prefix.len() {
        return false;
    }
    let last_start = stdout.len() - prefix.len();
    let mut start = 0;
    while start <= last_start
        invariant
            success,
            prefix@ == LIBTEST_PASS_PREFIX.spec_bytes(),
            prefix.len() > 0,
            last_start + prefix.len() == stdout.len(),
            forall|earlier: int| 0 <= earlier < start ==> !#[trigger] libtest_pass_at(stdout@, earlier),
        decreases last_start + 1 - start,
    {
        if start == 0 || stdout[start - 1] == '\n' as u8 {
            let (_, rest) = stdout.split_at(start);
            let (candidate, _) = rest.split_at(prefix.len());
            assert(candidate@ =~= stdout@.subrange(start as int, start + prefix.len()));
            if identifiers::equal_bytes(candidate, prefix) {
                assert(libtest_pass_at(stdout@, start as int));
                return true;
            }
        }
        assert(!libtest_pass_at(stdout@, start as int));
        start += 1;
    }
    false
}
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
    assert!(
        isolated_test_passed(output.status.success(), &output.stdout),
        "isolated capability test failed: {}; stdout: {}; stderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
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

/// Run the case with private command fixtures supplied by the calling test.
#[cfg(not(verus_keep_ghost))]
pub fn isolated_git_with_observation(
    model: &std::path::Path,
    cargo: Option<&std::path::Path>,
    retirement: impl FnOnce(&std::path::Path) -> Result<bool, String>,
    forward_observation: bool,
) -> bool {
    let thread = std::thread::current();
    let name = thread.name().expect("libtest case identity");
    const MARKER: &str = "CHRONO_SELECTED_GIT_BOUNDARY";
    if same_test(name, std::env::var(MARKER).ok().as_deref()) {
        return true;
    }
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("git");
    std::fs::copy(model, &program).unwrap();
    if let Some(cargo) = cargo {
        std::fs::copy(cargo, directory.path().join("cargo")).unwrap();
    }
    let mut path = vec![directory.path().to_path_buf()];
    path.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    // Own this exact private directory through the final removal decision.
    // An incidental forked copy cannot release this owner's live lock.
    let ownership =
        chrono_harness::ownership::DirectoryLease::acquire_local(directory.path(), true, None)
            .unwrap()
            .expect("new private fixture directory is available");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let value = run_isolated_test(
            name,
            MARKER,
            &[
                ("PATH", std::env::join_paths(path).unwrap()),
                (
                    "CHRONO_TEST_CUSTODY_ROOT",
                    directory.path().join("custody").into_os_string(),
                ),
            ],
            forward_observation,
        );
        let custody = directory.path().join("custody");
        let retired = if custody.try_exists().unwrap() {
            retirement(&custody)
                .unwrap_or_else(|error| panic!("fixture retirement unknown: {error}"))
        } else {
            true
        };
        ownership
            .stable()
            .expect("original private fixture directory identity");
        (value, release_custody(true, retired))
    }));
    match result {
        Ok((value, released)) => {
            if !released {
                eprintln!(
                    "Git-boundary custody handoff (use its registered finish/maintain policy): {}",
                    directory.path().display()
                );
                let _ = directory.keep();
            } else {
                directory
                    .close()
                    .expect("released private Git fixture removal");
            }
            value
        }
        Err(error) => {
            eprintln!(
                "failed Git-boundary fixture originals: {}",
                directory.path().display()
            );
            let _ = directory.keep();
            std::panic::resume_unwind(error)
        }
    }
}

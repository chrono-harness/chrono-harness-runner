#![allow(unexpected_cfgs)]
#[cfg(verus_keep_ghost)]
#[path = "../../../test-process/src/lib.rs"]
mod process;
#[cfg(not(verus_keep_ghost))]
pub use chrono_test_process::*;
#[cfg(verus_keep_ghost)]
pub use process::*;

/// Bind legacy Git acquisition to a private command mock before libtest runs
/// the original case. The parent verifies the child's actual case result.
#[cfg(not(verus_keep_ghost))]
pub fn isolated_git() -> bool {
    isolated_git_with_retirement(|_| Ok(false))
}

/// The fixture publisher supplies its existing owner's retirement observation.
/// This uses the original child join; there is no second completion channel.
#[cfg(not(verus_keep_ghost))]
pub fn isolated_git_with_retirement(
    retirement: impl FnOnce(&std::path::Path) -> Result<bool, String>,
) -> bool {
    isolated_git_with_observation(retirement, false)
}

#[cfg(not(verus_keep_ghost))]
pub fn isolated_git_with_observation(
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
    std::fs::copy(
        option_env!("CARGO_BIN_EXE_chrono-test-git-model").expect("registered Git mock binary"),
        &program,
    )
    .unwrap();
    if let Some(cargo) = option_env!("CARGO_BIN_EXE_chrono-test-cargo-model") {
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

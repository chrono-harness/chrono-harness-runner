//! Check the observed version and help of the binary to be published.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
pub fn help_marker() -> (marker: &'static str)
    ensures marker@ == "chrono-harness check"@,
{ "chrono-harness check" }

pub fn accepts(version_matches: bool, help_matches: bool) -> (ok: bool)
    ensures ok == (version_matches && help_matches),
{ version_matches && help_matches }
}

#[cfg(not(verus_keep_ghost))]
fn observed(expected: &[u8], version: &[u8], help: &[u8]) -> bool {
    let marker = help_marker().as_bytes();
    accepts(
        version == expected,
        help.windows(marker.len()).any(|part| part == marker),
    )
}

#[cfg(not(verus_keep_ghost))]
fn main() -> Result<(), String> {
    use std::{env, fs, os::unix::ffi::OsStrExt};
    let args: Vec<_> = env::args_os().skip(1).collect();
    let [expected_version, observed_version, help_file] = args.as_slice() else {
        return Err(
            "usage: release_observation EXPECTED_VERSION OBSERVED_VERSION HELP_FILE".into(),
        );
    };
    let help = fs::read(help_file).map_err(|e| e.to_string())?;
    if !observed(
        expected_version.as_bytes(),
        observed_version.as_bytes(),
        &help,
    ) {
        return Err("unexpected runner release version or help".into());
    }
    Ok(())
}

#[cfg(all(test, not(verus_keep_ghost)))]
#[test]
fn actual_version_and_help_bytes_determine_the_verdict() {
    let expected = b"chrono-harness 0.1.9";
    let help = b"usage\n  chrono-harness check [args]\n";
    assert!(observed(expected, expected, help));
    assert!(!observed(expected, b"other", help));
    assert!(!observed(expected, expected, b"chrono-harness che"));
    assert!(!observed(expected, expected, b""));
    assert!(observed(
        expected,
        expected,
        b"prefix\x00chrono-harness check\xff"
    ));
}

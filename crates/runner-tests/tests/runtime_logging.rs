use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn binary() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_chrono-harness-test-cli").into()
}
fn policy(root: &Path, max_bytes: u64) {
    fs::create_dir_all(root.join(".chrono-harness")).unwrap();
    fs::write(
        root.join(".chrono-harness/logging.json"),
        serde_json::to_vec(&json!({
            "schema":"chrono-runtime-logging/v1","level":"info",
            "directory":".chrono-harness/state/logs/", "max_bytes":max_bytes,
            "max_files":3,"stderr":false,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn cli_forwards_lifecycle_and_diagnostics_without_changing_the_business_streams() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    policy(&root, 1024);
    fs::write(root.join("mock-diagnostics.json"), br#"{"record":true}"#).unwrap();
    for arg in ["--version", "unknown-command"] {
        let expected = chrono_harness::dispatch(&[arg]);
        let actual = Command::new(binary())
            .arg(arg)
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(actual.status.code(), Some(i32::from(expected.exit_code)));
        assert_eq!(actual.stdout, expected.stdout.as_bytes());
        assert_eq!(actual.stderr, expected.stderr.as_bytes());
    }
    let lines = fs::read_to_string(root.join("mock-diagnostics.jsonl")).unwrap();
    let rows = lines
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .collect::<Vec<_>>();
    let starts = rows
        .iter()
        .filter(|r| r.get("start").is_some())
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 2);
    assert_eq!(starts[0]["start"]["name"], "chrono-harness");
    assert_eq!(starts[0]["start"]["operation"], "--version");
    assert!(starts[0]["start"]["pid"].as_u64().unwrap() > 0);
    assert!(rows.iter().any(|r| r["exit_code"] == 0));
    assert!(rows.iter().any(|r| r["exit_code"] == 2));
    assert!(lines.contains("cli.diagnostic"));
}
#[test]
fn cli_preserves_owner_failure_before_ready_and_after_business_output() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    policy(&root, 1024);
    for (mode, diagnostic, output) in [
        ("reject", "mock policy rejected", false),
        ("finish_failure", "mock cannot finish", true),
    ] {
        fs::write(
            root.join("mock-diagnostics.json"),
            serde_json::to_vec(&json!({mode:true})).unwrap(),
        )
        .unwrap();
        let cli = Command::new(binary())
            .arg("--version")
            .current_dir(&root)
            .output()
            .unwrap();
        assert_eq!(cli.status.code(), Some(2));
        assert_eq!(!cli.stdout.is_empty(), output);
        assert_eq!(std::str::from_utf8(&cli.stderr).unwrap().trim(), diagnostic);
    }
}
#[test]
fn absent_host_adoption_leaves_argument_errors_with_the_command() {
    let fixture = tempfile::tempdir().unwrap();
    let missing = fixture.path().join("missing-host");
    let args = ["unknown-command", "--host-root", missing.to_str().unwrap()];
    let expected = chrono_harness::dispatch(&args);
    let actual = Command::new(binary())
        .args(args)
        .current_dir(fixture.path())
        .output()
        .unwrap();
    assert_eq!(actual.status.code(), Some(i32::from(expected.exit_code)));
    assert_eq!(actual.stdout, expected.stdout.as_bytes());
    assert_eq!(actual.stderr, expected.stderr.as_bytes());
    assert!(!missing.exists());
}

#[test]
fn command_stream_preserves_raw_outputs_and_checks_the_original_executable_when_requested() {
    use chrono_harness::command_stream::serve;
    for identity in [None, Some("0123456789abcdef")] {
        let mut header = serde_json::json!({"program":"tool","args":["read"],"env":{"PATH":"/tools"},"timeout_seconds":3,"output_limit_bytes":64});
        if let Some(digest) = identity {
            header["expected_sha256"] = digest.into();
        }
        let mut request = serde_json::to_vec(&header).unwrap();
        request.extend_from_slice(b"\n\0\xff");
        let mut output = Vec::new();
        serve(request.as_slice(), &mut output, |command, bytes, digest| {
            assert_eq!(command.program, "tool");
            assert_eq!(command.args, ["read"]);
            assert_eq!(command.env["PATH"], "/tools");
            assert_eq!(
                (command.timeout_seconds, command.output_limit_bytes),
                (3, 64)
            );
            assert_eq!(bytes, b"\0\xff");
            assert_eq!(digest, identity);
            Ok((17, vec![0, 255], vec![13, 10, 128]))
        })
        .unwrap();
        assert_eq!(
            output,
            b"{\"exit_code\":17,\"stdout_length\":2,\"stderr_length\":3}\n\x00\xff\r\n\x80"
        );
        output.clear();
        let error = serve(request.as_slice(), &mut output, |_, _, _| {
            Err("original process timeout".into())
        })
        .unwrap_err();
        assert_eq!(error, "original process timeout");
        assert!(output.is_empty());
    }
    let error = serve(b"{}".as_slice(), Vec::new(), |_, _, _| {
        panic!("incomplete header reached the process")
    })
    .unwrap_err();
    assert!(error.contains("JSON line"));
}

#[test]
fn command_record_requires_identity_and_preserves_the_original_after_failure() {
    use chrono_harness::command_stream::serve_record;
    use serde_json::{Value, json};
    let mut header =
        json!({"program":"decoder","args":[],"env":{},"timeout_seconds":3,"output_limit_bytes":64});
    let encode = |h: &Value| {
        let mut request = serde_json::to_vec(h).unwrap();
        request.extend_from_slice(b"\n\x00\xff");
        request
    };
    let mut output = Vec::new();
    assert!(
        serve_record(&encode(&header)[..], &mut output, |_, _, _| panic!(
            "unbound command ran"
        ))
        .unwrap_err()
        .contains("expected_sha256")
    );
    assert!(output.is_empty());
    let digest = "ab".repeat(32);
    header["expected_sha256"] = json!(digest);
    let original = json!({"argv":["decoder"], "cwd":"/work", "environment":{}, "environment_digest":"env",
        "stdin_sha256":"in", "stdout_bytes":[255], "stderr_bytes":[0], "stdout_complete":false,
        "stderr_complete":true, "stdout_sha256":"out", "stderr_sha256":"err", "failure":"timeout",
        "exit_code":137, "owned_work_joined":true,"exit_signal":9,"stdout":"�","stderr":"\0",
        "executable":"/bin/decoder","sha256":digest,"launcher_pid":45,"child_pid":46,
        "launch_original":{"selected":"decoder"}});
    serve_record(&encode(&header)[..], &mut output, |spec, input, bound| {
        assert_eq!(spec.program, "decoder");
        assert_eq!(input, [0, 255]);
        assert_eq!(bound, digest);
        serde_json::from_value(original.clone()).map_err(|e| e.to_string())
    })
    .unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&output).unwrap(), original);
}

#[test]
fn public_compiler_query_preserves_owner_paths_and_rejects_missing_originals() {
    use chrono_harness::route_evidence::serve;
    use serde_json::{Value, json};
    let name = "{operation_state}/receipt.rlib";
    let argv = json!([
        "--crate-type=lib",
        "--no-cheating",
        "--no-external-by-default",
        "--num-threads",
        "2",
        "--output-json",
        "--compile",
        format!("--emit=link={name},dep-info={name}.d"),
        "src.rs",
        "-o",
        name,
        "-Zchecksum-hash-algorithm=sha256"
    ]);
    let mut output = Vec::new();
    let bind = json!({"operation":"bind","argv":argv,"directory":"bound"});
    serve(
        "compiler-output-v1",
        bind.to_string().as_bytes(),
        &mut output,
    )
    .unwrap();
    let bound: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(bound[10], "bound/receipt.rlib");
    assert_eq!(
        bound[7],
        "--emit=link=bound/receipt.rlib,dep-info=bound/receipt.rlib.d"
    );
    let request = json!({"operation":"check","contract":{"version":"fixture","commit":"fixture"},
        "stdout":[0,255],"argv":bound,"exit":0,"process_failed":false,"dependency":null,"limit":4096,"artifact":null});
    assert!(
        serve(
            "compiler-output-v1",
            request.to_string().as_bytes(),
            Vec::new()
        )
        .unwrap_err()
        .contains("E_VERUS_RESULT")
    );
    let mut original = Vec::new();
    let invalid = json!({"operation":"observe","contract":{"version":"1","commit":"x"},
        "argv":[],"exit":1,"process_failed":true,"limit":128});
    serve(
        "compiler-output-v1",
        invalid.to_string().as_bytes(),
        &mut original,
    )
    .unwrap();
    let observed: Value = serde_json::from_slice(&original).unwrap();
    assert!(
        observed["dependency"]["Err"]
            .as_str()
            .unwrap()
            .contains("E_VERUS_SOURCES")
    );
    assert!(
        observed["artifact"]["Err"]
            .as_str()
            .unwrap()
            .contains("E_VERUS_ARTIFACT")
    );
    let raw = json!({"root":".","program":"missing","version_argv":[],"environment":{},"timeout_seconds":1,"output_limit_bytes":100});
    assert_eq!(
        serve("observed-tool-v1", raw.to_string().as_bytes(), Vec::new()).unwrap_err(),
        "tool resolution requires declared PATH"
    );
}

#[test]
fn proof_rows_in_the_compiler_protocol_require_the_matching_whole_crate() {
    let report = json!({
        "verus": {"version": "v", "commit": "c"},
        "verification-results": {"success": true, "verified": 1, "errors": 0,
            "encountered-error": false, "encountered-vir-error": false,
            "is-verifying-entire-crate": true}
    });
    let mut partial = report.clone();
    partial["verification-results"]["is-verifying-entire-crate"] = json!(false);
    let mut different = report.clone();
    different["verus"]["commit"] = json!("other");
    let request = json!({"operation":"proof-rows","contract":{"version":"v","commit":"c"},
        "rows":[report.to_string(), partial.to_string(), different.to_string(), "invalid"]});
    let mut result = Vec::new();
    chrono_harness::route_evidence::serve(
        "compiler-output-v1",
        request.to_string().as_bytes(),
        &mut result,
    )
    .unwrap();
    let rows: Value = serde_json::from_slice(&result).unwrap();
    assert_eq!(rows, json!([true, false, false, null]));
}

#[test]
fn compiler_originals_use_the_operation_directory_and_its_byte_limit() {
    use chrono_harness::route_evidence::serve;
    use serde_json::{Value, json};
    let root = std::env::current_dir().unwrap();
    let dir = tempfile::tempdir_in(&root).unwrap();
    let output = dir
        .path()
        .join("library.rlib")
        .strip_prefix(&root)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let args = json!([
        "--crate-type=lib",
        "--no-cheating",
        "--no-external-by-default",
        "--num-threads",
        "2",
        "--output-json",
        "--compile",
        format!("--emit=link={output},dep-info={output}.d"),
        "bounded.rs",
        "-o",
        output,
        "-Zchecksum-hash-algorithm=sha256",
    ]);
    let observe = |exit: i32, failure: Option<&str>| {
        let input = json!({"operation":"observe","contract":{"version":"fixture","commit":"fixture"},
            "argv":args,"exit":exit,"process_failed":failure.is_some(),"limit":4});
        let mut bytes = Vec::new();
        serve(
            "compiler-output-v1",
            input.to_string().as_bytes(),
            &mut bytes,
        )
        .unwrap();
        serde_json::from_slice::<Value>(&bytes).unwrap()
    };
    let dependency = |exit, failure| {
        serde_json::from_value::<Result<Vec<u8>, String>>(
            observe(exit, failure)["dependency"].clone(),
        )
        .unwrap()
    };
    let artifact = || {
        serde_json::from_value::<Result<chrono_harness::verified_verifier::CompiledArtifact, String>>(observe(0, None)["artifact"].clone()).unwrap()
    };
    assert!(dependency(0, None).is_err());
    assert!(artifact().is_err());
    std::fs::write(dir.path().join("library.rlib.d"), [0u8, 255, 1, 127]).unwrap();
    let content = vec![255u8; 65537];
    std::fs::write(dir.path().join("library.rlib"), &content).unwrap();
    assert_eq!(dependency(0, None).unwrap(), [0, 255, 1, 127]);
    let original = artifact().unwrap();
    assert_eq!(original.path, output.as_bytes());
    assert_eq!(
        original.sha256,
        b"ef9993e97e36b66b65b16a741da437aecddffa22185e176a49f8cfe9ecc7a07c"
    );
    assert_eq!(original.length, 65537);
    assert!(dependency(9, None).is_err());
    assert!(dependency(0, Some("capture failed")).is_err());
    std::fs::write(dir.path().join("library.rlib.d"), [255u8; 5]).unwrap();
    assert!(
        dependency(0, None)
            .unwrap_err()
            .contains("exceeds its bound")
    );
    std::fs::remove_file(dir.path().join("library.rlib")).unwrap();
    std::os::unix::fs::symlink("library.rlib.d", dir.path().join("library.rlib")).unwrap();
    assert!(artifact().is_err());
}

#[test]
fn observed_command_forwards_binary_streams_and_original_terminal_evidence() {
    use chrono_harness::{observed_command::serve, sha256};
    use serde_json::{Value, json};
    use std::{io::Read, time::Duration};
    let mut request=serde_json::to_vec(&json!({"command":{"program":"/tool","args":[],"env":{},"timeout_seconds":3,"output_limit_bytes":128},"expected_sha256":"expected","timeout":{"secs":0,"nanos":312}})).unwrap();
    request.extend_from_slice(b"\n\x00\xff");
    let mut output = Vec::new();
    serve(request.as_slice(),&mut output,|spec,input,expected,deadline,fwd|{
        assert_eq!((spec.program.as_str(),input,expected,deadline),("/tool",b"\0\xff".as_slice(),"expected",Duration::from_nanos(312)));
        fwd.stderr.write_all(&[0,128]).unwrap();
        fwd.stdout.write_all(&[255]).unwrap();
        serde_json::from_value(json!({"argv":["/tool"],"cwd":"/root","environment":{},"environment_digest":"env","stdin_sha256":"input","stdout_bytes":[255],"stderr_bytes":[0,128],"stdout_complete":true,"stderr_complete":false,"stdout_sha256":sha256(&[255]),"stderr_sha256":sha256(&[0,128]),"failure":"stream limit","exit_code":-1,"exit_signal":9,"stdout":"","stderr":"","executable":"/tool","sha256":"expected","launcher_pid":42,"child_pid":43})).map_err(|e|e.to_string())
    }).unwrap();
    let mut raw = output.as_slice();
    let mut next = |tag: u8| {
        let mut header = [0; 9];
        raw.read_exact(&mut header).unwrap();
        assert_eq!(header[0], tag);
        let mut bytes = vec![0; u64::from_be_bytes(header[1..].try_into().unwrap()) as usize];
        raw.read_exact(&mut bytes).unwrap();
        bytes
    };
    assert_eq!(next(b'E'), [0, 128]);
    assert_eq!(next(b'O'), [255]);
    let result: Value = serde_json::from_slice(&next(b'T')).unwrap();
    assert_eq!(
        (
            result["launcher_pid"].as_u64(),
            result["child_pid"].as_u64()
        ),
        (Some(42), Some(43))
    );
    assert_eq!(result["failure"], "stream limit");
    assert_eq!(result["stderr_complete"], false);
    assert_eq!(result["exit_signal"], 9);
    assert_eq!(result["stdout_sha256"], sha256(&[255]));
    assert!(raw.is_empty());
    let result = serve(request.as_slice(), Vec::new(), |_, _, _, _, _| {
        Err("prelaunch error".into())
    });
    assert_eq!(result, Err("prelaunch error".into()));
    let oversized = b"{\"command\":{\"program\":\"/tool\",\"timeout_seconds\":1,\"output_limit_bytes\":1},\"expected_sha256\":\"d\",\"timeout\":{\"secs\":1,\"nanos\":1}}\n";
    let result = serve(oversized.as_slice(), Vec::new(), |_, _, _, _, _| {
        panic!("out-of-bounds command reached the launcher")
    });
    assert!(result.unwrap_err().contains("timeout exceeds"));
}

#[test]
fn declaration_query_checks_roots_and_preserves_reference_path_order() {
    use chrono_harness::route_evidence::serve;
    use serde_json::{Value, json};

    let file = json!({"path":"pkg/a.rs","verification":{"role":"program","roots":[{"path":"root.rs","operation":"verify"}]}});
    let query = |file: Value, operations: Value| {
        let input = json!({"operation":"source-declarations","files":[file],"operations":operations,
            "reference_paths":["pkg/a.rs","./a.rs","two//parts","real"]});
        let mut output = Vec::new();
        serve(
            "compiler-output-v1",
            input.to_string().as_bytes(),
            &mut output,
        )?;
        serde_json::from_slice::<Vec<bool>>(&output).map_err(|e| e.to_string())
    };
    assert_eq!(
        query(file.clone(), json!(["verify"])).unwrap(),
        vec![true, false, false, true]
    );
    assert!(
        query(file.clone(), json!([]))
            .unwrap_err()
            .contains("pkg/a.rs: E_VERUS_COVERAGE: proof root operation is not registered")
    );
    let mut invalid = file;
    invalid["verification"]["roots"][0]["path"] = json!("./root.rs");
    assert!(
        query(invalid, json!(["verify"]))
            .unwrap_err()
            .contains("pkg/a.rs: E_VERUS_COVERAGE: invalid file proof declaration")
    );
}

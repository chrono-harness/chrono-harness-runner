use super::*;
use std::os::unix::fs::PermissionsExt;

fn payload() -> (tempfile::TempDir, String) {
    let (dir, _, _) = fixture();
    fs::write(dir.path().join("payload"), b"canonical\n").unwrap();
    let candidate = commit(dir.path());
    (dir, candidate)
}

#[test]
fn raw_checkout_rejects_mode_hidden_by_local_git_configuration() {
    if !git_fixture::isolated(
        "checkout::raw_checkout_rejects_mode_hidden_by_local_git_configuration",
    ) {
        return;
    }
    let (dir, candidate) = payload();
    let root = dir.path();
    git(root, &["config", "core.filemode", "false"]);
    fs::set_permissions(root.join("payload"), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(git(root, &["diff", "--name-only", &candidate, "--"]).is_empty());
    let observed = facts::checkout(root, &candidate).unwrap();
    assert!(observed.tracked.contains(&"payload".into()), "{observed:?}");
}

#[test]
fn raw_checkout_rejects_bytes_hidden_by_clean_filter_without_running_filter() {
    if !git_fixture::isolated(
        "checkout::raw_checkout_rejects_bytes_hidden_by_clean_filter_without_running_filter",
    ) {
        return;
    }
    let (dir, _) = payload();
    let root = dir.path();
    fs::write(root.join(".gitattributes"), "payload filter=hide\n").unwrap();
    let filter = env!("CARGO_BIN_EXE_chrono-test-cli-child");
    let quote = |value: &str| format!("'{}'", value.replace('\'', "'\\''"));
    git(
        root,
        &[
            "config",
            "filter.hide.clean",
            &format!("{} filter-clean", quote(filter)),
        ],
    );
    let candidate = commit(root);
    fs::write(root.join("payload"), b"uncommitted actual bytes\0\xff").unwrap();
    assert!(git(root, &["diff", "--name-only", &candidate, "--"]).is_empty());
    // A snapshot check must not delegate byte identity to a clean filter.
    git(
        root,
        &[
            "config",
            "filter.hide.clean",
            &format!("{} filter-fail", quote(filter)),
        ],
    );
    git(root, &["config", "filter.hide.required", "true"]);
    let observed = facts::checkout(root, &candidate)
        .expect("raw facts must not run the configured clean filter");
    assert!(observed.tracked.contains(&"payload".into()), "{observed:?}");
}

#[test]
fn raw_checkout_rejects_line_endings_hidden_by_git_conversion() {
    if !git_fixture::isolated(
        "checkout::raw_checkout_rejects_line_endings_hidden_by_git_conversion",
    ) {
        return;
    }
    let (dir, _) = payload();
    let root = dir.path();
    fs::write(root.join(".gitattributes"), "payload text\n").unwrap();
    let candidate = commit(root);
    fs::write(root.join("payload"), b"canonical\r\n").unwrap();
    assert!(git(root, &["diff", "--name-only", &candidate, "--"]).is_empty());
    let observed = facts::checkout(root, &candidate).unwrap();
    assert!(observed.tracked.contains(&"payload".into()), "{observed:?}");
}

#[test]
fn raw_checkout_checks_symlinks_types_and_literal_parent_paths() {
    if !git_fixture::isolated(
        "checkout::raw_checkout_checks_symlinks_types_and_literal_parent_paths",
    ) {
        return;
    }
    use std::os::unix::fs::symlink;
    let (dir, _) = payload();
    let root = dir.path();
    fs::create_dir(root.join("parent")).unwrap();
    fs::write(root.join("parent/file"), b"contents\0\xff").unwrap();
    symlink("missing target", root.join("alias")).unwrap();
    let candidate = commit(root);
    assert!(
        facts::checkout(root, &candidate)
            .unwrap()
            .tracked
            .is_empty()
    );
    fs::remove_file(root.join("alias")).unwrap();
    fs::write(root.join("alias"), "missing target").unwrap();
    git(root, &["config", "core.symlinks", "false"]);
    assert!(git(root, &["diff", "--name-only", &candidate, "--"]).is_empty());
    assert!(
        facts::checkout(root, &candidate)
            .unwrap()
            .tracked
            .contains(&"alias".into())
    );
    fs::rename(root.join("parent"), root.join("elsewhere")).unwrap();
    symlink("elsewhere", root.join("parent")).unwrap();
    fs::remove_file(root.join("payload")).unwrap();
    fs::create_dir(root.join("payload")).unwrap();
    let observed = facts::checkout(root, &candidate).unwrap();
    for name in ["alias", "parent/file", "payload"] {
        assert!(observed.tracked.contains(&name.into()), "{observed:?}");
    }
}

#[test]
fn raw_checkout_supports_sha256_and_keeps_index_changes_separate() {
    if !git_fixture::isolated(
        "checkout::raw_checkout_supports_sha256_and_keeps_index_changes_separate",
    ) {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q", "--object-format=sha256"]);
    let bytes = vec![0xabu8; 1024 * 1024 + 13];
    fs::write(root.join("payload"), &bytes).unwrap();
    let candidate = commit(root);
    assert_eq!(candidate.len(), 64);
    assert!(
        facts::checkout(root, &candidate)
            .unwrap()
            .tracked
            .is_empty()
    );
    fs::write(root.join("payload"), b"staged replacement").unwrap();
    git(root, &["add", "payload"]);
    fs::write(root.join("payload"), &bytes).unwrap();
    assert_eq!(
        facts::checkout(root, &candidate).unwrap().tracked,
        vec!["payload"]
    );
    git(root, &["reset", "--mixed", &candidate]);
    fs::remove_file(root.join("payload")).unwrap();
    assert_eq!(
        facts::checkout(root, &candidate).unwrap().tracked,
        vec!["payload"]
    );
}

#[test]
fn git_facts_session_rejects_invalid_open_and_incomplete_frames() {
    use chrono_harness::git_facts_session::serve;
    let root = std::path::Path::new(".");
    let invalid =
        b"{\"mode\":\"config\",\"config\":\"c\",\"base\":\"dev\",\"candidate\":\"main\"}\n";
    let mut stdout = Vec::new();
    serve(root, invalid.as_slice(), &mut stdout).unwrap();
    assert!(
        serde_json::from_slice::<Result<serde_json::Value, String>>(&stdout)
            .unwrap()
            .is_err()
    );
    assert!(
        serve(root, invalid[..invalid.len() - 1].as_ref(), Vec::new())
            .unwrap_err()
            .contains("frame")
    );
}

#[test]
fn history_sessions_reject_symbolic_inputs_and_endpoints_without_invoking_git() {
    let input = concat!(
        "{\"mode\":\"history\"}\n",
        "{\"op\":\"ancestors\",\"commit\":\"dev\"}\n",
        "{\"op\":\"verify-commit\",\"commit\":\"HEAD\"}\n",
        "{\"op\":\"registry-snapshot\",\"endpoint\":\"candidate\"}\n",
        "{\"op\":\"observation\"}\n"
    );
    let mut output = Vec::new();
    chrono_harness::git_facts_session::serve(
        std::path::Path::new("."),
        input.as_bytes(),
        &mut output,
    )
    .unwrap();
    let replies: Vec<Result<serde_json::Value, String>> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(replies[0], Ok(serde_json::json!({"bound":false})));
    assert!(replies[1].is_err());
    assert!(replies[2].is_err());
    assert!(
        replies[3]
            .as_ref()
            .unwrap_err()
            .contains("no configured history endpoints")
    );
    assert_eq!(replies[4], Ok(serde_json::Value::Null));
}

#[test]
fn evidence_frames_keep_original_decoding_and_transport_limits() {
    let input = concat!(
        "{\"operation\":\"decode-json\",\"text\":\"{\\\"a\\\":1,\\\"a\\\":2}\"}\n",
        "{\"operation\":\"report-transport-bytes\",\"report\":{\"judges\":[{\"process\":{\"encoding\":\"chrono-retained-process/v2\",\"stdout_length\":7,\"stderr_length\":2}}]},\"metadata_bytes\":17}\n",
        "{\"operation\":\"expand-record\",\"record\":{\"process\":{\"encoding\":\"chrono-retained-process/v2\",\"stdout_zlib_hex\":\"00\",\"stdout_length\":12}}}\n"
    );
    let mut output = Vec::new();
    chrono_harness::judge_evidence::serve(input.as_bytes(), &mut output).unwrap();
    let replies: Vec<Result<serde_json::Value, String>> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(replies[0].is_err());
    assert_eq!(replies[1], Ok(serde_json::json!(26)));
    assert!(replies[2].is_err());
    let mut output = Vec::new();
    let query = concat!(
        "{\"operation\":\"validate-schedule\",\"bindings\":[]}\n",
        "{\"operation\":\"full-execution-units\",\"profile\":{}}\n",
        "{\"operation\":\"full-execution-units\",\"profile\":{\"schema_version\":4,\"execution_units\":{}}}\n"
    );
    chrono_harness::judge_evidence::serve(query.as_bytes(), &mut output).unwrap();
    let replies: Vec<Result<serde_json::Value, String>> = std::str::from_utf8(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        replies[0]
            .as_ref()
            .unwrap_err()
            .contains("no configured judges")
    );
    assert_eq!(replies[1], Ok(serde_json::Value::Null));
    assert!(replies[2].is_err());
    assert!(chrono_harness::judge_evidence::serve(b"{}".as_slice(), Vec::new()).is_err());
}

#[test]
fn scoped_git_endpoints_validate_collection_inputs_before_running_git() {
    use serde_json::{Value, json};
    fn exchange(requests: &[Value]) -> Vec<Result<Value, String>> {
        let mut frames = Vec::new();
        for request in requests {
            serde_json::to_writer(&mut frames, request).unwrap();
            frames.push(b'\n');
        }
        let mut output = Vec::new();
        chrono_harness::git_facts_session::serve(
            std::path::Path::new("."),
            frames.as_slice(),
            &mut output,
        )
        .unwrap();
        serde_json::Deserializer::from_slice(&output)
            .into_iter()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    let candidate = "a".repeat(40);
    let results = exchange(&[
        json!({"mode":"scoped","config":null,"base":null,"candidate":candidate}),
        json!({"op":"tree-entries","endpoint":"base"}),
        json!({"op":"registry-at","endpoint":"candidate","config":"../outside"}),
        json!({"op":"blob-batch","endpoint":"candidate","paths":["../outside"]}),
        json!({"op":"blob-batch","endpoint":"base","paths":[]}),
        json!({"op":"observation"}),
    ]);
    assert_eq!(results[0], Ok(json!({"bound":false})));
    assert!(
        results[1]
            .as_ref()
            .unwrap_err()
            .contains("no base endpoint")
    );
    assert!(results[2].is_err() && results[3].is_err());
    assert!(
        results[4]
            .as_ref()
            .unwrap_err()
            .contains("no base endpoint")
    );
    assert_eq!(results[5], Ok(Value::Null));
    let invalid = exchange(&[
        json!({"mode":"prepared-collection","config":"c","base":candidate,"candidate":candidate,"context":"c","scope":{"kind":"unit","unit":"test"},"judge":"ci"}),
    ]);
    assert!(invalid[0].is_err());
}

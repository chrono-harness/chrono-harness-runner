//! Exercise the runner's real initial/full execution and reporting with external judges modeled at the process boundary.
use chrono_harness::{facts, sha256};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tempfile::TempDir;

struct Host {
    dir: TempDir,
    base: String,
    candidate: String,
}
// Self-contained registry inputs; tests never query the source checkout's Git.
fn fixture_registries() -> Value {
    serde_json::from_str(include_str!("fixtures/host-registries.json")).unwrap()
}
fn fixture_git() -> PathBuf {
    Path::new(env!("CHRONO_TEST_GIT")).canonicalize().unwrap()
}
fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new(fixture_git())
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn write(root: &Path, path: &str, value: &Value) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn commit(root: &Path) -> String {
    git(root, &["add", "."]);
    git(
        root,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--no-gpg-sign",
            "-qm",
            "fixed fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}
fn file(path: &str) -> Value {
    json!({"path":path,"owner":"repository","surface":"documentation","cost":"unmeasured","edges":[]})
}
impl Host {
    fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("registration host with spaces ")
            .tempdir()
            .unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        fs::create_dir_all(root.join(".chrono-harness/bin")).unwrap();
        for (project, binary) in [
            ("runner", "chrono-harness"),
            ("judge-registration", "chrono-judge-registration"),
        ] {
            let p = match (project, binary) {
                ("runner", "chrono-harness") => {
                    PathBuf::from(env!("CARGO_BIN_EXE_chrono-harness-test-cli"))
                }
                ("judge-registration", "chrono-judge-registration") => {
                    env!("CARGO_BIN_EXE_chrono-test-registration-regression").into()
                }
                _ => panic!("unregistered fixture binary: {project}/{binary}"),
            };
            assert!(
                p.is_file(),
                "build registered fixture binaries first: {}",
                p.display()
            );
            fs::copy(p, root.join(format!(".chrono-harness/bin/{binary}"))).unwrap();
        }
        let mut values = std::collections::BTreeMap::new();
        for name in ["config", "judges", "projects", "FILEMAP", "workflow"] {
            let mut v = fixture_registries()[name].clone();
            v["status"] = "active".into();
            values.insert(name, v);
        }
        let c = values.get_mut("config").unwrap();
        c["enforcement"] = "enabled".into();
        c["runner"]["sha256"] =
            sha256(&fs::read(root.join(".chrono-harness/bin/chrono-harness")).unwrap()).into();
        c["input_closure"] = json!({"status":"declared-complete","unresolved":[]});
        c["tools"] = json!([]);
        c["semantic_fields"] = json!([]);
        c["environment"] = json!({"inherit":["PATH"],"values":{},"inputs":[]});
        c["artifacts"] = json!([{"path":".chrono-harness/bin/","owner":"repository","kind":"executable","tracked":false},{"path":".chrono-harness/state/","owner":"repository","kind":"evidence","tracked":false}]);
        let j = values.get_mut("judges").unwrap();
        let mut reg = j["judges"][0].clone();
        reg["sha256"] =
            sha256(&fs::read(root.join(".chrono-harness/bin/chrono-judge-registration")).unwrap())
                .into();
        j["judges"] = json!([reg]);
        j["migration_validator"] = "registration".into();
        let p = values.get_mut("projects").unwrap();
        p["owners"] = json!(["repository"]);
        p["projects"] = json!([]);
        p["scripts"] = json!([]);
        let m = values.get_mut("FILEMAP").unwrap();
        m["files"] = json!([
            file(".gitignore"),
            file("document.txt"),
            file(".chrono-harness/config.json"),
            file(".chrono-harness/judges.json"),
            file(".chrono-harness/projects.json"),
            file(".chrono-harness/FILEMAP.json"),
            file(".chrono-harness/workflow.json")
        ]);
        m["project_edges"] = json!([]);
        m["test_costs"] = json!([]);
        let w = values.get_mut("workflow").unwrap();
        w["stability"] = json!([]);
        w["integration"]["tests"] = json!([]);
        for (name, v) in &values {
            write(root, &format!(".chrono-harness/{name}.json"), v);
        }
        fs::write(
            root.join(".gitignore"),
            ".chrono-harness/bin/\n.chrono-harness/state/\nignored.txt\n",
        )
        .unwrap();
        fs::write(root.join("document.txt"), "base document").unwrap();
        fs::write(
            root.join("historical-unregistered.txt"),
            "unrelated historical omission",
        )
        .unwrap();
        let base = commit(root);
        fs::write(root.join("document.txt"), "candidate document").unwrap();
        let candidate = commit(root);
        Self {
            dir,
            base,
            candidate,
        }
    }
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn edit(&mut self, path: &str, f: impl FnOnce(&mut Value)) {
        let mut v: Value =
            serde_json::from_slice(&fs::read(self.root().join(path)).unwrap()).unwrap();
        f(&mut v);
        write(self.root(), path, &v);
        self.candidate = commit(self.root());
    }
    fn context(&self) -> Value {
        json!({"schema_version":1,"base":self.base,"candidate":self.candidate,"dev_tip":self.base,"branch_ref":"integration/fixture","fork_point":self.base,"branch_started_at":"2026-01-01T00:00:00Z","observed_at":"2026-01-01T01:00:00Z","operation":"validate.delta","integration_evidence":null})
    }
    fn run_context(&self, context: Value) -> (i32, Value) {
        write(self.root(), ".chrono-harness/state/context.json", &context);
        let out = Command::new(self.root().join(".chrono-harness/bin/chrono-harness"))
            .current_dir("/")
            .env_remove("HOME")
            .args([
                "check",
                "--config",
                self.root()
                    .join(".chrono-harness/config.json")
                    .to_str()
                    .unwrap(),
                "--base",
                &self.base,
                "--candidate",
                &self.candidate,
                "--context",
                ".chrono-harness/state/context.json",
            ])
            .output()
            .unwrap();
        let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
            json!({
                "stdout": String::from_utf8_lossy(&out.stdout),
                "stderr": String::from_utf8_lossy(&out.stderr)
            })
        });
        (out.status.code().unwrap(), v)
    }
    fn run(&self) -> (i32, Value) {
        self.run_context(self.context())
    }
}
fn finding(v: &Value, code: &str) -> bool {
    v["judges"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|j| j["response"]["findings"].as_array().into_iter().flatten())
        .any(|f| f["code"] == code)
}

fn initial_host() -> Host {
    let mut h = Host::new();
    git(h.root(), &["checkout", "--orphan", "initial-fixture"]);
    for name in ["config", "judges", "projects", "FILEMAP", "workflow"] {
        let path = format!(".chrono-harness/{name}.json");
        let mut v: Value =
            serde_json::from_slice(&fs::read(h.root().join(&path)).unwrap()).unwrap();
        v["status"] = json!("proposed");
        if name == "config" {
            v["enforcement"] = json!("not-implemented");
            v["input_closure"] = json!({"status":"incomplete","unresolved":["toolchain"]});
        }
        if name == "FILEMAP" {
            v["files"].as_array_mut().unwrap().extend([
                file("historical-unregistered.txt"),
                file(".chrono-harness/initial.json"),
            ]);
        }
        write(h.root(), &path, &v);
    }
    let binary = h
        .root()
        .join(".chrono-harness/bin/chrono-judge-registration");
    write(
        h.root(),
        ".chrono-harness/initial.json",
        &json!({
            "schema":"chrono-initial-check/v1", "host_config":".chrono-harness/config.json",
            "timeout_seconds":30, "stdout_limit_bytes":1048576,
            "judges":[{"id":"initial-registration","executable":".chrono-harness/bin/chrono-judge-registration",
                "sha256":sha256(&fs::read(binary).unwrap()),"version":"0.1.0",
                "argv":["--protocol","chrono-initial-judge/v1"],"selector":"every-initial","after":[],"modes":["inventory"]}]
        }),
    );
    h.candidate = commit(h.root());
    h
}

fn initial_run(h: &Host) -> (i32, Value) {
    let o = Command::new(h.root().join(".chrono-harness/bin/chrono-harness"))
        .current_dir("/")
        .env_remove("HOME")
        .args([
            "check",
            "--config",
            h.root()
                .join(".chrono-harness/initial.json")
                .to_str()
                .unwrap(),
            "--candidate",
            &h.candidate,
            "--initial",
        ])
        .output()
        .unwrap();
    let report = serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|_| json!({"stderr":String::from_utf8_lossy(&o.stderr)}));
    (o.status.code().unwrap(), report)
}

fn amend_initial(h: &mut Host) {
    git(h.root(), &["add", "."]);
    git(
        h.root(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--amend",
            "--no-gpg-sign",
            "-qm",
            "fixed initial fixture",
        ],
    );
    h.candidate = git(h.root(), &["rev-parse", "HEAD"]);
}

fn mock_registration(host: &Host, codes: &[&str], status: &str) {
    write(
        host.root(),
        ".chrono-harness/state/mock-registration.json",
        &json!({"codes":codes,"status":status}),
    );
}
fn assert_report_contract(h: &Host, report: &Value) {
    // Required consumer fields come from SPEC §9, independently of serialization.
    let missing: Vec<_> = [
        "schema_version",
        "status",
        "base",
        "candidate",
        "candidate_tree",
        "context_digest",
        "registry_digest",
        "executables",
        "tools",
        "environment",
        "effective_inputs",
        "parity",
        "delta",
        "impact",
        "judges",
        "tests",
        "findings",
        "costs",
    ]
    .into_iter()
    .filter(|key| report.get(key).is_none())
    .collect();
    assert!(
        missing.is_empty(),
        "SPEC9 missing {missing:?}; base={}, candidate={}",
        h.base,
        h.candidate
    );
    assert_eq!(report["base"], h.base);
    assert_eq!(report["candidate"], h.candidate);
    assert_eq!(report["scope"], "configured-judges");
    assert_eq!(
        report["parity"],
        json!({"status":"unestablished","compared_report":null})
    );
    let stored: Value = serde_json::from_slice(
        &fs::read(h.root().join(".chrono-harness/state/report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(&stored, report);
    for (target, sources) in report["sources"].as_object().unwrap() {
        let actual = report.pointer(target).unwrap();
        for source in sources.as_array().unwrap() {
            let original = report
                .pointer(source.as_str().unwrap())
                .expect("source must resolve in the report");
            if !actual.is_null() {
                assert_eq!(actual, original, "source fidelity for {target}");
            }
        }
    }
}

fn assert_unresolved(report: &Value, field: &str) {
    assert!(
        report.get(field).is_some_and(Value::is_null),
        "{field}: {report:#}"
    );
    assert!(
        report["unresolved"][format!("/{field}")]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "missing reason for {field}: {report:#}"
    );
}

fn install_regression_child(h: &Host, path: &str) -> String {
    fs::copy(
        env!("CARGO_BIN_EXE_chrono-test-registration-regression"),
        h.root().join(path),
    )
    .unwrap();
    sha256(&fs::read(h.root().join(path)).unwrap())
}

fn retain_regression(h: &Host, case: &str, report: &Value) {
    let destination = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("regression-evidence")
        .join(case);
    fs::create_dir_all(&destination).unwrap();
    fs::write(
        destination.join("report.json"),
        serde_json::to_vec_pretty(report).unwrap(),
    )
    .unwrap();
    for record in report["judges"].as_array().unwrap() {
        let id = record["id"].as_str().unwrap();
        if !record["process"].is_null() {
            let process = chrono_harness::full::expand_process(&record["process"]).unwrap();
            for stream in ["stdout", "stderr"] {
                let bytes: Vec<u8> =
                    serde_json::from_value(process[format!("{stream}_bytes")].clone()).unwrap();
                fs::write(destination.join(format!("{id}.{stream}.bin")), bytes).unwrap();
            }
        }
        let request = h
            .root()
            .join(format!(".chrono-harness/state/{id}.request.json"));
        if request.exists() {
            fs::copy(request, destination.join(format!("{id}.request.json"))).unwrap();
        }
    }
    for name in [
        "tool-version.txt",
        "tool-version.stderr.bin",
        "tool-version.status.json",
    ] {
        let original = h.root().join(".chrono-harness/state").join(name);
        if original.exists() {
            fs::copy(original, destination.join(name)).unwrap();
        }
    }
}

fn assert_rejected_regression_inputs(mode: &str, request: &Value) {
    use std::{io::Write, process::Stdio};
    let mut cases = vec![
        (mode.to_owned(), b"{malformed".to_vec()),
        ("unsupported".into(), serde_json::to_vec(request).unwrap()),
    ];
    for field in ["protocol", "mode", "request_id", "judge_id"] {
        let mut wrong = request.clone();
        wrong[field] = json!("wrong");
        cases.push((mode.into(), serde_json::to_vec(&wrong).unwrap()));
    }
    cases.push((mode.into(), b"{}".to_vec()));
    if mode == "inventory" {
        for field in ["base", "delta"] {
            let mut wrong = request.clone();
            wrong[field] = json!(null);
            cases.push((mode.into(), serde_json::to_vec(&wrong).unwrap()));
        }
        let mut wrong: chrono_harness::initial::Request =
            serde_json::from_value(request.clone()).unwrap();
        wrong.judge_id = "dependent".into();
        wrong.seal().unwrap(); // Valid identity, but the required predecessor is absent.
        cases.push((mode.into(), serde_json::to_vec(&wrong).unwrap()));
        wrong.judge_id = "independent".into();
        wrong
            .prior_results
            .push(wrong.response(chrono_harness::wire::Status::Pass));
        wrong.seal().unwrap();
        cases.push((mode.into(), serde_json::to_vec(&wrong).unwrap()));
    }
    for (mode, bytes) in cases {
        let isolated = tempfile::tempdir().unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_chrono-test-registration-regression"))
            .arg(&mode)
            .current_dir(isolated.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(&bytes).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success(), "{mode} rejected input must fail");
        assert!(output.stdout.is_empty(), "{mode} must not forge a response");
        assert!(!isolated.path().join(".chrono-harness/state").exists());
    }
}

fn install_report_fixture(h: &mut Host, mode: &str, plan: &[(&str, Vec<&str>)]) {
    let path = ".chrono-harness/bin/report-fixture";
    let digest = install_regression_child(h, path);
    h.edit(".chrono-harness/judges.json", |v| {
        v["judges"] = plan.iter().map(|(id, after)| json!({
            "id":id, "after":after, "executable":path, "sha256":digest,
            "version":"configured-version-only", "argv":[mode], "selector":"every-delta", "modes":["evaluate"]
        })).collect();
    });
}

#[test]
fn initial_inventory_reads_original_parents_through_git_replace_overlays() {
    let mut h = initial_host();
    let root = h.candidate.clone();
    git(
        h.root(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "--no-gpg-sign",
            "-qm",
            "same tree child",
        ],
    );
    h.candidate = git(h.root(), &["rev-parse", "HEAD"]);
    git(h.root(), &["replace", &h.candidate, &root]);
    assert!(
        !git(h.root(), &["cat-file", "-p", &h.candidate])
            .lines()
            .take_while(|l| !l.is_empty())
            .any(|l| l.starts_with("parent "))
    );
    let (exit, r) = initial_run(&h);
    assert_ne!(exit, 0, "{r}");
    assert!(r["stderr"].as_str().unwrap().contains("parentless"), "{r}");
    assert!(
        !h.root()
            .join(".chrono-harness/state/initial-report.json")
            .exists()
    );
}

#[test]
fn initial_inventory_uses_only_declared_dag_and_blocks_failed_dependents() {
    let mut h = initial_host();
    let script = ".chrono-harness/inventory-plugin";
    let digest = install_regression_child(&h, script);
    let fm = ".chrono-harness/FILEMAP.json";
    let mut map: Value = serde_json::from_slice(&fs::read(h.root().join(fm)).unwrap()).unwrap();
    map["files"].as_array_mut().unwrap().push(file(script));
    write(h.root(), fm, &map);
    let path = ".chrono-harness/initial.json";
    let mut profile: Value =
        serde_json::from_slice(&fs::read(h.root().join(path)).unwrap()).unwrap();
    for (id, after) in [
        ("dependent", vec!["initial-registration"]),
        ("independent", vec![]),
    ] {
        profile["judges"].as_array_mut().unwrap().push(json!({"id":id,"executable":script,"version":"fixture",
            "sha256":digest,"argv":["inventory"],"selector":"every-initial","after":after,"modes":["inventory"]}));
    }
    write(h.root(), path, &profile);
    amend_initial(&mut h);
    let (exit, r) = initial_run(&h);
    assert_eq!(exit, 0, "{r}");
    let records = r["judges"].as_array().unwrap();
    assert_eq!(records.len(), 3);
    assert!(
        records
            .iter()
            .all(|r| r["state"] == "executed" && r["process"]["exit_code"] == 0)
    );
    retain_regression(&h, "inventory-success", &r);
    for id in ["dependent", "independent"] {
        let request: Value = serde_json::from_slice(
            &fs::read(
                h.root()
                    .join(format!(".chrono-harness/state/{id}.request.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(request.get("base").is_none() && request.get("delta").is_none());
        if id == "dependent" {
            let registration = records
                .iter()
                .find(|record| record["id"] == "initial-registration")
                .unwrap();
            assert_eq!(
                request["prior_results"],
                json!([registration["response"].clone()])
            );
        } else {
            assert_eq!(request["prior_results"], json!([]));
            assert_rejected_regression_inputs("inventory", &request);
        }
    }
    for id in ["dependent", "independent"] {
        let path = h.root().join(format!(".chrono-harness/state/{id}-ran"));
        assert_eq!(fs::read(&path).unwrap(), b"ran");
        fs::remove_file(path).unwrap();
        fs::remove_file(
            h.root()
                .join(format!(".chrono-harness/state/{id}.request.json")),
        )
        .unwrap();
    }
    fs::write(h.root().join("unregistered.txt"), "break first judge").unwrap();
    amend_initial(&mut h);
    mock_registration(&h, &["E_UNREGISTERED"], "fail");
    let (exit, r) = initial_run(&h);
    retain_regression(&h, "inventory-failure", &r);
    assert_ne!(exit, 0, "{r}");
    assert!(finding(&r, "E_UNREGISTERED"), "{r}");
    let records = r["judges"].as_array().unwrap();
    let dependent = records.iter().find(|r| r["id"] == "dependent").unwrap();
    assert_eq!(dependent["state"], "blocked");
    assert_eq!(dependent["blocked_by"], json!(["initial-registration"]));
    assert!(dependent["process"].is_null());
    assert!(
        !h.root()
            .join(".chrono-harness/state/dependent-ran")
            .exists()
    );
    assert_eq!(
        fs::read(h.root().join(".chrono-harness/state/independent-ran")).unwrap(),
        b"ran"
    );
}

#[test]
fn initial_inventory_rejects_real_parents_even_in_shallow_history_and_cannot_activate_delta() {
    let mut h = initial_host();
    h.base = h.candidate.clone();
    fs::write(h.root().join("document.txt"), "later change").unwrap();
    h.candidate = commit(h.root());
    let (exit, r) = initial_run(&h);
    assert_ne!(exit, 0, "{r}");
    assert!(r["stderr"].as_str().unwrap().contains("parentless"), "{r}");
    mock_registration(&h, &["E_ACTIVATION"], "error");
    let (exit, r) = h.run();
    assert_ne!(exit, 0, "{r}");
    assert!(finding(&r, "E_ACTIVATION"), "{r}");
    let dir = tempfile::tempdir().unwrap();
    let clone = dir.path().join("shallow host");
    git(
        dir.path(),
        &[
            "clone",
            "--quiet",
            "--depth",
            "1",
            &format!("file://{}", h.root().display()),
            clone.to_str().unwrap(),
        ],
    );
    assert_eq!(git(&clone, &["rev-list", "--count", "HEAD"]), "1");
    fs::create_dir_all(clone.join(".chrono-harness/bin")).unwrap();
    for binary in ["chrono-harness", "chrono-judge-registration"] {
        fs::copy(
            h.root().join(format!(".chrono-harness/bin/{binary}")),
            clone.join(format!(".chrono-harness/bin/{binary}")),
        )
        .unwrap();
    }
    let o = Command::new(clone.join(".chrono-harness/bin/chrono-harness"))
        .current_dir(&clone)
        .args([
            "check",
            "--config",
            ".chrono-harness/initial.json",
            "--candidate",
            &h.candidate,
            "--initial",
        ])
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("parentless"));
    assert!(
        !clone
            .join(".chrono-harness/state/initial-report.json")
            .exists()
    );
}

#[test]
fn explicit_initial_inventory_runs_candidate_judge_without_a_delta_or_activation_claim() {
    let h = initial_host();
    let (exit, r) = initial_run(&h);
    assert_eq!(exit, 0, "{r}");
    assert_eq!(r["scope"], "initial-inventory");
    assert_eq!(r["status"], "complete");
    assert!(r["base"].is_null() && r["delta"].is_null());
    assert_eq!(r["candidate"], h.candidate);
    assert_eq!(r["governance"], "not-evaluated");
    assert_eq!(r["previous_enforcement"], "none");
    assert_eq!(r["judges"][0]["process"]["exit_code"], 0);
    assert_eq!(
        r["judges"][0]["response"]["outputs"]["inventory"]["scope"],
        "schema-references-checkout"
    );
    let stored: Value = serde_json::from_slice(
        &fs::read(h.root().join(".chrono-harness/state/initial-report.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(r, stored);
    let cfg: Value =
        serde_json::from_slice(&fs::read(h.root().join(".chrono-harness/config.json")).unwrap())
            .unwrap();
    assert_eq!(cfg["status"], "proposed");
    assert_eq!(cfg["input_closure"]["status"], "incomplete");
}

#[test]
fn selected_initial_inventory_keeps_entry_identity_and_runs_the_effective_policy() {
    let mut h = initial_host();
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let selector = ".chrono-harness/platforms.json";
    let git_image = chrono_harness::resolve_program(h.root(), "git", None).unwrap();
    let git_path = git_image.to_str().unwrap();
    let git_version = String::from_utf8(
        Command::new(git_path)
            .arg("--version")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim_end()
    .to_string();
    let mut config: Value =
        serde_json::from_slice(&fs::read(h.root().join(".chrono-harness/config.json")).unwrap())
            .unwrap();
    config["schema_version"] = 3.into();
    config["facts_git"] = json!({"tool":"git","input":"git-binary"});
    config["tools"] = json!([{"id":"git","program":git_path,"resolution":"PATH-once","version_argv":["--version"],"expected_version":git_version}]);
    config["environment"]["inputs"] = json!([{"id":"git-binary","location":git_path,"presence":"present","sha256":sha256(&fs::read(git_path).unwrap())}]);
    config["canonical_check"]["argv"][3] = selector.into();
    write(h.root(), ".chrono-harness/config.json", &config);
    write(
        h.root(),
        selector,
        &json!({"schema":"chrono-git-configs/v1","platforms":{platform: ".chrono-harness/config.json"}}),
    );
    let profile_path = ".chrono-harness/initial.json";
    let mut profile: Value =
        serde_json::from_slice(&fs::read(h.root().join(profile_path)).unwrap()).unwrap();
    profile["host_config"] = selector.into();
    write(h.root(), profile_path, &profile);
    let filemap_path = ".chrono-harness/FILEMAP.json";
    let mut filemap: Value =
        serde_json::from_slice(&fs::read(h.root().join(filemap_path)).unwrap()).unwrap();
    filemap["files"]
        .as_array_mut()
        .unwrap()
        .push(file(selector));
    write(h.root(), filemap_path, &filemap);
    amend_initial(&mut h);

    let (exit, report) = initial_run(&h);
    assert_eq!(exit, 0, "{report}");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["git_facts"]["selection"]["path"], selector);
    assert_eq!(
        report["git_facts"]["selection"]["config_path"],
        ".chrono-harness/config.json"
    );
    assert_eq!(report["judges"][0]["response"]["status"], "pass");
}

#[test]
fn full_report_required_fields_and_unknowns_for_bounded_host() {
    let h = Host::new();
    let (exit, report) = h.run();
    assert_eq!(exit, 0, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["status"], "pass");
    assert_eq!(report["findings"], json!([]));
    assert_eq!(
        report["judges"][0]["response"]["outputs"]["registration"]["scope"],
        "registration-only"
    );
    for field in ["tools", "effective_inputs", "impact", "tests", "costs"] {
        assert_unresolved(&report, field);
    }
}

#[test]
fn full_report_executable_list_distinguishes_observed_and_configured() {
    let h = Host::new();
    let (exit, report) = h.run();
    assert_eq!(exit, 0, "{report:#}");
    let executables = report["executables"].as_array().unwrap_or_else(|| {
        panic!(
            "SPEC9 executables must be a list; base={}, candidate={}; actual={}",
            h.base, h.candidate, report["executables"]
        )
    });
    assert_eq!(executables.len(), 2);
    for (entry, binary) in executables
        .iter()
        .zip(["chrono-harness", "chrono-judge-registration"])
    {
        assert_eq!(entry.as_object().unwrap().len(), 3);
        assert_eq!(
            fs::canonicalize(entry["path"].as_str().unwrap()).unwrap(),
            fs::canonicalize(h.root().join(format!(".chrono-harness/bin/{binary}"))).unwrap()
        );
        assert_eq!(
            entry["sha256"],
            sha256(&fs::read(entry["path"].as_str().unwrap()).unwrap())
        );
    }
    assert_eq!(
        executables[0]["version"],
        fixture_registries()["config"]["runner"]["version"]
    );
    assert!(executables[1]["version"].is_null());
    assert!(
        report["unresolved"]["/executables/1/version"]
            .as_str()
            .unwrap()
            .contains("configured")
    );
    assert_eq!(report["judges"][0]["binding"]["version"], "0.1.0");
    assert_eq!(report["judges"][0]["executable_index"], 1);
}

#[test]
fn full_report_preserves_failure_findings_and_incomplete_status() {
    let mut h = Host::new();
    fs::write(h.root().join("new.txt"), "unregistered").unwrap();
    h.candidate = commit(h.root());
    mock_registration(&h, &["E_UNREGISTERED"], "fail");
    let (exit, report) = h.run();
    assert_eq!(exit, 1, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["status"], "fail");
    assert_eq!(
        report["findings"],
        report["judges"][0]["response"]["findings"]
    );
    assert!(
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["code"] == "E_UNREGISTERED")
    );
    assert_eq!(
        report["sources"]["/findings/0"],
        json!(["/judges/0/response/findings/0"])
    );
    h.edit(".chrono-harness/config.json", |v| {
        v["status"] = "proposed".into();
        v["input_closure"]["status"] = "incomplete".into();
    });
    mock_registration(&h, &["E_ACTIVATION", "E_EVIDENCE_UNRESOLVED"], "error");
    let (exit, report) = h.run();
    assert_eq!(exit, 2, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["status"], "error");
    assert_eq!(
        report["findings"],
        report["judges"][0]["response"]["findings"]
    );
    assert!(finding(&report, "E_ACTIVATION"));
    assert!(finding(&report, "E_EVIDENCE_UNRESOLVED"));
    for field in ["tools", "effective_inputs", "impact", "tests", "costs"] {
        assert_unresolved(&report, field);
    }
}

#[test]
fn full_report_forwards_real_named_outputs_and_warn_without_inventing_observations() {
    let mut h = Host::new();
    install_report_fixture(&mut h, "named", &[("observations", vec![])]);
    let (exit, report) = h.run();
    retain_regression(&h, "named-outputs", &report);
    let request: Value = serde_json::from_slice(
        &fs::read(
            h.root()
                .join(".chrono-harness/state/observations.request.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_rejected_regression_inputs("named", &request);
    assert_eq!(report["effective_inputs"]["context"], request["context"]);
    assert_eq!(
        report["judges"][0]["process"]["stdin_sha256"],
        sha256(
            &fs::read(
                h.root()
                    .join(".chrono-harness/state/observations.request.json")
            )
            .unwrap()
        )
    );
    assert_eq!(
        report["impact"]["seeds"],
        json!(
            report["delta"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| format!("file:{}", d["path"].as_str().unwrap()))
                .collect::<Vec<_>>()
        )
    );
    let version_status: Value = serde_json::from_slice(
        &fs::read(
            h.root()
                .join(".chrono-harness/state/tool-version.status.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(report["tests"][0]["exit_code"], version_status["exit_code"]);
    assert_eq!(exit, 0, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["status"], "warn");
    assert_eq!(report["judges"][0]["exit_code"], 0);
    for field in ["tools", "effective_inputs", "impact", "tests", "costs"] {
        assert_eq!(
            report[field],
            report["judges"][0]["response"]["outputs"][field]
        );
        assert_eq!(
            report["sources"][format!("/{field}")],
            json!([format!("/judges/0/response/outputs/{field}")])
        );
        assert!(report["unresolved"].get(format!("/{field}")).is_none());
    }
    assert_eq!(report["tests"][0]["state"], "executed");
    assert_eq!(report["tests"][0]["exit_code"], 0);
    assert_eq!(
        report["tools"][0]["path"],
        fs::canonicalize(h.root().join(".chrono-harness/bin/report-fixture"))
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(
        report["tools"][0]["version"],
        format!(
            "chrono-test-registration-regression {}",
            env!("CARGO_PKG_VERSION")
        )
    );
    assert!(
        fs::read(
            h.root()
                .join(".chrono-harness/state/tool-version.stderr.bin")
        )
        .unwrap()
        .is_empty()
    );
    let evidence = &report["tests"][0]["evidence"][0];
    let bytes = fs::read(h.root().join(evidence["path"].as_str().unwrap())).unwrap();
    assert_eq!(evidence["sha256"], sha256(&bytes));
    assert_eq!(
        report["tools"][0]["version"],
        String::from_utf8(bytes).unwrap().trim()
    );
    assert_eq!(
        report["effective_inputs"]["context"]["sha256"],
        report["context_digest"]
    );
    assert!(report["costs"]["measured"].is_null());
    assert_eq!(report["findings"][0]["code"], "W_FIXTURE_COST");
    assert_eq!(
        report["judges"][0]["binding"]["version"],
        "configured-version-only"
    );
    assert!(report["executables"][1]["version"].is_null());
    assert_eq!(
        report["judges"][0]["response"]["outputs"]["fixture_extra"]["retained"],
        true
    );
}

#[test]
fn full_report_missing_responses_and_conflicting_outputs_remain_unresolved() {
    let mut h = Host::new();
    install_report_fixture(
        &mut h,
        "broken",
        &[("broken", vec![]), ("dependent", vec!["broken"])],
    );
    let (exit, report) = h.run();
    retain_regression(&h, "missing-response", &report);
    let original = chrono_harness::full::expand_process(&report["judges"][0]["process"]).unwrap();
    let original_bytes: Vec<u8> = serde_json::from_value(original["stdout_bytes"].clone()).unwrap();
    assert_eq!(original_bytes, b"invalid JSON\n");
    let request: Value = serde_json::from_slice(
        &fs::read(h.root().join(".chrono-harness/state/broken.request.json")).unwrap(),
    )
    .unwrap();
    assert_rejected_regression_inputs("broken", &request);
    assert_eq!(exit, 2, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["status"], "error");
    assert_unresolved(&report, "findings");
    assert_eq!(report["judges"][0]["exit_code"], 9);
    assert_eq!(report["judges"][1]["state"], "blocked");
    assert!(report["judges"][1]["executable_index"].is_null());
    assert_eq!(report["executables"].as_array().unwrap().len(), 2);
    for field in ["tools", "effective_inputs", "impact", "tests", "costs"] {
        assert_unresolved(&report, field);
    }
    // A rejected binding is metadata, never an observed executable.
    h.edit(".chrono-harness/judges.json", |v| {
        v["judges"][0]["sha256"] = "0".repeat(64).into()
    });
    let (exit, report) = h.run();
    retain_regression(&h, "rejected-digest", &report);
    assert!(report["judges"][0]["process"].is_null());
    assert_eq!(exit, 2, "{report:#}");
    assert_report_contract(&h, &report);
    assert_eq!(report["executables"].as_array().unwrap().len(), 1);
    assert!(
        report["judges"][0]["transport_failure"]
            .as_str()
            .unwrap()
            .contains("digest mismatch")
    );
    assert_unresolved(&report, "findings");
    // Two independent producers cannot be silently resolved by traversal order.
    install_report_fixture(&mut h, "conflict", &[("first", vec![]), ("second", vec![])]);
    let (exit, report) = h.run();
    retain_regression(&h, "conflicting-outputs", &report);
    let request: Value = serde_json::from_slice(
        &fs::read(h.root().join(".chrono-harness/state/first.request.json")).unwrap(),
    )
    .unwrap();
    assert_rejected_regression_inputs("conflict", &request);
    assert!(
        report["judges"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["process"]["exit_code"] == 0 && r["response"]["status"] == "pass")
    );
    for field in ["impact", "costs"] {
        assert_eq!(
            report["sources"][format!("/{field}")],
            json!([
                format!("/judges/0/response/outputs/{field}"),
                format!("/judges/1/response/outputs/{field}")
            ])
        );
    }
    assert_eq!(exit, 0, "configured statuses preserved: {report:#}");
    assert_report_contract(&h, &report);
    assert_unresolved(&report, "impact");
    assert!(
        report["unresolved"]["/impact"]
            .as_str()
            .unwrap()
            .contains("conflicting")
    );
    assert_unresolved(&report, "tests");
    assert_eq!(report["sources"]["/impact"].as_array().unwrap().len(), 2);
    assert_eq!(report["costs"], json!({"unknown":["fixture"]}));
    assert_eq!(report["sources"]["/costs"].as_array().unwrap().len(), 2);
}

#[test]
fn missing_oid_and_symbolic_endpoint_are_errors() {
    let h = Host::new();
    assert!(facts::verify_oid(h.root(), "HEAD").is_err());
    assert!(facts::verify_oid(h.root(), &"a".repeat(40)).is_err());
}

#[path = "registered_git.rs"]
mod registered_git;

#[test]
fn full_report_represents_a_rename_as_delete_add_and_equal_endpoints_as_an_empty_delta() {
    let mut h = Host::new();
    fs::rename(h.root().join("document.txt"), h.root().join("renamed.txt")).unwrap();
    h.edit(".chrono-harness/FILEMAP.json", |v| {
        let rows = v["files"].as_array_mut().unwrap();
        rows.retain(|f| f["path"] != "document.txt");
        rows.push(file("renamed.txt"));
    });
    let (exit, r) = h.run();
    assert_eq!(exit, 0, "{r:#}");
    assert!(
        r["delta"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "D" && d["path"] == "document.txt")
    );
    assert!(
        r["delta"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["kind"] == "A" && d["path"] == "renamed.txt")
    );
    h.base = h.candidate.clone();
    assert_eq!(h.run().1["delta"], json!([]));
}

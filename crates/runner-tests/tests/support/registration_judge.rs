use chrono_harness::{facts, initial, sha256, wire};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    process::{Command, ExitCode},
};

fn observe_checkout(
    reader: &facts::Reader,
    candidate: &wire::Endpoint,
    config_path: &str,
    response: &mut wire::Response,
    initial: bool,
) -> Result<(), String> {
    let root = &candidate.root;
    reader.git(root, &["rev-parse", "--show-toplevel"])?;
    reader.tree(root, &candidate.commit)?;
    let snapshot = reader.registry_snapshot(root, &candidate.commit, config_path)?;
    let config = &snapshot.values[&snapshot.effective_path];
    let checkout = reader.checkout_excluding(
        root,
        &candidate.commit,
        &facts::artifact_directories(config)?,
    )?;
    if !checkout.tracked.is_empty() || !checkout.untracked.is_empty() {
        response.status = wire::Status::Error;
        response.findings.push(wire::Finding {
            code: "E_SNAPSHOT_DIRTY".into(),
            level: "error".into(),
            message: if initial {
                "initial candidate checkout is dirty or changed"
            } else {
                "dirty fixture checkout"
            }
            .into(),
            causes: vec![],
            delta_refs: vec!["/checkout".into()],
        });
    }
    response
        .outputs
        .insert("git_facts".into(), reader.observation());
    Ok(())
}

fn run(mode: &str, bytes: &[u8]) -> Result<u8, String> {
    let value = chrono_harness::json(bytes)?;
    if matches!(mode, "chrono-judge/v1" | "chrono-initial-judge/v1") {
        let setup = match fs::read(".chrono-harness/state/mock-registration.json") {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                json!({"status":"pass","codes":[]})
            }
            Err(error) => return Err(error.to_string()),
        };
        let status: wire::Status =
            serde_json::from_value(setup["status"].clone()).map_err(|e| e.to_string())?;
        let mut response = if mode == initial::PROTOCOL {
            let request: initial::Request =
                serde_json::from_value(value).map_err(|e| e.to_string())?;
            request.validate()?;
            let reader = chrono_harness::facts::Reader::from_observations(
                &request.candidate.root,
                &request.config_path,
                &request.candidate.commit,
                &request.observations,
            )?;
            let mut response = request.response(status);
            if reader.is_bound() {
                observe_checkout(
                    &reader,
                    &request.candidate,
                    &request.config_path,
                    &mut response,
                    true,
                )?;
            }

            response.outputs.insert(
                "inventory".into(),
                json!({"scope":"schema-references-checkout"}),
            );
            response
        } else {
            let request: wire::Request =
                serde_json::from_value(value).map_err(|e| e.to_string())?;
            request.validate()?;
            let reader = chrono_harness::facts::Reader::for_request(&request)?;
            let mut response = request.response(status);
            if reader.is_bound() {
                reader.tree(&request.candidate.root, &request.base.commit)?;
                observe_checkout(
                    &reader,
                    &request.candidate,
                    &request.config_path,
                    &mut response,
                    false,
                )?;
            }

            response
                .outputs
                .insert("registration".into(), json!({"scope":"registration-only"}));
            response
        };
        for code in setup["codes"]
            .as_array()
            .ok_or("mock finding codes missing")?
        {
            response.findings.push(serde_json::from_value(json!({
                "code":code,"level":"error","message":"fixture verdict","causes":[],"delta_refs":["/fixture"]
            })).map_err(|e| e.to_string())?);
        }
        io::stdout()
            .write_all(&wire::canonical(&response)?)
            .map_err(|e| e.to_string())?;
        return Ok(response.status.exit_code() as u8);
    }
    if mode == "inventory" {
        // These are independent consumer expectations, not defaults for missing input.
        assert!(value.get("base").is_none() && value.get("delta").is_none());
        let request: initial::Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
        request.validate()?;
        match request.judge_id.as_str() {
            "dependent" => {
                assert_eq!(request.prior_results.len(), 1);
                let prior = serde_json::to_value(&request.prior_results[0]).unwrap();
                assert_eq!(prior["judge_id"], "initial-registration");
                assert_eq!(prior["status"], "pass");
                assert_eq!(
                    prior["outputs"]["inventory"]["scope"],
                    "schema-references-checkout"
                );
                let original = request.observations["judges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|record| record["id"] == "initial-registration")
                    .unwrap();
                assert_eq!(prior, original["response"]);
            }
            "independent" => assert!(request.prior_results.is_empty()),
            _ => return Err("unsupported inventory consumer".into()),
        }
        fs::create_dir_all(".chrono-harness/state").map_err(|e| e.to_string())?;
        fs::write(
            format!(".chrono-harness/state/{}.request.json", request.judge_id),
            bytes,
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            format!(".chrono-harness/state/{}-ran", request.judge_id),
            b"ran",
        )
        .map_err(|e| e.to_string())?;
        let mut response = request.response(wire::Status::Pass);
        response.outputs.insert("plugin".into(), json!(true));
        serde_json::to_writer(io::stdout().lock(), &response).map_err(|e| e.to_string())?;
        return Ok(0);
    }
    if !["named", "broken", "conflict"].contains(&mode) {
        return Err("unsupported regression mode".into());
    }
    let request: wire::Request = serde_json::from_value(value).map_err(|e| e.to_string())?;
    request.validate()?;
    fs::create_dir_all(".chrono-harness/state").map_err(|e| e.to_string())?;
    // Separate original requests are retained even when producers disagree.
    let name = match (mode, request.judge_id.as_str()) {
        ("named", "observations") => "observations",
        ("broken", "broken" | "dependent") => "broken",
        ("conflict", "first") => "first",
        ("conflict", "second") => "second",
        _ => return Err("unsupported report consumer".into()),
    };
    fs::write(format!(".chrono-harness/state/{name}.request.json"), bytes)
        .map_err(|e| e.to_string())?;
    if mode == "broken" {
        io::stdout()
            .write_all(b"invalid JSON\n")
            .map_err(|e| e.to_string())?;
        return Ok(9);
    }
    let mut response = request.response(wire::Status::Pass);
    let outputs = if mode == "conflict" {
        json!({"impact":{"seeds":[request.judge_id]},"tests":null,"costs":{"unknown":["fixture"]}})
    } else {
        // Observe the actual fixture executable without a host interpreter dependency.
        let tool = std::env::current_exe().map_err(|e| e.to_string())?;
        let child = Command::new(&tool)
            .arg("--version")
            .output()
            .map_err(|e| e.to_string())?;
        let exit = child
            .status
            .code()
            .ok_or("version child terminated without exit code")?;
        let path = ".chrono-harness/state/tool-version.txt";
        fs::write(path, &child.stdout).map_err(|e| e.to_string())?;
        fs::write(
            ".chrono-harness/state/tool-version.stderr.bin",
            &child.stderr,
        )
        .map_err(|e| e.to_string())?;
        fs::write(
            ".chrono-harness/state/tool-version.status.json",
            serde_json::to_vec(&json!({"exit_code":exit})).unwrap(),
        )
        .map_err(|e| e.to_string())?;
        let evidence =
            json!({"path":path,"sha256":sha256(&child.stdout),"kind":"fixture-tool-version"});
        let version = String::from_utf8(child.stdout).map_err(|e| e.to_string())?;
        response.status = wire::Status::Warn;
        response.findings = serde_json::from_value(json!([{"code":"W_FIXTURE_COST","level":"warning","message":"fixture costs unmeasured","delta_refs":["/costs"],"causes":[]}])).unwrap();
        response.evidence = serde_json::from_value(json!([evidence.clone()])).unwrap();
        json!({
            "tools":[{"path":tool,"version":version.trim(),"evidence":[evidence.clone()]}],
            "effective_inputs":{"scope":"fixture-context-only","context":request.context},
            "impact":{"seeds":request.delta.iter().map(|d|format!("file:{}",d.path)).collect::<Vec<_>>(),"edges":[],"tests":[],"retired_tests":[]},
            "tests":[{"id":"fixture-version-command","state":"executed","exit_code":exit,"evidence":[evidence]}],
            "costs":{"declared_before":null,"declared_after":null,"affected_tests":["fixture-version-command"],"retired_tests":[],"unknown":["fixture-version-command"],"measured":null},
            "fixture_extra":{"retained":true}
        })
    };
    response.outputs =
        serde_json::from_value::<std::collections::BTreeMap<String, Value>>(outputs).unwrap();
    serde_json::to_writer(io::stdout().lock(), &response).map_err(|e| e.to_string())?;
    Ok(0)
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() == ["--version"] {
        println!(
            "chrono-test-registration-regression {}",
            env!("CARGO_PKG_VERSION")
        );
        return ExitCode::SUCCESS;
    }
    if !(args.len() == 1 || (args.len() == 2 && args[0] == "--protocol")) {
        eprintln!("expected one regression mode");
        return ExitCode::from(2);
    }
    let mut bytes = Vec::new();
    let result = io::stdin()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())
        .and_then(|_| run(args.last().unwrap(), &bytes));
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

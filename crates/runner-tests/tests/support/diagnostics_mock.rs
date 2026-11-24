//! Protocol double for the runner's diagnostics subprocess. Only scripted replies and caller IO are exercised here.
use serde_json::{Value, json};
use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::Path,
    process::ExitCode,
};

fn receive(input: &mut impl BufRead) -> Value {
    let mut line = String::new();
    assert_ne!(input.read_line(&mut line).unwrap(), 0);
    serde_json::from_str(&line).unwrap()
}
fn send(value: Value) {
    let mut out = io::stdout().lock();
    serde_json::to_writer(&mut out, &value).unwrap();
    out.write_all(b"\n").unwrap();
    out.flush().unwrap()
}
fn cli(root: &Path, name: &str, operation: &str, pid: &str, input: &mut impl BufRead) -> u8 {
    let fixture = root.join("mock-diagnostics.json");
    let spec = fs::read(&fixture)
        .ok()
        .map(|b| serde_json::from_slice::<Value>(&b).unwrap())
        .unwrap_or(Value::Null);
    let record = |v: Value| {
        if spec["record"] == true {
            let mut f = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(root.join("mock-diagnostics.jsonl"))
                .unwrap();
            writeln!(&mut f, "{v}").unwrap();
        }
    };
    record(json!({"start":{"name":name,"operation":operation,"pid":pid.parse::<u32>().unwrap()}}));
    if spec["reject"] == true {
        eprintln!("mock policy rejected");
        return 2;
    }
    send(json!({"ready":true}));
    loop {
        let r = receive(input);
        record(r.clone());
        if let Some(a) = r["invalid_argument"].as_object() {
            let i = a["argv_index"].as_u64().unwrap();
            eprintln!(
                "{}",
                json!({"schema":"chrono-log/v1","producer":name,"event":"cli.invalid_argument","error":{"schema":"chrono-error/v1","error":{"code":"E_USAGE","source":null,"context":{"argv_index":i,"argument_index":i.checked_sub(1),"argument_bytes":a["bytes"]}}}})
            );
        }
        if let Some(exit) = r["exit_code"].as_u64() {
            if spec["finish_failure"] == true {
                eprintln!("mock cannot finish");
                return 2;
            }
            return exit as u8;
        }
    }
}
fn writer_error(phase: &str, attempt: Option<&[u8]>, ack: &Value) -> Value {
    let e = &ack["error"];
    let mut context = json!({"phase":phase});
    if let Some(bytes) = attempt {
        context["attempted_record"] = json!(bytes);
    }
    json!({"schema":"chrono-error/v1","error":{"type":"fixture","code":"E_LOG_IO","message":"writer failed","context":context,"source":{"type":"std::io::Error","code":format!("io::{}",e["kind"].as_str().unwrap()),"context":{},"message":e["message"],"native_code":e["native_code"],"source":null}}})
}
fn trace(input: &mut impl BufRead) -> u8 {
    send(json!("ready"));
    let mut errors = vec![];
    loop {
        let request = receive(input);
        if request == "finish" {
            if errors.is_empty() {
                send(json!("flush"));
                let ack = receive(input);
                if ack != "ok" {
                    errors.push(writer_error("flush", None, &ack));
                }
            }
            let status = if errors.is_empty() { 0 } else { 2 };
            send(json!({"finished":errors}));
            return status;
        }
        if let Some(fields) = request.pointer("/event/new_event/values") {
            let text = |k: &str| fields[k]["string"].as_str().unwrap();
            let original: Value = serde_json::from_str(text("chrono_error")).unwrap();
            #[derive(serde::Serialize)]
            struct Log<'a> {
                schema: &'a str,
                producer: &'a str,
                event: &'a str,
                error: Value,
            }
            let event = Log {
                schema: "chrono-log/v1",
                producer: "chrono-harness",
                event: text("event_id"),
                error: original,
            };
            let row = format!("{}\n", serde_json::to_string(&event).unwrap());
            send(json!({"write":row}));
            let ack = receive(input);
            if ack.pointer("/error/message").and_then(Value::as_str)
                == Some("observed before session disconnect")
            {
                return 2;
            }
            if ack != "ok" {
                errors.push(writer_error("write", Some(row.as_bytes()), &ack));
            } else {
                send(json!("flush"));
                let ack = receive(input);
                if ack != "ok" {
                    errors.push(writer_error("flush", Some(row.as_bytes()), &ack));
                }
            }
        }
        send(json!("accepted"));
    }
}
fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut input = io::stdin().lock();
    let code = match args.as_slice() {
        [mode, root, producer, op, pid] if mode == "cli-events-v1" => {
            cli(Path::new(root), producer, op, pid, &mut input)
        }
        [mode, sink, producer]
            if mode == "trace-events-v1"
                && sink == "--sink-only"
                && producer == "chrono-harness" =>
        {
            trace(&mut input)
        }
        _ => panic!("unregistered mock call {args:?}"),
    };
    ExitCode::from(code)
}

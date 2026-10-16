#![cfg(not(target_arch = "wasm32"))]
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn shutdown_answers_every_request_even_if_tasks_never_poll() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_candle-cache-owner"))
        .env_remove("REDIS_URL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    for id in 0..20 {
        let message = serde_json::json!({"kind":"fetch","id":id,"request":{"cacheKey":format!("cancel-{id}"),"interval":"1m","fromMs":1700000000000_u64,"toMs":1700000000000_u64,"limit":1,"nowMs":1700000120000_u64}});
        writeln!(input, "{message}").unwrap();
    }
    writeln!(input, "{{\"kind\":\"shutdown\"}}").unwrap();
    drop(input);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .filter(|value: &serde_json::Value| value["kind"] == "result")
        .collect();
    assert_eq!(replies.len(), 20);
    let ids: std::collections::BTreeSet<_> = replies
        .iter()
        .map(|reply| reply["id"].as_u64().unwrap())
        .collect();
    assert_eq!(ids, (0..20).collect());
}

#[test]
fn empty_redis_url_starts_memory_owner_like_bun() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_candle-cache-owner"))
        .env("REDIS_URL", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "{{\"kind\":\"shutdown\"}}").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ready: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(ready["kind"], "ready");
}

#[test]
fn owner_rejects_positional_request_arrays() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_candle-cache-owner"))
        .env_remove("REDIS_URL")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        "{}",
        serde_json::json!({"kind":"fetch","id":1,
        "request":["k","1m",0,0,1,null,null]})
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("request must be an object"));
}

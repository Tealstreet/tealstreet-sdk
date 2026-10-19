use serde_json::Value;

#[test]
fn source_buffered_dispatch_pins_snapshot_and_error_boundaries() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_buffer_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 60);
}

use safe_cex_webworker::{
    payload::{StoreWorkerOp, StoreWorkerPayload},
    worker_buffer::flush_buffered_messages,
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

struct Failure(&'static str);
fn payload(index: usize) -> StoreWorkerPayload {
    StoreWorkerPayload {
        op: StoreWorkerOp::WorkerPing,
        instance_id: None,
        data: Some(json!({"i":index})),
    }
}
fn native(count: usize, mode: &str) -> Value {
    let original = RefCell::new((0..count).map(payload).collect::<Vec<_>>());
    let buffer = RefCell::new(original.borrow().clone());
    let events = RefCell::new(Vec::new());
    let failure = Rc::new(Failure("handler failure"));
    let result = flush_buffered_messages(
        &buffer,
        |message, extra| {
            events
                .borrow_mut()
                .push(json!({"log":{"level":"info","message":message,"extra":extra}}));
            match mode {
                "log-throw" => return Err(Rc::new(Failure("log failure"))),
                "log-append" => buffer.borrow_mut().push(payload(99)),
                "log-clear" => buffer.borrow_mut().clear(),
                _ => {}
            }
            Ok(())
        },
        |item| {
            let index = item.data.as_ref().unwrap()["i"].as_u64().unwrap() as usize;
            events.borrow_mut().push(json!({"handle":index}));
            if mode == "handle-append" {
                buffer.borrow_mut().push(payload(100 + index));
            }
            if index == 0
                && [
                    "handle-throw",
                    "mutate-throw",
                    "string-throw",
                    "string-mutate",
                    "report-throw",
                ]
                .contains(&mode)
            {
                if mode == "mutate-throw" {
                    item.op = StoreWorkerOp::Log;
                }
                return Err(failure.clone());
            }
            Ok(())
        },
        |error| {
            assert!(
                Rc::ptr_eq(error, &failure),
                "BUFFER coercion error identity"
            );
            events.borrow_mut().push(json!({"stringify":true}));
            if mode == "string-mutate" {
                original.borrow_mut()[0].op = StoreWorkerOp::Error;
            }
            if mode == "string-throw" {
                return Err(Rc::new(Failure("string failure")));
            }
            Ok("TypeError: handler failed".into())
        },
        |message, error, payload, index, total| {
            let extra = json!({"payloadOp":payload.op,"messageIndex":index,"totalBuffered":total});
            assert!(Rc::ptr_eq(&error, &failure), "BUFFER report error identity");
            events
                .borrow_mut()
                .push(json!({"report":{"message":message,"extra":extra}}));
            if mode == "report-throw" {
                return Err(Rc::new(Failure("report failure")));
            }
            Ok(())
        },
    );
    json!({"events":events.into_inner(),"pending":buffer.into_inner(),"original":original.into_inner(),"error":result.err().map(|e|e.0)})
}

#[test]
fn buffered_dispatch_matches_source_order_and_failure_propagation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_buffer_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for row in source["rows"].as_array().unwrap() {
        let result = native(
            row["count"].as_u64().unwrap() as usize,
            row["mode"].as_str().unwrap(),
        );
        let mut expected = row.clone();
        if row["mode"] == "string-mutate" && row["count"].as_u64().unwrap() > 0 {
            let source_report = expected["events"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|event| event.get("report").is_some())
                .unwrap();
            assert_eq!(
                source_report["report"]["extra"]["payloadOp"], "error",
                "BUFFER source coercion alias changes report"
            );
            let native_report = result["events"]
                .as_array()
                .unwrap()
                .iter()
                .find(|event| event.get("report").is_some())
                .unwrap();
            assert_eq!(
                native_report["report"]["extra"]["payloadOp"], "__worker_ping__",
                "CHOSEN BUFFER owned in-flight payload ignores caller alias mutation during coercion"
            );
            assert_eq!(
                result["original"][0]["op"], "error",
                "BUFFER native caller clone did change during coercion"
            );
            source_report["report"]["extra"]["payloadOp"] = json!("__worker_ping__");
        }
        for field in ["events", "pending", "error"] {
            assert_eq!(
                result[field], expected[field],
                "BUFFER dispatch {} {} {field}",
                row["count"], row["mode"]
            );
        }
        if row["mode"] == "mutate-throw" && row["count"].as_u64().unwrap() > 0 {
            assert_eq!(
                row["original"][0]["op"], "log",
                "BUFFER source caller alias observes handler mutation"
            );
            assert_eq!(
                result["original"][0]["op"], "__worker_ping__",
                "CHOSEN BUFFER caller owns an independent payload snapshot"
            );
        } else {
            assert_eq!(result["original"], row["original"]);
        }
    }
}

#[test]
fn buffered_payload_numbers_and_error_strings_keep_their_boundaries() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_buffer_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for row in source["numeric"].as_array().unwrap() {
        let mut item = payload(0);
        item.data = Some(
            serde_json::from_str(&format!("{{\"value\":{}}}", row["token"].as_str().unwrap()))
                .unwrap(),
        );
        let buffer = RefCell::new(vec![item]);
        let mut observed = None;
        flush_buffered_messages::<(), _>(
            &buffer,
            |_, _| Ok(()),
            |item| {
                observed = Some(
                    safe_cex_num::numeric_ingress::js_value_as_f64(
                        &item.data.as_ref().unwrap()["value"],
                    )
                    .unwrap()
                    .to_bits(),
                );
                Ok(())
            },
            |_| unreachable!(),
            |_, _, _, _, _| unreachable!(),
        )
        .unwrap();
        assert_eq!(
            format!("{:016x}", observed.unwrap()),
            row["bits"].as_str().unwrap(),
            "BUFFER native numeric bits {}",
            row["token"]
        );
    }
    for row in source["unicode"].as_array().unwrap() {
        let buffer = RefCell::new(vec![payload(0)]);
        let mut message = String::new();
        flush_buffered_messages(
            &buffer,
            |_, _| Ok(()),
            |_| Err(()),
            |_| Ok(row["text"].as_str().unwrap().to_owned()),
            |text, _, _, _, _| {
                message = text.into();
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            message,
            row["message"].as_str().unwrap(),
            "BUFFER scalar error text"
        );
    }
    assert!(
        source["surrogate"]["message"]
            .as_str()
            .unwrap()
            .contains(r"\ud800")
    );
    assert!(
        serde_json::from_str::<String>(source["surrogate"]["input"].as_str().unwrap()).is_err(),
        "CHOSEN BUFFER shared UTF-8 ingress refuses source lone surrogate"
    );
}

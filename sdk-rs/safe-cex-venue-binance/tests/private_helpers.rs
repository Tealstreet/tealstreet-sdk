use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::private_helpers::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeSet,
    rc::Rc,
};
type Failure = Rc<ExchangeError>;
fn oracle() -> Vec<Value> {
    let r = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-helpers.mjs"
        ))
        .output()
        .unwrap();
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&r.stdout).unwrap();
    assert_eq!(rows.len(), 32, "fixed authored detach oracle floor 2*16");
    rows
}
fn error() -> Failure {
    let mut e = ExchangeError::new("original helper failure");
    e.name = "RangeError".into();
    e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    Rc::new(e)
}
fn original(result: &Result<(), Failure>, failure: bool, e: &Failure) {
    match result {
        Ok(()) => assert!(!failure, "expected original failure"),
        Err(r) => {
            assert!(failure);
            assert!(Rc::ptr_eq(r, e), "native original error identity");
        }
    }
}
#[derive(Default)]
struct Socket {
    closed: Cell<bool>,
    ready: Cell<f64>,
}
fn detachment(variant: &str) {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|c| c["variant"] == variant && c["method"] == "detachSocket")
    {
        let name = case["name"].as_str().unwrap();
        let initial = match name {
            "open" | "listeners-absent" | "swapped-owner" | "late-state-change" => 1.0,
            "closing" | "changed-ready" => 2.0,
            "closed" => 3.0,
            _ => 0.0,
        };
        let socket = Rc::new(Socket {
            ready: Cell::new(initial),
            ..Default::default()
        });
        let e = error();
        let owner = Cell::new(if name == "missing" { None } else { Some(100) });
        let gets = Cell::new(0);
        let removed = RefCell::new(vec![]);
        let remaining = RefCell::new(BTreeSet::from(["message", "close", "open"]));
        let reads = Cell::new(0);
        let closes = Cell::new(0);
        let close_handler = Cell::new(202);
        let get = || {
            gets.set(gets.get() + 1);
            Ok(if name == "missing" {
                None
            } else {
                Some(socket.clone())
            })
        };
        let remove = |arg: &Rc<Socket>, event| {
            assert!(
                Rc::ptr_eq(arg, &socket),
                "native captured socket for removal"
            );
            if name == "listeners-absent" {
                return Ok(());
            }
            let id = match event {
                "message" => 101,
                "close" => close_handler.get(),
                _ => 303,
            };
            removed.borrow_mut().push((event, id));
            if name == format!("remove{}-fail", removed.borrow().len()) {
                return Err(e.clone());
            }
            remaining.borrow_mut().remove(event);
            if removed.borrow().len() == 1 {
                if name == "swapped-owner" {
                    owner.set(Some(404));
                }
                if name == "handler-change" {
                    close_handler.set(999);
                }
                if name == "late-state-change" {
                    arg.ready.set(3.0);
                }
            }
            Ok(())
        };
        let ready = |arg: &Rc<Socket>| {
            assert!(
                Rc::ptr_eq(arg, &socket),
                "native captured socket for ready state"
            );
            reads.set(reads.get() + 1);
            if name == format!("read{}-fail", reads.get()) {
                return Err(e.clone());
            }
            Ok(if name == "changed-ready" {
                if reads.get() == 1 { 2.0 } else { 0.0 }
            } else {
                arg.ready.get()
            })
        };
        let close = |arg: &Rc<Socket>| {
            assert!(Rc::ptr_eq(arg, &socket), "native captured socket for close");
            closes.set(closes.get() + 1);
            if name == "close-fail" {
                return Err(e.clone());
            }
            arg.closed.set(true);
            Ok(())
        };
        let result = detach_binance_private_socket(get, remove, ready, close);
        assert_eq!(gets.get(), 1, "native owner socket captured once");
        let removal_count = match name {
            "missing" | "listeners-absent" => 0,
            "remove1-fail" => 1,
            "remove2-fail" => 2,
            _ => 3,
        };
        let expected_removed: Vec<_> = [
            ("message", 101),
            ("close", if name == "handler-change" { 999 } else { 202 }),
            ("open", 303),
        ]
        .into_iter()
        .take(removal_count)
        .collect();
        assert_eq!(
            *removed.borrow(),
            expected_removed,
            "native ordered listener identities"
        );
        let mut expected_remaining = vec!["close", "message", "open"];
        let committed = if name == "missing" || name == "listeners-absent" {
            0
        } else if name.starts_with("remove") {
            removal_count - 1
        } else {
            3
        };
        for (event, _) in expected_removed.iter().take(committed) {
            expected_remaining.retain(|e| e != event);
        }
        assert_eq!(
            remaining.borrow().iter().copied().collect::<Vec<_>>(),
            expected_remaining,
            "native independent live listeners"
        );
        let expected_reads = if name == "missing" || name.starts_with("remove") {
            0
        } else if name == "read1-fail"
            || matches!(name, "open" | "listeners-absent" | "swapped-owner")
        {
            1
        } else {
            2
        };
        assert_eq!(
            reads.get(),
            expected_reads,
            "native short-circuit ready-state reads"
        );
        let should_close = !matches!(name, "missing" | "closing" | "closed" | "late-state-change")
            && !name.starts_with("remove")
            && !name.starts_with("read");
        assert_eq!(
            closes.get(),
            usize::from(should_close),
            "PRIVATE {variant} detach native close count"
        );
        assert_eq!(
            socket.closed.get(),
            should_close && name != "close-fail",
            "native captured socket closed flag"
        );
        assert_eq!(
            owner.get(),
            if name == "missing" {
                None
            } else if name == "swapped-owner" {
                Some(404)
            } else {
                Some(100)
            },
            "native current owner socket retained"
        );
        original(&result, name.ends_with("fail"), &e);
        // All decoded native values/counts above, source effects below.
        let source_trace = case["trace"].as_array().unwrap();
        assert_eq!(
            source_trace.len(),
            removal_count + expected_reads + usize::from(should_close)
        );
        for (i, (event, id)) in expected_removed.iter().enumerate() {
            assert_eq!(
                source_trace[i].as_array().unwrap(),
                &vec![json!("remove"), json!(100), json!(event), json!(id)]
            );
        }
        for i in 0..expected_reads {
            assert_eq!(
                source_trace[removal_count + i].as_array().unwrap(),
                &vec![json!("read"), json!(100)]
            );
        }
        if should_close {
            assert_eq!(
                source_trace.last().unwrap().as_array().unwrap(),
                &vec![json!("close"), json!(100)]
            );
        }
        assert_eq!(case["removed"].as_array().unwrap().len(), removal_count);
        for (source, (event, id)) in case["removed"]
            .as_array()
            .unwrap()
            .iter()
            .zip(&expected_removed)
        {
            assert_eq!(source[0], *event);
            assert_eq!(source[1], *id);
        }
        assert_eq!(case["remaining"], json!(expected_remaining));
        assert_eq!(case["reads"], expected_reads);
        assert_eq!(case["closeCalls"], usize::from(should_close));
        assert_eq!(case["closed"], socket.closed.get());
        assert_eq!(
            case["owner"],
            owner.get().map(|id| json!(id)).unwrap_or(Value::Null)
        );
        assert_eq!(
            case["failure"].as_str(),
            if name.ends_with("fail") {
                Some("original")
            } else {
                None
            }
        );
        completed += 1;
    }
    assert_eq!(completed, 16, "fixed detachment cases per family");
}
#[test]
#[ignore = "requires source oracle"]
fn futures_detach() {
    detachment("binance");
}
#[test]
#[ignore = "requires source oracle"]
fn pm_detach() {
    detachment("binancepm");
}

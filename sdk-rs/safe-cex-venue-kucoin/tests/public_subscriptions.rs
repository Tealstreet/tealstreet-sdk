use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kucoin::{public_subscriptions::*, ws_helpers::KucoinPublicRole};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};

#[test]
fn public_subscriptions_native_arguments_before_source() {
    let mut cause = ExchangeError::new("subscription failure");
    cause.name = "TypeError".into();
    cause.code = Some(safe_cex_exchange::js_runtime::value::Js::str("SUBSCRIBE"));
    cause.store_subscriber_ids = vec![7, 3];
    let error = Rc::new(cause);
    let mut active = vec![];
    let mut native_active = vec![];
    for subscribe in [true, false] {
        for disposed in [false, true] {
            for symbol in [
                vec![],
                "XBTUSDTM".encode_utf16().collect(),
                vec![65, 44, 66],
                vec![0xd800, 65, 0xdc00],
            ] {
                for fail in [false, true] {
                    let calls = RefCell::new(vec![]);
                    let is_disposed = Cell::new(disposed);
                    let send = |operation: &str, channels: &[&str], symbols: &[Vec<u16>]| {
                        assert_eq!(
                            operation,
                            if subscribe {
                                "subscribe"
                            } else {
                                "unsubscribe"
                            },
                            "KUCOIN-PUBLIC native active operation"
                        );
                        assert_eq!(
                            channels,
                            ["/contractMarket/tickerV2", "/contract/instrument"],
                            "KUCOIN-PUBLIC native active channel order"
                        );
                        assert_eq!(
                            symbols,
                            [symbol.clone()],
                            "KUCOIN-PUBLIC native symbol code units"
                        );
                        calls.borrow_mut().push(
                            json!({"operation":operation,"channels":channels,"symbols":symbols}),
                        );
                        if fail { Err(error.clone()) } else { Ok(()) }
                    };
                    let result = if subscribe {
                        subscribe_active_ticker(&symbol, &is_disposed, send)
                    } else {
                        unsubscribe_active_ticker(&symbol, &is_disposed, send)
                    };
                    let should_send = !symbol.is_empty() && !disposed;
                    assert_eq!(
                        calls.borrow().len(),
                        if should_send { 1 } else { 0 },
                        "KUCOIN-PUBLIC native active call count"
                    );
                    assert_eq!(
                        result.is_err(),
                        should_send && fail,
                        "native active rejection branch"
                    );
                    if let Err(actual) = &result {
                        assert!(Rc::ptr_eq(actual, &error), "native active error pointer");
                        assert_eq!(actual, &error, "native active complete error");
                    }
                    active.push(json!({"subscribe":subscribe,"disposed":disposed,"symbol":symbol,"fail":fail}));
                    native_active.push(json!({"calls":*calls.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
                }
            }
        }
    }
    assert_eq!(active.len(), 32, "declared active operation floor");
    let shards = [
        vec![],
        vec![vec![65], vec![0xd800, 66], vec![65]],
        (0..101)
            .map(|i| format!("S{i}").encode_utf16().collect::<Vec<_>>())
            .collect(),
    ];
    let mut snapshot = vec![];
    let mut native_snapshot = vec![];
    for role in [KucoinPublicRole::Detail, KucoinPublicRole::Snapshot] {
        for symbols in &shards {
            for failure in ["none", "shard", "send"] {
                let calls = RefCell::new(vec![]);
                let trace = RefCell::new(vec![]);
                let result = subscribe_public(
                    role,
                    || {
                        trace.borrow_mut().push("shard");
                        if failure == "shard" {
                            Err(error.clone())
                        } else {
                            Ok(symbols.clone())
                        }
                    },
                    |operation, channels, actual| {
                        trace.borrow_mut().push("send");
                        assert_eq!(
                            operation, "subscribe",
                            "KUCOIN-PUBLIC native snapshot operation"
                        );
                        assert_eq!(
                            channels,
                            ["/contractMarket/snapshot"],
                            "KUCOIN-PUBLIC native snapshot channel"
                        );
                        assert_eq!(
                            actual, symbols,
                            "KUCOIN-PUBLIC native snapshot symbols/order/duplicates"
                        );
                        calls.borrow_mut().push(
                            json!({"operation":operation,"channels":channels,"symbols":actual}),
                        );
                        if failure == "send" {
                            Err(error.clone())
                        } else {
                            Ok(())
                        }
                    },
                );
                let expected_trace = if role == KucoinPublicRole::Detail {
                    vec![]
                } else if failure == "shard" {
                    vec!["shard"]
                } else {
                    vec!["shard", "send"]
                };
                assert_eq!(
                    *trace.borrow(),
                    expected_trace,
                    "KUCOIN-PUBLIC native snapshot lazy call order"
                );
                assert_eq!(
                    calls.borrow().len(),
                    if role == KucoinPublicRole::Snapshot && failure != "shard" {
                        1
                    } else {
                        0
                    },
                    "native snapshot call count includes empty shard"
                );
                assert_eq!(
                    result.is_err(),
                    role == KucoinPublicRole::Snapshot && failure != "none",
                    "native snapshot rejection branch"
                );
                if let Err(actual) = &result {
                    assert!(Rc::ptr_eq(actual, &error), "native snapshot error pointer");
                    assert_eq!(actual, &error, "native snapshot complete error");
                }
                snapshot.push(json!({"role":role,"symbols":symbols,"failure":failure}));
                native_snapshot.push(json!({"trace":*trace.borrow(),"calls":*calls.borrow(),"error":result.err().map(|e|json!({"name":e.name,"message":e.message,"same":true}))}));
            }
        }
    }
    assert_eq!(snapshot.len(), 18, "declared snapshot subscription floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscriptions.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({"active":active,"snapshot":snapshot})).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source,
        json!({"active":native_active,"snapshot":native_snapshot}),
        "live oracle after all native arguments/pointers/errors"
    );
}

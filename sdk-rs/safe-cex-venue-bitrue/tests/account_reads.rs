use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_bitrue::{account_reads::fetch_balance_and_positions_raw, wire::OpaqueValue};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    task::{Context, Poll, Waker},
};
fn run<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    for _ in 0..3 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
    panic!("unexpected pending account read");
}
#[test]
fn account_reads_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-reads-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    println!("capture {}", capture["header"]);
    let inputs: [Option<&str>; 14] = [
        None,
        Some("null"),
        Some("{}"),
        Some(r#"{"code":0,"sentinel":17}"#),
        Some(r#"{"code":"0","sentinel":19}"#),
        Some(r#"{"code":200,"sentinel":23}"#),
        Some(r#"{"code":"200","sentinel":29}"#),
        Some(r#"{"code":"00","msg":"bad-code"}"#),
        Some(r#"{"code":false,"msg":"rejected"}"#),
        Some(r#"{"code":1,"msg":0}"#),
        Some(r#"{"code":1,"msg":37}"#),
        Some(r#"{"code":1,"msg":[3,7]}"#),
        Some(r#"{"code":1,"msg":{}}"#),
        Some(r#"{"code":1,"msg":{"toString":1}}"#),
    ];
    let faults: [Value; 5] = [
        Value::Null,
        json!({"message":"request rejected"}),
        json!({"message":"fallback","response":{"data":{"msg":"response rejected"}}}),
        json!({"message":"fallback","response":{"data":{"msg":0}}}),
        json!({"message":"fallback","response":{"data":{"msg":{"tag":41}}}}),
    ];
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 140, "BITRUE-ACCOUNT-READ source total");
    let mut compared = 0;
    let mut native_branches = [0; 3];
    for (i, raw) in inputs.iter().enumerate() {
        for (j, fault) in faults.iter().enumerate() {
            for (k, emit_fails) in [false, true].into_iter().enumerate() {
                let row = &rows[i * 10 + j * 2 + k];
                assert_eq!(
                    row.get("input"),
                    Some(&json!({"raw":raw,"fault":fault,"emitFails":emit_fails})),
                    "BITRUE-ACCOUNT-READ live input content"
                );
                let calls = RefCell::new(Vec::new());
                let events = RefCell::new(Vec::new());
                let emitted = RefCell::new(Vec::new());
                let result = run(fetch_balance_and_positions_raw(
                    |url| {
                        Ok({
                            calls.borrow_mut().push(url);
                            events.borrow_mut().push("request");
                            let mut pending = true;
                            let events = &events;
                            poll_fn(move |_| {
                                if pending {
                                    pending = false;
                                    return Poll::Pending;
                                }
                                events.borrow_mut().push("response");
                                if !fault.is_null() {
                                    let mut error =
                                        ExchangeError::new(fault["message"].as_str().unwrap());
                                    if let Some(response) = fault.get("response") {
                                        error =
                                            error.with_retained_response(response.clone().into());
                                    }
                                    return Poll::Ready(Err(error));
                                }
                                Poll::Ready(Ok(
                                    raw.map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value)
                                ))
                            })
                        })
                    },
                    |message| {
                        emitted
                            .borrow_mut()
                            .push(message.cloned().unwrap_or(Value::Null));
                        events.borrow_mut().push("emit");
                        if emit_fails {
                            let mut error = ExchangeError::new("emitter rejected");
                            error.name = "SubscriberError".into();
                            error.store_subscriber_ids = vec![17, 0];
                            Err(error)
                        } else {
                            Ok(())
                        }
                    },
                ));
                let actual = match result {
                    Ok(Some(value)) => {
                        native_branches[0] += 1;
                        json!({"kind":"data","value":value})
                    }
                    Ok(None) => {
                        native_branches[1] += 1;
                        json!({"kind":"undefined"})
                    }
                    Err(error) => {
                        native_branches[2] += 1;
                        assert_eq!(
                            error.store_subscriber_ids,
                            vec![17, 0],
                            "BITRUE-ACCOUNT-READ emitter identity"
                        );
                        json!({"kind":"error","name":error.name,"message":error.message})
                    }
                };
                assert_eq!(
                    Some(&actual),
                    row.get("result"),
                    "BITRUE-ACCOUNT-READ native result case {}",
                    i * 10 + j * 2 + k
                );
                assert_eq!(
                    &*calls.borrow(),
                    &["/fapi/v2/account"],
                    "BITRUE-ACCOUNT-READ native request"
                );
                assert_eq!(
                    json!(*calls.borrow()),
                    row["calls"],
                    "BITRUE-ACCOUNT-READ source request"
                );
                assert_eq!(
                    json!(*events.borrow()),
                    row["events"],
                    "BITRUE-ACCOUNT-READ event order"
                );
                assert_eq!(
                    json!(*emitted.borrow()),
                    row["emitted"],
                    "BITRUE-ACCOUNT-READ native emitted values"
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 140, "BITRUE-ACCOUNT-READ comparisons");
    assert_eq!(
        native_branches,
        [8, 66, 66],
        "BITRUE-ACCOUNT-READ native branch counts"
    );
    let balance_inputs: [Option<&str>; 6] = [
        None,
        Some("null"),
        Some("false"),
        Some(r#"{"data":{"account":[]}}"#),
        Some(
            r#"{"data":{"account":[{"marginCoin":"USDT","totalEquity":31,"accountNormal":7,"sumOpenRealizedAmount":-23}]}}"#,
        ),
        Some("{}"),
    ];
    let balances = capture["balances"].as_array().unwrap();
    assert_eq!(balances.len(), 12, "BITRUE-BALANCE-READ source total");
    for (i, raw) in balance_inputs.iter().enumerate() {
        for (j, reject) in [false, true].into_iter().enumerate() {
            use safe_cex_exchange::native_store::DefaultStore;
            use safe_cex_store::{Store, StoreUpdate};
            use safe_cex_types::Balance;
            let row = &balances[i * 2 + j];
            assert_eq!(
                row.get("input"),
                Some(&json!({"raw":raw,"reject":reject})),
                "BITRUE-BALANCE-READ live input content"
            );
            let store = DefaultStore::default();
            let balance = |total, free, used, upnl| Balance {
                total,
                free,
                used,
                upnl,
                info: None,
                currencies: None,
                coins: None,
            };
            Store::update(
                &store,
                StoreUpdate {
                    balance: Some(balance(101., 37., 64., -11.)),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            let events = RefCell::new(Vec::new());
            let result = run(safe_cex_venue_bitrue::account_reads::fetch_balance(
                &store,
                || {
                    Ok({
                        events.borrow_mut().push("fetch");
                        let mut pending = true;
                        let store = &store;
                        let events = &events;
                        poll_fn(move |_| {
                            if pending {
                                pending = false;
                                return Poll::Pending;
                            }
                            Store::update(
                                store,
                                StoreUpdate {
                                    balance: Some(balance(211., 43., 168., -19.)),
                                    ..Default::default()
                                },
                                false,
                            )
                            .unwrap();
                            events.borrow_mut().push("resolve");
                            Poll::Ready(if reject {
                                Err(ExchangeError::new("fetch rejected"))
                            } else {
                                Ok(raw.map(|raw| js_from_str::<OpaqueValue>(raw).unwrap().value))
                            })
                        })
                    })
                },
                |warning| panic!("unexpected refusal: {warning}"),
            ));
            let actual = match result {
                Ok(value) => {
                    let stored = store.balance_handle().unwrap();
                    let same = value
                        .native_value()
                        .downcast::<safe_cex_exchange::js_runtime::value::Js>()
                        .unwrap()
                        .same_reference(
                            &stored
                                .native_value()
                                .downcast::<safe_cex_exchange::js_runtime::value::Js>()
                                .unwrap(),
                        );
                    let value = value.read().unwrap();
                    json!({"kind":"balance","stored":same,"fields":([value.total,value.free,value.used,value.upnl].map(|v|format!("{:016x}",v.to_bits()))),"currencies":value.currencies.as_ref().map(|c|c.keys().collect::<Vec<_>>()),"info":value.info})
                }
                Err(error) => json!({"kind":"error","name":error.name,"message":error.message}),
            };
            assert_eq!(
                Some(&actual),
                row.get("result"),
                "BITRUE-BALANCE-READ native result"
            );
            assert_eq!(
                &*events.borrow(),
                &["fetch", "resolve"],
                "BITRUE-BALANCE-READ awaited fetch"
            );
            assert_eq!(
                json!(*events.borrow()),
                row["events"],
                "BITRUE-BALANCE-READ source events"
            );
        }
    }
    let supplements = capture["supplements"].as_array().unwrap();
    let expected: [String; 2] = [
        r#"{"code":0,"extra":"\ud800"}"#.into(),
        format!(
            "{{\"code\":0,\"extra\":{}3{}}}",
            "[".repeat(160),
            "]".repeat(160)
        ),
    ];
    assert_eq!(supplements.len(), 2, "BITRUE-ACCOUNT-READ supplement total");
    for (i, raw) in expected.iter().enumerate() {
        assert_eq!(
            supplements[i].as_str(),
            Some(raw.as_str()),
            "BITRUE-ACCOUNT-READ supplement content"
        );
        assert!(
            js_from_str::<OpaqueValue>(raw).is_err(),
            "BITRUE-ACCOUNT-READ CHOSEN ingress refusal"
        );
    }
}

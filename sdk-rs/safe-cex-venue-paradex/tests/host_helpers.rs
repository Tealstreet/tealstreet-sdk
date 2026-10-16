use safe_cex_types::{OrderStopTriggerType, OrderTimeInForce, OrderType};
use safe_cex_venue_paradex::{
    batch_predicate::{NativeBatchOrder, is_native_batch_order},
    sdk_state::{JwtCache, ParadexSdkAccount, clear_token, get_account_address},
    subscription_delegates::{listen_ohlcv, listen_order_book},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn text(key: &str, value: &str) -> Value {
    json!({"key":key,"kind":"string","value":value.encode_utf16().collect::<Vec<_>>()})
}
fn undefined(key: &str) -> Value {
    json!({"key":key,"kind":"undefined"})
}
fn number(key: &str, value: f64) -> Value {
    json!({"key":key,"kind":"number","value":bits(value)})
}
fn base() -> NativeBatchOrder {
    NativeBatchOrder {
        r#type: OrderType::Limit,
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        close: None,
        time_in_force: None,
    }
}
fn cases() -> Vec<(NativeBatchOrder, Vec<Value>)> {
    let mut cases = Vec::new();
    for (kind, key) in [
        (OrderType::Market, "market"),
        (OrderType::Limit, "limit"),
        (OrderType::Conditional, "conditional"),
        (OrderType::StopLoss, "stop_market"),
        (OrderType::TakeProfit, "take_profit_market"),
        (OrderType::TrailingStopLoss, "trailing_stop_market"),
    ] {
        let mut value = base();
        value.r#type = kind;
        cases.push((value, vec![text("type", key)]));
    }
    for (tif, key) in [
        (None, None),
        (
            Some(OrderTimeInForce::GoodTillCancel),
            Some("GoodTillCancel"),
        ),
        (
            Some(OrderTimeInForce::ImmediateOrCancel),
            Some("ImmediateOrCancel"),
        ),
        (Some(OrderTimeInForce::PostOnly), Some("PostOnly")),
        (Some(OrderTimeInForce::FillOrKill), Some("FillOrKill")),
    ] {
        let mut value = base();
        value.time_in_force = tif;
        cases.push((
            value,
            vec![
                text("type", "limit"),
                key.map(|key| text("timeInForce", key))
                    .unwrap_or_else(|| undefined("timeInForce")),
            ],
        ));
    }
    for close in [None, Some(false), Some(true)] {
        let mut value = base();
        value.close = close;
        cases.push((
            value,
            vec![
                text("type", "limit"),
                close
                    .map(|b| json!({"key":"close","kind":"boolean","value":b}))
                    .unwrap_or_else(|| undefined("close")),
            ],
        ));
    }
    for field in ["triggerPrice", "stopLoss", "takeProfit"] {
        for present in [
            None,
            Some(0.0),
            Some(-0.0),
            Some(17.25),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            let mut value = base();
            match field {
                "triggerPrice" => value.trigger_price = present,
                "stopLoss" => value.stop_loss = present,
                _ => value.take_profit = present,
            }
            cases.push((
                value,
                vec![
                    text("type", "limit"),
                    present
                        .map(|n| number(field, n))
                        .unwrap_or_else(|| undefined(field)),
                ],
            ));
        }
    }
    for field in ["tpTriggerType", "slTriggerType"] {
        for (trigger, key) in [
            (None, None),
            (Some(OrderStopTriggerType::MarkPrice), Some("mark_price")),
            (Some(OrderStopTriggerType::LastPrice), Some("last_price")),
            (Some(OrderStopTriggerType::IndexPrice), Some("index_price")),
        ] {
            let mut value = base();
            if field == "tpTriggerType" {
                value.tp_trigger_type = trigger;
            } else {
                value.sl_trigger_type = trigger;
            }
            cases.push((
                value,
                vec![
                    text("type", "limit"),
                    key.map(|key| text(field, key))
                        .unwrap_or_else(|| undefined(field)),
                ],
            ));
        }
    }
    let mut value = base();
    value.trigger_price = Some(0.0);
    value.stop_loss = Some(0.0);
    value.take_profit = Some(0.0);
    value.tp_trigger_type = Some(OrderStopTriggerType::MarkPrice);
    value.sl_trigger_type = Some(OrderStopTriggerType::LastPrice);
    value.close = Some(true);
    value.time_in_force = Some(OrderTimeInForce::FillOrKill);
    cases.push((
        value,
        vec![
            text("type", "limit"),
            number("triggerPrice", 0.0),
            number("stopLoss", 0.0),
            number("takeProfit", 0.0),
            text("tpTriggerType", "mark_price"),
            text("slTriggerType", "last_price"),
            json!({"key":"close","kind":"boolean","value":true}),
            text("timeInForce", "FillOrKill"),
        ],
    ));
    cases.push((
        base(),
        vec![
            text("type", "limit"),
            json!({"key":"symbol","kind":"string","value":[0xd800]}),
        ],
    ));
    cases
}
#[derive(Debug)]
struct Marker {
    tag: &'static str,
}
struct Address {
    value: Rc<Vec<u16>>,
    changed: Rc<Vec<u16>>,
    reads: Cell<usize>,
}
#[test]
fn actual_typescript_host_helpers() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/host_helpers_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "helpers_oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        value => value,
    };
    assert_eq!(corpus["header"]["arch"], arch);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() > 0);
    let expected = cases();
    let batch = corpus["batch"].as_array().unwrap();
    assert_eq!(expected.len(), 42, "batch_independent_cases");
    assert_eq!(batch.len(), 42, "batch_source_cases");
    let mut completed = 0;
    let mut eligible = 0;
    for (i, (input, fields)) in expected.iter().enumerate() {
        assert_eq!(batch[i]["fields"], json!(fields), "batch_live_input {i}");
        let native = is_native_batch_order(input);
        assert_eq!(
            Some(native),
            batch[i]["output"].as_bool(),
            "batch_native_value {i}"
        );
        eligible += usize::from(native);
        completed += 1;
    }
    assert_eq!(completed, 42, "batch_completed_native");
    assert_eq!(eligible, 13, "batch_native_eligible_count");
    let account = Rc::new(Marker { tag: "account" });
    let config = Rc::new(Marker { tag: "config" });
    let provider = Rc::new(Marker { tag: "provider" });
    let sdk_record = ParadexSdkAccount {
        account: account.clone(),
        config: config.clone(),
        provider: provider.clone(),
    };
    let source_record = corpus["sdkIdentity"].as_array().unwrap();
    assert_eq!(source_record.len(), 3, "sdk_source_fields");
    assert!(
        Rc::ptr_eq(&sdk_record.account, &account),
        "sdk_native_account_identity"
    );
    assert!(
        Rc::ptr_eq(&sdk_record.config, &config),
        "sdk_native_config_identity"
    );
    assert!(
        Rc::ptr_eq(&sdk_record.provider, &provider),
        "sdk_native_provider_identity"
    );
    for (i, (key, value)) in [
        ("account", &sdk_record.account),
        ("config", &sdk_record.config),
        ("provider", &sdk_record.provider),
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(source_record[i]["key"], *key);
        assert_eq!(source_record[i]["tag"], value.tag, "sdk_native_field");
        assert_eq!(source_record[i]["same"].as_bool(), Some(true));
    }
    let source_delegations = corpus["delegations"].as_array().unwrap();
    assert_eq!(source_delegations.len(), 6, "delegate_source_count");
    let mut delegated = 0;
    for method in ["listenOrderBook", "listenOHLCV"] {
        for mode in ["return", "throw", "callback"] {
            let source = &source_delegations[delegated];
            assert_eq!(source["method"], method);
            assert_eq!(source["mode"], mode);
            let input = Rc::new(vec![0xd800u16]);
            let event = Rc::new(Marker { tag: "event" });
            let returned = Rc::new(Marker {
                tag: "subscription",
            });
            let error = Rc::new(Marker {
                tag: "delegate-error",
            });
            let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
            let callbacks = Rc::new(Cell::new(0));
            let calls = Cell::new(0);
            let callback = Rc::new({
                let trace = trace.clone();
                let callbacks = callbacks.clone();
                let event = event.clone();
                move |value: Rc<Marker>| {
                    callbacks.set(callbacks.get() + 1);
                    trace
                        .borrow_mut()
                        .push(json!(["callback", Rc::ptr_eq(&value, &event)]));
                    "callback-result"
                }
            });
            let listen = |arg: Rc<Vec<u16>>, cb: Rc<_>| {
                calls.set(calls.get() + 1);
                let same_input = Rc::ptr_eq(&arg, &input);
                let same_callback = Rc::ptr_eq(&cb, &callback);
                assert!(same_input, "delegate_native_argument_identity");
                assert!(same_callback, "delegate_native_callback_identity");
                trace
                    .borrow_mut()
                    .push(json!(["delegate", same_input, same_callback]));
                if mode == "throw" {
                    return Err(error.clone());
                }
                if mode == "callback" {
                    let result = cb(event.clone());
                    trace.borrow_mut().push(json!(["callback-return", result]));
                }
                Ok(returned.clone())
            };
            let result = if method == "listenOrderBook" {
                listen_order_book(input.clone(), callback.clone(), listen)
            } else {
                listen_ohlcv(input.clone(), callback.clone(), listen)
            };
            if method == "listenOrderBook" {
                assert_eq!(calls.get(), 1, "book_native_calls");
            } else {
                assert_eq!(calls.get(), 1, "ohlcv_native_calls");
            }
            assert_eq!(
                callbacks.get(),
                usize::from(mode == "callback"),
                "delegate_native_callback_count"
            );
            match result {
                Ok(value) => {
                    assert_eq!(source["result"]["ok"].as_bool(), Some(true));
                    assert!(
                        Rc::ptr_eq(&value, &returned),
                        "delegate_native_result_identity"
                    );
                }
                Err(value) => {
                    assert_eq!(source["result"]["ok"].as_bool(), Some(false));
                    assert!(Rc::ptr_eq(&value, &error), "delegate_native_error_identity");
                }
            }
            assert_eq!(source["result"]["same"].as_bool(), Some(true));
            assert_eq!(
                &*trace.borrow(),
                source["trace"].as_array().unwrap(),
                "delegate_native_trace"
            );
            delegated += 1;
        }
    }
    assert_eq!(delegated, 6, "delegate_completed_cases");
    let sources = corpus["addresses"].as_array().unwrap();
    assert_eq!(sources.len(), 4, "address_source_count");
    let mut addresses = 0;
    for (i, mode) in ["read", "changed", "throw", "account-throw"]
        .iter()
        .enumerate()
    {
        let source = &sources[i];
        assert_eq!(source["mode"], *mode);
        let original = Rc::new(vec![0xd800]);
        let changed = Rc::new("changed".encode_utf16().collect::<Vec<_>>());
        let account = Address {
            value: if *mode == "changed" {
                changed.clone()
            } else {
                original.clone()
            },
            changed: changed.clone(),
            reads: Cell::new(0),
        };
        let sdk = ParadexSdkAccount {
            account,
            config: (),
            provider: (),
        };
        let error = Rc::new(Marker {
            tag: "address-error",
        });
        let account_reads = Cell::new(0);
        let result = get_account_address(
            || {
                account_reads.set(account_reads.get() + 1);
                if *mode == "account-throw" {
                    Err(error.clone())
                } else {
                    Ok(&sdk.account)
                }
            },
            |account| {
                account.reads.set(account.reads.get() + 1);
                if *mode == "throw" {
                    Err(error.clone())
                } else {
                    Ok(if account.reads.get() == 1 {
                        account.value.clone()
                    } else {
                        account.changed.clone()
                    })
                }
            },
        );
        match result {
            Ok(value) => {
                assert_eq!(source["result"]["ok"].as_bool(), Some(true));
                let expected_units: Vec<u16> = source["result"]["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                    .collect();
                assert_eq!(
                    value.as_slice(),
                    expected_units.as_slice(),
                    "address_native_value"
                );
                assert!(
                    Rc::ptr_eq(&value, &sdk.account.value),
                    "address_native_identity"
                );
            }
            Err(value) => {
                assert_eq!(source["result"]["ok"].as_bool(), Some(false));
                assert_eq!(source["result"]["same"].as_bool(), Some(true));
                assert!(Rc::ptr_eq(&value, &error), "address_native_error_identity");
            }
        }
        let expected_reads = usize::from(*mode != "account-throw");
        assert_eq!(
            sdk.account.reads.get(),
            expected_reads,
            "address_native_reads"
        );
        assert_eq!(source["reads"].as_u64(), Some(expected_reads as u64));
        assert_eq!(account_reads.get(), 1, "address_native_account_reads");
        assert_eq!(source["accountReads"], 1);
        addresses += 1;
    }
    assert_eq!(addresses, 4, "address_completed_cases");
    let sources = corpus["caches"].as_array().unwrap();
    assert_eq!(sources.len(), 10, "clear_source_count");
    let mut cleared = 0;
    let token = vec![0xd800];
    let refresh = Marker { tag: "refresh" };
    for expiry in [0.0, -0.0, 123456.75, f64::INFINITY, f64::NAN] {
        for populated in [false, true] {
            let source = &sources[cleared];
            assert_eq!(source["expiry"], bits(expiry));
            assert_eq!(source["populated"].as_bool(), Some(populated));
            let mut cache = JwtCache {
                token: populated.then_some(&token),
                token_expiry: expiry,
                refresh_promise: populated.then_some(&refresh),
            };
            clear_token(&mut cache);
            assert!(cache.token.is_none(), "clear_native_token");
            assert_eq!(
                cache.token_expiry.to_bits(),
                0.0f64.to_bits(),
                "clear_native_expiry"
            );
            assert!(cache.refresh_promise.is_none(), "clear_native_refresh");
            assert_eq!(source["tokenNull"].as_bool(), Some(true));
            assert_eq!(source["expiryBits"], bits(cache.token_expiry));
            assert_eq!(source["refreshNull"].as_bool(), Some(true));
            assert_eq!(source["undefined"].as_bool(), Some(true));
            assert_eq!(
                refresh.tag, source["originalRefresh"],
                "clear_native_refresh_survives"
            );
            assert_eq!(token, vec![0xd800], "clear_native_token_survives");
            cleared += 1;
        }
    }
    assert_eq!(cleared, 10, "clear_completed_cases");
}

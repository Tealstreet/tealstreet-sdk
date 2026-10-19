use safe_cex_exchange::{
    fold_position_adapter::{FoldPositionAdapterDeps, map_folded_interval},
    position_intervals::{
        FundingRow, PositionInterval, PositionIntervalOptions, attribute_funding,
        reconstruct_position_intervals,
    },
};
use safe_cex_types::{
    Execution, PositionProvenance, PositionRecord, PositionRecordLeg, PositionRecordState,
    PositionSide, PositionSourceAxis,
};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
fn numeric(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "JOURNAL native {label}"
    );
}
fn optional(actual: Option<f64>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "JOURNAL native absent {label}");
    } else {
        numeric(actual.unwrap(), expected, label);
    }
}
fn side(value: &Value) -> PositionSide {
    match value.as_str().unwrap() {
        "long" => PositionSide::Long,
        "short" => PositionSide::Short,
        _ => panic!("source side"),
    }
}
fn leg(value: &Value) -> PositionRecordLeg {
    match value.as_str().unwrap() {
        "long" => PositionRecordLeg::Long,
        "short" => PositionRecordLeg::Short,
        "both" => PositionRecordLeg::Both,
        _ => panic!("source leg"),
    }
}
fn decoded(value: &Value) -> f64 {
    let text = value["number"].as_str().unwrap();
    if text == "nan" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(text, 16).unwrap())
    }
}
fn record(actual: &PositionRecord, expected: &Value) {
    for (label, value) in [
        ("key", &actual.key),
        ("account", &actual.account),
        ("symbol", &actual.symbol),
        ("settleCurrency", &actual.settle_currency),
    ] {
        assert_eq!(
            value,
            expected[label].as_str().unwrap(),
            "JOURNAL native {label}"
        );
    }
    assert_eq!(actual.side, side(&expected["side"]), "JOURNAL native side");
    assert_eq!(
        actual.position_side,
        leg(&expected["positionSide"]),
        "JOURNAL native leg"
    );
    assert_eq!(actual.state, PositionRecordState::Closed);
    assert_eq!(expected["state"], "closed");
    for (label, value) in [
        ("closedSize", actual.closed_size),
        ("avgEntryPrice", actual.avg_entry_price),
        ("realizedPnl", actual.realized_pnl),
        ("fees", actual.fees),
        ("funding", actual.funding),
        ("closeCount", actual.close_count),
    ] {
        numeric(value, &expected[label], label);
    }
    for (label, value) in [
        ("openedAt", actual.opened_at),
        ("closedAt", actual.closed_at),
        ("maxSize", actual.max_size),
        ("avgExitPrice", actual.avg_exit_price),
    ] {
        optional(value, &expected[label], label);
    }
    assert_eq!(
        actual.funding_attributed,
        expected["fundingAttributed"].as_bool().unwrap(),
        "JOURNAL native funding attributed"
    );
    assert!(actual.close_reason.is_none());
    assert_eq!(expected["closeReason"]["undefined"], true);
    assert_eq!(actual.provenance, PositionProvenance::Derived);
    assert_eq!(expected["provenance"], "derived");
    assert_eq!(actual.source_axis, PositionSourceAxis::Fills);
    assert_eq!(expected["sourceAxis"], "fills");
    assert!(
        actual.leverage.is_none() && actual.closed_notional.is_none() && actual.venue_id.is_none()
    );
}
fn interval(actual: &PositionInterval, expected: &Value) {
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "INTERVAL native symbol"
    );
    assert_eq!(actual.side, side(&expected["side"]), "INTERVAL native side");
    assert_eq!(
        actual.position_side,
        leg(&expected["positionSide"]),
        "INTERVAL native leg"
    );
    for (label, value) in [
        ("openedAt", actual.opened_at),
        ("maxSize", actual.max_size),
        ("closedSize", actual.closed_size),
        ("avgEntryPrice", actual.avg_entry_price),
        ("avgExitPrice", actual.avg_exit_price),
        ("fees", actual.fees),
    ] {
        numeric(value, &expected[label], label);
    }
    optional(actual.closed_at, &expected["closedAt"], "interval closedAt");
    optional(
        actual.realized_pnl,
        &expected["realizedPnl"],
        "interval realizedPnl",
    );
    assert_eq!(
        actual.complete,
        expected["complete"].as_bool().unwrap(),
        "INTERVAL native complete"
    );
    assert_eq!(
        actual.closed_by_flip,
        expected["closedByFlip"].as_bool().unwrap(),
        "INTERVAL native flip"
    );
    assert_eq!(
        actual.realized_pnl_partial,
        expected["realizedPnlPartial"].as_bool(),
        "INTERVAL native partial pnl"
    );
    assert_eq!(
        actual.fee_currencies,
        expected["feeCurrencies"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect::<Vec<_>>(),
        "INTERVAL native fee currencies"
    );
}
#[test]
fn interval_mixed_optional_ids_never_panic_or_lose_positions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["orderingCases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for (index, case) in cases.iter().enumerate() {
        let fills: Vec<Execution> = serde_json::from_value(case["fills"].clone()).unwrap();
        let actual =
            reconstruct_position_intervals(&fills, &mut PositionIntervalOptions::default())
                .expect("INTERVAL mixed optional IDs complete without a host panic");
        let expected = case["intervals"].as_array().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "INTERVAL mixed ID count {index}"
        );
        for row in &actual {
            let expected = expected
                .iter()
                .find(|value| value["symbol"] == row.symbol)
                .unwrap();
            interval(row, expected);
        }
    }
}
#[test]
fn interval_nonfinite_close_times_never_panic_or_lose_positions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["nonfiniteCases"].as_array().unwrap();
    assert_eq!(cases.len(), 128);
    for case in cases {
        let mut fills: Vec<Execution> = serde_json::from_value(case["fills"].clone()).unwrap();
        for (fill, timestamp) in fills.iter_mut().zip(case["timestamps"].as_array().unwrap()) {
            fill.timestamp = decoded(timestamp);
        }
        let actual =
            reconstruct_position_intervals(&fills, &mut PositionIntervalOptions::default())
                .expect("INTERVAL nonfinite close times complete without a host panic");
        let expected = case["intervals"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for row in &actual {
            let expected = expected
                .iter()
                .find(|value| value["symbol"] == row.symbol)
                .unwrap();
            interval(row, expected);
        }
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn journal_interval_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 192, "INTERVAL independent floor");
    let mut compared = 0;
    let funding = fixture["fundingRows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| FundingRow {
            symbol: r["symbol"].as_str().unwrap().into(),
            timestamp: r["timestamp"].as_f64().unwrap(),
            income: r["income"].as_f64().unwrap(),
        })
        .collect::<Vec<_>>();
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["sequence"].as_u64().unwrap() as usize,
            index / 16,
            "INTERVAL ordered scenario"
        );
        assert_eq!(
            case["seed"].as_u64().unwrap() as usize,
            (index / 4) % 4,
            "INTERVAL ordered seed"
        );
        assert_eq!(case["missing"].as_bool().unwrap(), (index / 2) % 2 == 1);
        assert_eq!(case["resolved"].as_bool().unwrap(), index % 2 == 1);
        let fills = serde_json::from_value::<Vec<Execution>>(case["fills"].clone()).unwrap();
        let calls = RefCell::new(Vec::new());
        let mut resolve = |e: &Execution| {
            calls.borrow_mut().push(e.id.clone().unwrap());
            (e.side == safe_cex_types::OrderSide::Buy).then_some(PositionRecordLeg::Long)
        };
        let mut options = PositionIntervalOptions {
            opening_size: serde_json::from_value(case["opening"].clone()).unwrap(),
            resolve_leg: if index % 2 == 1 {
                Some(&mut resolve)
            } else {
                None
            },
        };
        let actual = reconstruct_position_intervals(&fills, &mut options).unwrap();
        compared += 1;
        let expected = case["intervals"].as_array().unwrap();
        assert_eq!(
            actual.len(),
            expected.len(),
            "INTERVAL native count {index}"
        );
        assert_eq!(
            *calls.borrow(),
            serde_json::from_value::<Vec<String>>(case["calls"].clone()).unwrap(),
            "INTERVAL native leg resolver calls"
        );
        for (n, (actual, expected)) in actual.iter().zip(expected).enumerate() {
            interval(actual, expected);
            for (h, hedged) in [false, true].iter().enumerate() {
                let actual_funding = attribute_funding(actual, &funding, *hedged);
                let expected_funding = &case["funding"][n][h];
                numeric(
                    actual_funding.funding,
                    &expected_funding["funding"],
                    "interval funding",
                );
                assert_eq!(
                    actual_funding.attributed,
                    expected_funding["attributed"].as_bool().unwrap(),
                    "INTERVAL native funding attributed"
                );
            }
            let mut currency = |s: &str| Ok(format!("settle:{s}"));
            let mut resolve_funding = |i: &PositionInterval| attribute_funding(i, &funding, false);
            let mut deps = FoldPositionAdapterDeps {
                account: "acct",
                resolve_settle_currency: &mut currency,
                resolve_funding: Some(&mut resolve_funding),
            };
            let actual_record = map_folded_interval(actual, &mut deps).unwrap();
            let expected_record = &case["records"][n];
            if expected_record.is_null() {
                assert!(actual_record.is_none(), "FOLD native refusal");
            } else {
                record(&actual_record.unwrap(), expected_record);
            }
        }
    }
    assert_eq!(compared, 192, "INTERVAL compared floor");
    let direct = fixture["direct"].as_array().unwrap();
    assert_eq!(direct.len(), 4, "FOLD direct floor");
    for case in direct {
        let e = &case["input"];
        let actual = PositionInterval {
            symbol: e["symbol"].as_str().unwrap().into(),
            position_side: leg(&e["positionSide"]),
            side: side(&e["side"]),
            opened_at: decoded(&e["openedAt"]),
            closed_at: if e["closedAt"]["undefined"] == true {
                None
            } else {
                Some(decoded(&e["closedAt"]))
            },
            max_size: decoded(&e["maxSize"]),
            closed_size: decoded(&e["closedSize"]),
            complete: e["complete"].as_bool().unwrap(),
            closed_by_flip: false,
            avg_entry_price: decoded(&e["avgEntryPrice"]),
            avg_exit_price: decoded(&e["avgExitPrice"]),
            realized_pnl: if e["realizedPnl"]["undefined"] == true {
                None
            } else {
                Some(decoded(&e["realizedPnl"]))
            },
            realized_pnl_partial: None,
            fees: decoded(&e["fees"]),
            fee_currencies: vec!["USDT".into()],
        };
        let mut currency = |s: &str| Ok(format!("settle:{s}"));
        let result = map_folded_interval(
            &actual,
            &mut FoldPositionAdapterDeps {
                account: "acct",
                resolve_settle_currency: &mut currency,
                resolve_funding: None,
            },
        )
        .unwrap();
        if case["result"].is_null() {
            assert!(result.is_none(), "FOLD direct refusal");
        } else {
            record(&result.unwrap(), &case["result"]);
        }
    }
}

fn ready<F: std::future::Future>(runtime: &runtime::Runtime, future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    for _ in 0..128 {
        if let std::task::Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
        runtime.advance(0);
    }
    panic!("BUILD fixture unexpectedly pending");
}
fn trace_value(actual: &Value, expected: &Value) {
    if expected.get("number").is_some() {
        numeric(actual.as_f64().unwrap(), expected, "build callback number");
    } else if let Some(expected) = expected.as_array() {
        let actual = actual.as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "BUILD native callback arity");
        for (a, e) in actual.iter().zip(expected) {
            trace_value(a, e);
        }
    } else if let Some(expected) = expected.as_object() {
        let actual = actual.as_object().unwrap();
        assert_eq!(actual.len(), expected.len(), "BUILD native callback fields");
        for (k, e) in expected {
            trace_value(&actual[k], e);
        }
    } else {
        assert_eq!(actual, expected, "BUILD native callback value");
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn journal_build_native_source_parity() {
    use safe_cex_exchange::{
        build_records::{BuildRecordsOptions, FoldableExchange, build_folded_position_records},
        runtime::{ExchangeError, ExchangeFuture},
    };
    use safe_cex_types::{ExecutionsOptions, IncomeRecord};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["buildCases"].as_array().unwrap();
    assert_eq!(cases.len(), 12, "BUILD independent source floor");
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let mode = case["mode"].as_str().unwrap();
        assert_eq!(
            mode,
            ["absent", "success", "failure"][index / 4],
            "BUILD input mode"
        );
        assert_eq!(case["scoped"].as_bool().unwrap(), (index / 2) % 2 == 1);
        assert_eq!(case["failFills"].as_bool().unwrap(), index % 2 == 1);
        let rows = serde_json::from_value::<Vec<Execution>>(case["rows"].clone()).unwrap();
        let income_rows =
            serde_json::from_value::<Vec<IncomeRecord>>(case["income"].clone()).unwrap();
        let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
        let runtime = Rc::new(runtime::Runtime::default());
        let fills_trace = trace.clone();
        let read_fills = move |opts: ExecutionsOptions| -> ExchangeFuture<'static, Vec<Execution>> {
            assert_eq!(
                opts.from,
                Some(1500. - 1209600000.),
                "BUILD native lookback"
            );
            assert_eq!(opts.to, Some(2500.), "BUILD native upper bound");
            assert_eq!(opts.limit, Some(1000.), "BUILD native limit");
            assert!(opts.order_id.is_none());
            fills_trace.borrow_mut().push(serde_json::json!(["fills",{"from":opts.from.unwrap(),"to":opts.to.unwrap(),"limit":opts.limit.unwrap()}]));
            if let Some(symbol) = &opts.symbol {
                fills_trace.borrow_mut().last_mut().unwrap()[1]["symbol"] = symbol.clone().into();
            }
            let rows = rows
                .iter()
                .filter(|r| opts.symbol.as_ref().is_none_or(|s| s == &r.symbol))
                .cloned()
                .collect::<Vec<_>>();
            let fail = index % 2 == 1;
            Box::pin(async move {
                if fail {
                    let mut e = ExchangeError::new("fills failed");
                    e.name = "FillDenied".into();
                    Err(e)
                } else {
                    Ok(rows)
                }
            })
        };
        let income_trace = trace.clone();
        let fail_income = mode == "failure";
        let read_income = move |from: f64, to: f64| -> ExchangeFuture<'static, Vec<IncomeRecord>> {
            assert_eq!(from, 1500. - 1209600000., "BUILD native income lookback");
            assert_eq!(to, 2500., "BUILD native income upper bound");
            income_trace
                .borrow_mut()
                .push(serde_json::json!(["income", from, to]));
            let rows = income_rows.clone();
            let fail = fail_income;
            Box::pin(async move {
                if fail {
                    let mut e = ExchangeError::new("income failed");
                    e.name = "IncomeDenied".into();
                    Err(e)
                } else {
                    Ok(rows)
                }
            })
        };
        let currency_trace = trace.clone();
        let currency = move |symbol: &str| {
            currency_trace
                .borrow_mut()
                .push(serde_json::json!(["currency", symbol]));
            Ok(format!("settle:{symbol}"))
        };
        let result = ready(
            &runtime,
            build_folded_position_records(
                FoldableExchange {
                    runtime: runtime.clone(),
                    fetch_historical_fills: Rc::new(read_fills),
                    fetch_income_history_range: if mode == "absent" {
                        None
                    } else {
                        Some(Rc::new(read_income))
                    },
                },
                BuildRecordsOptions {
                    account: "acct".into(),
                    from: 1500.,
                    to: 2500.,
                    symbol_scoped: (index / 2) % 2 == 1,
                    resolve_settle_currency: Rc::new(currency),
                },
            ),
        );
        compared += 1;
        let expected = &case["result"];
        if expected.get("error").is_some() {
            let actual = result.unwrap_err();
            assert_eq!(
                actual.error.name,
                expected["error"].as_str().unwrap(),
                "BUILD native error name"
            );
            assert_eq!(
                actual.error.message,
                expected["message"].as_str().unwrap(),
                "BUILD native error message"
            );
            if expected["cause"].is_null() {
                assert!(actual.cause.is_none(), "BUILD native absent cause");
            } else {
                let cause = actual.cause.unwrap();
                assert_eq!(
                    cause.name,
                    expected["cause"]["name"].as_str().unwrap(),
                    "BUILD native cause name"
                );
                assert_eq!(
                    cause.message,
                    expected["cause"]["message"].as_str().unwrap(),
                    "BUILD native cause message"
                );
            }
        } else {
            let actual = result.unwrap();
            let expected = &expected["ok"];
            numeric(
                actual.total_fills as f64,
                &expected["totalFills"],
                "build total fills",
            );
            let expected_intervals = expected["intervals"].as_array().unwrap();
            assert_eq!(
                actual.intervals.len(),
                expected_intervals.len(),
                "BUILD native interval count"
            );
            for (a, e) in actual.intervals.iter().zip(expected_intervals) {
                interval(a, e);
            }
            let expected_records = expected["records"].as_array().unwrap();
            assert_eq!(
                actual.records.len(),
                expected_records.len(),
                "BUILD native record count"
            );
            for (a, e) in actual.records.iter().zip(expected_records) {
                record(a, e);
            }
        }
        trace_value(&Value::Array(trace.borrow().clone()), &case["trace"]);
    }
    assert_eq!(compared, 12, "BUILD native comparison floor");
}

#[test]
fn position_interval_native_bridge_retains_numbers_and_absence() {
    use safe_cex_exchange::js_runtime::value::{from_typed, to_typed};
    let original = PositionInterval {
        symbol: "BTCUSDT".into(),
        position_side: PositionRecordLeg::Both,
        side: PositionSide::Short,
        opened_at: -0.0,
        closed_at: None,
        max_size: 2.5,
        closed_size: 0.0,
        complete: false,
        closed_by_flip: true,
        avg_entry_price: f64::INFINITY,
        avg_exit_price: f64::NAN,
        realized_pnl: Some(-0.0),
        realized_pnl_partial: None,
        fees: 0.125,
        fee_currencies: vec!["USDT".into()],
    };
    let native = from_typed(&original).unwrap();
    assert!(native.has_own("openedAt"));
    assert!(!native.has_own("closedAt"));
    assert!(!native.has_own("realizedPnlPartial"));
    let restored: PositionInterval = to_typed(&native).unwrap();
    assert_eq!(restored.opened_at.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(restored.avg_entry_price, f64::INFINITY);
    assert!(restored.avg_exit_price.is_nan());
    assert_eq!(
        restored.realized_pnl.unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(restored.closed_at, None);
    assert_eq!(restored.position_side, PositionRecordLeg::Both);
    assert_eq!(restored.side, PositionSide::Short);
    assert_eq!(restored.fee_currencies, ["USDT"]);

    let positional = serde_json::json!([
        "BTCUSDT",
        "both",
        "short",
        11.25,
        27.5,
        3.25,
        2.5,
        true,
        false,
        101.5,
        107.75,
        15.625,
        false,
        0.125,
        ["USDT", "USDC"]
    ]);
    assert!(serde_json::from_value::<PositionInterval>(positional.clone()).is_err());
    assert!(<PositionInterval as serde::Deserialize>::deserialize(&positional).is_err());
}

#[path = "support/runtime.rs"]
mod runtime;

#[test]
fn journal_read_starts_at_call_and_finishes_after_waiter_drop() {
    use safe_cex_exchange::{
        build_records::{BuildRecordsOptions, FoldableExchange, build_folded_position_records},
        runtime::{ExchangeFuture, ExchangeRuntime},
    };
    use std::{cell::Cell, future::poll_fn, rc::Rc, task::Poll};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in source["buildTiming"]["timing"].as_array().unwrap() {
        let runtime = Rc::new(runtime::Runtime::default());
        let host: Rc<dyn ExchangeRuntime> = runtime.clone();
        let released = Rc::new(Cell::new(false));
        let wake = Rc::new(RefCell::new(None::<std::task::Waker>));
        let trace = Rc::new(RefCell::new(Vec::<String>::new()));
        let fills: Vec<Execution> = serde_json::from_value(serde_json::json!([
            {"side":"buy","qty":3,"price":11.25,"timestamp":1,"id":"100","symbol":"BTC","commission":0.125,"commissionAsset":"USDT","realizedPnl":0},
            {"side":"sell","qty":3,"price":12.75,"timestamp":2,"id":"101","symbol":"BTC","commission":0.125,"commissionAsset":"USDT","realizedPnl":2.25}
        ])).unwrap();
        let income = case["income"].as_bool().unwrap();
        let read_trace = trace.clone();
        let read_gate = released.clone();
        let read_wake = wake.clone();
        let read_fills = move |_| -> ExchangeFuture<'_, Vec<Execution>> {
            read_trace.borrow_mut().push("fills".into());
            let released = read_gate.clone();
            let wake = read_wake.clone();
            let fills = fills.clone();
            Box::pin(poll_fn(move |cx| {
                if income || released.get() {
                    Poll::Ready(Ok(fills.clone()))
                } else {
                    *wake.borrow_mut() = Some(cx.waker().clone());
                    Poll::Pending
                }
            }))
        };
        let read_trace = trace.clone();
        let read_gate = released.clone();
        let read_wake = wake.clone();
        let read_income = move |_, _| -> ExchangeFuture<'_, Vec<safe_cex_types::IncomeRecord>> {
            read_trace.borrow_mut().push("income".into());
            let released = read_gate.clone();
            let wake = read_wake.clone();
            Box::pin(poll_fn(move |cx| {
                if released.get() {
                    Poll::Ready(Ok(vec![]))
                } else {
                    *wake.borrow_mut() = Some(cx.waker().clone());
                    Poll::Pending
                }
            }))
        };
        let currency_trace = trace.clone();
        let currency = move |symbol: &str| {
            currency_trace
                .borrow_mut()
                .push(format!("currency:{symbol}"));
            Ok("USDT".into())
        };
        let exchange = FoldableExchange {
            runtime: host,
            fetch_historical_fills: Rc::new(read_fills),
            fetch_income_history_range: if income {
                Some(Rc::new(read_income))
            } else {
                None
            },
        };
        let options = BuildRecordsOptions {
            account: "acct".into(),
            from: 0.,
            to: 3.,
            symbol_scoped: false,
            resolve_settle_currency: Rc::new(currency),
        };
        let pending = build_folded_position_records(exchange, options);
        assert_eq!(
            serde_json::json!(*trace.borrow()),
            case["before"],
            "BUILD eager first read before caller polling"
        );
        drop(pending);
        released.set(true);
        wake.borrow_mut()
            .take()
            .expect("held read registered its waker")
            .wake();
        runtime.advance(0);
        assert_eq!(
            serde_json::json!(*trace.borrow()),
            case["after"],
            "BUILD discarded waiter retains fold continuation"
        );
        assert!(runtime.exceptions.borrow().is_empty());
        assert!(
            runtime.tasks.borrow().is_empty(),
            "BUILD completed drivers are released"
        );
    }
}

#[test]
fn journal_ready_reads_defer_fold_and_preserve_settlement_rejection() {
    use safe_cex_exchange::{
        build_records::{BuildRecordsOptions, FoldableExchange, build_folded_position_records},
        js_runtime::value::{Thrown, error_object},
    };
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = &source["buildTiming"]["settlement"];
    assert_eq!(expected["same"], true);
    assert_eq!(expected["before"], serde_json::json!([]));
    let runtime = Rc::new(runtime::Runtime::default());
    let rows: Vec<Execution> =
        serde_json::from_value(source["buildCases"][0]["rows"].clone()).unwrap();
    let rows: Vec<_> = rows.into_iter().filter(|r| r.symbol == "BTC").collect();
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let calls = trace.clone();
    let original = error_object("SettlementDenied", "settlement lookup failed");
    let rejection = original.clone();
    let pending = build_folded_position_records(
        FoldableExchange {
            runtime: runtime.clone(),
            fetch_historical_fills: Rc::new(move |_| {
                let rows = rows.clone();
                Box::pin(async move { Ok(rows) })
            }),
            fetch_income_history_range: None,
        },
        BuildRecordsOptions {
            account: "acct".into(),
            from: 0.,
            to: 3.,
            symbol_scoped: false,
            resolve_settle_currency: Rc::new(move |symbol| {
                calls.borrow_mut().push(symbol.into());
                Err(Thrown::new(rejection.clone()).to_exchange_error())
            }),
        },
    );
    assert!(
        trace.borrow().is_empty(),
        "BUILD ready read still crosses source await before folding"
    );
    let error = ready(&runtime, pending).unwrap_err();
    assert_eq!(error.error.name, "SettlementDenied");
    assert_eq!(error.error.message, "settlement lookup failed");
    assert!(error.cause.is_none());
    assert!(
        Thrown::from(error.error).value.same_reference(&original),
        "BUILD original settlement error referent"
    );
    assert_eq!(serde_json::json!(*trace.borrow()), expected["trace"]);
    assert!(runtime.exceptions.borrow().is_empty());
    assert!(
        runtime.tasks.borrow().is_empty(),
        "BUILD completed drivers are released"
    );
}

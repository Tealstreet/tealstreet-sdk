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
use std::cell::RefCell;
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
            let mut currency = |s: &str| format!("settle:{s}");
            let mut resolve_funding = |i: &PositionInterval| attribute_funding(i, &funding, false);
            let mut deps = FoldPositionAdapterDeps {
                account: "acct",
                resolve_settle_currency: &mut currency,
                resolve_funding: Some(&mut resolve_funding),
            };
            let actual_record = map_folded_interval(actual, &mut deps);
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
        let mut currency = |s: &str| format!("settle:{s}");
        let result = map_folded_interval(
            &actual,
            &mut FoldPositionAdapterDeps {
                account: "acct",
                resolve_settle_currency: &mut currency,
                resolve_funding: None,
            },
        );
        if case["result"].is_null() {
            assert!(result.is_none(), "FOLD direct refusal");
        } else {
            record(&result.unwrap(), &case["result"]);
        }
    }
}

fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut context) {
        std::task::Poll::Ready(value) => value,
        std::task::Poll::Pending => panic!("BUILD fixture unexpectedly pending"),
    }
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
        let trace = RefCell::new(Vec::<Value>::new());
        let mut read_fills = |opts: ExecutionsOptions| -> ExchangeFuture<'_, Vec<Execution>> {
            assert_eq!(
                opts.from,
                Some(1500. - 1209600000.),
                "BUILD native lookback"
            );
            assert_eq!(opts.to, Some(2500.), "BUILD native upper bound");
            assert_eq!(opts.limit, Some(1000.), "BUILD native limit");
            assert!(opts.order_id.is_none());
            trace.borrow_mut().push(serde_json::json!(["fills",{"from":opts.from.unwrap(),"to":opts.to.unwrap(),"limit":opts.limit.unwrap()}]));
            if let Some(symbol) = &opts.symbol {
                trace.borrow_mut().last_mut().unwrap()[1]["symbol"] = symbol.clone().into();
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
        let mut read_income = |from: f64, to: f64| -> ExchangeFuture<'_, Vec<IncomeRecord>> {
            assert_eq!(from, 1500. - 1209600000., "BUILD native income lookback");
            assert_eq!(to, 2500., "BUILD native income upper bound");
            trace
                .borrow_mut()
                .push(serde_json::json!(["income", from, to]));
            let rows = income_rows.clone();
            let fail = mode == "failure";
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
        let mut currency = |symbol: &str| {
            trace
                .borrow_mut()
                .push(serde_json::json!(["currency", symbol]));
            format!("settle:{symbol}")
        };
        let result = ready(build_folded_position_records(
            &mut FoldableExchange {
                fetch_historical_fills: &mut read_fills,
                fetch_income_history_range: if mode == "absent" {
                    None
                } else {
                    Some(&mut read_income)
                },
            },
            &mut BuildRecordsOptions {
                account: "acct",
                from: 1500.,
                to: 2500.,
                symbol_scoped: (index / 2) % 2 == 1,
                resolve_settle_currency: &mut currency,
            },
        ));
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
        trace_value(&Value::Array(trace.into_inner()), &case["trace"]);
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

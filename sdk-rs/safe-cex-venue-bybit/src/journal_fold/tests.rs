use super::*;
fn encode(value: f64) -> Value {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else if value == 0. && value.is_sign_negative() {
        "-0".into()
    } else {
        serde_json::json!(value)
    }
}
fn numeric_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        _ => a == b,
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn journal_buckets_and_totals_match_source_without_canonical_model() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (index, case) in cases.iter().enumerate() {
        let bucketed = buckets(case["rows"].as_array().unwrap(), |id| {
            case["legs"]
                .get(id)
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        let result = bucketed.and_then(|runs| {
            runs.into_iter()
                .map(|run| totals(&run).map(|total| (run, total)))
                .collect::<Result<Vec<_>, _>>()
        });
        if let Some(error) = case.get("error") {
            assert_eq!(
                result.err().as_deref(),
                error.as_str(),
                "case{index}: {case}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("case{index}: {e}"));
        assert_eq!(
            result.len(),
            case["runs"].as_array().unwrap().len(),
            "bucket count{index}"
        );
        for ((run, total), expected) in result.iter().zip(case["runs"].as_array().unwrap()) {
            assert_eq!(
                *run,
                *expected["rows"].as_array().unwrap(),
                "bucket members/order{index}"
            );
            if total.closed_size <= 0. {
                assert!(expected["result"].is_null());
                continue;
            }
            let record = &expected["result"];
            for (key, value) in [
                ("closedSize", total.closed_size),
                ("fees", total.fees),
                (
                    "realizedPnl",
                    total.net_pnl + total.fees - case["funding"].as_f64().unwrap(),
                ),
                ("avgExitPrice", total.exit_value / total.closed_size),
            ] {
                assert!(
                    numeric_equal(&encode(value), &record[key]),
                    "case{index} {key}: actual{} expected{}",
                    encode(value),
                    record[key]
                );
            }
            for (key, value) in [
                ("closedNotional", total.closed_notional),
                ("closedAt", total.closed_at),
            ] {
                if value == 0. {
                    assert!(record.get(key).is_none(), "case{index} {key}");
                } else {
                    assert!(
                        numeric_equal(&encode(value), &record[key]),
                        "case{index} {key}"
                    );
                }
            }
            assert_eq!(record["closeCount"].as_u64().unwrap(), run.len() as u64);
        }
    }
    assert_eq!(cases.len(), 369);
}

fn native_number(actual: f64, expected: &Value, label: &str) {
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
fn native_optional(actual: Option<f64>, expected: &Value, label: &str) {
    if expected.get("undefined").is_some() {
        assert!(actual.is_none(), "JOURNAL native absent {label}");
    } else {
        native_number(actual.unwrap(), expected, label);
    }
}
fn native_record(actual: &PositionRecord, expected: &Value) {
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
    assert_eq!(
        actual.side,
        match expected["side"].as_str().unwrap() {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            _ => panic!("unexpected source side"),
        },
        "JOURNAL native side"
    );
    assert_eq!(
        actual.position_side,
        match expected["positionSide"].as_str().unwrap() {
            "long" => PositionRecordLeg::Long,
            "short" => PositionRecordLeg::Short,
            "both" => PositionRecordLeg::Both,
            _ => panic!("unexpected source leg"),
        },
        "JOURNAL native leg"
    );
    assert_eq!(
        actual.state,
        PositionRecordState::Closed,
        "JOURNAL native state"
    );
    assert_eq!(expected["state"], "closed");
    for (label, value) in [
        ("closedSize", actual.closed_size),
        ("avgEntryPrice", actual.avg_entry_price),
        ("realizedPnl", actual.realized_pnl),
        ("fees", actual.fees),
        ("funding", actual.funding),
        ("closeCount", actual.close_count),
    ] {
        native_number(value, &expected[label], label);
    }
    for (label, value) in [
        ("openedAt", actual.opened_at),
        ("closedAt", actual.closed_at),
        ("maxSize", actual.max_size),
        ("leverage", actual.leverage),
        ("closedNotional", actual.closed_notional),
        ("avgExitPrice", actual.avg_exit_price),
    ] {
        native_optional(value, &expected[label], label);
    }
    if expected["venueId"].get("undefined").is_some() {
        assert!(actual.venue_id.is_none(), "JOURNAL native absent venueId");
    } else {
        assert_eq!(
            actual.venue_id.as_deref(),
            expected["venueId"].as_str(),
            "JOURNAL native venueId"
        );
    }
    assert_eq!(
        actual.funding_attributed,
        expected["fundingAttributed"].as_bool().unwrap(),
        "JOURNAL native funding attribution"
    );
    assert_eq!(
        actual.close_reason,
        Some(match expected["closeReason"].as_str().unwrap() {
            "user" => PositionCloseReason::User,
            "adl" => PositionCloseReason::Adl,
            "liquidation" => PositionCloseReason::Liquidation,
            "settlement" => PositionCloseReason::Settlement,
            _ => panic!("unexpected source reason"),
        }),
        "JOURNAL native close reason"
    );
    assert_eq!(
        actual.provenance,
        PositionProvenance::Reported,
        "JOURNAL native provenance"
    );
    assert_eq!(expected["provenance"], "reported");
    assert_eq!(
        actual.source_axis,
        match expected["sourceAxis"].as_str().unwrap() {
            "closed-positions" => PositionSourceAxis::ClosedPositions,
            "closed-positions+orders" => PositionSourceAxis::ClosedPositionsOrders,
            _ => panic!("unexpected source axis"),
        },
        "JOURNAL native axis"
    );
}

#[test]
#[ignore = "requires installed TypeScript; mandatory venue-oracle:bybit"]
fn journal_records_match_source_native_values() {
    use std::cell::RefCell;
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/journal-record-fixtures.cjs"
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
    assert_eq!(cases.len(), 595, "JOURNAL independent source case floor");
    let mut compared = 0;
    let mut outcomes = [0usize; 3];
    for (index, case) in cases.iter().enumerate() {
        let trace = RefCell::new(Vec::<Value>::new());
        let mut currency = |symbol: &str| {
            trace
                .borrow_mut()
                .push(serde_json::json!(["currency", symbol]));
            if symbol == "BTCUSDC" {
                "USDC".into()
            } else {
                "USDT".into()
            }
        };
        let mut leg = |id: &str| {
            trace.borrow_mut().push(serde_json::json!(["leg", id]));
            (id == "two").then_some(PositionRecordLeg::Short)
        };
        let mode = case["mode"].as_str().unwrap();
        let mut context = |symbol: &str, leg: PositionRecordLeg, at: f64| {
            trace
                .borrow_mut()
                .push(serde_json::json!(["context", symbol, leg_text(leg), at]));
            if mode == "missing" {
                None
            } else {
                Some(BybitPositionContext {
                    opened_at: Some(1699990000321.),
                    funding: -0.625,
                    funding_attributed: mode != "unattributed",
                })
            }
        };
        let mut deps = BybitPositionAdapterDeps {
            account: "acct",
            resolve_settle_currency: &mut currency,
            resolve_leg: if mode == "plain" || mode == "context-only" {
                None
            } else {
                Some(&mut leg)
            },
            resolve_context: if mode == "plain" || mode == "leg-only" {
                None
            } else {
                Some(&mut context)
            },
        };
        let actual = if case["op"] == "fold" {
            fold_run(case["rows"].as_array().unwrap(), &mut deps)
                .map(|r| r.into_iter().collect::<Vec<_>>())
        } else {
            aggregate_bybit_closed_pnl(case["rows"].as_array().unwrap(), &mut deps)
        };
        compared += 1;
        let expected = &case["result"];
        if let Some(error) = expected.get("error") {
            assert_eq!(error, "TypeError", "JOURNAL source error kind {index}");
            assert_eq!(
                actual.err().as_deref(),
                expected["message"].as_str(),
                "JOURNAL native error {index}"
            );
            outcomes[0] += 1;
        } else {
            let records = expected["ok"].as_array().unwrap();
            if records.iter().any(|r| !r["closeReason"].is_string()) {
                assert_eq!(
                    actual.err().as_deref(),
                    Some(
                        "RepresentationError: inherited closeReason is outside PositionCloseReason"
                    ),
                    "JOURNAL native inherited refusal {index}"
                );
                outcomes[1] += 1;
            } else {
                let actual =
                    actual.unwrap_or_else(|e| panic!("JOURNAL native success case {index}: {e}"));
                assert_eq!(
                    actual.len(),
                    records.len(),
                    "JOURNAL native record count {index}"
                );
                for (actual, expected) in actual.iter().zip(records) {
                    native_record(actual, expected);
                }
                outcomes[2] += 1;
            }
        }
        let expected_trace = case["trace"].as_array().unwrap();
        let native_trace = trace.borrow();
        assert_eq!(
            native_trace.len(),
            expected_trace.len(),
            "JOURNAL callback count {index}"
        );
        for (actual, expected) in native_trace.iter().zip(expected_trace) {
            let actual = actual.as_array().unwrap();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "JOURNAL trace arity");
            for (actual, expected) in actual.iter().zip(expected) {
                if expected.get("number").is_some() {
                    native_number(actual.as_f64().unwrap(), expected, "callback timestamp");
                } else {
                    assert_eq!(actual, expected, "JOURNAL callback argument {index}");
                }
            }
        }
    }
    assert_eq!(compared, 595, "JOURNAL executed comparison floor");
    assert_eq!(outcomes, [29, 12, 554], "JOURNAL outcome inventory");
    let numbers = fixture["numbers"].as_array().unwrap();
    assert_eq!(numbers.len(), 20, "JOURNAL numeric case floor");
    for case in numbers {
        let raw = case["raw"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let actual = num(raw.as_ref());
        if case["result"].get("error").is_some() {
            assert_eq!(
                actual.err().as_deref(),
                case["result"]["message"].as_str(),
                "JOURNAL num error"
            );
        } else {
            native_number(actual.unwrap(), &case["result"]["ok"], "num");
        }
    }
}

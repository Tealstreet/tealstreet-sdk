use safe_cex_types::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSide, PositionSourceAxis,
};
use safe_cex_venue_blofin::position_adapter::*;
use serde_json::Value;
use std::cell::RefCell;
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
    if expected.is_null() || expected.get("undefined").is_some() {
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
fn position_adapter_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position-adapter-oracle.cjs"
        ))
        .arg("blofin")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 181, "ADAPTER independent source floor");
    let mut compared = 0;
    for (index, case) in cases.iter().enumerate() {
        let trace = RefCell::new(Vec::<Value>::new());

        let mode = case["mode"].as_str().unwrap();
        let mut symbol = |id: &str| {
            trace.borrow_mut().push(serde_json::json!(["symbol", id]));
            Ok(if mode == "unknown" {
                None
            } else {
                Some(format!("mapped:{id}"))
            })
        };
        let mut currency = |id: &str| {
            trace.borrow_mut().push(serde_json::json!(["currency", id]));
            Ok(Some(
                if mode == "empty-currency" { "" } else { "USDC" }.into(),
            ))
        };
        let mut scale = |id: &str| {
            trace.borrow_mut().push(serde_json::json!(["scale", id]));
            match mode {
                "zero" => 0.,
                "nan" => f64::NAN,
                "negative" => -1.,
                _ => 2.5,
            }
        };
        let actual = map_blofin_position_history(
            &case["row"],
            &mut BlofinPositionAdapterDeps {
                account: "acct",
                resolve_symbol: &mut symbol,
                resolve_settle_currency: &mut currency,
                contract_size: if case["flag"] == true {
                    Some(&mut scale)
                } else {
                    None
                },
            },
        );

        compared += 1;
        let expected = &case["result"];
        if expected.get("error").is_some() {
            assert_eq!(expected["error"], "TypeError");
            assert_eq!(
                actual.err().map(|error| error.to_string()).as_deref(),
                expected["message"].as_str(),
                "ADAPTER native error {index}"
            );
        } else {
            let actual = actual.unwrap_or_else(|e| panic!("ADAPTER native success {index}: {e}"));
            if expected["ok"].is_null() {
                assert!(actual.is_none(), "ADAPTER native refusal {index}");
            } else {
                native_record(
                    &actual.expect("ADAPTER native record presence"),
                    &expected["ok"],
                );
            }
        }
        assert_eq!(
            *trace.borrow(),
            *case["trace"].as_array().unwrap(),
            "ADAPTER native callback trace {index}"
        );
    }
    assert_eq!(compared, 181, "ADAPTER executed comparison floor");
    let numbers = fixture["numbers"].as_array().unwrap();
    assert_eq!(numbers.len(), 6, "ADAPTER numeric floor");
    for case in numbers {
        let raw = case["raw"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap());
        let actual = num(raw.as_ref());
        if case["result"].get("error").is_some() {
            assert_eq!(
                actual.err().map(|error| error.to_string()).as_deref(),
                case["result"]["message"].as_str(),
                "ADAPTER native numeric error"
            );
        } else {
            native_number(actual.unwrap(), &case["result"]["ok"], "num");
        }
    }
}

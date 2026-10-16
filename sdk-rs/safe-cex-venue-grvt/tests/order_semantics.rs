#![cfg(not(target_arch = "wasm32"))]
#[path = "../../safe-cex-num/tests/support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_venue_grvt::order_semantics::get_grvt_order_time_in_force;

#[test]
fn time_in_force_matches_actual_source_nullish_and_post_only() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-semantics.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "TIF oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["dynamic"]["postType"], "function");
    assert!(corpus["dynamic"]["postResult"].is_null());
    assert_eq!(corpus["dynamic"]["priceType"], "symbol");
    assert_eq!(corpus["dynamic"]["priceError"]["name"], "TypeError");
    assert_eq!(
        corpus["dynamic"]["priceError"]["message"],
        "Cannot convert a Symbol value to a number"
    );
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 14400, "TIF corpus census");
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let order = serde_json::from_str(raw).unwrap();
        let actual = get_grvt_order_time_in_force(&order);
        let expected = match row["result"].as_str() {
            None => None,
            Some("GoodTillCancel") => Some(safe_cex_types::OrderTimeInForce::GoodTillCancel),
            Some("ImmediateOrCancel") => Some(safe_cex_types::OrderTimeInForce::ImmediateOrCancel),
            Some("FillOrKill") => Some(safe_cex_types::OrderTimeInForce::FillOrKill),
            Some("PostOnly") => Some(safe_cex_types::OrderTimeInForce::PostOnly),
            other => panic!("unexpected source TIF {other:?}"),
        };
        assert_eq!(
            actual, expected,
            "GRVT TIF native nullish/post-only semantics: {raw}"
        );
    }
    let refused = corpus["refused"].as_array().unwrap();
    assert_eq!(refused.len(), 2);
    for row in refused {
        assert!(
            serde_json::from_str::<serde_json::Value>(row["raw"].as_str().unwrap()).is_err(),
            "TIF UTF8 ingress refusal"
        );
        assert!(
            row["result"].is_null(),
            "source returns undefined for surrogate-valued field"
        );
    }
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let order = serde_json::from_str(raw).unwrap();
        let actual = get_grvt_order_time_in_force(&order);
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            row["result"],
            "GRVT TIF nullish/post-only semantics: {raw}"
        );
    }
}
#[test]
fn source_truthy_fallback_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "time_in_force_matches_actual_source_nullish_and_post_only",
            "--nocapture",
        ])
        .env("GRVT_TIF_MUTATION", "nullish")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("GRVT TIF native nullish/post-only semantics"),
        "TIF source mutation fails named assertion"
    );
}

#[test]
fn trigger_metadata_matches_actual_source_coercion_and_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-semantics.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "trigger oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["triggerRows"].as_array().unwrap();
    assert_eq!(rows.len(), 10998, "trigger corpus census");
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let order = serde_json::from_str(raw).unwrap();
        let actual = safe_cex_venue_grvt::order_semantics::get_grvt_trigger_metadata(&order);
        if row.get("error").is_some() {
            let error = actual.unwrap_err();
            assert_eq!(
                error.name,
                row["error"]["name"].as_str().unwrap(),
                "GRVT trigger native error name"
            );
            assert_eq!(
                error.message,
                row["error"]["message"].as_str().unwrap(),
                "GRVT trigger native error message"
            );
            continue;
        }
        let expected = &row["result"];
        let actual = actual.unwrap();
        if expected.is_null() {
            assert!(actual.is_none(), "GRVT trigger native absent metadata");
            continue;
        }
        let actual = actual.expect("GRVT trigger native present metadata");
        let kind = match expected["type"].as_str().unwrap() {
            "STOP_LOSS" => safe_cex_venue_grvt::types::GrvtTriggerKind::StopLoss,
            "TAKE_PROFIT" => safe_cex_venue_grvt::types::GrvtTriggerKind::TakeProfit,
            other => panic!("unexpected source trigger kind {other}"),
        };
        assert_eq!(actual.r#type, kind, "GRVT trigger native kind");
        let trigger = match expected["triggerType"].as_str() {
            None => None,
            Some("last_price") => Some(safe_cex_types::OrderStopTriggerType::LastPrice),
            Some("mark_price") => Some(safe_cex_types::OrderStopTriggerType::MarkPrice),
            Some("index_price") => Some(safe_cex_types::OrderStopTriggerType::IndexPrice),
            other => panic!("unexpected source trigger type {other:?}"),
        };
        assert_eq!(
            actual.trigger_type, trigger,
            "GRVT trigger native trigger-by"
        );
        assert_eq!(
            actual.close,
            expected["close"].as_bool(),
            "GRVT trigger native close flag"
        );
        assert_eq!(
            actual.is_split_position,
            expected["isSplitPosition"].as_bool(),
            "GRVT trigger native split flag"
        );
        let bits = actual
            .trigger_price
            .map(|price| format!("{:016x}", price.to_bits()));
        assert_eq!(
            bits.as_deref(),
            expected["priceBits"].as_str(),
            "GRVT trigger native Number bits and fields"
        );
    }
    source_cardinality::for_each(
        corpus["triggerBoundary"].as_array().unwrap(),
        1,
        "grvt-trigger-boundary",
        |row| {
            assert!(
                serde_json::from_str::<serde_json::Value>(row["raw"].as_str().unwrap()).is_err(),
                "trigger UTF8 refusal"
            );
            assert_eq!(row["type"], "STOP_LOSS");
            assert_eq!(row["priceType"], "undefined");
        },
    );
    for row in rows {
        let raw = row["raw"].as_str().unwrap();
        let order = serde_json::from_str(raw).unwrap();
        let actual = safe_cex_venue_grvt::order_semantics::get_grvt_trigger_metadata(&order);
        if row.get("error").is_some() {
            let error = actual.unwrap_err();
            assert_eq!(error.name, row["error"]["name"].as_str().unwrap());
            assert_eq!(
                error.message,
                row["error"]["message"].as_str().unwrap(),
                "GRVT trigger error {raw}"
            );
        } else {
            let actual = actual
                .unwrap()
                .map(|actual| {
                    let mut json = serde_json::to_value(&actual).unwrap();
                    let fields = json.as_object_mut().unwrap();
                    fields.remove("triggerPrice");
                    fields.insert(
                        "priceBits".into(),
                        actual
                            .trigger_price
                            .map(|v| serde_json::Value::String(format!("{:016x}", v.to_bits())))
                            .unwrap_or(serde_json::Value::Null),
                    );
                    json
                })
                .unwrap_or(serde_json::Value::Null);
            assert_eq!(
                actual, row["result"],
                "GRVT trigger Number bits and fields: {raw}"
            );
        }
    }
}

#[test]
fn source_prefix_number_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "trigger_metadata_matches_actual_source_coercion_and_bits",
            "--nocapture",
        ])
        .env("GRVT_TRIGGER_MUTATION", "number")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("GRVT trigger native Number bits and fields"),
        "trigger source mutation fails named assertion"
    );
}

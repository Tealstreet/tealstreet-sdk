#![cfg(not(target_arch = "wasm32"))]
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_grvt::records::*;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn check<T: DeserializeOwned>(row: &Value, field: impl Fn(T) -> String) {
    let positive = ["scalaré", "😀", "\u{fffd}", "\\ud800", " \t\u{feff}"];
    assert_eq!(
        row["positives"].as_array().unwrap().len(),
        5,
        "GRVT AUDIT positive input count"
    );
    for (index, expected) in positive.iter().enumerate() {
        let actual = serde_json::from_str::<T>(row["positives"][index].as_str().unwrap()).unwrap();
        assert_eq!(
            field(actual),
            *expected,
            "GRVT AUDIT native selected scalar field"
        );
    }
    assert_eq!(
        row["negatives"].as_array().unwrap().len(),
        2,
        "GRVT AUDIT refusal input count"
    );
    for (index, expected_unit) in [0xd800_u16, 0xdc00].into_iter().enumerate() {
        let raw = row["negatives"][index].as_str().unwrap();
        let retained = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
        let mut selected = &retained;
        for key in row["field"].as_str().unwrap().split('.') {
            selected = selected.get(key).expect("source selected string path");
        }
        assert_eq!(
            selected,
            &Input::Text(vec![expected_unit]),
            "GRVT AUDIT exact retained raw unit"
        );
        let native = retained
            .decode_native::<T>(false)
            .expect("canonical typed decoder keeps record");
        assert_eq!(
            field(native),
            "\u{fffd}",
            "GRVT AUDIT canonical typed String replacement"
        );
        let bare = serde_json::from_str::<T>(raw);
        if row["captured"] == true {
            assert_eq!(
                field(bare.expect("captured numeric decoder keeps record")),
                "\u{fffd}",
                "GRVT AUDIT captured typed String replacement"
            );
        } else {
            assert!(
                bare.is_err(),
                "GRVT AUDIT direct Serde parser still has a separate String boundary"
            );
        }
    }
    if row["captured"] == true {
        let mut value: Value = serde_json::from_str(row["positives"][0].as_str().unwrap()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("x".into(), serde_json::json!({}));
        let shallow = serde_json::from_value::<T>(value.clone()).unwrap();
        assert_eq!(
            field(shallow),
            "scalaré",
            "GRVT AUDIT native shallow extension retained field"
        );
        let mut extension = serde_json::json!({});
        for _ in 0..140 {
            extension = serde_json::json!({"next":extension});
        }
        value.as_object_mut().unwrap().insert("x".into(), extension);
        let error = serde_json::from_value::<T>(value)
            .err()
            .expect("GRVT AUDIT native captured unknown-depth refusal");
        assert!(
            error
                .to_string()
                .starts_with("numeric ingress recursion limit exceeded"),
            "GRVT AUDIT native numeric recursion diagnostic: {error}"
        );
    }
}

#[test]
fn selected_record_boundaries_are_observable() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-records-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GRVT AUDIT record source success: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "GrvtBuilderAuthorizationResult",
        "GrvtBuilderCheckResult",
        "GrvtBuilderInfo",
        "GrvtSignature",
        "OrderLeg",
        "OrderPayload",
        "GrvtInstrument",
        "GrvtInstrumentLite",
        "GrvtTicker",
        "GrvtOrderBookLevel",
        "GrvtOrderBook",
        "GrvtTrade",
        "GrvtKline",
        "GrvtPosition",
        "GrvtBalance",
        "GrvtAccountSummary",
        "GrvtOrder",
        "GrvtFill",
        "GrvtAuthRequest",
        "GrvtAuthResponse",
        "GrvtWebSocketSubscription",
        "GrvtWebSocketMessage",
    ];
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        22,
        "GRVT AUDIT native record floor"
    );
    let mut completed = 0;
    let mut captured = 0;
    for (index, name) in names.iter().enumerate() {
        let row = &source["rows"][index];
        assert_eq!(row["name"], *name, "GRVT AUDIT ordered record identity");
        match *name {
            "GrvtBuilderAuthorizationResult" => {
                check::<GrvtBuilderAuthorizationResult>(row, |r| r.message)
            }
            "GrvtBuilderCheckResult" => {
                check::<GrvtBuilderCheckResult>(row, |r| r.max_fee_rate.unwrap())
            }
            "GrvtBuilderInfo" => check::<GrvtBuilderInfo>(row, |r| r.builder),
            "GrvtSignature" => check::<GrvtSignature>(row, |r| r.r),
            "OrderLeg" => check::<OrderLeg>(row, |r| r.instrument),
            "OrderPayload" => check::<OrderPayload>(row, |r| r.sub_account_id),
            "GrvtInstrument" => check::<GrvtInstrument>(row, |r| r.instrument_id),
            "GrvtInstrumentLite" => check::<GrvtInstrumentLite>(row, |r| r.i),
            "GrvtTicker" => check::<GrvtTicker>(row, |r| r.instrument_id),
            "GrvtOrderBookLevel" => check::<GrvtOrderBookLevel>(row, |r| r.p),
            "GrvtOrderBook" => check::<GrvtOrderBook>(row, |r| r.instrument_id),
            "GrvtTrade" => check::<GrvtTrade>(row, |r| r.trade_id),
            "GrvtKline" => check::<GrvtKline>(row, |r| r.timestamp),
            "GrvtPosition" => check::<GrvtPosition>(row, |r| r.sub_account_id),
            "GrvtBalance" => check::<GrvtBalance>(row, |r| r.currency),
            "GrvtAccountSummary" => check::<GrvtAccountSummary>(row, |r| r.sub_account_id),
            "GrvtOrder" => check::<GrvtOrder>(row, |r| r.order_id),
            "GrvtFill" => check::<GrvtFill>(row, |r| r.fill_id),
            "GrvtAuthRequest" => check::<GrvtAuthRequest>(row, |r| r.api_key),
            "GrvtAuthResponse" => check::<GrvtAuthResponse>(row, |r| r.session_token),
            "GrvtWebSocketSubscription" => {
                check::<GrvtWebSocketSubscription>(row, |r| r.params.channel)
            }
            "GrvtWebSocketMessage" => {
                check::<GrvtWebSocketMessage>(row, |r| r.error.unwrap().message)
            }
            _ => panic!("unclassified record"),
        }
        captured += usize::from(row["captured"] == true);
        completed += 1;
    }
    assert_eq!(completed, 22, "GRVT AUDIT native completed records");
    assert_eq!(captured, 7, "GRVT AUDIT native captured record floor");
    assert_eq!(
        source["completed"], 22,
        "GRVT AUDIT source completed records"
    );
    assert_eq!(
        source["deepCompleted"], 7,
        "GRVT AUDIT source captured record floor"
    );
}

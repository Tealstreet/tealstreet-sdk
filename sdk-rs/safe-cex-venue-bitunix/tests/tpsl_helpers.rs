use safe_cex_num::{JsCoercionError, numeric_ingress::js_deserialize_passthrough};
use safe_cex_types::OrderStopTriggerType;
use safe_cex_venue_bitunix::tpsl_helpers::{
    TpslLeg, get_bitunix_reduce_only, get_bitunix_trigger_type, get_tpsl_leg_order_id,
};
use serde_json::{Value, json};
use std::{collections::HashMap, process::Command};
fn input(value: &Value) -> Result<Option<Value>, ()> {
    match value.as_str() {
        None => Ok(None),
        Some(raw) => js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw))
            .map(Some)
            .map_err(|_| {
                assert_eq!(raw, r#""\ud800""#, "TPSL only UTF8 refusal");
            }),
    }
}
fn string_result(value: Result<impl AsRef<str>, JsCoercionError>) -> Value {
    match value {
        Ok(s) => json!({"type":"string","units":s.as_ref().encode_utf16().collect::<Vec<_>>()}),
        Err(_) => json!({"error":"TypeError"}),
    }
}
#[test]
fn actual_typescript_tpsl_helpers() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/tpsl-helpers-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "TPSL actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        corpus["ids"].as_array().unwrap().len(),
        1296,
        "TPSL ID source total"
    );
    assert_eq!(
        corpus["triggers"].as_array().unwrap().len(),
        72,
        "TPSL trigger source total"
    );
    assert_eq!(
        corpus["reduce"].as_array().unwrap().len(),
        648,
        "TPSL reduce source total"
    );
    let cache: HashMap<String, bool> =
        serde_json::from_value::<Vec<(String, bool)>>(corpus["cache"].clone())
            .unwrap()
            .into_iter()
            .collect();
    assert_eq!(cache.len(), 7, "TPSL cache inventory");
    let mut counts = [0; 3];
    let mut refused = [0; 3];
    for row in corpus["ids"].as_array().unwrap() {
        let (Ok(raw), Ok(position)) = (input(&row["raw"]), input(&row["position"])) else {
            if row["raw"] == r#""\ud800""# && row["disambiguate"] == false {
                assert_eq!(
                    row["result"],
                    json!({"type":"string","units":[55296]}),
                    "TPSL source surrogate ID"
                );
            }
            refused[0] += 1;
            continue;
        };
        let leg = match row["leg"].as_str().unwrap() {
            "tp" => TpslLeg::Tp,
            "sl" => TpslLeg::Sl,
            _ => panic!("TPSL invalid leg"),
        };
        assert_eq!(
            string_result(get_tpsl_leg_order_id(
                raw.clone().map(Into::into).as_ref(),
                position.clone().map(Into::into).as_ref(),
                leg,
                row["disambiguate"].as_bool().unwrap()
            )),
            row["result"],
            "TPSL native leg ID {row}"
        );
        counts[0] += 1;
    }
    for row in corpus["triggers"].as_array().unwrap() {
        let choice = match row["choice"].as_str().unwrap() {
            "none" => None,
            "mark" => Some(OrderStopTriggerType::MarkPrice),
            "last" => Some(OrderStopTriggerType::LastPrice),
            _ => panic!("TPSL invalid trigger"),
        };
        let Ok(raw) = input(&row["raw"]) else {
            let expected = if choice == Some(OrderStopTriggerType::LastPrice) {
                "LAST_PRICE"
            } else {
                "MARK_PRICE"
            };
            assert_eq!(
                row["result"],
                string_result(Ok(expected)),
                "TPSL source surrogate trigger"
            );
            refused[1] += 1;
            continue;
        };
        assert_eq!(
            string_result(get_bitunix_trigger_type(
                choice,
                raw.map(Into::into).as_ref()
            )),
            row["result"],
            "TPSL native trigger {row}"
        );
        counts[1] += 1;
    }
    for row in corpus["reduce"].as_array().unwrap() {
        let fallback = row["fallback"].as_bool().unwrap();
        let (Ok(raw), Ok(client)) = (input(&row["raw"]), input(&row["client"])) else {
            if row["raw"] == r#""\ud800""# && row["client"].is_null() {
                assert_eq!(
                    row["result"],
                    json!({"type":"boolean","value":fallback}),
                    "TPSL source surrogate fallback"
                );
            }
            refused[2] += 1;
            continue;
        };
        let actual = match get_bitunix_reduce_only(
            raw.map(Into::into).as_ref(),
            client.map(Into::into).as_ref(),
            fallback,
            &cache,
        ) {
            Ok(v) => json!({"type":"boolean","value":v}),
            Err(_) => json!({"error":"TypeError"}),
        };
        assert_eq!(actual, row["result"], "TPSL native reduce-only {row}");
        counts[2] += 1;
    }
    assert_eq!(counts, [1156, 69, 578], "TPSL independent native counts");
    assert_eq!(refused, [140, 3, 70], "TPSL UTF8 refusal counts");
}

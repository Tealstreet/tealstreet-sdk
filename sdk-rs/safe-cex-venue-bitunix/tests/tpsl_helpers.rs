use safe_cex_num::JsCoercionError;
use safe_cex_types::OrderStopTriggerType;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_bitunix::tpsl_helpers::{
    TpslLeg, get_bitunix_reduce_only, get_bitunix_trigger_type, get_tpsl_leg_order_id,
};
use serde_json::{Value, json};
use std::{collections::HashMap, process::Command};
fn input(value: &Value) -> Option<Input> {
    value.as_str().map(|raw| {
        Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>())
            .expect("maintained source supplied valid retained JSON")
    })
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
    let output_refusals = corpus["ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["result"]["type"] == "string")
        .filter(|row| {
            let units = row["result"]["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                .collect::<Vec<_>>();
            String::from_utf16(&units).is_err()
        })
        .count();
    assert_eq!(
        output_refusals, 102,
        "independent source final ID output boundary"
    );
    let mut counts = [0; 3];
    let mut refused = [0; 3];
    for row in corpus["ids"].as_array().unwrap() {
        let (raw, position) = (input(&row["raw"]), input(&row["position"]));
        let leg = match row["leg"].as_str().unwrap() {
            "tp" => TpslLeg::Tp,
            "sl" => TpslLeg::Sl,
            _ => panic!("TPSL invalid leg"),
        };
        let result = get_tpsl_leg_order_id(
            raw.as_ref(),
            position.as_ref(),
            leg,
            row["disambiguate"].as_bool().unwrap(),
        );
        if let Err(error) = &result {
            if row["result"]["type"] == "string" {
                assert_eq!(error.name, "TypeError");
                let units = row["result"]["units"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
                    .collect::<Vec<_>>();
                assert!(
                    String::from_utf16(&units).is_err(),
                    "only the final UTF8 ID output is refused"
                );
                refused[0] += 1;
                continue;
            }
        }
        assert_eq!(
            string_result(result),
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
        let raw = input(&row["raw"]);
        assert_eq!(
            string_result(get_bitunix_trigger_type(choice, raw.as_ref())),
            row["result"],
            "TPSL native trigger {row}"
        );
        counts[1] += 1;
    }
    for row in corpus["reduce"].as_array().unwrap() {
        let fallback = row["fallback"].as_bool().unwrap();
        let (raw, client) = (input(&row["raw"]), input(&row["client"]));
        let actual = match get_bitunix_reduce_only(raw.as_ref(), client.as_ref(), fallback, &cache)
        {
            Ok(v) => json!({"type":"boolean","value":v}),
            Err(_) => json!({"error":"TypeError"}),
        };
        assert_eq!(actual, row["result"], "TPSL native reduce-only {row}");
        counts[2] += 1;
    }
    assert_eq!(counts, [1194, 72, 648], "TPSL independent native counts");
    assert_eq!(
        refused,
        [102, 0, 0],
        "TPSL final ID UTF8 output refusals only"
    );
}

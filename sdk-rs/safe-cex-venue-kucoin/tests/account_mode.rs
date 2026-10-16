use safe_cex_venue_kucoin::account_mode::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn live_account_mode_detection_and_constants() {
    let valid = json!({"code":"200000","data":{"selfAccountMode":"CLASSIC","classicSubAccount":[1,9007199254740991u64],"unifiedSubAccount":[]}});
    let mut cases = vec![
        valid.clone(),
        json!(null),
        json!([]),
        json!({}),
        json!(1),
        json!(true),
        json!("x"),
    ];
    for key in ["code", "data"] {
        let mut value = valid.clone();
        value.as_object_mut().unwrap().remove(key);
        cases.push(value);
        for replacement in [
            json!(null),
            json!(200000),
            json!(false),
            json!("bad"),
            json!([]),
        ] {
            let mut value = valid.clone();
            value[key] = replacement;
            cases.push(value);
        }
    }
    let mut extra = valid.clone();
    extra["extra"] = json!(true);
    cases.push(extra);
    for mode in [
        json!(null),
        json!(1),
        json!(""),
        json!("UNIFIED"),
        json!("future"),
        json!(" CLASSIC"),
        json!("classic"),
        json!("\u{feff}CLASSIC"),
    ] {
        let mut value = valid.clone();
        value["data"]["selfAccountMode"] = mode;
        cases.push(value);
    }
    for key in ["classicSubAccount", "unifiedSubAccount"] {
        for ids in [
            json!(null),
            json!({}),
            json!([0]),
            json!([-1]),
            json!([1.5]),
            json!(["1"]),
            json!([9007199254740992u64]),
            json!([1, 1]),
            json!([null]),
            json!([true]),
        ] {
            let mut value = valid.clone();
            value["data"][key] = ids;
            cases.push(value);
        }
    }
    for key in ["selfAccountMode", "classicSubAccount", "unifiedSubAccount"] {
        let mut value = valid.clone();
        value["data"].as_object_mut().unwrap().remove(key);
        cases.push(value);
    }
    cases.push(json!({"code":"bad","data":null}));
    cases.push(json!({"code":"200000","data":{"selfAccountMode":"FUTURE","classicSubAccount":[0],"unifiedSubAccount":[]}}));
    cases.push(json!({"code":"200000","data":{"selfAccountMode":"","classicSubAccount":[0],"unifiedSubAccount":[]}}));
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-mode.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected["constants"],
        json!({"KUCOIN_UTA_ACCOUNT_MODE_ROUTE": KUCOIN_UTA_ACCOUNT_MODE_ROUTE.iter().map(|(k,v)|((*k).to_owned(),json!(v))).collect::<serde_json::Map<_,_>>(),"KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE": KUCOIN_UTA_ACCOUNT_MODE_SUCCESS_CODE,"KUCOIN_UTA_ACCOUNT_MODES":KUCOIN_UTA_ACCOUNT_MODES,"KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED":KUCOIN_UTA_RUNTIME_ACTIVATION_ENABLED})
    );
    assert_eq!(cases.len(), expected["cases"].as_array().unwrap().len());
    for (input, expected) in cases.iter().zip(expected["cases"].as_array().unwrap()) {
        let detection = decode_kucoin_account_mode(input);
        assert_native_detection(&detection, &expected["detection"]);
        assert_native_detection(
            &KucoinAccountModeDetection::deserialize(expected["detection"].clone()).unwrap(),
            &expected["detection"],
        );
        let decision = match expected["decision"]["reason"].as_str().unwrap() {
            "classic-account-mode-confirmed" => KucoinClassicRuntimeDecision::Classic,
            "unified-account-mode-unsupported" => KucoinClassicRuntimeDecision::Unified,
            "account-mode-unknown" => KucoinClassicRuntimeDecision::Unknown,
            "account-mode-response-malformed" => KucoinClassicRuntimeDecision::Malformed,
            other => panic!("unknown source decision {other}"),
        };
        assert_eq!(
            decide_kucoin_classic_runtime(&detection),
            decision,
            "native runtime decision identity"
        );
        assert_eq!(
            KucoinClassicRuntimeDecision::deserialize(expected["decision"].clone()).unwrap(),
            decision,
            "native decoded decision identity"
        );
        assert_json_numbers_equal(&json!(detection), &expected["detection"]);
        assert_eq!(
            json!(decide_kucoin_classic_runtime(&detection)),
            expected["decision"],
            "{input}"
        );
        assert_eq!(
            KucoinAccountModeDetection::deserialize(expected["detection"].clone()).unwrap(),
            detection
        );
        assert_eq!(
            KucoinClassicRuntimeDecision::deserialize(expected["decision"].clone()).unwrap(),
            decide_kucoin_classic_runtime(&detection)
        );
    }
}

#[test]
fn account_mode_decoders_reject_widened_shapes() {
    for value in [
        json!({"CLASSIC":null}),
        json!(["CLASSIC"]),
        json!(null),
        json!(true),
        json!(1),
        json!("classic"),
        json!(" CLASSIC"),
        json!("CLASSIC\u{feff}"),
        json!("FUTURE"),
    ] {
        assert!(<KucoinAccountMode as Deserialize>::deserialize(value).is_err());
    }
    for value in [
        json!([]),
        json!(null),
        json!(false),
        json!("identified"),
        json!({"status":"unknown","reason":"invalid-data"}),
        json!({"status":"malformed","reason":"unknown-self-account-mode"}),
        json!({"status":"identified","accountMode":{"CLASSIC":null},"classicSubAccount":[],"unifiedSubAccount":[]}),
    ] {
        assert!(<KucoinAccountModeDetection as Deserialize>::deserialize(value).is_err());
    }
    let valid = json!(decide_kucoin_classic_runtime(
        &KucoinAccountModeDetection::Unknown
    ));
    for (key, value) in [
        ("eligible", json!(true)),
        ("route", json!("existing-classic-runtime")),
        ("reason", json!("classic-account-mode-confirmed")),
        ("utaRuntimeActivationEnabled", json!(true)),
    ] {
        let mut input = valid.clone();
        input[key] = value;
        assert!(KucoinClassicRuntimeDecision::deserialize(input).is_err());
    }
    assert!(
        KucoinClassicRuntimeDecision::deserialize(json!([
            false,
            "account-mode-unknown",
            "blocked",
            false
        ]))
        .is_err()
    );
}

fn assert_json_numbers_equal(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                assert_json_numbers_equal(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                assert_json_numbers_equal(value, b.get(key).expect("missing key"));
            }
        }
        _ => assert_eq!(actual, expected),
    }
}

fn assert_native_detection(actual: &KucoinAccountModeDetection, raw: &Value) {
    use KucoinAccountModeDetection as D;
    let expected = match raw["status"].as_str().unwrap() {
        "identified" => D::Identified {
            account_mode: match raw["accountMode"].as_str().unwrap() {
                "CLASSIC" => KucoinAccountMode::Classic,
                "UNIFIED" => KucoinAccountMode::Unified,
                other => panic!("unknown mode {other}"),
            },
            classic_sub_account: raw["classicSubAccount"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect(),
            unified_sub_account: raw["unifiedSubAccount"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect(),
        },
        "unknown" => D::Unknown,
        "malformed" => match raw["reason"].as_str().unwrap() {
            "invalid-envelope" => D::InvalidEnvelope,
            "invalid-success-code" => D::InvalidSuccessCode,
            "invalid-data" => D::InvalidData,
            "invalid-self-account-mode" => D::InvalidSelfAccountMode,
            other => panic!("unknown reason {other}"),
        },
        other => panic!("unknown status {other}"),
    };
    assert_eq!(actual, &expected, "native account-mode detection identity");
}

#[test]
fn subaccount_cardinality_supplement_retains_both_native_vectors() {
    let raw = json!({"status":"identified","accountMode":"CLASSIC",
        "classicSubAccount":[13,17],"unifiedSubAccount":[23,31]});
    for field in ["classicSubAccount", "unifiedSubAccount"] {
        assert_eq!(
            raw[field].as_array().unwrap().len(),
            2,
            "KUCOIN-CARDINALITY subaccount fixture total"
        );
    }
    let decoded = KucoinAccountModeDetection::deserialize(raw).unwrap();
    let KucoinAccountModeDetection::Identified {
        classic_sub_account,
        unified_sub_account,
        ..
    } = decoded
    else {
        panic!("KUCOIN-CARDINALITY subaccount expected identified arm");
    };
    assert_eq!(
        classic_sub_account.len(),
        2,
        "KUCOIN-CARDINALITY classic native retained rows"
    );
    assert_eq!(
        unified_sub_account.len(),
        2,
        "KUCOIN-CARDINALITY unified native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        assert_eq!(
            classic_sub_account
                .get(index)
                .expect("KUCOIN-CARDINALITY classic native index coverage")
                .to_bits(),
            [13.0_f64, 17.0][index].to_bits()
        );
        assert_eq!(
            unified_sub_account
                .get(index)
                .expect("KUCOIN-CARDINALITY unified native index coverage")
                .to_bits(),
            [23.0_f64, 31.0][index].to_bits()
        );
        comparisons += 2;
    }
    assert_eq!(
        comparisons, 4,
        "KUCOIN-CARDINALITY subaccount native comparisons"
    );
}

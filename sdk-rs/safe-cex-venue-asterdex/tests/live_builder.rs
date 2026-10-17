use safe_cex_venue_asterdex::builder::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::any::Any;
fn result<T: serde::Serialize>(result: Result<T, safe_cex_num::JsCoercionError>) -> Value {
    match result {
        Ok(v) => json!({"value":v}),
        Err(e) => json!({"error":{"name":e.name,"message":e.message}}),
    }
}
fn assert_decoded_identity<T: Any + std::fmt::Debug>(name: &str, input: &Value, value: &T) {
    match name {
        "AsterBuilderFeeConfig" => {
            let value = (value as &dyn Any)
                .downcast_ref::<AsterBuilderFeeConfig>()
                .unwrap();
            assert_eq!(
                value.builder_address, input["builderAddress"],
                "{name}.builderAddress identity"
            );
            assert_eq!(
                value.fee_rate_maker, input["feeRateMaker"],
                "{name}.feeRateMaker identity"
            );
            assert_eq!(
                value.fee_rate_taker, input["feeRateTaker"],
                "{name}.feeRateTaker identity"
            );
        }
        "AsterBuilderApprovalCheckResult" => {
            let value = (value as &dyn Any)
                .downcast_ref::<AsterBuilderApprovalCheckResult>()
                .unwrap();
            assert_eq!(
                value.approved, input["approved"],
                "{name}.approved identity"
            );
            assert_eq!(
                value.builder_address, input["builderAddress"],
                "{name}.builderAddress identity"
            );
            assert_eq!(
                value.error.as_deref(),
                input.get("error").and_then(Value::as_str),
                "{name}.error identity"
            );
            assert_eq!(
                value.max_fee_rate.as_deref(),
                input.get("maxFeeRate").and_then(Value::as_str),
                "{name}.maxFeeRate identity"
            );
            let expected = match input["approvalStatus"].as_str().unwrap() {
                "approved" => "Approved",
                "missing" => "Missing",
                "insufficient_fee_cap" => "InsufficientFeeCap",
                "unverified" => "Unverified",
                other => panic!("unknown status {other}"),
            };
            assert_eq!(
                format!("{:?}", value.approval_status),
                expected,
                "{name}.approvalStatus decoded variant identity"
            );
        }
        "AsterBuilderApprovalResult" => {
            let value = (value as &dyn Any)
                .downcast_ref::<AsterBuilderApprovalResult>()
                .unwrap();
            assert_eq!(value.message, input["message"], "{name}.message identity");
            assert_eq!(value.success, input["success"], "{name}.success identity");
        }
        "AsterBuilderApprovalStatus" => {
            let value = (value as &dyn Any)
                .downcast_ref::<AsterBuilderApprovalStatus>()
                .unwrap();
            let expected = match input.as_str().unwrap() {
                "approved" => "Approved",
                "missing" => "Missing",
                "insufficient_fee_cap" => "InsufficientFeeCap",
                "unverified" => "Unverified",
                other => panic!("unknown status {other}"),
            };
            assert_eq!(
                format!("{value:?}"),
                expected,
                "{name} decoded variant identity"
            );
        }
        _ => panic!("unclassified builder contract {name}"),
    }
}
fn contract<T: DeserializeOwned + serde::Serialize + Any + std::fmt::Debug>(
    name: &str,
    data: &Value,
) {
    for case in data["records"][name].as_array().unwrap() {
        let r = T::deserialize(case["input"].clone());
        assert_eq!(
            r.is_ok(),
            case["accept"].as_bool().unwrap(),
            "{name}: {}",
            case["input"]
        );
        if let Ok(value) = r {
            assert_decoded_identity(name, &case["input"], &value);
            assert_eq!(json!(value), case["input"], "{name} output");
        }
    }
}
#[test]
#[ignore = "requires live isolated TypeScript oracle"]
fn builder_helpers_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/builder-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in data["builderSurrogates"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<safe_cex_types::ExchangeOptions>(
                case["inputJson"].as_str().unwrap()
            )
            .is_err(),
            "shared UTF-8 representation refusal for valid JSON options"
        );
        assert_eq!(
            case["address"]["error"]["message"],
            "Invalid Aster builder address: expected an EVM address"
        );
        if case["inputJson"]
            .as_str()
            .unwrap()
            .contains("asterBuilderFeeRateMaker")
        {
            assert_eq!(
                case["config"],
                json!({"value":null}),
                "zero fees bypass the surrogate address in TS"
            );
        } else {
            assert_eq!(case["config"], case["address"]);
        }
    }
    for case in data["configs"].as_array().unwrap() {
        let options: safe_cex_types::ExchangeOptions =
            serde_json::from_value(case["options"].clone()).unwrap();
        let render = |r: Result<Value, String>| match r {
            Ok(value) => json!({"value":value}),
            Err(message) => json!({"error":{"name":"Error","message":message}}),
        };
        assert_eq!(
            render(resolve_builder_address(&options).map(|v| json!(v))),
            case["address"],
            "resolveBuilderAddress {case}"
        );
        assert_eq!(
            render(resolve_aster_builder_fee_config(&options).map(|v| json!(v))),
            case["config"],
            "resolveAsterBuilderFeeConfig {case}"
        );
        assert_eq!(
            render(get_aster_builder_approval_config(&options).map(|v| json!(v))),
            case["approvalConfig"],
            "getAsterBuilderApprovalConfig {case}"
        );
    }
    contract::<AsterBuilderFeeConfig>("AsterBuilderFeeConfig", &data);
    contract::<AsterBuilderApprovalCheckResult>("AsterBuilderApprovalCheckResult", &data);
    contract::<AsterBuilderApprovalResult>("AsterBuilderApprovalResult", &data);
    contract::<AsterBuilderApprovalStatus>("AsterBuilderApprovalStatus", &data);
    for case in data["parsing"].as_array().unwrap() {
        let input = case.get("input");
        assert_eq!(
            json!(extract_builder_rows(input)),
            case["rows"],
            "extractBuilderRows {case}"
        );
        let actual = result(parse_fee_rate(input));
        if actual["value"].is_number() {
            assert_eq!(
                actual["value"].as_f64(),
                case["rate"]["value"].as_f64(),
                "parseFeeRate {case}"
            );
        } else {
            assert_eq!(actual, case["rate"], "parseFeeRate {case}");
        }
        assert_eq!(
            get_provider_error_code(input),
            case["code"].as_f64(),
            "getProviderErrorCode {case}"
        );
        assert_eq!(
            json!(get_provider_error_message(input, None, "fallback")),
            case["message"],
            "getProviderErrorMessage {case}"
        );
    }
    for case in data["nativeErrors"].as_array().unwrap() {
        assert_eq!(
            get_provider_error_message(Some(&case["json"]), case["message"].as_str(), "fallback"),
            case["output"].as_str().unwrap(),
            "native Error {case}"
        );
    }
    for case in data["urls"].as_array().unwrap() {
        assert_eq!(
            json!(get_base_url(
                case["testnet"].as_bool().unwrap(),
                case["base"].as_str()
            )),
            case["output"],
            "getBaseUrl {case}"
        );
    }
    for case in data["orders"].as_array().unwrap() {
        let opts: safe_cex_types::PlaceOrderOpts =
            serde_json::from_value(case["opts"].clone()).unwrap();
        let post = case["opts"]["postOnly"].as_bool();
        let kind = case["apiOrderType"].as_str().unwrap();
        let config = serde_json::from_value(case["config"].clone()).unwrap();
        assert_eq!(
            json!(is_aster_maker_only_order(&opts, post, kind)),
            case["maker"],
            "isAsterMakerOnlyOrder {case}"
        );
        assert_eq!(
            json!(get_aster_builder_fee_rate_for_order(
                &opts, post, kind, &config
            )),
            case["rate"],
            "getAsterBuilderFeeRateForOrder {case}"
        );
    }
    for case in data["typed"].as_array().unwrap() {
        let p = &case["input"];
        assert_eq!(
            build_aster_approve_builder_typed_data(
                p["builderAddress"].as_str().unwrap(),
                p["builderName"].as_str(),
                p["maxFeeRate"].as_str().unwrap(),
                p["nonce"].as_str().unwrap(),
                p["testnet"].as_bool().unwrap(),
                p["userAddress"].as_str().unwrap()
            ),
            case["output"],
            "buildAsterApproveBuilderTypedData {case}"
        );
    }
    for c in data["decimalCases"].as_array().unwrap() {
        let input = c["input"].as_str().unwrap();
        assert_eq!(
            json!({"value":parse_scaled_decimal(input).map(|n|n.to_string())}),
            c["scaled"],
            "parseScaledDecimal {c}"
        );
        assert_eq!(
            json!({"value":normalize_decimal_string(input)}),
            c["normalized"],
            "normalizeDecimalString {c}"
        );
        let actual = match assert_fee_rate(input, "maker fee rate") {
            Ok(value) => json!({"value":value}),
            Err(message) => json!({"error":{"name":"Error","message":message}}),
        };
        assert_eq!(actual, c["checked"], "assertFeeRate {c}");
    }
    for c in data["caps"].as_array().unwrap() {
        let config = serde_json::from_value(c["config"].clone()).unwrap();
        assert_eq!(
            json!(get_aster_builder_approval_fee_rate(&config)),
            c["output"],
            "getAsterBuilderApprovalFeeRate {c}"
        );
    }
    for c in data["nonces"].as_array().unwrap() {
        let time = c["time"]
            .as_f64()
            .unwrap_or_else(|| match c["time"].as_str().unwrap() {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                _ => panic!("unknown time"),
            });
        let key = c["key"].as_str().unwrap();
        let result = if c["wrapper"].as_bool().unwrap() {
            safe_cex_venue_asterdex::api_helpers::build_aster_dex_v3_nonce(key, time)
        } else {
            safe_cex_venue_asterdex::api_helpers::build_v3_nonce(key, time)
        };
        let actual = match result {
            Ok(value) => json!({"value":value}),
            Err(e) => json!({"error":{"name":e.kind.name(),"message":e.message}}),
        };
        assert_eq!(actual, c["output"], "buildV3Nonce {c}");
    }
    let constants = json!({"TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET":TEALSTREET_ASTER_BUILDER_ADDRESS_MAINNET,"TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET":TEALSTREET_ASTER_BUILDER_ADDRESS_TESTNET,"DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER":DEFAULT_ASTER_BUILDER_FEE_RATE_MAKER,"DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER":DEFAULT_ASTER_BUILDER_FEE_RATE_TAKER,"ASTER_BUILDER_FEE_RATE_CAP":ASTER_BUILDER_FEE_RATE_CAP});
    assert_eq!(constants, data["constants"]);
}

#[test]
fn nonce_state_survives_native_thread_changes() {
    let first = std::thread::spawn(|| {
        safe_cex_venue_asterdex::api_helpers::build_v3_nonce("thread-change-signer", 1.0).unwrap()
    })
    .join()
    .unwrap();
    let second = std::thread::spawn(|| {
        safe_cex_venue_asterdex::api_helpers::build_aster_dex_v3_nonce("thread-change-signer", 1.0)
            .unwrap()
    })
    .join()
    .unwrap();
    assert_eq!((first, second), ("1000".into(), "1001".into()));
}

#[test]
#[ignore = "requires live isolated TypeScript and ethers oracle"]
fn builder_oracle_requires_real_ethers() {
    let output = std::process::Command::new("node")
        .args([
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/builder-oracle.cjs"),
            "--missing-package-control",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MODULE_NOT_FOUND") && stderr.contains("asterdex-missing-ethers-control"),
        "{stderr}"
    );
}

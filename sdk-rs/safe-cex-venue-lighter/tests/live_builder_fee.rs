use serde::Deserialize;
mod support;
use safe_cex_types::ExchangeName;
use safe_cex_venue_lighter::builder_fee::*;
use serde_json::{Value, json};
use std::process::Command;

fn mode_name(mode: LighterBuilderFeeMode) -> &'static str {
    match mode {
        LighterBuilderFeeMode::Dynamic => "dynamic",
        LighterBuilderFeeMode::Static => "static",
    }
}

#[test]
#[ignore = "requires Node and the maintained TypeScript compiler"]
fn lighter_builder_fee_tables_and_modes_match_live_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/builder-fee-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, count) in [
        ("resolved", 17),
        ("defaults", 12),
        ("bigints", 29),
        ("configs", 696),
    ] {
        assert_eq!(
            oracle[name].as_array().unwrap().len(),
            count,
            "TYPE-CARDINALITY {name} source cases"
        );
    }

    for literal in oracle["modes"].as_array().unwrap() {
        let invalid = serde_json::json!({literal.as_str().unwrap():null});
        assert!(
            LighterBuilderFeeMode::deserialize(invalid).is_err(),
            "LighterBuilderFeeMode public qualified decoder rejects object"
        );
    }

    support::assert_string_only::<LighterBuilderFeeMode>(&oracle["modes"], "LighterBuilderFeeMode");

    let tables = json!({
        "TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET":TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_MAINNET,
        "TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET":TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_TESTNET,
        "TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD":TEALSTREET_LIGHTER_INTEGRATOR_ACCOUNT_INDEX_ROBINHOOD,
        "LIGHTER_INTEGRATOR_MAKER_FEE_VALUE":LIGHTER_INTEGRATOR_MAKER_FEE_VALUE,
        "LIGHTER_INTEGRATOR_TAKER_FEE_VALUE":LIGHTER_INTEGRATOR_TAKER_FEE_VALUE,
    });
    assert_eq!(
        tables.as_object().unwrap().keys().collect::<Vec<_>>(),
        oracle["tables"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        "builder fee table inventory"
    );
    for (key, value) in tables.as_object().unwrap() {
        if value.is_number() {
            assert_eq!(value.as_f64(), oracle["tables"][key].as_f64(), "{key}");
        } else {
            assert_eq!(value, &oracle["tables"][key], "{key}");
        }
    }
    assert_eq!(
        json!(support::enum_inventory::<LighterBuilderFeeMode>()),
        oracle["modes"],
        "LighterBuilderFeeMode accepted wire inventory"
    );
    let modes = [
        LighterBuilderFeeMode::Dynamic,
        LighterBuilderFeeMode::Static,
    ];
    assert_eq!(
        serde_json::to_value(modes).unwrap(),
        oracle["modes"],
        "LighterBuilderFeeMode serialization"
    );
    for (mode, literal) in modes.into_iter().zip(oracle["modes"].as_array().unwrap()) {
        assert_eq!(
            mode_name(mode),
            literal.as_str().unwrap(),
            "LighterBuilderFeeMode exhaustive inventory"
        );
        assert_eq!(
            serde_json::from_value::<LighterBuilderFeeMode>(literal.clone()).unwrap(),
            mode
        );
    }
    assert_eq!(
        mode_name(resolve_mode(None)),
        oracle["missingMode"].as_str().unwrap(),
        "resolveMode undefined"
    );
    for case in oracle["resolved"].as_array().unwrap() {
        assert_eq!(
            serde_json::from_value::<LighterBuilderFeeMode>(case["input"].clone()).is_ok(),
            oracle["modes"].as_array().unwrap().contains(&case["input"]),
            "LighterBuilderFeeMode exact decoding {case}"
        );
        assert_eq!(
            mode_name(resolve_mode(Some(&case["input"]))),
            case["expected"].as_str().unwrap(),
            "resolveMode {case}"
        );
    }
    for case in oracle["defaults"].as_array().unwrap() {
        let exchange_name: Option<ExchangeName> =
            serde_json::from_value(case["exchangeName"].clone()).unwrap();
        assert_eq!(
            get_default_integrator_account_index(case["testnet"].as_bool(), exchange_name.as_ref()),
            case["expected"].as_str(),
            "getDefaultIntegratorAccountIndex {case}"
        );
    }
    assert_eq!(
        oracle["configSchema"],
        json!([
            {"name":"integratorAccountIndex","type":"bigint","optional":false},
            {"name":"integratorMakerFee","type":"number","optional":false},
            {"name":"integratorTakerFee","type":"number","optional":false},
            {"name":"mode","type":"LighterBuilderFeeMode","optional":false}
        ]),
        "LighterBuilderFeeConfig schema"
    );
    for case in oracle["bigints"].as_array().unwrap() {
        let actual = parse_positive_integer_big_int(case.get("input"))
            .map(|v| json!({"$bigint":v.to_string()}))
            .unwrap_or(Value::Null);
        assert_eq!(
            actual, case["expected"],
            "parsePositiveIntegerBigInt {case}"
        );
    }
    for case in oracle["configs"].as_array().unwrap() {
        let options = &case["options"];
        let extra = &options["extra"];
        let name: Option<ExchangeName> =
            serde_json::from_value(case["exchangeName"].clone()).unwrap();
        let actual = resolve_lighter_builder_fee_config(
            options["testnet"].as_bool(),
            extra.get("lighterBuilderFeeMode"),
            extra.get("lighterIntegratorAccountIndex"),
            name.as_ref(),
        );
        let Some(actual) = actual else {
            assert!(
                case["expected"].is_null(),
                "resolveLighterBuilderFeeConfig {case}"
            );
            continue;
        };
        let expected = &case["expected"];
        assert_eq!(
            actual.integrator_account_index.to_string(),
            expected["integratorAccountIndex"]["$bigint"]
                .as_str()
                .unwrap(),
            "resolveLighterBuilderFeeConfig integratorAccountIndex {case}"
        );
        assert_eq!(
            actual.integrator_maker_fee,
            expected["integratorMakerFee"].as_f64().unwrap(),
            "resolveLighterBuilderFeeConfig integratorMakerFee {case}"
        );
        assert_eq!(
            actual.integrator_taker_fee,
            expected["integratorTakerFee"].as_f64().unwrap(),
            "resolveLighterBuilderFeeConfig integratorTakerFee {case}"
        );
        assert_eq!(
            mode_name(actual.mode),
            expected["mode"].as_str().unwrap(),
            "resolveLighterBuilderFeeConfig mode {case}"
        );
    }
}

#[test]
fn builder_config_retains_the_full_declared_numeric_domain() {
    let negative = num_bigint::BigInt::parse_bytes(b"-123456789012345678901234567890", 10).unwrap();
    let config = LighterBuilderFeeConfig {
        integrator_account_index: negative.clone(),
        integrator_maker_fee: 16_777_217.25,
        integrator_taker_fee: -1e100,
        mode: LighterBuilderFeeMode::Dynamic,
    };
    assert_eq!(config.integrator_account_index, negative);
    assert_eq!(config.integrator_maker_fee, 16_777_217.25);
    assert_eq!(config.integrator_taker_fee, -1e100);
    for fee in [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let config = LighterBuilderFeeConfig {
            integrator_maker_fee: fee,
            integrator_taker_fee: fee,
            ..config.clone()
        };
        assert_eq!(config.integrator_maker_fee.to_bits(), fee.to_bits());
        assert_eq!(config.integrator_taker_fee.to_bits(), fee.to_bits());
    }
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn source_cardinality_controls_fail_by_name() {
    for (field, assertion) in [
        ("resolved", "TYPE-CARDINALITY resolved source cases"),
        ("defaults", "TYPE-CARDINALITY defaults source cases"),
        ("bigints", "TYPE-CARDINALITY bigints source cases"),
        ("configs", "TYPE-CARDINALITY configs source cases"),
    ] {
        for mode in ["empty", "subset"] {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "lighter_builder_fee_tables_and_modes_match_live_typescript",
                    "--ignored",
                    "--nocapture",
                ])
                .env(
                    "LIGHTER_TYPE_CARDINALITY_MUTATION",
                    format!("{field}:{mode}"),
                )
                .output()
                .unwrap();
            let log = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.status.code(), Some(101), "{log}");
            assert!(log.contains(assertion), "{log}");
        }
    }
}

use safe_cex_num::JsJsonValue;
use safe_cex_venue_apexomni::signer_utils as api;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn capture(mutation: Option<&str>) -> Value {
    let mut command = Command::new("node");
    command
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signer-utils.cjs"
        ))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH);
    if let Some(mutation) = mutation {
        command.env("APEXOMNI_SIGNER_MUTATE_TS", mutation);
    }
    let output = command
        .output()
        .expect("execute actual TypeScript signer utilities");
    assert!(
        output.status.success(),
        "TypeScript signer utilities: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn oracle() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| capture(None))
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn canonical(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn prepared(row: &Value) -> JsJsonValue {
    if row["type"] == "number" {
        return JsJsonValue::Number(number(&row["bits"]));
    }
    match row["id"].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "false" => JsJsonValue::Bool(false),
        "true" => JsJsonValue::Bool(true),
        "emptyString" => JsJsonValue::String(String::new()),
        "string" => JsJsonValue::String("α😀".into()),
        "array" => JsJsonValue::Array(vec![]),
        "nestedArray" => JsJsonValue::Array(vec![
            JsJsonValue::Number(1.0),
            JsJsonValue::Array(vec![JsJsonValue::Number(2.0)]),
        ]),
        "object" => JsJsonValue::Object(vec![]),
        "arrayLike" => JsJsonValue::Object(vec![("length".into(), JsJsonValue::Number(0.0))]),
        "bigint" => JsJsonValue::BigInt(1.into()),
        other => panic!("unclassified oracle input {other}"),
    }
}
fn outcome(result: Result<(), String>) -> Value {
    match result {
        Ok(()) => json!({"ok":true}),
        Err(message) => json!({"ok":false,"name":"Error","message":message}),
    }
}
fn timestamp(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "timestamps", 15) {
        comparisons += 1;
        let mut calls = 0;
        let result = api::timestamp(|| {
            calls += 1;
            number(&row["inputBits"])
        });
        assert_eq!(row["type"], "number");
        assert_eq!(
            calls,
            row["calls"].as_u64().unwrap(),
            "timestamp clock draw count"
        );
        assert_eq!(
            canonical(result),
            row["outputBits"],
            "timestamp native bits: {row}"
        );
    }
    assert_eq!(
        comparisons, 15,
        "APEX-CARDINALITY native timestamps comparisons"
    );
}
fn assertion(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "asserts", 6) {
        comparisons += 1;
        assert_eq!(
            outcome(api::assert(
                row["condition"].as_bool().unwrap(),
                row["message"].as_str().unwrap()
            )),
            row["result"],
            "assert exact message: {row}"
        );
    }
    assert_eq!(
        comparisons, 6,
        "APEX-CARDINALITY native asserts comparisons"
    );
}
type Check = fn(&JsJsonValue, &str) -> Result<(), String>;
fn predicate(corpus: &Value, name: &str, positive: &str, check: Check) {
    let cases = corpus_rows(corpus, "cases", 18);
    let row = cases
        .iter()
        .find(|row| row["id"] == positive)
        .unwrap_or_else(|| panic!("APEX-CARDINALITY {name} mandatory positive case"));
    let message = format!("message:{name}:α😀");
    // This successful axis is independent of assert's error-message mutation.
    assert_eq!(
        outcome(check(&prepared(row), &message)),
        row["results"][name],
        "{name} independent positive axis"
    );
    let mut comparisons = 0;
    for row in cases {
        comparisons += 1;
        assert_eq!(
            outcome(check(&prepared(row), &message)),
            row["results"][name],
            "{name} actual TypeScript result: {row}"
        );
    }
    assert_eq!(comparisons, 18, "APEX-CARDINALITY native cases comparisons");
}
fn array(c: &Value) {
    predicate(c, "assertTypeArray", "array", api::assert_type_array);
}
fn boolean(c: &Value) {
    predicate(c, "assertTypeBoolean", "false", api::assert_type_boolean);
}
fn numeric(c: &Value) {
    predicate(c, "assertTypeNumber", "nan", api::assert_type_number);
}
fn string(c: &Value) {
    predicate(
        c,
        "assertTypeString",
        "emptyString",
        api::assert_type_string,
    );
}
fn subaccount(c: &Value) {
    predicate(
        c,
        "assertSubAccountId",
        "undefined",
        api::assert_sub_account_id,
    );
}
#[test]
fn signer_rows_match_actual_typescript() {
    let corpus = oracle();
    assert_eq!(corpus["header"]["rustArch"], std::env::consts::ARCH);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() >= 20);
    assert!(!corpus["header"]["nodeArch"].as_str().unwrap().is_empty());
    eprintln!("ApexOmni signer utility capture {}", corpus["header"]);
    let mut failures = 0;
    for check in [
        timestamp, assertion, array, boolean, numeric, string, subaccount,
    ] {
        failures += usize::from(std::panic::catch_unwind(|| check(corpus)).is_err());
    }
    assert_eq!(
        failures, 0,
        "all seven independent signer utility row assertions"
    );
}
#[test]
fn declared_prepared_value_boundaries() {
    assert_eq!(
        oracle()["boundaries"]["foreignArray"],
        json!({"ok":false,"name":"Error","message":"foreign array"})
    );
    assert!(api::assert_type_array(&JsJsonValue::Array(vec![]), "prepared array").is_ok());
    assert_eq!(oracle()["boundaries"]["loneSurrogateUnits"], json!([55296]));
    assert!(
        serde_json::from_str::<String>(r#""\ud800""#).is_err(),
        "shared UTF8 message boundary"
    );
}
#[test]
fn actual_source_mutations_fail_named_assertions() {
    for (mutation, check, label) in [
        (
            "timestamp",
            timestamp as fn(&Value),
            "timestamp native bits",
        ),
        ("assert", assertion as fn(&Value), "assert exact message"),
        (
            "assertTypeArray",
            array as fn(&Value),
            "assertTypeArray independent positive axis",
        ),
        (
            "assertTypeBoolean",
            boolean as fn(&Value),
            "assertTypeBoolean independent positive axis",
        ),
        (
            "assertTypeNumber",
            numeric as fn(&Value),
            "assertTypeNumber independent positive axis",
        ),
        (
            "assertTypeString",
            string as fn(&Value),
            "assertTypeString independent positive axis",
        ),
        (
            "assertSubAccountId",
            subaccount as fn(&Value),
            "assertSubAccountId independent positive axis",
        ),
    ] {
        let corpus = capture(Some(mutation));
        let failure =
            std::panic::catch_unwind(|| check(&corpus)).expect_err("mutated TS must differ");
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.contains(label),
            "specific source mutation assertion: {message}"
        );
    }
}

fn validator_value(value: &Value) -> JsJsonValue {
    match value["kind"].as_str().unwrap() {
        "undefined" => JsJsonValue::Undefined,
        "null" => JsJsonValue::Null,
        "number" => JsJsonValue::Number(number(&value["bits"])),
        "bigint" => JsJsonValue::BigInt(value["value"].as_str().unwrap().parse().unwrap()),
        "string" => JsJsonValue::String(value["value"].as_str().unwrap().into()),
        "boolean" => JsJsonValue::Bool(value["value"].as_bool().unwrap()),
        "array" => JsJsonValue::Array(
            value["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(validator_value)
                .collect(),
        ),
        "object" => JsJsonValue::Object(
            value["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (pair[0].as_str().unwrap().into(), validator_value(&pair[1])))
                .collect(),
        ),
        kind => panic!("unclassified validator input {kind}"),
    }
}
const VALIDATORS: [(&str, Check); 8] = [
    ("assertChainId", api::assert_chain_id),
    ("assertAccountId", api::assert_account_id),
    ("assertNonce", api::assert_nonce),
    ("assertTokenId", api::assert_token_id),
    ("assertPubKeyHash", api::assert_pub_key_hash),
    ("assertTs", api::assert_ts),
    ("assertWithdrawFeeRatio", api::assert_withdraw_fee_ratio),
    ("assertTradingFeeRatio", api::assert_trading_fee_ratio),
];
fn validator(corpus: &Value, name: &str, check: Check) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "validators", 98) {
        comparisons += 1;
        let actual = check(
            &validator_value(&row["input"]),
            row["message"].as_str().unwrap(),
        );
        let expected = &row["results"][name];
        // API deliberately projects Error/TypeError objects to their message.
        // Do not fabricate a native error class for this comparison.
        let expected: Result<(), String> = if expected["ok"].as_bool().unwrap() {
            Ok(())
        } else {
            Err(expected["message"].as_str().unwrap().into())
        };
        assert_eq!(actual, expected, "{name} direct result: {row}");
    }
    assert_eq!(
        comparisons, 98,
        "APEX-CARDINALITY native validators comparisons"
    );
}
#[test]
fn validator_rows_match_actual_typescript() {
    let corpus = oracle();
    eprintln!("ApexOmni validator capture {}", corpus["header"]);
    let mut failures = 0;
    for (name, check) in VALIDATORS {
        failures +=
            usize::from(std::panic::catch_unwind(|| validator(corpus, name, check)).is_err());
    }
    assert_eq!(
        failures, 0,
        "all eight independent validator row assertions"
    );
}
#[test]
fn validator_source_mutations_fail_named_assertions() {
    for (name, check) in VALIDATORS {
        let corpus = capture(Some(name));
        let failure = std::panic::catch_unwind(|| validator(&corpus, name, check))
            .expect_err("mutated validator must differ");
        let message = failure
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| failure.downcast_ref::<&str>().copied())
            .unwrap_or("");
        assert!(
            message.contains(&format!("{name} direct result")),
            "specific validator mutation assertion: {message}"
        );
    }
}

fn corpus_rows<'a>(corpus: &'a Value, key: &str, count: usize) -> &'a [Value] {
    let rows = corpus[key].as_array().unwrap();
    assert_eq!(rows.len(), count, "APEX-CARDINALITY source {key} total");
    rows
}

#[test]
fn source_cardinality_rejects_empty_and_subset() {
    let original = oracle();
    for (key, count) in [
        ("cases", 18),
        ("asserts", 6),
        ("timestamps", 15),
        ("validators", 98),
    ] {
        for keep in [0, count - 1] {
            let mut shortened = original.clone();
            shortened[key].as_array_mut().unwrap().truncate(keep);
            let error = std::panic::catch_unwind(|| corpus_rows(&shortened, key, count))
                .expect_err("source omission must fail");
            let text = error.downcast_ref::<String>().unwrap();
            assert!(text.contains(&format!("APEX-CARDINALITY source {key} total")));
        }
    }
}

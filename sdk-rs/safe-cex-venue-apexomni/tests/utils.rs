use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_apexomni::{order_records::Iso8601, utils::*};
use serde_json::Value;
use std::{process::Command, sync::OnceLock};

fn capture(mutation: Option<&str>) -> Value {
    let mut command = Command::new("node");
    command
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/utils.cjs"))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH);
    if let Some(mutation) = mutation {
        command.env("APEXOMNI_UTILS_MUTATE_TS", mutation);
    }
    let output = command
        .output()
        .expect("execute actual TypeScript utilities");
    assert!(
        output.status.success(),
        "TypeScript utilities: {}",
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
fn canonical(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8000000000000
    } else {
        value.to_bits()
    }
}
fn precision(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "precision", 33) {
        comparisons += 1;
        let input: Iso8601 = js_from_str(row["raw"].as_str().unwrap()).unwrap();
        match &input {
            Iso8601::Number(value) => {
                assert_eq!(row["type"], "number");
                assert_eq!(
                    canonical(*value),
                    canonical(number(&row["bits"])),
                    "getPrecision ingress bits"
                );
            }
            Iso8601::String(_) => assert_eq!(row["type"], "string"),
        }
        assert_eq!(
            get_precision(&input),
            row["output"].as_u64().unwrap() as usize,
            "getPrecision actual TypeScript result: {row}"
        );
    }
    assert_eq!(
        comparisons, 33,
        "APEX-CARDINALITY native precision comparisons"
    );
}
fn precision_native(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "native", 8) {
        comparisons += 1;
        assert_eq!(row["type"], "number");
        assert_eq!(
            get_precision(&Iso8601::Number(number(&row["bits"]))),
            row["output"].as_u64().unwrap() as usize,
            "getPrecision native TypeScript result: {row}"
        );
    }
    assert_eq!(comparisons, 8, "APEX-CARDINALITY native native comparisons");
}
fn random(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "random", 10) {
        comparisons += 1;
        let mut calls = 0;
        let output = generate_apex_random_client_id(|| {
            calls += 1;
            number(&row["bits"])
        });
        assert_eq!(
            calls,
            row["calls"].as_u64().unwrap(),
            "generateApeXRandomClientId draw count"
        );
        assert_eq!(
            output,
            row["output"].as_str().unwrap(),
            "generateApeXRandomClientId actual TypeScript result: {row}"
        );
    }
    assert_eq!(
        comparisons, 10,
        "APEX-CARDINALITY native random comparisons"
    );
}
fn prefix(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "prefix", 10) {
        comparisons += 1;
        assert_eq!(
            drop_prefix(row["input"].as_str().unwrap()),
            row["output"].as_str().unwrap(),
            "dropPrefix actual TypeScript result: {row}"
        );
    }
    assert_eq!(
        comparisons, 10,
        "APEX-CARDINALITY native prefix comparisons"
    );
}
#[test]
fn utility_rows_match_actual_typescript() {
    let corpus = oracle();
    assert_eq!(corpus["header"]["rustArch"], std::env::consts::ARCH);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() >= 20);
    assert!(!corpus["header"]["nodeArch"].as_str().unwrap().is_empty());
    eprintln!("ApexOmni utility capture {}", corpus["header"]);
    let mut failures = 0;
    for check in [precision, precision_native, random, prefix] {
        failures += usize::from(std::panic::catch_unwind(|| check(corpus)).is_err());
    }
    assert_eq!(failures, 0, "all independent utility row assertions");
}
#[test]
fn source_mutations_fail_named_assertions() {
    for (mutation, check, label) in [
        (
            "precision",
            precision as fn(&Value),
            "getPrecision actual TypeScript result",
        ),
        (
            "random",
            random as fn(&Value),
            "generateApeXRandomClientId actual TypeScript result",
        ),
        (
            "prefix",
            prefix as fn(&Value),
            "dropPrefix actual TypeScript result",
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

fn corpus_rows<'a>(corpus: &'a Value, key: &str, count: usize) -> &'a [Value] {
    let rows = corpus[key].as_array().unwrap();
    assert_eq!(rows.len(), count, "APEX-CARDINALITY source {key} total");
    rows
}

#[test]
fn source_cardinality_rejects_empty_and_subset() {
    let original = oracle();
    for (key, count) in [
        ("precision", 33),
        ("native", 8),
        ("random", 10),
        ("prefix", 10),
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

fn exchange_capture(mutation: Option<&str>) -> Value {
    let mut command = Command::new("node");
    command
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-utils.cjs"
        ))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH);
    if let Some(mutation) = mutation {
        command.env("APEXOMNI_EXCHANGE_UTILS_MUTATE_TS", mutation);
    }
    let output = command
        .output()
        .expect("execute actual TypeScript exchange helpers");
    assert!(
        output.status.success(),
        "actual exchange helpers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn exchange_oracle() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| exchange_capture(None))
}
fn epoch_hours(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "epoch", 34) {
        let input = row["input"].as_str().unwrap();
        assert_eq!(
            canonical(safe_cex_num::number_from_string(input)),
            canonical(number(&row["inputBits"])),
            "isoTimestampToEpochHours input bits"
        );
        let actual = safe_cex_venue_apexomni::exchange_utils::iso_timestamp_to_epoch_hours(input);
        assert_eq!(
            canonical(actual),
            canonical(number(&row["outputBits"])),
            "isoTimestampToEpochHours native result: {row}"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 34, "APEX-CARDINALITY native epoch comparisons");
}
fn expiration_buffer(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "buffer", 16) {
        let actual = safe_cex_venue_apexomni::exchange_utils::add_order_expiration_buffer_hours(
            number(&row["inputBits"]),
        );
        assert_eq!(
            canonical(actual),
            canonical(number(&row["outputBits"])),
            "addOrderExpirationBufferHours native result: {row}"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 16,
        "APEX-CARDINALITY native buffer comparisons"
    );
}
#[test]
fn exchange_helpers_match_actual_typescript() {
    let corpus = exchange_oracle();
    assert_eq!(corpus["header"]["rustArch"], std::env::consts::ARCH);
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() >= 20);
    assert!(!corpus["header"]["nodeArch"].as_str().unwrap().is_empty());
    eprintln!("ApexOmni exchange helper capture {}", corpus["header"]);
    let mut failures = 0;
    for check in [epoch_hours, expiration_buffer] {
        failures += usize::from(std::panic::catch_unwind(|| check(corpus)).is_err());
    }
    assert_eq!(failures, 0, "both exchange helper rows");
}
#[test]
fn exchange_helper_source_controls() {
    for (mutation, check, label) in [
        (
            "epoch",
            epoch_hours as fn(&Value),
            "isoTimestampToEpochHours native result",
        ),
        (
            "buffer",
            expiration_buffer as fn(&Value),
            "addOrderExpirationBufferHours native result",
        ),
    ] {
        let corpus = exchange_capture(Some(mutation));
        let error = std::panic::catch_unwind(|| check(&corpus))
            .expect_err("actual source mutation must fail");
        assert!(error.downcast_ref::<String>().unwrap().contains(label));
    }
}
#[test]
fn exchange_helper_cardinality_controls() {
    for (key, count, check) in [
        ("epoch", 34, epoch_hours as fn(&Value)),
        ("buffer", 16, expiration_buffer as fn(&Value)),
        ("ids", 440, random_ids as fn(&Value)),
        ("clients", 396, client_ids as fn(&Value)),
        ("brackets", 960, bracket_prices as fn(&Value)),
    ] {
        for keep in [0, count - 1] {
            let mut corpus = exchange_oracle().clone();
            corpus[key].as_array_mut().unwrap().truncate(keep);
            let error =
                std::panic::catch_unwind(|| check(&corpus)).expect_err("source omission must fail");
            assert!(
                error
                    .downcast_ref::<String>()
                    .unwrap()
                    .contains(&format!("APEX-CARDINALITY source {key} total"))
            );
        }
    }
}

fn random_ids(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "ids", 440) {
        let e = row["lengthBits"]
            .as_str()
            .map(|_| number(&row["lengthBits"]));
        let t = row["alphabet"].as_str();
        let draws = row["drawBits"].as_array().unwrap();
        assert!(!draws.is_empty(), "d8 source draw pattern nonempty");
        let mut calls = 0;
        let result = safe_cex_venue_apexomni::exchange_utils::d8(e, t, || {
            let result = number(&draws[calls % draws.len()]);
            calls += 1;
            result
        });
        let expected_calls = if let Some(e) = e {
            if e > 1.0 { e.ceil() as usize } else { 1 }
        } else {
            16
        };
        assert_eq!(calls, expected_calls, "d8 native draw cardinality");
        assert_eq!(
            row["calls"].as_u64().unwrap() as usize,
            expected_calls,
            "d8 source draw cardinality"
        );
        use safe_cex_venue_apexomni::exchange_utils::RandomId;
        let (kind, output, bits) = match &result {
            RandomId::Undefined => ("undefined", None, None),
            RandomId::Nan => ("number", None, Some("7ff8000000000000")),
            RandomId::String(value) => ("string", Some(value.as_str()), None),
        };
        assert_eq!(output, row["output"].as_str(), "d8 native result: {row}");
        assert_eq!(
            kind,
            row["type"].as_str().unwrap(),
            "d8 native result type: {row}"
        );
        assert_eq!(
            bits,
            row["outputBits"].as_str(),
            "d8 native result numeric bits: {row}"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 440, "APEX-CARDINALITY native ids comparisons");
}
fn client_ids(corpus: &Value) {
    let mut comparisons = 0;
    for row in corpus_rows(corpus, "clients", 396) {
        let draws = row["drawBits"].as_array().unwrap();
        assert!(!draws.is_empty(), "client ID source draw pattern nonempty");
        let account = row["account"].as_str().unwrap();
        let clock = number(&row["clockBits"]);
        let trace = std::cell::RefCell::new(Vec::new());
        let mut calls = 0;
        let result = safe_cex_venue_apexomni::exchange_utils::generate_apex_random_client_id(
            account,
            || {
                trace.borrow_mut().push("now");
                clock
            },
            || {
                trace.borrow_mut().push("random");
                let result = number(&draws[calls % draws.len()]);
                calls += 1;
                result
            },
        );
        let prefix = format!(
            "apexomni-{account}-{}-",
            safe_cex_num::js_number_string(clock)
        );
        assert!(
            result.starts_with(&prefix),
            "generateApeXRandomClientId independent native prefix"
        );
        assert_eq!(calls, 6, "client ID native RNG cardinality");
        assert_eq!(row["calls"], 6, "client ID source RNG cardinality");
        assert_eq!(
            *trace.borrow(),
            [
                "now", "random", "random", "random", "random", "random", "random"
            ],
            "client ID native host-call order"
        );
        assert_eq!(
            serde_json::to_value(&*trace.borrow()).unwrap(),
            row["trace"],
            "client ID source host-call order"
        );
        assert_eq!(
            result,
            row["output"].as_str().unwrap(),
            "generateApeXRandomClientId exchange native result: {row}"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 396,
        "APEX-CARDINALITY native clients comparisons"
    );
}
#[test]
fn client_id_helpers_match_actual_typescript() {
    let mut failures = 0;
    for check in [random_ids, client_ids] {
        failures += usize::from(std::panic::catch_unwind(|| check(exchange_oracle())).is_err());
    }
    assert_eq!(failures, 0, "both independent client ID rows");
}
#[test]
fn client_id_source_controls() {
    for (mutation, check, label) in [
        ("d8", random_ids as fn(&Value), "d8 native result"),
        (
            "client",
            client_ids as fn(&Value),
            "generateApeXRandomClientId exchange native result",
        ),
    ] {
        let corpus = exchange_capture(Some(mutation));
        let error = std::panic::catch_unwind(|| check(&corpus))
            .expect_err("actual source mutation must fail");
        assert!(error.downcast_ref::<String>().unwrap().contains(label));
    }
}

fn bracket_prices(corpus: &Value) {
    let mut comparisons = 0;
    let mut sides = std::collections::BTreeMap::new();
    for row in corpus_rows(corpus, "brackets", 960) {
        let side = row["side"].as_str().unwrap();
        *sides.entry(side).or_insert(0) += 1;
        assert_eq!(
            row["outputType"], "number",
            "calculateTpSlPrice source native type"
        );
        let actual = safe_cex_venue_apexomni::exchange_utils::calculate_tp_sl_price(
            side,
            number(&row["priceBits"]),
            number(&row["stepBits"]),
        );
        assert_eq!(
            canonical(actual),
            canonical(number(&row["outputBits"])),
            "calculateTpSlPrice native bits: {row}"
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 960,
        "APEX-CARDINALITY native bracket comparisons"
    );
    assert_eq!(
        sides,
        std::collections::BTreeMap::from([
            ("BUY", 160),
            ("SELL", 160),
            ("buy", 160),
            ("", 160),
            ("OTHER", 160),
            ("😀", 160)
        ]),
        "APEX-CARDINALITY bracket side inventories"
    );
}
#[test]
fn bracket_prices_match_actual_typescript() {
    bracket_prices(exchange_oracle());
}
#[test]
fn bracket_price_source_control() {
    let error = std::panic::catch_unwind(|| bracket_prices(&exchange_capture(Some("tpsl"))))
        .expect_err("actual-source slippage swap must fail");
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("calculateTpSlPrice native bits")
    );
}

#[path = "support/prefix_boundary.rs"]
mod prefix_boundary_tests;

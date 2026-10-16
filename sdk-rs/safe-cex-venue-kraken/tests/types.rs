#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use safe_cex_venue_kraken::{spot_types as s, types as f};
use serde::Serialize;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/types-oracle.cjs"
            ))
            .output()
            .expect("start live TypeScript oracle");
        assert!(
            output.status.success(),
            "TypeScript oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("oracle JSON");
        eprintln!("types capture header: {}", value["header"]);
        assert!(
            value["header"]["node"].as_str().unwrap().starts_with('v'),
            "Node capture version"
        );
        assert!(
            !value["header"]["arch"].as_str().unwrap().is_empty(),
            "CPU capture architecture"
        );
        cardinality::source_census("types", &value);
        value
    })
}
fn text(value: &str) -> Value {
    json!({"kind":"string", "units":value.encode_utf16().collect::<Vec<_>>()})
}
fn string_enum<T: Serialize>(value: &T) -> Value {
    let wire = serde_json::to_value(value).unwrap();
    text(wire.as_str().expect("string-valued table"))
}
fn number(value: &f64) -> Value {
    // Native binary64 capture precedes JSON. Only NaN payload/sign is unobservable in JS.
    let bits = if value.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        value.to_bits()
    };
    json!({"kind":"number", "bits":format!("{bits:016x}")})
}
fn inherited(member: ProtoMember) -> Value {
    match member.length() {
        Some(length) => json!({"kind":"function", "text":member.js_string(), "length":length}),
        None => json!({"kind":"object", "identity":"Object.prototype"}),
    }
}
fn table_value<T>(table: &[(&str, T)], encode: &impl Fn(&T) -> Value) -> Value {
    Value::Array(
        table
            .iter()
            .map(|(key, value)| json!([key, encode(value)]))
            .collect(),
    )
}
fn check_table<T>(family: &str, name: &str, table: &[(&str, T)], encode: impl Fn(&T) -> Value) {
    let reference = oracle();
    cardinality::native_len(
        table.len(),
        &reference[family][name],
        &format!("table {family}.{name}"),
    );
    assert_eq!(
        table_value(table, &encode),
        reference[family][name],
        "table {family}.{name}"
    );
    check_probes(
        &format!("{family}.{name}"),
        table,
        &encode,
        &reference["probes"][family][name],
    );
}
fn check_probes<T>(
    label: &str,
    table: &[(&str, T)],
    encode: &impl Fn(&T) -> Value,
    probes: &Value,
) {
    let census: Value = serde_json::from_str(include_str!("support/cardinality.json")).unwrap();
    let parts: Vec<_> = label.split('.').collect();
    let count = if parts.len() == 2 {
        census["types"]["probes"][parts[0]][parts[1]]
            .as_u64()
            .unwrap()
    } else {
        census["types"]["nestedProbes"][parts[0]][parts[2]]
            .as_u64()
            .unwrap()
    } as usize;
    for index in 0..count {
        let probe = probes
            .as_array()
            .unwrap()
            .get(index)
            .expect("KRAKEN-CARDINALITY probe index coverage");
        let key = probe[0].as_str().unwrap();
        let actual = match js_table_lookup(table, key) {
            TableHit::Mapped(value) => encode(value),
            TableHit::Inherited(member) => inherited(member),
            TableHit::Missing => json!({"kind":"undefined"}),
        };
        assert_eq!(actual, probe[1], "lookup {label}[{key}]");
    }
}
fn check_array(family: &str, name: &str, values: &[&str]) {
    cardinality::native_len(
        values.len(),
        &oracle()[family][name],
        &format!("array {family}.{name}"),
    );
    assert_eq!(
        Value::Array(values.iter().map(|s| text(s)).collect()),
        oracle()[family][name],
        "array {family}.{name}"
    );
}

#[test]
fn types_live_oracle() {
    check_table("futures", "BASE_URL", f::BASE_URL, string_enum);
    check_table(
        "futures",
        "HISTORY_BASE_URL",
        f::HISTORY_BASE_URL,
        string_enum,
    );
    check_table("futures", "BASE_WS_URL", f::BASE_WS_URL, |table| {
        table_value(table, &string_enum)
    });
    for (family, table) in [("futures", f::BASE_WS_URL), ("spot", s::BASE_WS_URL)] {
        for (role, networks) in table {
            check_probes(
                &format!("{family}.BASE_WS_URL.{role}"),
                networks,
                &string_enum,
                &oracle()["nestedProbes"][family][role],
            );
        }
    }
    check_table("futures", "ENDPOINTS", f::ENDPOINTS, string_enum);
    check_array("futures", "PUBLIC_ENDPOINTS", f::PUBLIC_ENDPOINTS);
    check_table("futures", "ORDER_TYPE", f::ORDER_TYPE, string_enum);
    check_table("futures", "ORDER_SIDE", f::ORDER_SIDE, string_enum);
    check_table("futures", "POSITION_SIDE", f::POSITION_SIDE, string_enum);
    check_table("futures", "TIME_IN_FORCE", f::TIME_IN_FORCE, string_enum);
    check_table(
        "futures",
        "ORDER_TRIGGER_TYPE",
        f::ORDER_TRIGGER_TYPE,
        string_enum,
    );
    check_table("spot", "BASE_URL", s::BASE_URL, string_enum);
    check_table("spot", "BASE_WS_URL", s::BASE_WS_URL, |table| {
        table_value(table, &string_enum)
    });
    check_table("spot", "ENDPOINTS", s::ENDPOINTS, string_enum);
    check_array("spot", "PUBLIC_ENDPOINTS", s::PUBLIC_ENDPOINTS);
    check_array(
        "spot",
        "GRAFANA_ENDPOINTS_EVERY",
        s::GRAFANA_ENDPOINTS_EVERY,
    );
    check_array(
        "spot",
        "GRAFANA_ENDPOINTS_PERIODIC",
        s::GRAFANA_ENDPOINTS_PERIODIC,
    );
    check_table("spot", "INTERVAL", s::INTERVAL, number);
    check_table("spot", "ORDER_TYPE", s::ORDER_TYPE, string_enum);
    check_table("spot", "ORDER_SIDE", s::ORDER_SIDE, string_enum);
    check_table("spot", "TIME_IN_FORCE", s::TIME_IN_FORCE, string_enum);
    check_table("spot", "ASSET_ALIASES", s::ASSET_ALIASES, string_enum);
    assert_eq!(
        oracle()["futures"].as_object().unwrap().len(),
        10,
        "futures export census"
    );
    assert_eq!(
        oracle()["spot"].as_object().unwrap().len(),
        11,
        "spot export census"
    );
}

#[test]
fn normalize_asset_live_oracle() {
    for case in cardinality::cases("types", "normalize", oracle()) {
        let input: Vec<u16> = serde_json::from_value(case["input"].clone()).unwrap();
        let actual = match s::normalize_asset(&input) {
            TableHit::Mapped(value) => json!({"kind":"string", "units":value.as_ref()}),
            TableHit::Inherited(member) => inherited(member),
            TableHit::Missing => panic!("normalizeAsset must return the unmatched input"),
        };
        assert_eq!(actual, case["output"], "normalizeAsset {input:?}");
    }
}

#[test]
fn base_url_union_live_source() {
    use f::KrakenBaseUrl;
    use safe_cex_num::numeric_ingress::js_from_str;
    let variants = [KrakenBaseUrl::Derivatives, KrakenBaseUrl::History];
    assert_eq!(
        serde_json::to_value(variants).unwrap(),
        oracle()["variants"],
        "KrakenBaseUrl literal census"
    );
    cardinality::native_len(
        variants.len(),
        &oracle()["variants"],
        "KrakenBaseUrl native variant count",
    );
    for (wire, variant) in oracle()["variants"]
        .as_array()
        .unwrap()
        .iter()
        .zip(variants)
    {
        let decoded: KrakenBaseUrl = js_from_str(&wire.to_string()).unwrap();
        assert_eq!(decoded, variant, "KrakenBaseUrl decoded native variant");
    }
    for input in ["null", "0", "[]", "{}", "\"History\"", "\"\"", "\"other\""] {
        assert!(
            js_from_str::<KrakenBaseUrl>(input).is_err(),
            "KrakenBaseUrl rejects {input}"
        );
    }
}

#[test]
fn capture_keeps_signed_zero_and_canonicalizes_only_nan() {
    assert_eq!(number(&-0.0)["bits"], "8000000000000000", "negativeZero");
    assert_eq!(
        number(&f64::from_bits(0xfff8_0000_0000_1234))["bits"],
        "7ff8000000000000",
        "negativeNaN"
    );
    assert_eq!(number(&1.0)["bits"], "3ff0000000000000", "finite");
}

#![cfg(not(target_arch = "wasm32"))]

use safe_cex_venue_grvt::types::{
    GrvtAssetKind, GrvtBuilderUnauthorizedReason, GrvtCandlestickInterval, GrvtJsonRpcVersion,
    GrvtLiquidity, GrvtOrderSide, GrvtOrderStatus, GrvtOrderType, GrvtSubscriptionMethod,
    GrvtTimeInForce, GrvtTriggerKind,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
#[path = "support/enum_identity.rs"]
mod enum_identity;
use enum_identity::NativeEnum;

#[derive(Debug)]
struct VariantInventory(Vec<String>);
impl std::fmt::Display for VariantInventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for VariantInventory {}
impl serde::de::Error for VariantInventory {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(vec![message.to_string()])
    }
    fn unknown_variant(_: &str, variants: &'static [&'static str]) -> Self {
        Self(variants.iter().map(|v| (*v).to_owned()).collect())
    }
}
fn variants<T: DeserializeOwned>() -> Vec<String> {
    T::deserialize(serde::de::value::StrDeserializer::<VariantInventory>::new(
        "__unknown",
    ))
    .err()
    .expect("unknown string must refuse")
    .0
}

fn decode<T: DeserializeOwned + Serialize + NativeEnum>(
    input: &Value,
) -> Result<Value, serde_json::Error> {
    let value: T = serde_json::from_value(input.clone())?;
    assert_eq!(
        Some(value.native_wire()),
        input.as_str(),
        "GRVT native enum variant identity {}",
        std::any::type_name::<T>()
    );
    serde_json::to_value(value)
}

fn roundtrip(name: &str, input: &Value) -> Result<Value, serde_json::Error> {
    match name {
        "GrvtBuilderUnauthorizedReason" => decode::<GrvtBuilderUnauthorizedReason>(input),
        "GrvtTriggerKind" => decode::<GrvtTriggerKind>(input),
        "GrvtOrderSide" => decode::<GrvtOrderSide>(input),
        "GrvtOrderType" => decode::<GrvtOrderType>(input),
        "GrvtTimeInForce" => decode::<GrvtTimeInForce>(input),
        "GrvtCandlestickInterval" => decode::<GrvtCandlestickInterval>(input),
        "GrvtOrderStatus" => decode::<GrvtOrderStatus>(input),
        "GrvtAssetKind" => decode::<GrvtAssetKind>(input),
        "GrvtLiquidity" => decode::<GrvtLiquidity>(input),
        "GrvtJsonRpcVersion" => decode::<GrvtJsonRpcVersion>(input),
        "GrvtSubscriptionMethod" => decode::<GrvtSubscriptionMethod>(input),
        _ => panic!("unclassified TS protocol type {name}"),
    }
}

#[test]
fn protocol_string_union_accepted_sets_match_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/types.cjs"))
        .output()
        .expect("run TypeScript checker oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus = String::from_utf8(output.stdout).unwrap();
    assert_eq!(corpus.lines().count(), 44, "protocol literal census");
    for (name, mut actual) in [
        (
            "GrvtBuilderUnauthorizedReason",
            variants::<GrvtBuilderUnauthorizedReason>(),
        ),
        ("GrvtTriggerKind", variants::<GrvtTriggerKind>()),
        ("GrvtOrderSide", variants::<GrvtOrderSide>()),
        ("GrvtOrderType", variants::<GrvtOrderType>()),
        ("GrvtTimeInForce", variants::<GrvtTimeInForce>()),
        (
            "GrvtCandlestickInterval",
            variants::<GrvtCandlestickInterval>(),
        ),
        ("GrvtOrderStatus", variants::<GrvtOrderStatus>()),
        ("GrvtAssetKind", variants::<GrvtAssetKind>()),
        ("GrvtLiquidity", variants::<GrvtLiquidity>()),
        ("GrvtJsonRpcVersion", variants::<GrvtJsonRpcVersion>()),
        (
            "GrvtSubscriptionMethod",
            variants::<GrvtSubscriptionMethod>(),
        ),
    ] {
        let mut expected: Vec<_> = corpus
            .lines()
            .filter_map(|row| {
                let (kind, value) = row.split_once('\t').unwrap();
                (kind == name).then(|| value.to_owned())
            })
            .collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected, "GRVT complete variant inventory {name}");
    }
    for row in corpus.lines() {
        let (name, literal) = row.split_once('\t').unwrap();
        let input = json!(literal);
        assert_eq!(
            roundtrip(name, &input).ok(),
            Some(input),
            "GRVT protocol accepted literal {name}: {literal}"
        );
        for bad in [
            Value::Null,
            json!(true),
            json!(false),
            json!(0),
            json!(1.5),
            json!([]),
            json!([literal]),
            json!({}),
            json!({literal: null}),
            json!({literal: []}),
            json!({literal: {}}),
            json!(""),
            json!(format!(" {literal}")),
            json!(format!("{literal} ")),
            json!(format!("\u{feff}{literal}")),
            json!(if literal.to_lowercase() != literal {
                literal.to_lowercase()
            } else if literal.to_uppercase() != literal {
                literal.to_uppercase()
            } else {
                format!("{literal}_badcase")
            }),
            json!(format!("{literal}_UNKNOWN")),
        ] {
            assert!(
                roundtrip(name, &bad).is_err(),
                "GRVT string-only accepted-set refusal {name}: {bad}"
            );
        }
    }
}

#[test]
fn actual_rpc_literal_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "protocol_string_union_accepted_sets_match_typescript",
            "--nocapture",
        ])
        .env("GRVT_TYPES_MUTATION", "rpc")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("GRVT complete variant inventory GrvtJsonRpcVersion"),
        "rpc mutation control: {log}"
    );
}

#[test]
fn actual_builder_reason_mutation_is_detected() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "protocol_string_union_accepted_sets_match_typescript",
            "--nocapture",
        ])
        .env("GRVT_TYPES_MUTATION", "builder-reason")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        log.contains("GRVT complete variant inventory GrvtBuilderUnauthorizedReason"),
        "builder reason mutation: {log}"
    );
}

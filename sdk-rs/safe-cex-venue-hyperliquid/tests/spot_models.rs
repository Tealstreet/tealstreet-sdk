use safe_cex_venue_hyperliquid::{spot_json::JsString, spot_models::*};
use serde::de::DeserializeOwned;
use serde_json::Value;

trait Native {
    fn native(&self) -> Value;
}
fn number(value: &f64) -> Value {
    serde_json::json!(format!("{:016x}", value.to_bits()))
}
impl Native for JsString {
    fn native(&self) -> Value {
        serde_json::json!(self.0)
    }
}
impl<T: Native> Native for Vec<T> {
    fn native(&self) -> Value {
        Value::Array(self.iter().map(Native::native).collect())
    }
}
include!("fixtures/spot-model-native.rs");
include!("fixtures/spot-model-dispatch.rs");

fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-models-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SPOTMODEL source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn check<T: DeserializeOwned + Native>(row: &Value) {
    let raw = row["input"].as_str().unwrap();
    let name = row["name"].as_str().unwrap();
    if row["accepted"].as_bool().unwrap() {
        let native = serde_json::from_str::<T>(raw)
            .unwrap_or_else(|error| panic!("SPOTMODEL native accepted {name}: {error}; {raw}"));
        assert_eq!(
            native.native(),
            row["native"],
            "SPOTMODEL native {name} from_str mode {}",
            row["mode"]
        );
        let native = serde_json::from_slice::<T>(raw.as_bytes())
            .unwrap_or_else(|error| panic!("SPOTMODEL native accepted slice {name}: {error}"));
        assert_eq!(
            native.native(),
            row["native"],
            "SPOTMODEL native {name} from_slice mode {}",
            row["mode"]
        );
    } else {
        assert!(
            serde_json::from_str::<T>(raw).is_err(),
            "SPOTMODEL native rejection {name}: {raw}"
        );
        assert!(
            serde_json::from_slice::<T>(raw.as_bytes()).is_err(),
            "SPOTMODEL native slice rejection {name}: {raw}"
        );
    }
}
#[test]
fn native_spot_models_match_actual_typed_source() {
    const ROOTS: [&str; 4] = [
        "WsFill",
        "WsUserFunding",
        "WsLiquidation",
        "WsNonUserCancel",
    ];
    let source = source();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32, "SPOTMODEL independent source case floor");
    assert_eq!(
        source["iterationStop"], 31,
        "SPOTMODEL source iteration stop"
    );
    let actual_roots: Vec<_> = source["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(
        actual_roots, ROOTS,
        "SPOTMODEL independent source root enumeration"
    );
    let mut failures = Vec::new();
    let mut checked = 0;
    for name in ROOTS {
        let rows: Vec<_> = cases.iter().filter(|row| row["name"] == name).collect();
        assert_eq!(
            rows.len(),
            8,
            "SPOTMODEL independent per-root case floor {name}"
        );
        assert_eq!(
            rows.iter().filter(|row| row["accepted"] == true).count(),
            4,
            "SPOTMODEL independent accepted floor {name}"
        );
        checked += 1;
        if std::panic::catch_unwind(|| {
            for row in rows {
                dispatch(row);
            }
        })
        .is_err()
        {
            failures.push(name);
        }
    }
    assert_eq!(checked, 4, "SPOTMODEL native root iteration stop");
    assert!(
        failures.is_empty(),
        "SPOTMODEL named native failures: {failures:?}"
    );
}

// Manual projections pin the two most easily swapped field pairs independently
// of the generator's field-name transformation and enum metadata.

#[test]
fn native_declared_fields_ignore_unknown_utf16_keys_and_take_last_duplicate() {
    let cancel: WsNonUserCancel = serde_json::from_str(
        r#"{"coin":"old","coin":"\ud800X","oid":-0,"\ud800":{"ignored":"\udfff"}}"#,
    )
    .unwrap();
    assert_eq!(
        cancel.coin.0,
        vec![0xd800, 0x58],
        "SPOTMODEL native duplicate last coin units"
    );
    assert_eq!(
        cancel.oid.to_bits(),
        (-0.0f64).to_bits(),
        "SPOTMODEL native unknown-key number bits"
    );
    let escaped: WsNonUserCancel =
        serde_json::from_str(r#" { "\u0063oin":"A", "oid":1e400 } "#).unwrap();
    assert_eq!(
        escaped.coin.0,
        vec![0x41],
        "SPOTMODEL native escaped property-name units"
    );
    assert_eq!(
        escaped.oid.to_bits(),
        f64::INFINITY.to_bits(),
        "SPOTMODEL native whitespace overflow bits"
    );
}

#[test]
fn malformed_raw_json_is_rejected_before_native_decode() {
    for input in [
        "\"\\u12xy\"",
        "\"\\x41\"",
        "\"literal\nnewline\"",
        "\"unterminated",
        "\"X\" trailing",
    ] {
        assert!(
            serde_json::from_str::<JsString>(input).is_err(),
            "SPOTMODEL malformed string {input:?}"
        );
    }
    for input in ["{\"coin\":\"X\",\"oid\":0,}", "{\"coin\":\"X\",\"oid\":01}"] {
        assert!(
            serde_json::from_str::<WsNonUserCancel>(input).is_err(),
            "SPOTMODEL malformed object {input:?}"
        );
    }
}

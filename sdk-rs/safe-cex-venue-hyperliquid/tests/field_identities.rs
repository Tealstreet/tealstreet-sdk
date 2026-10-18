use serde::Deserialize;
use serde_json::Value;
trait NativeIdentity {
    fn native(&self) -> Value;
}
impl NativeIdentity for String {
    fn native(&self) -> Value {
        Value::String(self.clone())
    }
}
// This older identity corpus contains scalar Unicode only. Inspect the native
// carrier directly; surrogate-unit/order proof belongs to core_map_models.rs.
impl NativeIdentity for safe_cex_venue_hyperliquid::spot_json::JsString {
    fn native(&self) -> Value {
        Value::String(String::from_utf16(&self.0).expect("scalar-Unicode identity fixture"))
    }
}
impl<T: NativeIdentity> NativeIdentity for safe_cex_venue_hyperliquid::json_values::JsonObject<T> {
    fn native(&self) -> Value {
        Value::Object(
            self.0
                .iter()
                .map(|(key, value)| {
                    (
                        String::from_utf16(&key.0).expect("scalar-Unicode identity fixture key"),
                        value.native(),
                    )
                })
                .collect(),
        )
    }
}
impl NativeIdentity for f64 {
    fn native(&self) -> Value {
        serde_json::json!(self)
    }
}
impl NativeIdentity for bool {
    fn native(&self) -> Value {
        Value::Bool(*self)
    }
}
impl NativeIdentity for Value {
    fn native(&self) -> Value {
        self.clone()
    }
}
impl<T: NativeIdentity> NativeIdentity for Option<T> {
    fn native(&self) -> Value {
        self.as_ref().map_or(Value::Null, NativeIdentity::native)
    }
}
impl<T: NativeIdentity> NativeIdentity for Vec<T> {
    fn native(&self) -> Value {
        Value::Array(self.iter().map(NativeIdentity::native).collect())
    }
}
impl<A: NativeIdentity, B: NativeIdentity> NativeIdentity for (A, B) {
    fn native(&self) -> Value {
        Value::Array(vec![self.0.native(), self.1.native()])
    }
}
impl<T: NativeIdentity> NativeIdentity for std::collections::BTreeMap<String, T> {
    fn native(&self) -> Value {
        Value::Object(self.iter().map(|(k, v)| (k.clone(), v.native())).collect())
    }
}
include!("fixtures/native-identity-projections.rs");
fn roundtrip<T: for<'de> Deserialize<'de> + NativeIdentity>(raw: &str) -> Value {
    serde_json::from_str::<T>(raw).unwrap().native()
}
include!("fixtures/field-identity-bindings.rs");
fn equal(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Object(a), Value::Object(b)) => {
            b.iter().all(|(k, v)| a.get(k).is_some_and(|x| equal(x, v)))
                && a.iter().all(|(k, v)| b.contains_key(k) || v.is_null())
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| equal(x, y))
        }
        _ => actual == expected,
    }
}
#[test]
fn per_field_identity_survives_public_decode() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/field-identities.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 257, "FIELD-IDENTITY source total");
    let mut comparisons = 0;
    for index in 0..257 {
        let case = cases.get(index).expect("FIELD-IDENTITY index coverage");
        comparisons += 1;
        let raw = case["raw"].as_str().unwrap();
        let actual = dispatch(case["name"].as_str().unwrap(), raw);
        let expected: Value = serde_json::from_str(raw).unwrap();
        assert!(
            equal(&actual, &expected),
            "per-field identity {} {}: actual {actual} expected {expected}",
            case["name"],
            case["path"]
        );
    }
    assert_eq!(comparisons, 257, "FIELD-IDENTITY native comparison total");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/field-identities.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source identity checker: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

include!("fixtures/enum-field-identity-bindings.rs");
#[test]
fn enum_field_identity_survives_native_decode() {
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/enum-field-identities.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 33, "ENUM-IDENTITY source total");
    let mut comparisons = 0;
    for index in 0..33 {
        let case = cases.get(index).expect("ENUM-IDENTITY index coverage");
        comparisons += 1;
        let raw = case["raw"].as_str().unwrap();
        let actual = dispatch_enum(case["name"].as_str().unwrap(), raw);
        let expected: Value = serde_json::from_str(raw).unwrap();
        assert!(
            equal(&actual, &expected),
            "native enum field identity {} {} actual {actual} expected {expected}",
            case["name"],
            case["path"]
        );
    }
    assert_eq!(comparisons, 33, "ENUM-IDENTITY native comparison total");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/field-identities.cjs"
        ))
        .arg("--enums")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source enum identity checker {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn multi_element_collection_identity_survives_public_decode() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/collection-identities.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "COLLECTION-IDENTITY source compiler: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30, "COLLECTION-IDENTITY source total");
    assert_eq!(
        corpus["rejected"].as_array().unwrap().len(),
        0,
        "COLLECTION-IDENTITY rejected total"
    );
    assert_eq!(
        corpus["catalogHash"], "2870221aa542e8cc1ea6547e1155fb271aab8e4443af1fff8b40ae9bd3eebc81",
        "COLLECTION-IDENTITY source catalog fingerprint"
    );
    let catalog = corpus["catalog"].as_array().unwrap();
    assert_eq!(catalog.len(), 22, "COLLECTION-IDENTITY catalog total");
    assert_eq!(
        catalog.iter().filter(|row| row["kind"] == "array").count(),
        15,
        "COLLECTION-IDENTITY array catalog"
    );
    assert_eq!(
        catalog
            .iter()
            .filter(|row| row["kind"] == "tuple" && row["arity"] == 2)
            .count(),
        7,
        "COLLECTION-IDENTITY fixed pair catalog"
    );
    let inventory = std::collections::BTreeMap::from([
        (("ClearinghouseState", "/assetPositions"), 2),
        (("WsBook", "/levels/0"), 2),
        (("WsBook", "/levels/1"), 2),
        (("L2Book", "/levels/0"), 2),
        (("L2Book", "/levels/1"), 2),
        (("FundingHistory", "/"), 2),
        (("UserOpenOrders", "/"), 2),
        (("CancelOrderRequests", "/"), 2),
        (("WsUserFills", "/fills"), 2),
        (("WsUserFundings", "/fundings"), 2),
        (("WsUserNonFundingLedgerUpdates", "/updates"), 10),
    ]);
    let mut observed = std::collections::BTreeMap::new();
    let mut comparisons = 0;
    let mut elements = 0;
    let mut distinct = 0;
    for index in 0..30 {
        let row = cases
            .get(index)
            .expect("COLLECTION-IDENTITY native index coverage");
        let name = row["name"].as_str().unwrap();
        let path = row["path"].as_str().unwrap();
        *observed.entry((name, path)).or_insert(0) += 1;
        let raw = row["raw"].as_str().unwrap();
        let expected: Value = serde_json::from_str(raw).unwrap();
        let native = dispatch(name, raw);
        let array_at = |value: &Value| {
            if path == "/" {
                value.clone()
            } else {
                value.pointer(path).unwrap().clone()
            }
        };
        let expected_array = array_at(&expected);
        let native_array = array_at(&native);
        let expected_items = expected_array.as_array().unwrap();
        let native_items = native_array.as_array().unwrap();
        let required = 2 + index % 2;
        assert_eq!(
            expected_items.len(),
            required,
            "COLLECTION-IDENTITY source per-case {name} {path}"
        );
        assert_eq!(
            native_items.len(),
            required,
            "COLLECTION-IDENTITY native length {name} {path}"
        );
        for item in 0..required {
            assert!(
                equal(
                    native_items
                        .get(item)
                        .expect("COLLECTION-IDENTITY native element coverage"),
                    &expected_items[item]
                ),
                "COLLECTION-IDENTITY native element {name} {path} {item}"
            );
            elements += 1;
        }
        let unique: std::collections::BTreeSet<_> =
            expected_items.iter().map(Value::to_string).collect();
        if unique.len() == required {
            distinct += 1;
        }
        assert!(
            equal(&native, &expected),
            "COLLECTION-IDENTITY whole native record {name} {path}"
        );
        comparisons += 1;
    }
    assert_eq!(
        observed, inventory,
        "COLLECTION-IDENTITY root/path subcorpus inventory"
    );
    assert_eq!(
        comparisons, 30,
        "COLLECTION-IDENTITY native comparison total"
    );
    assert_eq!(elements, 75, "COLLECTION-IDENTITY native element total");
    assert_eq!(distinct, 30, "COLLECTION-IDENTITY distinct cases");
}

use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountValue},
    sdk_key_management::*,
    transaction_signing::SigningResult,
};
use serde_json::Value;
use std::{collections::BTreeMap, process::Command, rc::Rc};
struct CauseMarker;
impl AccountObject for CauseMarker {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Ok(AccountValue::Undefined)
    }
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn string(v: &Value) -> Vec<u16> {
    assert_eq!(v["kind"], "string");
    units(&v["units"])
}
fn assert_error(error: &KeyManagerError, expected: &Value, label: &str) {
    assert_eq!(
        error.name,
        expected["name"].as_str().unwrap(),
        "{label} native error name"
    );
    assert_eq!(
        error.message,
        units(&expected["message"]),
        "{label} native error UTF16"
    );
    assert!(
        matches!(error.cause, AccountValue::Undefined),
        "{label} native undefined cause"
    );
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn key_management_native_values_errors_and_cause_identity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/key-management-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        121,
        "independent authored floor, stopped iteration120"
    );
    // Independent field expectations witness the type's two distinct native values.
    let pair = KeyPair {
        private_key: vec![65, 55296],
        public_key: vec![66, 55297],
    };
    assert_eq!(
        pair.private_key,
        vec![65, 55296],
        "KeyPair native private field"
    );
    assert_eq!(
        pair.public_key,
        vec![66, 55297],
        "KeyPair native public field"
    );
    let mut counts = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let args = row["args"].as_array().unwrap();
        let label = format!("{method} iteration{i}");
        match method {
            "isValidPrivateKey" | "isValidPublicKey" => {
                let key = string(&args[0]);
                let actual = if method == "isValidPrivateKey" {
                    is_valid_private_key(&key)
                } else {
                    is_valid_public_key(&key)
                };
                assert!(row["error"].is_null(), "{label} source success");
                assert_eq!(
                    actual,
                    row["result"].as_bool().unwrap(),
                    "{label} native boolean"
                );
            }
            "secureCompareKeys" => {
                let actual = secure_compare_keys(&string(&args[0]), &string(&args[1]));
                assert!(row["error"].is_null(), "{label} source success");
                assert_eq!(
                    actual,
                    row["result"].as_bool().unwrap(),
                    "{label} native comparison"
                );
            }
            "normalizePrivateKey" | "normalizePublicKey" => {
                let u = if args[0]["kind"] == "string" {
                    string(&args[0])
                } else {
                    vec![]
                };
                let bytes: Vec<u8> = args[0]["bytes"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
                            .collect()
                    })
                    .unwrap_or_default();
                let input = match args[0]["kind"].as_str().unwrap() {
                    "string" => KeyInput::String(&u),
                    "buffer" => KeyInput::Buffer(&bytes),
                    "uint8" => KeyInput::Uint8Array(&bytes),
                    "undefined" => KeyInput::Unsupported,
                    _ => panic!("unwitnessed input"),
                };
                let actual = if method == "normalizePrivateKey" {
                    normalize_private_key(input)
                } else {
                    normalize_public_key(input)
                };
                match actual {
                    Ok(value) => {
                        assert!(row["error"].is_null(), "{label} native success");
                        assert_eq!(value, string(&row["result"]), "{label} native UTF16 result");
                    }
                    Err(error) => {
                        assert!(!row["error"].is_null(), "{label} native rejection");
                        assert_error(&error, &row["error"], &label);
                    }
                }
            }
            "KeyManagerError.constructor" => {
                let marker: Rc<dyn AccountObject> = Rc::new(CauseMarker);
                let cause = if args[1]["kind"] == "marker" {
                    Some(AccountValue::Object(marker.clone()))
                } else {
                    None
                };
                let actual = KeyManagerError::new(string(&args[0]), cause);
                let expected = &row["result"];
                assert_eq!(
                    actual.name,
                    expected["name"].as_str().unwrap(),
                    "{label} native error name"
                );
                assert_eq!(
                    actual.message,
                    units(&expected["message"]),
                    "{label} native error UTF16"
                );
                if expected["cause"] == "marker" {
                    let AccountValue::Object(value) = actual.cause else {
                        panic!("{label} native cause kind")
                    };
                    assert!(Rc::ptr_eq(&value, &marker), "{label} native cause identity");
                } else {
                    assert!(
                        matches!(actual.cause, AccountValue::Undefined),
                        "{label} native undefined cause"
                    );
                }
            }
            _ => panic!("unwitnessed method"),
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("isValidPrivateKey", 15),
            ("isValidPublicKey", 15),
            ("normalizePrivateKey", 34),
            ("normalizePublicKey", 34),
            ("secureCompareKeys", 15),
            ("KeyManagerError.constructor", 8)
        ]),
        "independent per-method floor"
    );
    // All native fields/booleans/code units/error causes precede source-only pin.
    assert_eq!(
        oracle["contentHash"], "40147f5cea3d44ede62ac065fbe0186a7db239c7720641bf5a390417f5bcf1fc",
        "authored source-only full corpus pin after native assertions"
    );
}

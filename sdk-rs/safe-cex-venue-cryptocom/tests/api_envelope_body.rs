use safe_cex_venue_cryptocom::api_envelope_body::{self as api, EnvelopeValue, ResponseBody};
use serde_json::Value;
use std::{cell::RefCell, process::Command, rc::Rc};

#[derive(Debug, Clone)]
struct Handle {
    role: String,
    identity: Rc<()>,
}
#[derive(Debug, PartialEq)]
enum Event {
    Array(String),
    Get(String),
    Json,
    String,
}
#[derive(Debug)]
enum Native {
    Boolean(bool),
    String(Vec<u16>),
    Undefined,
    Original(Rc<()>),
}
impl PartialEq for Native {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Undefined, Self::Undefined) => true,
            (Self::Original(a), Self::Original(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn envelope_value(v: &Value, root: &Rc<()>, params: &Rc<()>) -> EnvelopeValue<Handle> {
    match v["kind"].as_str().unwrap() {
        "other" => EnvelopeValue::Other,
        "string" => EnvelopeValue::String(units(&v["units"])),
        "number" => EnvelopeValue::Number(f64::from_bits(
            u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "object" => {
            let role = v["role"].as_str().unwrap();
            EnvelopeValue::Object(Handle {
                role: role.into(),
                identity: if role == "root" {
                    root.clone()
                } else {
                    params.clone()
                },
            })
        }
        other => panic!("unexpected source class {other}"),
    }
}
fn source_native(v: &Value, thrown: &Rc<()>) -> Native {
    match v["kind"].as_str().unwrap() {
        "boolean" => Native::Boolean(v["value"].as_bool().unwrap()),
        "string" => Native::String(units(&v["units"])),
        "undefined" => Native::Undefined,
        "original" => Native::Original(thrown.clone()),
        other => panic!("unexpected source result {other}"),
    }
}
fn event(v: &Value) -> Event {
    match v["kind"].as_str().unwrap() {
        "array" => Event::Array(v["role"].as_str().unwrap().into()),
        "get" => Event::Get(v["field"].as_str().unwrap().into()),
        "json" => Event::Json,
        "string" => Event::String,
        other => panic!("unexpected source effect {other}"),
    }
}
const ENVELOPES: [&str; 33] = [
    "undefined",
    "null",
    "primitive",
    "function",
    "array",
    "revoked-root",
    "valid",
    "method-missing",
    "method-case",
    "method-object",
    "key-missing",
    "key-mismatch",
    "key-boxed",
    "params-null",
    "params-array",
    "params-function",
    "params-boxed",
    "params-revoked",
    "sig-missing",
    "sig-boxed",
    "sig-surrogate",
    "nonce-nan",
    "nonce-infinity",
    "nonce-negative-zero",
    "nonce-string",
    "nonce-bigint",
    "nonce-boxed",
    "getter-method",
    "getter-api_key",
    "getter-params",
    "getter-sig",
    "getter-nonce",
    "mutating-getter",
];
const BODIES: [&str; 22] = [
    "empty",
    "literal",
    "surrogate",
    "undefined",
    "symbol",
    "function",
    "null",
    "false",
    "number",
    "negative-zero",
    "nan",
    "infinity",
    "object",
    "array",
    "boxed",
    "date",
    "bigint",
    "cyclic",
    "getter-throw",
    "toJSON-throw",
    "both-throw",
    "mutating-toJSON",
];
fn primitive_body(id: &str) -> Option<Vec<u16>> {
    match id {
        "empty" => Some(vec![]),
        "literal" => Some("{\"a\":1}".encode_utf16().collect()),
        "surrogate" => Some(vec![0xd800, 0x58, 0xdfff]),
        _ => None,
    }
}
#[test]
fn api_envelope_body_matches_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api_envelope_body_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ENVELOPE-BODY source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut compared = 0;
    for (group, ids, start) in [
        ("envelope", ENVELOPES.as_slice(), 0),
        ("body", BODIES.as_slice(), 33),
    ] {
        for (index, id) in ids.iter().copied().enumerate() {
            let row = &rows[start + index];
            let effects = RefCell::new(Vec::new());
            let root = Rc::new(());
            let params = Rc::new(());
            let thrown = Rc::new(());
            let actual = if group == "envelope" {
                let value = envelope_value(&row["input"], &root, &params);
                let method: Vec<_> = "private/create-order".encode_utf16().collect();
                let key: Vec<_> = [0x6b, 0xd800, 0xdfff].into();
                let array_index = RefCell::new(0);
                let result = api::is_authenticated_envelope(
                    &value,
                    &method,
                    &key,
                    |handle| {
                        effects.borrow_mut().push(Event::Array(handle.role.clone()));
                        let expected = if handle.role == "root" {
                            &root
                        } else {
                            &params
                        };
                        assert!(
                            Rc::ptr_eq(&handle.identity, expected),
                            "ENVELOPE-BODY native array argument identity"
                        );
                        let mut ordinal = array_index.borrow_mut();
                        let observed = &row["arrayResults"][*ordinal];
                        *ordinal += 1;
                        assert_eq!(
                            observed["role"].as_str(),
                            Some(handle.role.as_str()),
                            "ENVELOPE-BODY array boundary receiver"
                        );
                        if observed["error"].as_bool() == Some(true) {
                            Err(thrown.clone())
                        } else {
                            Ok(observed["result"].as_bool().unwrap())
                        }
                    },
                    |handle, field| {
                        assert!(
                            Rc::ptr_eq(&handle.identity, &root),
                            "ENVELOPE-BODY native getter receiver"
                        );
                        effects.borrow_mut().push(Event::Get(field.into()));
                        if row["throwField"].as_str() == Some(field) {
                            return Err(thrown.clone());
                        }
                        let read = row["reads"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|v| v["field"].as_str() == Some(field))
                            .expect("source property observation");
                        Ok(envelope_value(&read["value"], &root, &params))
                    },
                );
                match result {
                    Ok(value) => Native::Boolean(value),
                    Err(error) => Native::Original(error),
                }
            } else {
                let json_thrown = Rc::new(());
                let body = match primitive_body(id) {
                    Some(text) => ResponseBody::String(text),
                    None => ResponseBody::Other(root.clone()),
                };
                let result = api::stringify_response_body(
                    body,
                    |handle| {
                        assert!(
                            Rc::ptr_eq(handle, &root),
                            "ENVELOPE-BODY native JSON input identity"
                        );
                        effects.borrow_mut().push(Event::Json);
                        if row["jsonError"].as_bool().unwrap() {
                            Err(json_thrown.clone())
                        } else {
                            Ok(if row["jsonResult"].is_null() {
                                None
                            } else {
                                Some(units(&row["jsonResult"]))
                            })
                        }
                    },
                    |handle| {
                        assert!(
                            Rc::ptr_eq(handle, &root),
                            "ENVELOPE-BODY native fallback input identity"
                        );
                        effects.borrow_mut().push(Event::String);
                        if row["stringError"].as_bool().unwrap() {
                            Err(thrown.clone())
                        } else {
                            Ok(units(&row["stringResult"]))
                        }
                    },
                );
                match result {
                    Ok(Some(text)) => Native::String(text),
                    Ok(None) => Native::Undefined,
                    Err(error) => Native::Original(error),
                }
            };
            assert_eq!(
                actual,
                source_native(&row["result"], &thrown),
                "CRYPTOCOM-ENVELOPE-BODY-NATIVE {group} case{index} {id}"
            );
            let expected: Vec<_> = row["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(event)
                .collect();
            assert_eq!(
                *effects.borrow(),
                expected,
                "CRYPTOCOM-ENVELOPE-BODY-NATIVE-EFFECTS {group} case{index} {id}"
            );
            compared += 1;
        }
    }
    // All native assertions above precede independent inventories and ingress checks.
    assert_eq!(compared, 55, "ENVELOPE-BODY independent native case floor");
    assert_eq!(
        rows.len(),
        55,
        "ENVELOPE-BODY independent source case floor"
    );
    for (group, ids, start) in [
        ("envelope", ENVELOPES.as_slice(), 0),
        ("body", BODIES.as_slice(), 33),
    ] {
        for (index, id) in ids.iter().copied().enumerate() {
            let row = &rows[start + index];
            assert_eq!(row["group"].as_str(), Some(group));
            assert_eq!(row["id"].as_str(), Some(id));
            if group == "body" {
                assert_eq!(
                    if row["primitive"].is_null() {
                        None
                    } else {
                        Some(units(&row["primitive"]))
                    },
                    primitive_body(id)
                );
            } else {
                assert_eq!(
                    units(&row["method"]),
                    "private/create-order".encode_utf16().collect::<Vec<_>>()
                );
                assert_eq!(units(&row["key"]), vec![0x6b, 0xd800, 0xdfff]);
            }
        }
    }
    println!("ENVELOPE-BODY floor: 55 native cases; last body case21 mutating-toJSON");
}

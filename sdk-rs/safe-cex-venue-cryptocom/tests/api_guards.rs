use safe_cex_venue_cryptocom::api_guards as guards;
use serde_json::Value;
use std::{cell::RefCell, process::Command, rc::Rc};

#[derive(Debug, PartialEq)]
enum Event {
    Array,
    String,
    Parse(Vec<u16>),
    Record,
    Flag,
}
#[derive(Debug)]
enum Native {
    Boolean(bool),
    Undefined,
    Parsed(Rc<()>),
    Original(Rc<()>),
}
impl PartialEq for Native {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::Undefined, Self::Undefined) => true,
            (Self::Parsed(a), Self::Parsed(b)) | (Self::Original(a), Self::Original(b)) => {
                Rc::ptr_eq(a, b)
            }
            _ => false,
        }
    }
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn source_native(value: &Value, parsed: &Rc<()>, thrown: &Rc<()>) -> Native {
    match value["kind"].as_str().unwrap() {
        "boolean" => Native::Boolean(value["value"].as_bool().unwrap()),
        "undefined" => Native::Undefined,
        "parsed" => Native::Parsed(parsed.clone()),
        "original" => Native::Original(thrown.clone()),
        other => panic!("unexpected source native kind {other}"),
    }
}
fn event(value: &Value) -> Event {
    match value["kind"].as_str().unwrap() {
        "array" => Event::Array,
        "string" => Event::String,
        "parse" => Event::Parse(units(&value["units"])),
        "record" => Event::Record,
        "flag" => Event::Flag,
        other => panic!("unexpected source event {other}"),
    }
}
fn boolean(result: Result<bool, Rc<()>>) -> Native {
    match result {
        Ok(value) => Native::Boolean(value),
        Err(error) => Native::Original(error),
    }
}
const RECORD: [&str; 20] = [
    "undefined",
    "null",
    "false",
    "true",
    "number",
    "string",
    "symbol",
    "bigint",
    "function",
    "object",
    "array",
    "date",
    "map",
    "boxed",
    "null-prototype",
    "proxy-object",
    "proxy-array",
    "revoked-object",
    "revoked-array",
    "revoked-function",
];
const PARSE: [&str; 19] = [
    "object",
    "member",
    "array",
    "null",
    "boolean",
    "zero",
    "negative-zero",
    "infinity",
    "string",
    "surrogate-string",
    "surrogate-object",
    "proto-key",
    "bad",
    "empty",
    "whitespace",
    "duplicate",
    "nested-array",
    "parser-throw",
    "record-throw",
];
const CODE: [&str; 19] = [
    "undefined",
    "null",
    "false",
    "true",
    "zero",
    "negative-zero",
    "nan",
    "infinity",
    "number-code",
    "bigint-code",
    "string-code",
    "padded",
    "surrogate",
    "symbol",
    "array",
    "boxed",
    "object",
    "coercion-throw",
    "proxy-throw",
];
const MESSAGE: [&str; 16] = [
    "undefined",
    "null",
    "number",
    "boxed",
    "object",
    "empty",
    "exact",
    "prefix",
    "suffix",
    "case",
    "other-code",
    "longer-code",
    "gap",
    "surrogate-prefix",
    "surrogate-suffix",
    "nul",
];
const SUPPRESS: [&str; 13] = [
    "true",
    "false",
    "undefined",
    "null",
    "one",
    "string",
    "boxed",
    "identity",
    "symbol",
    "wrong-code",
    "missing-code",
    "code-throw",
    "flag-throw",
];
fn is_object(id: &str) -> bool {
    matches!(
        id,
        "object"
            | "array"
            | "date"
            | "map"
            | "boxed"
            | "null-prototype"
            | "proxy-object"
            | "proxy-array"
            | "revoked-object"
            | "revoked-array"
    )
}
fn parsed_object(id: &str) -> bool {
    matches!(
        id,
        "object"
            | "member"
            | "array"
            | "surrogate-object"
            | "proto-key"
            | "whitespace"
            | "duplicate"
            | "nested-array"
            | "record-throw"
    )
}
fn message(id: &str) -> Option<Vec<u16>> {
    let needle = "Crypto.com API Error 40101";
    let text = match id {
        "empty" => "".into(),
        "exact" => needle.into(),
        "prefix" => format!("prefix: {needle}"),
        "suffix" => format!("{needle}: denied"),
        "case" => needle.to_lowercase(),
        "other-code" => "Crypto.com API Error 40102".into(),
        "longer-code" => format!("{needle}0"),
        "gap" => "Crypto.com API Error  40101".into(),
        "surrogate-prefix" => {
            return Some([0xd800].into_iter().chain(needle.encode_utf16()).collect());
        }
        "surrogate-suffix" => return Some(needle.encode_utf16().chain([0xdfff]).collect()),
        "nul" => "Crypto.com API Error\0 40101".into(),
        "undefined" | "null" | "number" | "boxed" | "object" => return None,
        other => panic!("unexpected fixed message {other}"),
    };
    Some(text.encode_utf16().collect())
}

#[test]
fn api_guards_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/api_guards_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "API-GUARDS source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let specs = [
        ("record", RECORD.as_slice(), 0),
        ("parse", PARSE.as_slice(), 20),
        ("code", CODE.as_slice(), 39),
        ("message", MESSAGE.as_slice(), 58),
        ("suppress", SUPPRESS.as_slice(), 74),
    ];
    let mut compared = 0;
    for (group, ids, start) in specs {
        for (index, id) in ids.iter().copied().enumerate() {
            let row = &rows[start + index];
            let events = RefCell::new(Vec::new());
            let input = Rc::new(());
            let parsed = Rc::new(());
            let thrown = Rc::new(());
            let actual = match group {
                "record" => boolean(guards::is_record(
                    is_object(id).then_some(&input),
                    |argument| {
                        assert!(
                            Rc::ptr_eq(argument, &input),
                            "API-GUARDS native Array argument identity"
                        );
                        events.borrow_mut().push(Event::Array);
                        if matches!(id, "revoked-object" | "revoked-array") {
                            Err(thrown.clone())
                        } else {
                            Ok(matches!(id, "array" | "proxy-array"))
                        }
                    },
                )),
                "parse" => {
                    let text = units(&row["units"]);
                    let result = guards::parse_json_object(
                        &text,
                        |argument| {
                            events.borrow_mut().push(Event::Parse(argument.to_vec()));
                            if matches!(id, "bad" | "empty" | "parser-throw") {
                                Err(thrown.clone())
                            } else {
                                Ok(parsed.clone())
                            }
                        },
                        |argument| {
                            assert!(
                                Rc::ptr_eq(argument, &parsed),
                                "API-GUARDS native record argument identity"
                            );
                            events.borrow_mut().push(Event::Record);
                            guards::is_record(parsed_object(id).then_some(argument), |_| {
                                events.borrow_mut().push(Event::Array);
                                if id == "record-throw" {
                                    Err(thrown.clone())
                                } else {
                                    Ok(matches!(id, "array" | "nested-array"))
                                }
                            })
                        },
                    );
                    match result {
                        Some(value) => Native::Parsed(value),
                        None => Native::Undefined,
                    }
                }
                "code" => boolean(guards::is_cryptocom_authentication_error_code(
                    &input,
                    |argument| {
                        assert!(
                            Rc::ptr_eq(argument, &input),
                            "API-GUARDS native String argument identity"
                        );
                        events.borrow_mut().push(Event::String);
                        if matches!(id, "coercion-throw" | "proxy-throw") {
                            Err(thrown.clone())
                        } else {
                            Ok(units(&row["converted"]))
                        }
                    },
                )),
                "message" => Native::Boolean(guards::is_cryptocom_authentication_failure_message(
                    message(id).as_deref(),
                )),
                "suppress" => boolean(guards::should_suppress_validation_auth_error_emission(
                    &input,
                    |argument| {
                        assert!(
                            Rc::ptr_eq(argument, &input),
                            "API-GUARDS native suppression code identity"
                        );
                        events.borrow_mut().push(Event::String);
                        if id == "code-throw" {
                            Err(thrown.clone())
                        } else {
                            Ok(units(&row["converted"]))
                        }
                    },
                    || {
                        events.borrow_mut().push(Event::Flag);
                        if id == "flag-throw" {
                            Err(thrown.clone())
                        } else {
                            Ok(match id {
                                "true" | "wrong-code" | "missing-code" => Some(true),
                                "false" => Some(false),
                                _ => None,
                            })
                        }
                    },
                )),
                _ => unreachable!(),
            };
            assert_eq!(
                actual,
                source_native(&row["result"], &parsed, &thrown),
                "CRYPTOCOM-API-GUARD-NATIVE {group} case{index} {id}"
            );
            let expected_events: Vec<_> = row["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(event)
                .collect();
            assert_eq!(
                *events.borrow(),
                expected_events,
                "CRYPTOCOM-API-GUARD-NATIVE-EFFECTS {group} case{index} {id}"
            );
            compared += 1;
        }
    }
    // Native assertions above precede independent inventory and ingress checks.
    assert_eq!(
        compared, 87,
        "API-GUARDS independent native comparison floor"
    );
    assert_eq!(rows.len(), 87, "API-GUARDS independent source floor");
    for (group, ids, start) in specs {
        for (index, id) in ids.iter().copied().enumerate() {
            let row = &rows[start + index];
            assert_eq!(row["group"].as_str(), Some(group));
            assert_eq!(row["id"].as_str(), Some(id));
            match group {
                "record" => assert_eq!(row["object"].as_bool(), Some(is_object(id))),
                "parse" => {
                    assert_eq!(row["object"].as_bool(), Some(parsed_object(id)));
                    assert_eq!(
                        row["parseError"].as_bool(),
                        Some(matches!(id, "bad" | "empty" | "parser-throw"))
                    );
                    assert_eq!(row["arrayError"].as_bool(), Some(id == "record-throw"));
                }
                "message" => assert_eq!(
                    if row["units"].is_null() {
                        None
                    } else {
                        Some(units(&row["units"]))
                    },
                    message(id)
                ),
                "code" => assert_eq!(
                    row["stringError"].as_bool(),
                    Some(matches!(id, "coercion-throw" | "proxy-throw"))
                ),
                "suppress" => assert_eq!(row["stringError"].as_bool(), Some(id == "code-throw")),
                _ => unreachable!(),
            }
        }
    }
    println!("API-GUARDS floor: 87 native cases; last suppress case12 flag-throw");
}

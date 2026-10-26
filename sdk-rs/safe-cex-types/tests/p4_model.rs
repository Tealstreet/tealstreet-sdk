use safe_cex_types::{PositionMode, PositionSide, model_utils::*};
use serde_json::{Value, json};
fn equal(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect()
            );
            for (k, v) in a {
                equal(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
#[ignore = "runs current TypeScript model utility sources"]
fn model_utilities_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_model_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert!(rows.len() > 114);
    for row in rows {
        let args = &row["args"];
        if row["error"].is_string() {
            assert_eq!(row["errorName"], "TypeError");
            let rejection = match row["op"].as_str().unwrap() {
                "incomeCategoryFromType" => income_category_from_type(Some(&args[0])).map(|_| ()),
                "normalizeExecutionPositionSide" => {
                    normalize_execution_position_side(Some(&args[0])).map(|_| ())
                }
                "inverseObj" => inverse_obj(args[0].as_object().unwrap()).map(|_| ()),
                "replaceUrlParams" => {
                    replace_url_params(args[0].as_str(), args[1].as_object().unwrap()).map(|_| ())
                }
                _ => unreachable!(),
            };
            let error = rejection.expect_err("source TypeError must be propagated, not panic");
            assert_eq!(error.name, row["errorName"].as_str().unwrap());
            assert_eq!(error.message, row["error"].as_str().unwrap());
            continue;
        }
        let actual = match row["op"].as_str().unwrap() {
            "incomeCategoryFromType" => {
                serde_json::to_value(income_category_from_type(Some(&args[0])).unwrap()).unwrap()
            }
            "normalizeExecutionPositionSide" => {
                json!(normalize_execution_position_side(Some(&args[0])).unwrap())
            }
            "createPositionId" => json!(create_position_id(
                args[0].as_str().unwrap(),
                match args[1].as_str() {
                    Some("long") => Some(PositionSide::Long),
                    Some("short") => Some(PositionSide::Short),
                    _ => None,
                },
                match args[2].as_str() {
                    Some("oneway") => Some(PositionMode::OneWay),
                    Some(_) => Some(PositionMode::Hedged),
                    _ => None,
                }
            )),
            "inverseObj" => Value::Object(inverse_obj(args[0].as_object().unwrap()).unwrap()),
            "replaceUrlParams" => {
                let (url, params) =
                    replace_url_params(args[0].as_str(), args[1].as_object().unwrap()).unwrap();
                if args[0].is_null() {
                    json!({"params":params})
                } else {
                    json!({"url":url,"params":params})
                }
            }
            "isPlainObject" => json!(is_plain_object(&args[0])),
            "fastClone" => fast_clone(&args[0]),
            "clone" => clone(&args[0]),
            "omitSpecial" => Value::Object(omit_undefined(&[
                ("__proto__".into(), Some(json!(1))),
                ("constructor".into(), Some(json!(2))),
                ("prototype".into(), Some(json!(3))),
                ("2".into(), Some(json!(4))),
                ("1".into(), Some(json!(5))),
                ("z".into(), Some(json!(6))),
            ])),
            "omitUndefined" => Value::Object(omit_undefined(&[
                ("x".into(), None),
                ("n".into(), Some(Value::Null)),
                ("y".into(), Some(json!(1))),
            ])),
            op => panic!("{op}"),
        };
        let result = std::panic::catch_unwind(|| equal(&actual, &row["expected"]));
        assert!(result.is_ok(), "{row}\nactual {actual}");
    }
}
#[test]
fn clone_dictionary_follows_js_integer_keys_then_insertion_order() {
    let value: Value = serde_json::from_str(r#"{"z":1,"2":2,"a":3,"1":4}"#).unwrap();
    let copied = fast_clone(&value);
    assert_eq!(
        copied
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["1", "2", "z", "a"]
    );
}

#[test]
fn omit_undefined_uses_integer_key_order() {
    let copied = omit_undefined(&[
        ("2".into(), Some(json!(2))),
        ("1".into(), Some(json!(1))),
        ("z".into(), Some(json!(0))),
    ]);
    assert_eq!(
        copied.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2", "z"]
    );
}

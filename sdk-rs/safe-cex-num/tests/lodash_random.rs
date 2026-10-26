use safe_cex_num::{JsPrimitive, lodash_random, lodash_random_json};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

fn bits(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn primitive(value: &Value) -> JsPrimitive {
    match value[0].as_str().unwrap() {
        "undefined" => JsPrimitive::Undefined,
        "number" => JsPrimitive::Number(bits(&value[1])),
        "symbol" => JsPrimitive::Symbol,
        "bigint" => JsPrimitive::BigInt(value[1].as_str().unwrap().parse().unwrap()),
        "json" => match &value[1] {
            Value::Null => JsPrimitive::Null,
            Value::Bool(v) => JsPrimitive::Boolean(*v),
            Value::String(v) => JsPrimitive::String(v.clone()),
            Value::Number(v) => JsPrimitive::Number(v.as_f64().unwrap()),
            _ => panic!("primitive descriptor"),
        },
        _ => panic!("primitive tag"),
    }
}
fn json(value: &Value) -> Option<Value> {
    match value[0].as_str().unwrap() {
        "undefined" => None,
        "number" => Some(Value::from(bits(&value[1]))),
        "json" => Some(value[1].clone()),
        _ => panic!("JSON tag"),
    }
}
#[test]
fn injected_lodash_random_matches_live_package_bits() {
    let mut command = Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/lodash_random.cjs"
    ));
    if std::env::var_os("SAFE_CEX_RANDOM_SPAN_CONTROL").is_some() {
        command.arg("--control-integer-span");
    }
    let mut child = command.stdout(Stdio::piped()).spawn().unwrap();
    let mut count = 0;
    let mut failures = Vec::new();
    for line in BufReader::new(child.stdout.take().unwrap()).lines() {
        let row: Value = serde_json::from_str(&line.unwrap()).unwrap();
        let draw = bits(&row["draw"]);
        let mut calls = 0;
        let source = || {
            calls += 1;
            draw
        };
        let args = row["args"].as_array().unwrap();
        let actual = if row["mode"] == "primitive" {
            let args: Vec<_> = args.iter().map(primitive).collect();
            lodash_random(&args[0], &args[1], &args[2], source)
        } else {
            let args: Vec<_> = args.iter().map(json).collect();
            let lower = if row["alias"].is_null() {
                args[0].as_ref()
            } else {
                let object = args[2].as_ref().unwrap();
                if let Some(index) = row["alias"].as_u64() {
                    object.get(index as usize)
                } else if row["alias"] == "length" {
                    None
                } else {
                    object.get(row["alias"].as_str().unwrap().parse::<usize>().unwrap())
                }
            };
            let length;
            let lower = if row["alias"] == "length" {
                length = Value::from(args[2].as_ref().unwrap().as_array().unwrap().len());
                Some(&length)
            } else {
                lower
            };
            lodash_random_json(lower, args[1].as_ref(), args[2].as_ref(), source)
        };
        let matches = match actual {
            Ok(value) if row["bits"].is_string() => {
                let expected = bits(&row["bits"]);
                value.to_bits() == expected.to_bits() || value.is_nan() && expected.is_nan()
            }
            Err(ref error) => row["error"] == error.name,
            _ => false,
        };
        if (!matches || row["calls"].as_u64() != Some(calls)) && failures.len() < 10 {
            failures.push(format!(
                "injected random bounds {row}: actual {actual:?}, calls {calls}"
            ));
        }
        count += 1;
    }
    assert!(
        child.wait().unwrap().success(),
        "pinned random oracle must execute"
    );
    assert_eq!(count, 123081, "complete injected random corpus");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn random_oracle_detects_live_integer_span_mutation() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "injected_lodash_random_matches_live_package_bits",
            "--nocapture",
        ])
        .env("SAFE_CEX_RANDOM_SPAN_CONTROL", "1")
        .output()
        .unwrap();
    assert!(!output.status.success(), "integer span mutation must fail");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("injected random bounds"),
        "named bounds comparison: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn random_oracle_refuses_missing_package() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/lodash_random.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/nonexistent-install/typescript"
            ),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
#[test]
fn random_string_bounds_pin_shared_surrogate_refusal() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/lodash_random.cjs"
        ))
        .arg("--surrogate-witnesses")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 6);
    for (index, row) in rows.iter().enumerate() {
        let parsed = serde_json::from_str::<String>(row["raw"].as_str().unwrap());
        if index < 2 {
            assert!(parsed.is_err(), "shared UTF-8 representation refusal");
        } else {
            let value = lodash_random(
                &JsPrimitive::String(parsed.unwrap()),
                &JsPrimitive::Number(2.0),
                &JsPrimitive::Undefined,
                || 0.5,
            )
            .unwrap();
            assert_eq!(value.to_bits(), bits(&row["bits"]).to_bits());
        }
    }
}

use safe_cex_venue_mexc::exchange_helpers::extract_rows;
use serde_json::{Value, json};
#[test]
fn extract_rows_preserves_exact_array_property_precedence_and_names_mutable_prototype_cost() {
    let payload = json!({"data":{"data":[[1,2]],"items":[7]}});
    let mut calls = 0;
    for (key, expected) in [
        ("0", vec![json!(1), json!(2)]),
        ("00", vec![json!(7)]),
        ("-0", vec![json!(7)]),
        ("__proto__", vec![]),
    ] {
        calls += 1;
        let actual = extract_rows(Some(&payload), &[key.into(), "items".into()]);
        assert_eq!(
            actual, expected,
            "extractRows native exact array property precedence {key}"
        );
        if key != "__proto__" {
            let owner = if key == "0" {
                payload["data"]["data"][0].as_array().unwrap()
            } else {
                payload["data"]["items"].as_array().unwrap()
            };
            assert_eq!(
                actual.as_ptr(),
                owner.as_ptr(),
                "extractRows native borrowed input row owner"
            );
        }
    }
    // The native ABI has no realm-owned mutable prototype state. These are
    // repeated reads of the pristine stock model, not a simulated native push.
    calls += 1;
    let retained = extract_rows(Some(&payload), &["__proto__".into(), "items".into()]);
    assert_eq!(
        retained,
        &[] as &[Value],
        "extractRows native retained pristine prototype"
    );
    calls += 1;
    let fresh = extract_rows(Some(&payload), &["__proto__".into(), "items".into()]);
    assert_eq!(
        fresh,
        &[] as &[Value],
        "extractRows native fresh lookup keeps pristine prototype model"
    );
    // A valid JSON lone surrogate is program-readable after Source returns
    // the original row; native Value refuses the whole input before any call.
    assert!(
        serde_json::from_str::<Value>(r#"{"data":[{"symbol":"\ud800"}]}"#).is_err(),
        "extractRows native whole-input Value refusal for returned lone surrogate"
    );
    for (raw, expected) in [
        (r#"{"data":[{"symbol":"\u00e9"}]}"#, vec![0x00e9_u16]),
        (
            r#"{"data":[{"symbol":"\ud83d\ude03"}]}"#,
            vec![0xd83d_u16, 0xde03],
        ),
        (
            r#"{"data":[{"symbol":"\ufeff\u3000"}]}"#,
            vec![0xfeff_u16, 0x3000],
        ),
    ] {
        let payload: Value = serde_json::from_str(raw).unwrap();
        calls += 1;
        let rows = extract_rows(Some(&payload), &[]);
        assert_eq!(
            rows.len(),
            1,
            "extractRows native independent scalar positive row count"
        );
        let symbol = rows[0]["symbol"]
            .as_str()
            .expect("extractRows native returned symbol string");
        assert_eq!(
            symbol.encode_utf16().collect::<Vec<_>>(),
            expected,
            "extractRows native exact returned scalar UTF-16 units"
        );
        assert_eq!(
            rows.as_ptr(),
            payload["data"].as_array().unwrap().as_ptr(),
            "extractRows native scalar positives borrow original input rows"
        );
    }
    assert_eq!(
        calls, 9,
        "extractRows native independent actual helper call floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/extract_rows_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "extractRows actual Source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        9,
        "extractRows source independent output root floor"
    );
    for (i, (key, expected)) in [
        ("0", json!([1, 2])),
        ("00", json!([7])),
        ("-0", json!([7])),
        ("__proto__", json!([])),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source[i]["key"], key);
        assert_eq!(source[i]["rows"], expected);
        assert_eq!(source[i]["isArrayPrototype"], key == "__proto__");
    }
    assert_eq!(
        source[4],
        json!({"key":"live-prototype","before":0,"after":1,"marker":42,"sameOwner":true})
    );
    assert_ne!(
        source[4]["after"],
        json!(retained.len()),
        "extractRows retained source prototype mutation is an explicit cost"
    );
    assert_ne!(
        source[4]["after"],
        json!(fresh.len()),
        "extractRows fresh mutated source prototype lookup is a separate explicit cost"
    );
    for (i, (key, expected)) in [
        ("unicode-lone", vec![0xd800_u16]),
        ("unicode-bmp", vec![0x00e9_u16]),
        ("unicode-astral", vec![0xd83d_u16, 0xde03]),
        ("unicode-whitespace", vec![0xfeff_u16, 0x3000]),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(source[i + 5]["key"], key);
        assert_eq!(source[i + 5]["units"], json!(expected));
    }
}

use super::*;
fn captured_units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| u16::try_from(unit.as_u64().unwrap()).unwrap())
        .collect()
}

#[test]
fn prefix_boundary_observes_returned_units_before_wire() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/prefix-boundary.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "APEX-PREFIX source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    let labels = [
        "high",
        "low",
        "high-tail",
        "low-prefix",
        "pair",
        "scalar",
        "replacement",
        "ascii-escape",
        "empty",
        "uppercase",
        "leading",
        "interior",
    ];
    assert_eq!(rows.len(), 12, "APEX-PREFIX independent source floor");
    let mut typed_boundaries = 0;
    let mut positive = 0;
    for (index, (row, label)) in rows.iter().zip(labels).enumerate() {
        assert_eq!(row["label"], label, "APEX-PREFIX frozen order");
        assert_eq!(row["inputType"], "string", "APEX-PREFIX source input kind");
        assert_eq!(
            row["outputType"], "string",
            "APEX-PREFIX source returned kind"
        );
        let actual = js_from_str::<String>(row["raw"].as_str().unwrap());
        if index < 4 {
            let actual = actual.unwrap();
            assert_eq!(
                actual,
                String::from_utf16_lossy(&captured_units(&row["inputUnits"]))
            );
            typed_utf16::assert_projection(row["raw"].as_str().unwrap(), &actual);
            typed_boundaries += 1;
        } else {
            let input = actual.unwrap_or_else(|error| {
                panic!("APEX-PREFIX {label} native unexpected refusal: {error}")
            });
            assert_eq!(
                input.encode_utf16().collect::<Vec<_>>(),
                captured_units(&row["inputUnits"]),
                "APEX-PREFIX {label} decoded native input units"
            );
            let native = drop_prefix(&input);
            assert_eq!(
                native.encode_utf16().collect::<Vec<_>>(),
                captured_units(&row["outputUnits"]),
                "APEX-PREFIX {label} native returned units"
            );
            positive += 1;
        }
    }
    assert_eq!(
        typed_boundaries, 4,
        "APEX-PREFIX independent typed-boundary floor"
    );
    assert_eq!(positive, 8, "APEX-PREFIX independent native-positive floor");
    for (index, expected) in [
        vec![0xd800_u16],
        vec![0xdc00],
        vec![0xd800, 65],
        vec![65, 0xdc00],
    ]
    .iter()
    .enumerate()
    {
        let row = &rows[index];
        assert_eq!(
            row["outputType"], "string",
            "APEX-PREFIX source returned kind"
        );
        assert_eq!(
            expected,
            &captured_units(&row["outputUnits"]),
            "APEX-PREFIX {} source returned units",
            row["label"].as_str().unwrap()
        );
    }
    // Wire observations follow every native/boundary and source-unit assertion.
    assert_eq!(
        rows[0]["outputWire"], r#""\ud800""#,
        "APEX-PREFIX high source wire"
    );
}

#[path = "../../../test-support/typed_utf16.rs"]
mod typed_utf16;

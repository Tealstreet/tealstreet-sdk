use safe_cex_types::{MarginType, PositionMode, PositionSide};
use safe_cex_venue_orderly::margin::{
    create_orderly_position_id, map_margin_type, to_orderly_margin_mode,
};
use serde_json::Value;
use std::process::Command;

fn native_margin(value: &Value) -> MarginType {
    match value.as_str().expect("source margin string") {
        "isolated" => MarginType::Isolated,
        "cross" => MarginType::Cross,
        value => panic!("unexpected source margin: {value}"),
    }
}

#[test]
fn actual_typescript_margin_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/margin-oracle.cjs"
        ))
        .output()
        .expect("one Node margin corpus");
    assert!(
        output.status.success(),
        "source margin oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["positions"].as_array().unwrap().len(),
        72,
        "ORDERLY-CARDINALITY margin/positions source total"
    );
    assert_eq!(
        corpus["surrogateIdUnits"].as_array().unwrap().len(),
        16,
        "ORDERLY-CARDINALITY margin/surrogateIdUnits source total"
    );
    assert_eq!(
        corpus["mapped"].as_array().unwrap().len(),
        349,
        "ORDERLY-CARDINALITY margin/mapped source total"
    );
    assert_eq!(
        corpus["modes"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY margin/modes source total"
    );
    eprintln!("margin runtime: {}", corpus["runtime"]);
    let mut compared_mapped = 0;
    for (index, row) in corpus["mapped"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            map_margin_type(row["input"].as_str()),
            native_margin(&row["result"]),
            "margin/map/native/{index}"
        );
        compared_mapped += 1;
    }
    assert_eq!(compared_mapped, 349, "ORDERLY-COMPARISONS margin/mapped");
    assert_eq!(
        corpus["modes"].as_array().unwrap().len(),
        2,
        "shared margin enum census"
    );
    let mut compared_modes = 0;
    for (index, row) in corpus["modes"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            to_orderly_margin_mode(native_margin(&row["input"])),
            row["result"].as_str().unwrap(),
            "margin/mode/source/{index}"
        );
        compared_modes += 1;
    }
    assert_eq!(compared_modes, 2, "ORDERLY-COMPARISONS margin/modes");
    let mut compared_positions = 0;
    for (index, row) in corpus["positions"].as_array().unwrap().iter().enumerate() {
        let side = match row["side"].as_str().unwrap() {
            "long" => PositionSide::Long,
            "short" => PositionSide::Short,
            value => panic!("unexpected source side: {value}"),
        };
        let mode = match row["mode"].as_str().unwrap() {
            "oneway" => PositionMode::OneWay,
            "hedged" => PositionMode::Hedged,
            value => panic!("unexpected source mode: {value}"),
        };
        assert_eq!(
            create_orderly_position_id(
                row["symbol"].as_str().unwrap(),
                side,
                mode,
                native_margin(&row["margin"])
            ),
            row["result"].as_str().unwrap(),
            "position-id/source/{index}"
        );
        compared_positions += 1;
    }
    assert_eq!(
        compared_positions, 72,
        "ORDERLY-COMPARISONS margin/positions"
    );
    let mut expected_units = vec![0xd800_u16];
    expected_units.extend(":short:isolated".encode_utf16());
    assert_eq!(
        corpus["surrogateIdUnits"],
        serde_json::json!(expected_units),
        "source position-id surrogate preservation"
    );
    assert_eq!(
        corpus["boundary"]["firstUnit"], 0xd800,
        "source margin surrogate unit"
    );
    assert_eq!(
        corpus["boundary"]["result"], "cross",
        "source margin surrogate result"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "chosen margin UTF-8 boundary refusal"
    );
}

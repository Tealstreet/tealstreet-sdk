use super::{arg, same_optional_value, same_value};
use serde_json::Value;

mod observation {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/value-input-observers.rs"
    ));
}

#[test]
fn native_value_input_helpers_match_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/value-input-oracle.cjs"
        ))
        .output()
        .expect("independent Node value semantics are required");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 83);
    for row in rows {
        let input = &row["input"];
        let method = row["method"].as_str().unwrap();
        let actual = match method {
            "arg" => observation::argument(input, arg),
            "same_value" => observation::equality(input, same_value),
            "same_optional_value" => observation::optional(input, same_optional_value),
            _ => panic!("unreviewed value helper"),
        };
        assert_eq!(actual, row["output"], "NATIVE_VALUE_INPUT {method} {input}");
    }
}

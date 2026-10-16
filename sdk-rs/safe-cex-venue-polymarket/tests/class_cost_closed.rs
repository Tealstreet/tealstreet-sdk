use safe_cex_types::ExchangeName;
use safe_cex_venue_polymarket::spot_contracts::name;
use serde_json::Value;
use std::process::Command;

#[test]
fn class_cost_closed_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/class_cost_closed.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "closed source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["cases"].as_array().unwrap().len(),
        512,
        "closed independent case floor"
    );
    for index in 0..512 {
        let actual = name();
        assert_eq!(
            actual,
            ExchangeName::Polymarket,
            "class_cost_closed_native:{index}"
        );
        assert_eq!(
            data["cases"][index]["index"], index,
            "closed input identity"
        );
        let expected: Vec<u16> =
            serde_json::from_value(data["cases"][index]["units"].clone()).unwrap();
        assert_eq!(
            expected,
            "polymarket".encode_utf16().collect::<Vec<_>>(),
            "closed source value:{index}"
        );
    }
}

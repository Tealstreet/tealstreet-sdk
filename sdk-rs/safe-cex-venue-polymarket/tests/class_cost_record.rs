use safe_cex_venue_polymarket::types::PolymarketOrderBookLevel;
use serde_json::Value;
use std::process::Command;
#[test]
fn class_cost_record_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/class_cost_record.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "record source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["cases"].as_array().unwrap().len(),
        512,
        "record independent case floor"
    );
    for index in 0..512 {
        let case = &data["cases"][index];
        let native: PolymarketOrderBookLevel =
            serde_json::from_value(case["input"].clone()).unwrap();
        let price: Vec<u16> = serde_json::from_value(case["priceUnits"].clone()).unwrap();
        let size: Vec<u16> = serde_json::from_value(case["sizeUnits"].clone()).unwrap();
        assert_eq!(
            native.price.encode_utf16().collect::<Vec<_>>(),
            price,
            "class_cost_record_price:{index}"
        );
        assert_eq!(
            native.size.encode_utf16().collect::<Vec<_>>(),
            size,
            "class_cost_record_size:{index}"
        );
        assert_eq!(case["index"], index, "record input identity");
        assert_eq!(
            case["input"]["price"],
            if index % 8 == 0 {
                String::new()
            } else {
                format!("price-😀-{index}")
            },
            "record independent price input"
        );
        assert_eq!(
            case["input"]["size"],
            format!("size-é-{}", 512 - index),
            "record independent size input"
        );
    }
}

use safe_cex_store::incremental_fills::{clean_float, create_incremental_fills};
use serde_json::{Value, json};

fn hex(n: f64) -> String {
    format!("{:016x}", n.to_bits())
}
fn read(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}

#[test]
fn fill_increments_match_actual_typescript_at_venue_decimals() {
    let sequences: Vec<Vec<(f64, Option<f64>)>> = vec![
        vec![(100000.0, Some(1.0)), (100000.1, Some(1.25))],
        vec![(0.1, Some(8357.88)), (0.3, Some(83578.8))],
        vec![(0.1, Some(8357.88)), (0.3, Some(83578.79999999999))],
        vec![(1e-7, None), (3e-7, None)],
        vec![(1.5, Some(1e21)), (2.25, Some(3e21))],
        vec![(0.000001, None), (0.0000015, None)],
        vec![(12345678.12345679, None), (12345678.98765432, None)],
    ];
    let cleans = [
        83578.79999999999,
        0.1 + 0.2,
        1000000000000005.0,
        1e-7 * 3.0,
        -0.0,
        1e300 * 10.0,
    ];
    let mut native = vec![];
    for frames in &sequences {
        let mut fills = create_incremental_fills(500.0);
        native.push(
            frames
                .iter()
                .map(|(qty, value)| {
                    let r = fills("A", *qty, *value);
                    json!({"qty": hex(r.qty), "value": r.value.map(hex)})
                })
                .collect::<Vec<_>>(),
        );
    }
    // Native identities first: the noise is gone at the venue's decimals.
    assert_eq!(
        read(&native[0][1]["qty"]),
        0.1,
        "native small fill off a large total"
    );
    assert_eq!(
        read(&native[1][1]["value"]),
        75220.92,
        "native value at venue decimals"
    );
    assert_eq!(clean_float(83578.79999999999), 83578.8, "native cleanFloat");
    let input = json!({
        "sequences": sequences.iter().map(|frames| frames.iter().map(|(q, v)| json!([hex(*q), v.map(hex)])).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "cleans": cleans.iter().map(|n| hex(*n)).collect::<Vec<_>>(),
    });
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fills_noise_oracle.mjs"
        ))
        .arg(input.to_string())
        .output()
        .expect("Node required; the fill-noise oracle never skips");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["sequences"], json!(native), "source fill increments");
    let native_cleans: Vec<String> = cleans.iter().map(|n| hex(clean_float(*n))).collect();
    assert_eq!(source["cleans"], json!(native_cleans), "source cleanFloat");
}

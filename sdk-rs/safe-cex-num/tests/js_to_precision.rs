use safe_cex_num::js_to_precision;

fn corpus() -> Vec<f64> {
    let mut values = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        0.1,
        100000.1 - 100000.0,
        83578.79999999999,
        1000000000000005.0,
        1000000000000015.0,
        999999999999999.5,
        // The double nearest 1e23, which JavaScript prints as 9.999999999999999e+22.
        1e23,
        1e21,
        1.5e-7,
        1e-7,
        1e-6,
        123.456,
        f64::MAX,
        f64::MIN_POSITIVE,
        5e-324,
        2.5e-323,
    ];
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    while values.len() < 600 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let value = f64::from_bits(state);
        if value.is_finite() {
            values.push(value);
        }
    }
    values
}

#[test]
fn to_precision_matches_live_node_bits() {
    let values = corpus();
    let precisions = [1.0, 2.0, 7.0, 15.0, 16.0, 17.0, 21.0, 100.0];
    let input = values
        .iter()
        .map(|value| format!("{:016x}", value.to_bits()))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "const p={precisions:?};const out=[];for(const h of process.argv[1].split(',')){{const b=Buffer.from(h,'hex');const v=b.readDoubleBE(0);for(const q of p)out.push(v.toPrecision(q));}}process.stdout.write(JSON.stringify(out));"
    );
    let output = std::process::Command::new("node")
        .args(["-e", &script, &input])
        .output()
        .expect("Node required; the toPrecision oracle never skips");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        values.len() * precisions.len(),
        "source cardinality"
    );
    let mut compared = 0;
    for (i, value) in values.iter().enumerate() {
        for (j, precision) in precisions.iter().enumerate() {
            assert_eq!(
                js_to_precision(*value, *precision).unwrap(),
                source[i * precisions.len() + j],
                "native toPrecision({precision}) of {:016x}",
                value.to_bits()
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 4800, "executed comparisons");
    assert_eq!(js_to_precision(f64::NAN, 3.0).unwrap(), "NaN");
    assert_eq!(
        js_to_precision(f64::NEG_INFINITY, 3.0).unwrap(),
        "-Infinity"
    );
    assert!(js_to_precision(1.0, 0.0).is_err());
    assert!(js_to_precision(1.0, 101.0).is_err());
}

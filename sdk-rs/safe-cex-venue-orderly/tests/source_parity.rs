use safe_cex_venue_orderly::{types::*, utils::*};
use serde_json::{Value, json};
use std::process::Command;

#[test]
fn actual_typescript_constants_and_symbols() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/source-oracle.cjs"
        ))
        .output()
        .expect("start actual TypeScript oracle");
    assert!(
        output.status.success(),
        "TypeScript oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["BASE_URL"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY source/BASE_URL source total"
    );
    assert_eq!(
        source["BASE_WS_URL"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY source/BASE_WS_URL source total"
    );
    assert_eq!(
        source["ENDPOINTS"].as_array().unwrap().len(),
        20,
        "ORDERLY-CARDINALITY source/ENDPOINTS source total"
    );
    assert_eq!(
        source["GRAFANA_ENDPOINTS_EVERY"].as_array().unwrap().len(),
        9,
        "ORDERLY-CARDINALITY source/GRAFANA_ENDPOINTS_EVERY source total"
    );
    assert_eq!(
        source["GRAFANA_ENDPOINTS_PERIODIC"]
            .as_array()
            .unwrap()
            .len(),
        6,
        "ORDERLY-CARDINALITY source/GRAFANA_ENDPOINTS_PERIODIC source total"
    );
    assert_eq!(
        source["ORDER_TYPE"].as_array().unwrap().len(),
        5,
        "ORDERLY-CARDINALITY source/ORDER_TYPE source total"
    );
    assert_eq!(
        source["ORDER_SIDE"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY source/ORDER_SIDE source total"
    );
    assert_eq!(
        source["symbols"].as_array().unwrap().len(),
        53,
        "ORDERLY-CARDINALITY source/symbols source total"
    );
    assert_eq!(
        source["surrogates"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY source/surrogates source total"
    );
    assert_eq!(
        source["cache"].as_array().unwrap().len(),
        2,
        "ORDERLY-CARDINALITY source/cache source total"
    );
    eprintln!("symbol runtime: {}", source["runtime"]);
    let native = json!({
        "RECV_WINDOW": RECV_WINDOW, "BROKER_ID": BROKER_ID,
        "TESTNET_BROKER_ID": TESTNET_BROKER_ID, "BASE_URL": BASE_URL,
        "BASE_WS_URL": BASE_WS_URL, "ENDPOINTS": ENDPOINTS,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "ORDER_TYPE": ORDER_TYPE, "ORDER_SIDE": ORDER_SIDE,
    });
    for (name, value) in native.as_object().unwrap() {
        // Named direct comparisons retain keys, entry order, and duplicate array entries.
        if name != "RECV_WINDOW" {
            assert_eq!(value, &source[name], "constant/{name}");
        }
    }
    assert_eq!(
        format!("{:016x}", RECV_WINDOW.to_bits()),
        source["recvBits"].as_str().unwrap(),
        "constant/RECV_WINDOW/bits"
    );
    let mut compared_symbols = 0;
    for row in source["symbols"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        assert_eq!(
            normalize_symbol(input),
            row["normalize"].as_str().unwrap(),
            "normalize/{input}"
        );
        assert_eq!(
            reverse_symbol(input),
            row["reverse"].as_str().unwrap(),
            "reverse/{input}"
        );
        compared_symbols += 1;
    }
    assert_eq!(compared_symbols, 53, "ORDERLY-COMPARISONS source/symbols");
    let high: Vec<u16> = "PERP_"
        .encode_utf16()
        .chain([0xd800])
        .chain("_USDC".encode_utf16())
        .collect();
    let high_normalized: Vec<u16> = [0xd800].into_iter().chain("USDC".encode_utf16()).collect();
    let high_reversed: Vec<u16> = "PERP_PERP_"
        .encode_utf16()
        .chain([0xd800])
        .chain("__USDC".encode_utf16())
        .collect();
    let low: Vec<u16> = [0xdfff].into_iter().chain("USDC".encode_utf16()).collect();
    let low_normalized: Vec<u16> = "undefinedundefined".encode_utf16().collect();
    let low_reversed: Vec<u16> = "PERP_"
        .encode_utf16()
        .chain([0xdfff])
        .chain("_USDC".encode_utf16())
        .collect();
    assert_eq!(
        source["surrogates"],
        json!([
            [high, high_normalized, high_reversed],
            [low, low_normalized, low_reversed],
        ]),
        "source surrogate code units"
    );
    for row in source["surrogates"].as_array().unwrap() {
        for value in row.as_array().unwrap() {
            let units: Vec<u16> = value
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect();
            // Some normalization paths discard the surrogate; assert refusal only where retained.
            if units.iter().any(|u| (0xd800..=0xdfff).contains(u)) {
                assert!(
                    String::from_utf16(&units).is_err(),
                    "shared UTF-8 representation refusal"
                );
            }
        }
    }
    assert_eq!(
        source["cache"],
        json!(["injected-normalize", "injected-reverse"]),
        "source public cache mutation"
    );
    assert_ne!(
        normalize_symbol("cache-control"),
        "injected-normalize",
        "chosen cache omission/normalize"
    );
    assert_ne!(
        reverse_symbol("cache-control"),
        "injected-reverse",
        "chosen cache omission/reverse"
    );
}

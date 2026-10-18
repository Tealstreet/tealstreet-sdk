#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::Market;
use safe_cex_venue_blofin::symbols::*;
use serde_json::{Value, json};
use std::process::Command;
#[test]
fn live_symbols_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/symbols_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual_typescript_symbols: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pin = cardinality::Census::new("symbols", &corpus);
    let header = &corpus["header"];
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    assert_eq!(header["architecture"], arch, "capture_architecture");
    assert!(
        header["nodeMajor"].as_u64().is_some_and(|v| v > 0),
        "capture_node_major"
    );
    eprintln!("symbols capture header: {header}");
    for i in 0..pin.count("/cases") {
        let case = &corpus["cases"][i];
        for path in 0..3 {
            let markets: Vec<Market> = match path {
                0 => js_from_str(&case["markets"].to_string()).unwrap(),
                1 => js_from_value(case["markets"].clone()).unwrap(),
                _ => js_from_value_ref(&case["markets"]).unwrap(),
            };
            let store = DefaultStore::new();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(markets),
                    ..Default::default()
                },
                false,
            )
            .expect("fixture store update");
            let key = case["key"].as_str().unwrap();
            let name = case["name"].as_str().unwrap();
            let actual = if name == "symbolToMarketId" {
                symbol_to_market_id(key, &store)
            } else {
                market_id_to_symbol(key, &store)
            };
            assert_eq!(
                actual
                    .unwrap()
                    .map_or(json!({"undefined":true}), Value::String),
                case["expected"],
                "symbols_{name}_native case={i} path={path}"
            );
            pin.compared(i, path);
        }
        pin.hit("cases");
    }
    pin.finish("cases", pin.count("/cases"));
    pin.finish_comparisons(3);
    let raw = "\"\\ud800\"";
    let native = js_from_str::<String>(raw).unwrap();
    assert_eq!(native, "\u{fffd}", "typed symbol replacement");
    typed_utf16::assert_projection(raw, &native);
    for index in 0..pin.count("/boundaries") {
        let case = &corpus["boundaries"][index];
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(js_from_value(case["markets"].clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .expect("boundary fixture store update");
        assert_eq!(
            case["source"],
            json!({"undefined":true}),
            "source_replaced_store_prototype"
        );
        let actual = if case["name"] == "symbolToMarketId" {
            symbol_to_market_id("id", &store)
        } else {
            market_id_to_symbol("id", &store)
        };
        assert_eq!(
            actual.unwrap().as_deref(),
            Some("id"),
            "typed_store_prototype_boundary"
        );
        pin.hit("boundaries-case-1");
    }
    pin.finish("boundaries-case-1", pin.count("/boundaries"));
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;

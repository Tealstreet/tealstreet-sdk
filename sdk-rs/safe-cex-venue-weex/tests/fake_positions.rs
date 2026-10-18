#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_types::{MarginType, Market, PositionMode, PositionSide};
use safe_cex_venue_weex::utils::fake_positions_for_market;
use serde_json::Value;
use std::{path::Path, process::Command};
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
#[test]
fn fake_positions_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fake-positions-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    cardinality::check(&corpus, "positions");
    assert!(corpus["header"]["arch"].is_string(), "capture_arch");
    assert!(corpus["header"]["node"].is_string(), "capture_node");
    assert!(
        corpus["header"]["nodeMajor"].is_number(),
        "capture_node_major"
    );
    assert_eq!(
        corpus["header"]["nanBits"], "7ff8000000000000",
        "capture_nan_canonical"
    );
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        84,
        "fake_position::source_cases"
    );
    let expected_total: usize = corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|case| case["positions"].as_array().unwrap().len())
        .sum();
    assert_eq!(expected_total, 168, "cardinality::positions::source_total");
    let mut compared = 0;
    for case in corpus["cases"].as_array().unwrap() {
        let market: Market = js_from_str(&case["market"].to_string()).unwrap();
        let mode: PositionMode = serde_json::from_value(case["mode"].clone()).unwrap();
        let margin: MarginType = serde_json::from_value(case["margin"].clone()).unwrap();
        let leverage = f64::from_bits(
            u64::from_str_radix(case["leverage"]["bits"].as_str().unwrap(), 16).unwrap(),
        );
        let native = fake_positions_for_market(&market, mode, margin, leverage);
        assert_eq!(native.len(), 2, "fake_position::count");
        let expected = case["positions"].as_array().unwrap();
        assert_eq!(expected.len(), 2, "cardinality::positions::source_per_case");
        assert_eq!(
            native.len(),
            expected.len(),
            "cardinality::positions::native_length"
        );
        assert_eq!(native.len(), expected.len(), "fake_position::arity");
        assert_eq!(expected.len(), 2, "fake_position::source_count");
        for (index, (p, e)) in native
            .iter()
            .zip(case["positions"].as_array().unwrap())
            .enumerate()
        {
            compared += 1;
            assert_eq!(
                p.id,
                e["id"].as_str().unwrap(),
                "fake_position::{index}::id"
            );
            assert_eq!(
                p.symbol,
                e["symbol"].as_str().unwrap(),
                "fake_position::{index}::symbol"
            );
            let side = match e["side"].as_str().unwrap() {
                "long" => PositionSide::Long,
                "short" => PositionSide::Short,
                other => panic!("unknown side {other}"),
            };
            let source_margin = match e["marginType"].as_str().unwrap() {
                "cross" => MarginType::Cross,
                "isolated" => MarginType::Isolated,
                other => panic!("unknown margin {other}"),
            };
            let source_mode = match e["positionMode"].as_str().unwrap() {
                "oneway" => PositionMode::OneWay,
                "hedged" => PositionMode::Hedged,
                other => panic!("unknown mode {other}"),
            };
            let mut keys = e
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            keys.sort();
            let mut expected_keys = vec![
                "id",
                "symbol",
                "side",
                "entryPrice",
                "notional",
                "leverage",
                "unrealizedPnl",
                "contracts",
                "liquidationPrice",
                "marginType",
                "positionMode",
                "info",
            ];
            expected_keys.sort();
            assert_eq!(keys, expected_keys, "fake_position::source_keys");
            assert_eq!(
                e["info"]["kind"], "object",
                "fake_position::source_info_shape"
            );
            assert!(
                e["info"]["keys"].as_array().unwrap().is_empty(),
                "fake_position::source_info_keys"
            );
            assert_eq!(p.side, side, "fake_position::{index}::side");
            assert_eq!(
                p.margin_type,
                Some(source_margin),
                "fake_position::{index}::margin_type"
            );
            assert_eq!(
                p.position_mode,
                Some(source_mode),
                "fake_position::{index}::position_mode"
            );
            for (field, n) in [
                ("entryPrice", p.entry_price),
                ("notional", p.notional),
                ("leverage", p.leverage),
                ("unrealizedPnl", p.unrealized_pnl),
                ("contracts", p.contracts),
                ("liquidationPrice", p.liquidation_price),
            ] {
                assert_eq!(
                    bits(n),
                    e[field]["bits"].as_str().unwrap(),
                    "fake_position::{index}::{field}"
                );
            }
            assert!(
                p.info.as_ref().unwrap().as_object().unwrap().is_empty(),
                "fake_position::{index}::info"
            );
            assert!(
                p.realized_pnl.is_none()
                    && p.margin_size.is_none()
                    && p.extra.is_none()
                    && p.last_update.is_none(),
                "fake_position::{index}::absent_fields"
            );
        }
    }
    assert_eq!(compared, 168, "cardinality::positions::compared");
    assert_eq!(compared, 168, "fake_position::compared");
    typed_utf16::assert_projection(
        corpus["refusal"].as_str().unwrap(),
        &(js_from_str::<Market>(corpus["refusal"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    eprintln!(
        "fake-position cases {}; header {}",
        corpus["cases"].as_array().unwrap().len(),
        corpus["header"]
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;

//! Option and inverse routing helpers against the maintained bybit.options.ts / bybit.inverse.ts.
use safe_cex_types::Position;
use safe_cex_venue_bybit::{inverse_symbol, options};
use serde_json::{Value, json};

fn source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/options-inverse-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "options/inverse source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
// Compare numbers by value: serde keeps 5.0 where JSON.stringify writes 5.
fn binary64(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(binary64).collect()),
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(k, v)| (k.clone(), binary64(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn position(case: &Value) -> Position {
    let upnl = match &case["unrealizedPnl"] {
        Value::String(text) if text == "NaN" => f64::NAN,
        value => value.as_f64().unwrap(),
    };
    let mut position: Position = serde_json::from_value(json!({
        "id":"p", "symbol":case["symbol"], "side":"long", "entryPrice":1, "leverage":1,
        "liquidationPrice":0, "notional":case["notional"], "contracts":case["contracts"],
        "unrealizedPnl":0
    }))
    .unwrap();
    position.unrealized_pnl = upnl;
    position
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn options_and_inverse_helpers_match_maintained_source() {
    // Native expectations first, independent of the source run.
    for (symbol, inverse) in [
        ("BTCUSD", true),
        ("ETHUSDZ26", true),
        ("BTCUSDT", false),
        ("USD", false),
    ] {
        assert_eq!(
            inverse_symbol::is_bybit_inverse_symbol(symbol),
            inverse,
            "native inverse symbol {symbol}"
        );
    }
    for (symbol, option) in [
        ("BTC-25SEP26-130000-C-USDT", true),
        ("SOL-6NOV26-112d5-P", true),
        ("BTC-125SEP26-1-C", false),
        ("BTCUSD", false),
    ] {
        assert_eq!(
            options::is_bybit_option_symbol(symbol),
            option,
            "native option symbol {symbol}"
        );
    }
    let sol = options::map_bybit_option_instrument(
        &json!({"status":"Trading",
        "symbol":"SOL-6NOV26-112d5-P","optionsType":"Put","baseCoin":"SOL"})
        .into(),
    )
    .unwrap()
    .expect("native tradable put");
    assert_eq!(
        sol.option.as_ref().unwrap().strike.to_bits(),
        112.5_f64.to_bits(),
        "native decimal strike"
    );

    let source = source();
    let symbols = source["symbols"].as_array().unwrap();
    assert_eq!(symbols.len(), 26, "symbol source floor");
    for case in symbols {
        let symbol = case["s"].as_str().unwrap();
        assert_eq!(
            json!(inverse_symbol::is_bybit_inverse_symbol(symbol)),
            case["inverse"],
            "isBybitInverseSymbol {symbol}"
        );
        assert_eq!(
            json!(options::is_bybit_option_symbol(symbol)),
            case["option"],
            "isBybitOptionSymbol {symbol}"
        );
    }
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "row source floor");
    for (index, case) in rows.iter().enumerate() {
        let native = options::map_bybit_option_instrument(&case["raw"].clone().into()).unwrap();
        assert_eq!(
            binary64(&serde_json::to_value(&native).unwrap()),
            binary64(&case["market"]),
            "mapBybitOptionInstrument row {index}"
        );
    }
    for (index, case) in source["positions"].as_array().unwrap().iter().enumerate() {
        let native = inverse_symbol::position_upnl_in_quote(&position(&case["p"]));
        assert_eq!(
            native.to_bits(),
            case["upnl"].as_f64().unwrap().to_bits(),
            "positionUpnlInQuote {index}"
        );
    }
    assert_eq!(source["units"], options::BYBIT_GREEK_UNITS, "greek units");
}

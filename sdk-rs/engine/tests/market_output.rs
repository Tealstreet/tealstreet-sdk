pub use safe_cex_engine::{QuantityUnits, model, wire};
#[path = "../src/market_output.rs"]
mod market_output;

fn market() -> model::Market {
    model::Market {
        id: "dated-native".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: model::Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: model::Limits {
            amount: model::MinMax { min: 0., max: 0. },
            leverage: model::MinMax { min: 0., max: 0. },
            price: Some(model::Min { min: 0. }),
        },
        contract_size: Some(0.),
        is_expiring: Some(true),
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: Some(0.),
        maker_fee: Some(0.),
    }
}

#[test]
fn output_retains_native_identity_precision_and_zero_but_omits_nonfinite_optional_values() {
    let mut row = market();
    market_output::prepare(&mut row, 3);
    assert_eq!(row.id, "dated-native");
    assert_eq!(row.symbol, "BTCUSDT");
    assert_eq!(row.pretty_symbol.as_deref(), Some("BTC/USDT XPERP"));
    assert_eq!(
        market_output::numbers(&row),
        [0.001, 0.1, 0., 0., 0., 0., 0., 0., 0., 0.]
    );
    row.contract_size = Some(f64::INFINITY);
    row.taker_fee = Some(f64::NEG_INFINITY);
    row.maker_fee = None;
    row.limits.price = Some(model::Min { min: f64::NAN });
    let numbers = market_output::numbers(&row);
    assert!(numbers[6..].iter().all(|v| v.is_nan()));
    assert_eq!(row.contract_size, Some(f64::INFINITY));
}

#[test]
fn explicit_labels_and_swap_identity_are_preserved() {
    let mut row = market();
    row.pretty_symbol = Some(String::new());
    market_output::prepare(&mut row, 3);
    assert_eq!(row.pretty_symbol.as_deref(), Some(""));
    row.pretty_symbol = None;
    row.id = "BTC-USDT-SWAP".into();
    market_output::prepare(&mut row, 3);
    assert_eq!(row.pretty_symbol, None);
    row.id = "spot-native".into();
    market_output::prepare(&mut row, 0);
    assert_eq!(row.pretty_symbol, None);
}

#[test]
fn native_writer_publishes_a_real_market_section() {
    let rows = [market()];
    let mut writer = wire::FrameWriter::new(Vec::new(), 1000.);
    market_output::write(&rows, 3, &mut writer);
    let bytes = writer.finish();
    assert_eq!(&bytes[..4], b"SCXF");
    assert_eq!(
        u16::from_le_bytes(bytes[16..18].try_into().unwrap()),
        wire::section::MARKETS
    );
    assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 1);
}

//! Regenerate the Market slice's packed fixture with the production native writer.
pub use safe_cex_engine::{QuantityUnits, model, wire};
#[path = "../src/market_output.rs"]
#[cfg(all(feature = "product-semantics", feature = "underlying-tokens"))]
mod market_output;

#[cfg(all(feature = "product-semantics", feature = "underlying-tokens"))]
fn main() {
    use model::{
        DerivativeProduct, InstrumentClassification, Limits, Market, Min, MinMax, Precision,
        ProductKind, UnderlyingToken,
    };
    use safe_cex_engine::{QuantityUnits, store::Store};
    let mut rows: Vec<_> = [
        ("native-spot", "BTCUSDT"),
        ("dated-native", "BTC-FUTURE"),
        ("native-call", "BTC-CALL"),
        ("native-put", "BTC-PUT"),
    ]
    .into_iter()
    .map(|(id, symbol)| Market {
        id: id.into(),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax { min: 0., max: 0. },
            leverage: MinMax { min: 0., max: 0. },
            price: Some(Min { min: 0. }),
        },
        contract_size: Some(0.),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: Some(0.),
        maker_fee: Some(0.),
    })
    .collect();
    rows[0].is_spot = Some(true);
    rows[1].is_expiring = Some(true);
    rows[1].contract_size = None;
    rows[1].taker_fee = None;
    rows[1].limits.price = None;
    for row in &mut rows {
        market_output::prepare(row, 3);
    }
    let mut expected = wire::FrameWriter::new(Vec::new(), 1000.);
    market_output::write(&rows, 3, &mut expected);
    let expected = expected.finish();
    let mut store = Store::default();
    store.set_quantity_units(QuantityUnits::ContractCount);
    store.set_markets(rows);
    store.set_instrument_classifications(vec![InstrumentClassification {
        market: 0,
        symbol_type: Some("innovation".into()),
        market_region: Some(String::new()),
        underlying_ticker: None,
    }]);
    store.set_underlying_tokens(vec![UnderlyingToken {
        market: 0,
        name: "UBTC".into(),
        full_name: Some("Unit Bitcoin".into()),
    }]);
    store.set_derivative_products(
        [
            (1, ProductKind::Future, None, None),
            (2, ProductKind::Call, Some(0.), Some(0.)),
            (3, ProductKind::Put, Some(123.), None),
        ]
        .into_iter()
        .map(|(market, kind, expiry, strike)| DerivativeProduct {
            market,
            kind,
            underlying: "BTC".into(),
            expiry,
            strike,
            settlement_asset: Some("USDT".into()),
            premium_asset: Some("USDT".into()),
            style: Some("european".into()),
            settlement: Some("cash".into()),
        })
        .collect(),
    );
    let mut writer = wire::FrameWriter::new(Vec::new(), 1000.);
    store.write(&store.everything(), &mut writer);
    let bytes = writer.finish();
    let length = u32::from_le_bytes(expected[20..24].try_into().unwrap()) as usize;
    assert_eq!(&bytes[16..24 + length], &expected[16..24 + length]);
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/market-output.scxf");
    if std::env::args().any(|arg| arg == "--check") {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    } else {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
}

#[cfg(not(all(feature = "product-semantics", feature = "underlying-tokens")))]
fn main() {
    panic!("fixture requires product-semantics,underlying-tokens");
}

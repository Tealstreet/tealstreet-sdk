use super::{CoinbaseDependencies, CoinbaseExchange};
use crate::{
    cdp_signing::CdpSigningCache,
    exchange_lifecycle::{LifecycleHost, LifecycleTimer},
    market_position_reads::{
        ReadItem, ReadItems, ReadMarket, ReadMarkets, ReadPosition, ReadPositions, ReadTicker,
        ReadTickers,
    },
    order_sync::PrivateOrderMarkets,
    public_market_reads::{CatalogMarket, CatalogMarkets, CatalogSupportedTypes, PublicCandles},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    js_runtime::{
        client,
        value::{Js, from_json, to_typed},
    },
    native::native_runtime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{
    ExchangeName, Market, OrderSide, Position, Ticker, retained_json::RetainedJsonInput as Input,
};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

fn opaque() -> Js {
    let input = Js::object(vec![
        (
            "values",
            Js::array(vec![
                Js::Number(-0.0),
                Js::Number(f64::NAN),
                Js::Number(f64::INFINITY),
                Js::Undefined,
                Js::String(vec![0xd800].into()),
            ]),
        ),
        ("missing", Js::Undefined),
    ]);
    input
        .define_key(&[0xd800], Js::String(vec![0xdc00].into()))
        .unwrap();
    input
}
fn assert_opaque(input: &Input) {
    let Input::Object(fields) = input else {
        panic!("native metadata object")
    };
    assert_eq!(fields.len(), 3);
    assert_eq!(fields[0].0, "values".encode_utf16().collect::<Vec<_>>());
    assert_eq!(fields[1].0, "missing".encode_utf16().collect::<Vec<_>>());
    assert!(matches!(fields[1].1, Input::Undefined));
    assert_eq!(fields[2].0, [0xd800]);
    assert!(matches!(&fields[2].1,Input::Text(units) if units==&[0xdc00]));
    let Input::Array(values) = &fields[0].1 else {
        panic!("native metadata array")
    };
    assert_eq!(values.len(), 5);
    assert!(matches!(values[0],Input::Float(v) if v.to_bits()==(-0.0_f64).to_bits()));
    assert!(matches!(values[1],Input::Float(v) if v.is_nan()));
    assert!(matches!(values[2],Input::Float(v) if v==f64::INFINITY));
    assert!(matches!(values[3], Input::Undefined));
    assert!(matches!(&values[4],Input::Text(units) if units==&[0xd800]));
}
fn market(id: &str) -> Js {
    let value = from_json(
        &json!({"id":id,"symbol":"BTCUSD","base":"BTC","quote":"USD","active":true,
        "precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":10}},"isSpot":false}),
    );
    value.define("info", opaque()).unwrap();
    value
        .define(
            "precision",
            Js::object(vec![
                ("amount", Js::Number(-0.0)),
                ("price", Js::Number(f64::NAN)),
            ]),
        )
        .unwrap();
    value
}
#[test]
fn retained_read_and_market_aliases_use_actual_native_decoders_and_live_roots() {
    let item: ReadItem = to_typed(&opaque()).unwrap();
    assert_opaque(&item.borrow());
    let rows: ReadItems = to_typed(&Js::array(vec![opaque(), Js::Null])).unwrap();
    let captured = rows.clone();
    assert!(Rc::ptr_eq(&rows, &captured));
    assert_eq!(captured.borrow().len(), 2);
    assert_opaque(&captured.borrow()[0].borrow());
    assert!(matches!(&*captured.borrow()[1].borrow(), Input::Null));
    let first = captured.borrow()[0].clone();
    assert!(Rc::ptr_eq(&first, &rows.borrow()[0]));
    *first.borrow_mut() = Input::Bool(true);
    assert!(matches!(&*rows.borrow()[0].borrow(), Input::Bool(true)));
    let one: ReadMarket = to_typed(&market("first")).unwrap();
    assert_eq!(one.borrow().id, "first");
    assert_eq!(
        one.borrow().precision.amount.to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(one.borrow().precision.price.is_nan());
    assert_opaque(one.borrow().info.as_ref().unwrap());
    let markets: ReadMarkets =
        to_typed(&Js::array(vec![market("first"), market("second")])).unwrap();
    let captured = markets.clone();
    assert!(Rc::ptr_eq(&markets, &captured));
    assert_eq!(captured.borrow().len(), 2);
    assert_eq!(captured.borrow()[0].borrow().id, "first");
    assert_eq!(captured.borrow()[1].borrow().id, "second");
    let row = captured.borrow()[0].clone();
    assert!(Rc::ptr_eq(&row, &markets.borrow()[0]));
    assert_opaque(row.borrow().info.as_ref().unwrap());
    row.borrow_mut().id = "mutated".into();
    assert_eq!(markets.borrow()[0].borrow().id, "mutated");
    let by_id: PrivateOrderMarkets = to_typed(&Js::object(vec![
        ("first", market("first")),
        ("second", market("second")),
    ]))
    .unwrap();
    let captured = by_id.clone();
    assert!(Rc::ptr_eq(&by_id, &captured));
    assert_eq!(
        captured.borrow().keys().cloned().collect::<Vec<_>>(),
        ["first", "second"]
    );
    let row = captured.borrow().get("first").unwrap().clone();
    assert!(Rc::ptr_eq(&row, by_id.borrow().get("first").unwrap()));
    assert_opaque(row.borrow().info.as_ref().unwrap());
    row.borrow_mut().symbol = "changed".into();
    assert_eq!(by_id.borrow()["first"].borrow().symbol, "changed");
}

#[test]
fn typed_data_collections_keep_scalar_bits_and_separate_info_roots() {
    let info: ReadItem = to_typed(&opaque()).unwrap();
    let mut ticker:Ticker=serde_json::from_value(json!({"id":"ticker","symbol":"BTCUSD","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":11})).unwrap();
    ticker.bid = -0.0;
    ticker.ask = f64::NAN;
    ticker.last = f64::INFINITY;
    let row = Rc::new(ReadTicker {
        fields: ticker,
        info: Some(info.clone()),
    });
    let tickers: ReadTickers = Rc::new(RefCell::new(vec![row.clone(), row.clone()]));
    let captured = tickers.clone();
    assert!(Rc::ptr_eq(&tickers, &captured));
    assert_eq!(captured.borrow().len(), 2);
    assert!(Rc::ptr_eq(&captured.borrow()[0], &row));
    assert!(Rc::ptr_eq(&captured.borrow()[1], &row));
    assert!(Rc::ptr_eq(row.info.as_ref().unwrap(), &info));
    assert!(row.fields.info.is_none());
    assert_eq!(row.fields.id, "ticker");
    assert_eq!(row.fields.bid.to_bits(), (-0.0_f64).to_bits());
    assert!(row.fields.ask.is_nan());
    assert_eq!(row.fields.last, f64::INFINITY);
    let projected = crate::binding::lifecycle::frame(&row.fields).unwrap();
    assert!(
        matches!(projected.get("bid"),Some(Frame::Number(v)) if v.to_bits()==(-0.0_f64).to_bits())
    );
    assert!(matches!(projected.get("ask"),Some(Frame::Number(v)) if v.is_nan()));
    let mut position:Position=serde_json::from_value(json!({"id":"position","symbol":"BTCUSD","side":"long","entryPrice":1,"notional":2,"leverage":3,"marginType":"cross","positionMode":"oneway","unrealizedPnl":4,"contracts":5,"liquidationPrice":6})).unwrap();
    position.entry_price = -0.0;
    position.notional = f64::INFINITY;
    position.contracts = f64::NAN;
    let position = Rc::new(ReadPosition {
        fields: position,
        info: Some(info.clone()),
    });
    let positions: ReadPositions = Rc::new(RefCell::new(vec![position.clone()]));
    let captured = positions.clone();
    assert!(Rc::ptr_eq(&positions, &captured));
    assert_eq!(captured.borrow().len(), 1);
    assert!(Rc::ptr_eq(&captured.borrow()[0], &position));
    assert!(Rc::ptr_eq(position.info.as_ref().unwrap(), &info));
    assert_eq!(position.fields.id, "position");
    assert_eq!(position.fields.entry_price.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(position.fields.notional, f64::INFINITY);
    assert!(position.fields.contracts.is_nan());
    let fields: Market = to_typed(&market("catalog")).unwrap();
    let catalog_row = Rc::new(RefCell::new(CatalogMarket {
        fields,
        info: info.clone(),
    }));
    let catalog: CatalogMarkets = Rc::new(RefCell::new(vec![catalog_row.clone()]));
    let captured = catalog.clone();
    assert!(Rc::ptr_eq(&catalog, &captured));
    assert!(Rc::ptr_eq(&captured.borrow()[0], &catalog_row));
    assert!(Rc::ptr_eq(&catalog_row.borrow().info, &info));
    assert_eq!(catalog_row.borrow().fields.id, "catalog");
    assert_eq!(
        catalog_row.borrow().fields.precision.amount.to_bits(),
        (-0.0_f64).to_bits()
    );
    assert!(catalog_row.borrow().fields.precision.price.is_nan());
    assert_opaque(&info.borrow());
    *info.borrow_mut() = Input::Undefined;
    assert!(matches!(
        &*row.info.as_ref().unwrap().borrow(),
        Input::Undefined
    ));
    assert!(matches!(
        &*position.info.as_ref().unwrap().borrow(),
        Input::Undefined
    ));
    assert!(matches!(
        &*catalog_row.borrow().info.borrow(),
        Input::Undefined
    ));
}

#[test]
fn candle_and_supported_type_aliases_decode_native_values_before_reporting() {
    let raw = Js::array(vec![
        Js::object(vec![
            ("timestamp", Js::Number(-0.0)),
            ("open", Js::Number(f64::NAN)),
            ("high", Js::Number(f64::INFINITY)),
            ("low", Js::Number(f64::NEG_INFINITY)),
            ("close", Js::Number(17.0)),
            ("volume", Js::Number(0.0)),
            ("lastSide", Js::str("buy")),
        ]),
        Js::object(vec![
            ("timestamp", Js::Number(1.0)),
            ("open", Js::Number(2.0)),
            ("high", Js::Number(3.0)),
            ("low", Js::Number(4.0)),
            ("close", Js::Number(5.0)),
            ("volume", Js::Number(-0.0)),
            ("lastSide", Js::str("sell")),
        ]),
    ]);
    let candles: PublicCandles = to_typed(&raw).unwrap();
    let captured = candles.clone();
    assert!(Rc::ptr_eq(&candles, &captured));
    assert_eq!(captured.borrow().len(), 2);
    for (index, expected) in [
        [-0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 17.0, 0.0],
        [1.0, 2.0, 3.0, 4.0, 5.0, -0.0],
    ]
    .into_iter()
    .enumerate()
    {
        let row = captured.borrow()[index].clone();
        assert!(Rc::ptr_eq(&row, &candles.borrow()[index]));
        let row = row.borrow();
        for (actual, expected) in [
            row.timestamp,
            row.open,
            row.high,
            row.low,
            row.close,
            row.volume,
        ]
        .into_iter()
        .zip(expected)
        {
            if expected.is_nan() {
                assert!(actual.is_nan())
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits())
            }
        }
        assert_eq!(
            row.last_side,
            Some(if index == 0 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            })
        );
    }
    let types: CatalogSupportedTypes =
        to_typed(&Js::array(vec![Js::str("SPOT"), Js::str("INTX")])).unwrap();
    let captured = types.clone();
    assert!(Rc::ptr_eq(&types, &captured));
    assert_eq!(&*captured.borrow(), &["SPOT", "INTX"]);
    types.borrow_mut().push("CFM".into());
    assert_eq!(&*captured.borrow(), &["SPOT", "INTX", "CFM"]);
}

#[test]
fn lifecycle_timer_alias_is_created_and_cancelled_by_the_real_runtime() {
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
            let runtime = native_runtime();
            let http = client::rest_http_host(
                runtime.clone(),
                Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
            );
            let exchange = CoinbaseExchange::new(
                name,
                serde_json::from_value(json!({"key":"synthetic","secret":"synthetic"})).unwrap(),
                Rc::new(DefaultStore::new()),
                CoinbaseDependencies {
                    runtime,
                    http,
                    external_public_data: None,
                    signing_cache: Rc::new(CdpSigningCache::default()),
                    local_wall_time_to_utc: Rc::new(|v| v),
                },
            )
            .unwrap();
            let fired = Rc::new(Cell::new(0));
            let callback_count = fired.clone();
            let timer: LifecycleTimer = exchange
                .host
                .set_timeout(
                    Rc::new(move || {
                        callback_count.set(callback_count.get() + 1);
                        Ok(())
                    }),
                    5.0,
                )
                .unwrap();
            let captured = timer.clone();
            assert!(Rc::ptr_eq(&timer, &captured));
            assert!(captured.borrow().is_object(), "native Timeout projects to its JSON object view");
            exchange.host.clear_timeout(captured).unwrap();
            tokio::time::sleep(Duration::from_millis(15)).await;
            assert_eq!(fired.get(), 0);
            let callback_count = fired.clone();
            let _timer: LifecycleTimer = exchange
                .host
                .set_timeout(
                    Rc::new(move || {
                        callback_count.set(callback_count.get() + 1);
                        Ok(())
                    }),
                    0.0,
                )
                .unwrap();
            tokio::time::timeout(Duration::from_secs(1), async {
                while fired.get() == 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            assert_eq!(fired.get(), 1);
        }
    });
}

//! Combined Bybit products use native V5 categories on one account connection.
mod support;
use safe_cex_engine::model::{
    OptionChainQuery, OptionRight, OrderSide, OrderType, PlaceOrderOpts, ProductKind,
};
use safe_cex_engine::{Command, Event, Query};
use safe_cex_venue_bybit::{Options, Variant};
use support::Harness;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const OPTION: &str = "BTC-25DEC26-100000-C-USDT";
const OPTIONS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","status":"Trading","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Call","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.01","minOrderQty":"0.01","maxOrderQty":"100"},"priceFilter":{"tickSize":"5","minPrice":"5"}},{"symbol":"ETH-25DEC26-3000-P-USDT","status":"Trading","baseCoin":"ETH","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Put","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.1","minOrderQty":"0.1","maxOrderQty":"100"},"priceFilter":{"tickSize":"0.5","minPrice":"0.5"}}],"nextPageCursor":""}}"#;
const QUOTE: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","bid1Price":"1000","ask1Price":"1010","markPrice":"1005","lastPrice":"1000","markIv":"0.5","bid1Iv":"0.49","ask1Iv":"0.51","underlyingPrice":"99000","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}]}}"#;

fn inverse_instruments() -> String {
    support::INSTRUMENTS
        .replace("BTCUSDT", "BTCUSD")
        .replace("ETHUSDT", "ETHUSD")
        .replace("LinearPerpetual", "InversePerpetual")
        .replace("\"USDT\"", "\"USD\"")
        .replace("1190.000", "10000")
        .replace("0.001", "1")
}

fn options(private: bool) -> Options {
    Options {
        enable_inverse_markets: true,
        enable_option_markets: true,
        public_data_only: !private,
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Options::default()
    }
}

fn respond(h: &mut Harness, path: &str, category: &str, extra: Option<&str>, body: &str) {
    let index = h
        .requests
        .iter()
        .position(|request| {
            request.url.contains(path)
                && request.url.contains(&format!("category={category}"))
                && extra.is_none_or(|extra| request.url.contains(extra))
        })
        .unwrap_or_else(|| panic!("missing {path} {category} {extra:?}: {:?}", h.paths()));
    let request = h.requests.remove(index);
    h.reply(request.id, 200, body);
}

fn ready(private: bool) -> Harness {
    let mut h = Harness::started(options(private));
    if private {
        h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    }
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
        if private {
            respond(&mut h, "fee-rate", category, None, EMPTY);
        }
    }
    respond(&mut h, "instruments-info", "option", None, OPTIONS);
    respond(
        &mut h,
        "instruments-info",
        "inverse",
        None,
        &inverse_instruments(),
    );
    assert!(!h.engine.store().loaded().markets);
    let linear = support::INSTRUMENTS.replace(
        "\"symbolType\":\"\"",
        "\"symbolType\":\"xstock\",\"marketRegion\":\"US\",\"underlyingTicker\":\"BTC\"",
    );
    respond(&mut h, "instruments-info", "linear", None, &linear);
    assert!(h.engine.store().loaded().markets);
    assert_eq!(h.engine.store().markets().len(), 6);
    respond(&mut h, "tickers", "option", Some("baseCoin=ETH"), EMPTY);
    respond(
        &mut h,
        "tickers",
        "inverse",
        None,
        &support::TICKERS
            .replace("BTCUSDT", "BTCUSD")
            .replace("ETHUSDT", "ETHUSD"),
    );
    respond(&mut h, "tickers", "linear", None, support::TICKERS);
    assert!(!h.engine.store().loaded().tickers);
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
    assert!(h.engine.store().loaded().tickers);
    for category in ["linear", "inverse", "option"] {
        let socket = h.socket(&format!("public/{category}")).unwrap();
        h.event(Event::WsOpened { socket });
    }
    if private {
        assert_eq!(
            h.sockets
                .iter()
                .filter(|(_, url)| url.ends_with("/private"))
                .count(),
            1
        );
        let socket = h.socket("/private").unwrap();
        h.event(Event::WsOpened { socket });
        h.message("/private", r#"{"op":"auth","success":true}"#);
        h.message("/private", r#"{"op":"subscribe","success":true}"#);
    }
    h
}

fn order(symbol: &str, amount: f64) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol: symbol.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        amount,
        price: Some(1000.0),
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        close: None,
        stp_mode: None,
        implied_volatility: None,
        max_slippage_pct: None,
        bybit_bbo: None,
        bybit_slippage_tolerance_ticks: None,
    }
}

#[test]
fn discovery_is_atomic_across_categories_and_preserves_classification_and_option_terms() {
    let h = ready(false);
    let store = h.engine.store();
    let by_symbol = |symbol: &str| {
        store
            .instrument_classifications()
            .unwrap()
            .iter()
            .find(|row| store.markets()[row.market as usize].symbol == symbol)
            .unwrap()
    };
    assert_eq!(by_symbol("BTCUSDT").symbol_type.as_deref(), Some("xstock"));
    assert_eq!(by_symbol("BTCUSDT").market_region.as_deref(), Some("US"));
    assert_eq!(by_symbol("BTCUSD").symbol_type.as_deref(), Some(""));
    let product = store
        .derivative_products()
        .unwrap()
        .iter()
        .find(|row| store.markets()[row.market as usize].symbol == OPTION)
        .unwrap();
    assert_eq!(product.kind, ProductKind::Call);
    assert_eq!(product.strike, Some(100000.0));
    assert_eq!(product.settlement_asset.as_deref(), Some("USDT"));
    assert_eq!(store.market(OPTION).unwrap().limits.leverage.max, 1.0);
    assert_eq!(store.market("BTCUSD").unwrap().is_inverse, Some(true));
    assert!(
        h.sent_on("public/option")
            .iter()
            .all(|message| !message.contains("BTCUSDT"))
    );
    assert!(
        h.sent_on("public/linear")
            .iter()
            .all(|message| !message.contains(OPTION))
    );
}

#[test]
fn native_options_chain_expiries_greeks_and_iv_order_keep_their_wire_contract() {
    let mut h = ready(true);
    h.command(Command::FetchOptionChain {
        command: 51,
        query: OptionChainQuery {
            underlying: "BTC".into(),
            expiry: None,
            right: Some(OptionRight::Call),
            strike_min: Some(90000.0),
            strike_max: Some(110000.0),
        },
    });
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
    let chain: Vec<serde_json::Value> =
        serde_json::from_str(h.result(51).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0]["market"]["option"]["right"], "call");
    assert_eq!(chain[0]["quote"]["greeks"]["delta"], 0.4);
    h.command(Command::FetchOptionExpiries {
        command: 52,
        underlying: "BTC".into(),
    });
    assert_eq!(h.result(52).unwrap().as_ref().unwrap(), "[1798185600000.0]");
    h.command(Command::FetchAccountGreeks { command: 53 });
    let request = h.take("/v5/asset/coin-greeks");
    assert!(Harness::header(&request, "X-BAPI-SIGN").is_some());
    h.reply(request.id, 200, r#"{"retCode":0,"result":{"list":[{"baseCoin":"BTC","totalDelta":"1","totalGamma":"2","totalVega":"3","totalTheta":"-4"}]}}"#);
    assert!(
        h.result(53)
            .unwrap()
            .as_ref()
            .unwrap()
            .contains("\"theta\":-4.0")
    );
    let mut opts = order(OPTION, 0.12);
    opts.implied_volatility = Some(0.6);
    opts.client_order_id = Some("option-caller".into());
    h.command(Command::PlaceOrders {
        command: 54,
        orders: vec![opts],
    });
    let request = h.take("/v5/order/create");
    let body = request.body.unwrap();
    assert!(body.contains("\"category\":\"option\""));
    assert!(body.contains("\"orderIv\":\"0.6\""));
    assert!(!body.contains("\"price\""));
    assert!(!body.contains("positionIdx"));
    assert!(body.contains("option-caller"));
    h.reply(
        request.id,
        200,
        r#"{"retCode":0,"result":{"orderId":"option-order"}}"#,
    );
    assert_eq!(
        h.result(54).unwrap().as_ref().unwrap(),
        "[\"option-order\"]"
    );
}

#[test]
fn malformed_option_stream_greeks_preserve_proven_position_and_frame_values() {
    let mut h = ready(true);
    let row = r#"{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}"#;
    let message = |row: &str| format!(r#"{{"topic":"position","data":[{row}]}}"#);
    h.message("/private", &message(row));
    let positions = h.engine.store().positions().to_vec();
    let greeks = h.engine.store().position_greeks().unwrap().to_vec();
    assert_eq!(positions.len(), 1);
    assert_eq!(greeks.len(), 1);
    for (field, original) in [
        ("delta", "0.4"),
        ("gamma", "0.01"),
        ("vega", "12"),
        ("theta", "-8"),
    ] {
        for invalid in ["bad", "NaN", "inf"] {
            let malformed = row
                .replace(
                    &format!(r#""{field}":"{original}""#),
                    &format!(r#""{field}":"{invalid}""#),
                )
                .replace(r#""size":"0.2""#, r#""size":"0.3""#);
            h.message("/private", &message(&malformed));
            assert_eq!(h.engine.store().positions(), positions, "{field}={invalid}");
            assert_eq!(h.engine.store().position_greeks().unwrap(), greeks);
            let frame = h.engine.snapshot_frame(h.now);
            let section = safe_cex_engine::wire::sections(frame)
                .find(|(kind, _)| *kind == safe_cex_engine::wire::section::POSITION_GREEKS)
                .unwrap()
                .1;
            assert_eq!(u32::from_le_bytes(section[..4].try_into().unwrap()), 1);
            let positions = safe_cex_engine::wire::sections(frame)
                .find(|(kind, _)| *kind == safe_cex_engine::wire::section::POSITIONS)
                .unwrap()
                .1;
            // One position: count/market/enums/padding + nine f64 columns,
            // followed by its exact ID, matching the Greek metadata reference.
            assert_eq!(&positions[88..92], &section[4..8]);
            let values: [f64; 4] = std::array::from_fn(|index| {
                let at = 16 + index * 8;
                f64::from_le_bytes(section[at..at + 8].try_into().unwrap())
            });
            assert_eq!(values, [0.4, 0.01, 12.0, -8.0]);
        }
    }
    // Empty optional values follow REST policy: absent gamma is represented as 0.
    h.message(
        "/private",
        &message(&row.replace(r#""gamma":"0.01""#, r#""gamma":"""#)),
    );
    assert_eq!(
        h.engine.store().position_greeks().unwrap()[0].greeks.gamma,
        0.0
    );
    h.message("/private", &message(row));
    assert_eq!(h.engine.store().position_greeks().unwrap(), greeks);
}

#[test]
fn category_snapshots_and_private_rows_preserve_siblings_and_native_inverse_sizing() {
    let mut h = ready(true);
    for category in ["option", "inverse", "linear"] {
        while h.requests.iter().any(|request| {
            request.url.contains("order/realtime")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "order/realtime", category, None, EMPTY);
        }
        while h.requests.iter().any(|request| {
            request.url.contains("position/list")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "position/list", category, None, EMPTY);
        }
    }
    assert!(h.engine.store().loaded().orders);
    assert!(h.engine.store().loaded().positions);
    h.message("/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"1","avgPrice":"50000","positionValue":"50000","unrealisedPnl":"2","leverage":"10","positionIdx":0},{"category":"inverse","symbol":"BTCUSD","side":"Buy","size":"1000","avgPrice":"50000","unrealisedPnl":"0.01","leverage":"10","positionIdx":0},{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}]}"#);
    assert_eq!(h.engine.store().positions().len(), 3);
    let inverse = h
        .engine
        .store()
        .positions()
        .iter()
        .find(|row| row.symbol == "BTCUSD")
        .unwrap();
    assert_eq!(inverse.notional, 1000.0);
    assert!((inverse.contracts - 0.01500002).abs() < 0.000001);
    let greek = &h.engine.store().position_greeks().unwrap()[0];
    assert_eq!(greek.greeks.delta, 0.4);
    let opts = order("BTCUSD", 42.6);
    h.command(Command::PlaceOrders {
        command: 71,
        orders: vec![opts],
    });
    let request = h.take("/v5/order/create-batch");
    let body = request.body.unwrap();
    assert!(body.contains("\"category\":\"inverse\""));
    assert!(body.contains("\"qty\":\"43\""));
}

#[test]
fn product_toggles_are_disabled_on_demo_and_standalone_variants() {
    for variant in [Variant::Demo, Variant::Inverse, Variant::Spot] {
        let mut opts = options(false);
        opts.variant = variant;
        let h = Harness::started(opts);
        assert!(
            h.requests
                .iter()
                .all(|request| !request.url.contains("category=option"))
        );
        if variant != Variant::Inverse {
            assert!(
                h.requests
                    .iter()
                    .all(|request| !request.url.contains("category=inverse"))
            );
        }
    }
}

#[test]
fn invalid_option_in_a_mixed_batch_fails_before_any_native_write() {
    let mut h = ready(true);
    let mut invalid = order(OPTION, 1.0);
    invalid.stop_loss = Some(500.0);
    h.command(Command::PlaceOrders {
        command: 81,
        orders: vec![invalid, order("BTCUSDT", 1.0)],
    });
    assert!(h.result(81).unwrap().is_err());
    assert!(
        h.requests
            .iter()
            .all(|request| request.method != safe_cex_engine::Method::Post)
    );
}

#[test]
fn account_wide_histories_visit_enabled_categories_and_symbol_queries_route_directly() {
    let mut h = ready(true);
    h.command(Command::Query {
        command: 91,
        query: Query::OrderHistory {
            symbol: None,
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
    });
    for category in ["linear", "inverse", "option"] {
        respond(&mut h, "order/history", category, None, EMPTY);
    }
    assert_eq!(h.result(91).unwrap().as_ref().unwrap(), "[]");
    h.command(Command::Query {
        command: 92,
        query: Query::Fills {
            symbol: Some(OPTION.into()),
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(10),
        },
    });
    respond(&mut h, "execution/list", "option", None, EMPTY);
    assert_eq!(h.result(92).unwrap().as_ref().unwrap(), "[]");
}

#[test]
fn failed_inverse_poll_preserves_all_categories_and_stale_option_snapshot_keeps_stream_rows() {
    let mut h = ready(true);
    for category in ["linear", "inverse", "option"] {
        while h.requests.iter().any(|request| {
            request.url.contains("order/realtime")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "order/realtime", category, None, EMPTY);
        }
        while h.requests.iter().any(|request| {
            request.url.contains("position/list")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "position/list", category, None, EMPTY);
        }
    }
    h.advance(5000.0);
    h.message("/private", r#"{"topic":"position","data":[{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"1","avgPrice":"50000","positionValue":"50000","unrealisedPnl":"2","leverage":"10","positionIdx":1},{"category":"inverse","symbol":"BTCUSD","side":"Buy","size":"1000","avgPrice":"50000","unrealisedPnl":"0.01","leverage":"10","positionIdx":0},{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}]}"#);
    let index = h
        .requests
        .iter()
        .position(|request| {
            request.url.contains("position/list") && request.url.contains("category=inverse")
        })
        .unwrap();
    let inverse = h.requests.remove(index);
    h.reply(
        inverse.id,
        503,
        r#"{"retCode":10000,"retMsg":"temporary error"}"#,
    );
    respond(&mut h, "position/list", "option", None, EMPTY);
    while h.requests.iter().any(|request| {
        request.url.contains("position/list") && request.url.contains("category=linear")
    }) {
        respond(&mut h, "position/list", "linear", None, EMPTY);
    }
    assert_eq!(h.engine.store().positions().len(), 3);
    assert!(h.engine.store().hedged());
    assert_eq!(h.engine.store().position_greeks().unwrap().len(), 1);
}

#[test]
fn malformed_option_queries_fail_instead_of_succeeding_with_partial_rows() {
    let mut h = ready(true);
    h.command(Command::FetchAccountGreeks { command: 101 });
    h.respond("coin-greeks", 200, r#"{"retCode":0,"result":{"list":[{"baseCoin":"BTC","totalDelta":"1","totalGamma":"2","totalVega":"3","totalTheta":"-4"},{"baseCoin":"ETH","totalDelta":"bad"}]}}"#);
    assert!(h.result(101).unwrap().is_err());
    h.command(Command::FetchOptionChain {
        command: 102,
        query: OptionChainQuery {
            underlying: "BTC".into(),
            expiry: None,
            right: None,
            strike_min: None,
            strike_max: None,
        },
    });
    respond(
        &mut h,
        "tickers",
        "option",
        None,
        r#"{"retCode":0,"result":{"list":[{"symbol":4}]}}"#,
    );
    assert!(h.result(102).unwrap().is_err());
}

#[test]
fn account_history_limit_is_applied_after_every_category_returns_its_newest_rows() {
    let mut h = ready(true);
    h.command(Command::Query {
        command: 111,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(1),
        },
    });
    for (category, symbol, id, time) in [
        ("linear", "BTCUSDT", "old", 1760000000000u64),
        ("inverse", "BTCUSD", "new", 1760000001000),
        ("option", OPTION, "newest", 1760000002000),
    ] {
        let body = format!(
            r#"{{"retCode":0,"result":{{"list":[{{"execId":"{id}","symbol":"{symbol}","orderId":"{id}","execTime":"{time}","execQty":"1","execPrice":"1000","side":"Buy","execType":"Trade"}}]}}}}"#
        );
        respond(&mut h, "execution/list", category, None, &body);
    }
    let result = h.result(111).unwrap().as_ref().unwrap();
    assert!(result.contains("newest"));
    assert!(!result.contains("\"id\":\"old\""));
    assert!(!result.contains("\"id\":\"new\""));
}

#[test]
fn option_snapshot_keeps_other_categories_market_settings() {
    let mut h = ready(true);
    let linear = r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","category":"linear","size":"1","side":"Buy","avgPrice":"50000","leverage":"10","positionIdx":0}]}}"#;
    let inverse = r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSD","category":"inverse","size":"1000","side":"Buy","avgPrice":"50000","leverage":"5","positionIdx":0}]}}"#;
    while h.requests.iter().any(|request| {
        request.url.contains("position/list") && request.url.contains("category=linear")
    }) {
        respond(&mut h, "position/list", "linear", None, linear);
    }
    respond(&mut h, "position/list", "inverse", None, inverse);
    respond(&mut h, "position/list", "option", None, EMPTY);
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    assert_eq!(h.engine.store().setting("BTCUSD").unwrap().leverage, 5.0);
}

#[test]
fn reconnect_repairs_each_category_and_ignores_old_socket_data_and_old_http_snapshot() {
    let mut h = ready(true);
    for category in ["linear", "inverse", "option"] {
        while h.requests.iter().any(|request| {
            request.url.contains("order/realtime")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "order/realtime", category, None, EMPTY);
        }
        while h.requests.iter().any(|request| {
            request.url.contains("position/list")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "position/list", category, None, EMPTY);
        }
    }
    h.advance(5000.0);
    let index = h
        .requests
        .iter()
        .position(|request| {
            request.url.contains("position/list") && request.url.contains("category=option")
        })
        .unwrap();
    let old_read = h.requests.remove(index);
    let old = h.socket("/private").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(1000.0);
    let new = h
        .sockets
        .iter()
        .rev()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    assert_ne!(old, new);
    h.event(Event::WsOpened { socket: new });
    h.event(Event::WsMessage {
        socket: new,
        text: r#"{"op":"auth","success":true}"#,
    });
    h.event(Event::WsMessage {
        socket: new,
        text: r#"{"op":"subscribe","success":true}"#,
    });
    for category in ["linear", "inverse", "option"] {
        assert!(
            h.requests
                .iter()
                .any(|request| request.url.contains("order/realtime")
                    && request.url.contains(&format!("category={category}")))
        );
        assert!(
            h.requests
                .iter()
                .any(|request| request.url.contains("position/list")
                    && request.url.contains(&format!("category={category}")))
        );
    }
    let position = r#"{"topic":"position","data":[{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4"}]}"#;
    h.event(Event::WsMessage {
        socket: old,
        text: position,
    });
    assert!(h.engine.store().positions().is_empty());
    h.event(Event::WsMessage {
        socket: new,
        text: position,
    });
    assert_eq!(h.engine.store().positions().len(), 1);
    h.reply(old_read.id, 200, EMPTY);
    assert_eq!(h.engine.store().positions().len(), 1);
}

#[test]
fn malformed_successful_private_rows_preserve_category_state_and_siblings() {
    let mut h = ready(true);
    for category in ["linear", "inverse", "option"] {
        while h.requests.iter().any(|request| {
            request.url.contains("order/realtime")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "order/realtime", category, None, EMPTY);
        }
        while h.requests.iter().any(|request| {
            request.url.contains("position/list")
                && request.url.contains(&format!("category={category}"))
        }) {
            respond(&mut h, "position/list", category, None, EMPTY);
        }
    }
    h.message("/private", r#"{"topic":"position","data":[{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4"},{"category":"linear","symbol":"BTCUSDT","side":"Buy","size":"1","avgPrice":"50000","leverage":"10","positionIdx":0}]}"#);
    h.message("/private", r#"{"topic":"order","data":[{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","orderId":"existing-option","orderType":"Limit","orderStatus":"New","qty":"0.2","price":"1000","side":"Buy"}]}"#);
    h.advance(5000.0);
    respond(
        &mut h,
        "position/list",
        "option",
        None,
        r#"{"retCode":0,"result":{"list":[{"broken":true}]}}"#,
    );
    assert_eq!(h.engine.store().positions().len(), 2);
    // Reconnecting starts a fresh order reconciliation for every category.
    let socket = h.socket("/private").unwrap();
    h.event(Event::WsClosed { socket, code: 1006 });
    h.advance(1000.0);
    let new = h
        .sockets
        .iter()
        .rev()
        .find(|(_, url)| url.ends_with("/private"))
        .unwrap()
        .0;
    h.event(Event::WsOpened { socket: new });
    h.event(Event::WsMessage {
        socket: new,
        text: r#"{"op":"auth","success":true}"#,
    });
    h.event(Event::WsMessage {
        socket: new,
        text: r#"{"op":"subscribe","success":true}"#,
    });
    respond(
        &mut h,
        "order/realtime",
        "option",
        None,
        r#"{"retCode":0,"result":{"list":[{"broken":true}]}}"#,
    );
    assert!(h.engine.store().order("existing-option").is_some());
    assert_eq!(h.engine.store().position_greeks().unwrap().len(), 1);
}

#[test]
fn nonoption_iv_is_rejected_before_a_mixed_batch_can_take_position_stop_shortcuts() {
    for symbol in ["BTCUSDT", "BTCUSD"] {
        for kind in [OrderType::StopLoss, OrderType::TrailingStopLoss] {
            let mut h = ready(true);
            let mut invalid = order(symbol, 1.0);
            invalid.kind = kind;
            invalid.implied_volatility = Some(0.5);
            let good = order(OPTION, 0.1);
            h.command(Command::PlaceOrders {
                command: 121,
                orders: vec![good, invalid],
            });
            assert!(h.result(121).unwrap().is_err());
            assert!(
                h.requests
                    .iter()
                    .all(|request| request.method != safe_cex_engine::Method::Post)
            );
        }
    }
}

#[test]
fn inverse_position_uses_native_mark_before_last_when_the_ticker_mark_is_cold() {
    let mut h = ready(true);
    h.engine.store_mut().update_ticker("BTCUSD", |ticker| {
        ticker.mark = 0.0;
        ticker.last = 50000.0;
    });
    h.message("/private", r#"{"topic":"position","data":[{"category":"inverse","symbol":"BTCUSD","side":"Buy","size":"1000","avgPrice":"50000","markPrice":"100000","unrealisedPnl":"0.01","leverage":"10","positionIdx":0}]}"#);
    let inverse = &h.engine.store().positions()[0];
    assert_eq!(inverse.notional, 1000.0);
    assert_eq!(inverse.contracts, 0.01);
}

// https://bybit-exchange.github.io/docs/v5/websocket/public/ticker (option example, an object)
#[test]
fn documented_option_ticker_frame_updates_quotes_and_change() {
    let mut h = ready(false);
    let symbol = "BTC-25DEC26-100000-C-USDT";
    let frame = |bid: &str| {
        format!(
            r#"{{"id":"tickers.{symbol}-1","topic":"tickers.{symbol}","ts":1672917511074,"data":{{"symbol":"{symbol}","bidPrice":"{bid}","bidSize":"1","bidIv":"0.4","askPrice":"1020","askSize":"2","askIv":"0.6","lastPrice":"1001","highPrice24h":"1100","lowPrice24h":"900","markPrice":"1003","indexPrice":"99100","markPriceIv":"0.5","underlyingPrice":"99050","openInterest":"3","turnover24h":"10","volume24h":"5","totalVolume":"6","totalTurnover":"11","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8","predictedDeliveryPrice":"0","change24h":"-0.05"}},"type":"snapshot"}}"#
        )
    };
    h.message("public/option", &frame("990"));
    let t = h.engine.store().ticker(symbol).unwrap().clone();
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark),
        (990.0, 1020.0, 1001.0, 1003.0)
    );
    assert!((t.percentage + 5.0).abs() < 1e-9, "{}", t.percentage);
    h.message("public/option", &frame("0"));
    assert_eq!(h.engine.store().ticker(symbol).unwrap().bid, 0.0);
    // Derivative frames keep their own spelling.
    h.message(
        "public/linear",
        r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","bid1Price":"41000","ask1Price":"41001","price24hPcnt":"0.01"}}"#,
    );
    let t = h.engine.store().ticker("BTCUSDT").unwrap();
    assert_eq!((t.bid, t.ask), (41000.0, 41001.0));
    assert!((t.percentage - 1.0).abs() < 1e-9);
}

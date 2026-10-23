//! Combined Bybit products use native V5 categories on one account connection.
mod support;
use safe_cex_engine::model::{
    OptionChainQuery, OptionRight, OrderSide, OrderType, PlaceOrderOpts, ProductKind,
};
use safe_cex_engine::{Command, Event, Query};
use safe_cex_venue_bybit::Variant;
use support::Harness;
use support::Options;

const EMPTY: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}}"#;
const OPTION: &str = "BTC-25DEC26-100000-C-USDT";
const OPTIONS: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","status":"Trading","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Call","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.01","minOrderQty":"0.01","maxOrderQty":"100"},"priceFilter":{"tickSize":"5","minPrice":"5"}},{"symbol":"ETH-25DEC26-3000-P-USDT","status":"Trading","baseCoin":"ETH","quoteCoin":"USDT","settleCoin":"USDT","optionsType":"Put","deliveryTime":"1798185600000","lotSizeFilter":{"qtyStep":"0.1","minOrderQty":"0.1","maxOrderQty":"100"},"priceFilter":{"tickSize":"0.5","minPrice":"0.5"}}],"nextPageCursor":""}}"#;
const QUOTE: &str = r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"symbol":"BTC-25DEC26-100000-C-USDT","bid1Price":"1000","ask1Price":"1010","markPrice":"1005","lastPrice":"1000","markIv":"0.5","bid1Iv":"0.49","ask1Iv":"0.51","underlyingPrice":"99000","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}]}}"#;

#[test]
fn option_seed_envelope_limit_preserves_reset_headers_and_retries_after_reset() {
    let mut h = Harness::started(options(false));
    respond(&mut h, "instruments-info", "option", None, OPTIONS);
    let index = h
        .requests
        .iter()
        .position(|r| r.url.contains("tickers") && r.url.contains("baseCoin=BTC"))
        .unwrap();
    let request = h.requests.remove(index);
    let headers = format!("X-Bapi-Limit-Reset-Timestamp: {}", h.now + 60_000.0);
    h.event(Event::HttpResponse {
        request: request.id,
        response: safe_cex_engine::Response {
            status: 200,
            headers: &headers,
            body: r#"{"retCode":10006,"retMsg":"Too many visits!","result":{}}"#,
        },
    });
    h.advance(59_999.0);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("baseCoin=BTC") && r.url.contains("tickers"))
    );
    h.advance(1.0);
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
}

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

#[test]
fn each_category_socket_is_supervised_on_its_own() {
    let mut h = ready(false);
    let socket = |h: &Harness, category: &str| h.socket(&format!("public/{category}")).unwrap();
    let [linear, inverse, option] = ["linear", "inverse", "option"].map(|c| socket(&h, c));
    let opened = h.now;
    for _ in 0..19 {
        h.advance(1_000.0);
        h.message(
            "public/inverse",
            r#"{"topic":"tickers.BTCUSD","type":"delta","data":{"symbol":"BTCUSD","lastPrice":"70001"}}"#,
        );
    }
    h.advance(opened + 20_000.0 - h.now);
    assert!(h.closed.contains(&linear) && h.closed.contains(&option));
    assert!(
        !h.closed.contains(&inverse),
        "inverse traffic kept it alive"
    );
    h.advance(500.0);
    let fresh = socket(&h, "linear");
    assert_ne!(fresh, linear);
    assert_ne!(socket(&h, "option"), option);
    assert_eq!(socket(&h, "inverse"), inverse);
    h.event(Event::WsOpened { socket: fresh });
    h.advance(100.0);
    let sent: Vec<_> = h.sent.iter().filter(|(s, _)| *s == fresh).collect();
    assert!(
        sent.iter()
            .any(|(_, text)| text.contains("tickers.BTCUSDT"))
    );
    assert!(
        sent.iter()
            .all(|(_, text)| !text.contains("tickers.BTCUSD\""))
    );
}

#[test]
fn generated_option_identity_receipt_precedes_create() {
    let mut h = ready(true);
    h.command(Command::PlaceOrders {
        command: 91,
        orders: vec![order(OPTION, 0.12)],
    });
    let request = h.take("/v5/order/create");
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity {
        order_link_id: String,
    }
    let wire: Identity = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(h.progress.last().unwrap().0, 91);
    let receipts = &h.progress.last().unwrap().1;
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].symbol, OPTION);
    assert_eq!(receipts[0].identity, wire.order_link_id);
    assert_eq!(
        h.progress_at_request
            .iter()
            .find(|(id, _)| *id == request.id)
            .unwrap()
            .1,
        h.progress.len()
    );
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

#[test]
fn failed_option_catalog_does_not_stall_linear_and_inverse_and_recovers_later() {
    let mut h = Harness::started(options(false));
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
    }
    let failed = h
        .requests
        .iter()
        .position(|r| r.url.contains("instruments-info") && r.url.contains("category=option"))
        .unwrap();
    let failed = h.requests.remove(failed);
    h.reply(failed.id, 503, "upstream unavailable");
    respond(
        &mut h,
        "instruments-info",
        "inverse",
        None,
        &inverse_instruments(),
    );
    respond(
        &mut h,
        "instruments-info",
        "linear",
        None,
        support::INSTRUMENTS,
    );
    assert!(
        h.engine.store().loaded().markets,
        "option failure withheld working markets"
    );
    let linear_index = h
        .engine
        .store()
        .markets()
        .iter()
        .position(|m| m.symbol == "BTCUSDT")
        .unwrap();
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
    assert!(h.engine.store().loaded().tickers);
    assert!(h.socket("public/linear").is_some() && h.socket("public/inverse").is_some());
    assert!(h.socket("public/option").is_none());
    assert!(h.logs.iter().any(|m| m.contains("Failed to fetch Bybit")));
    // A live linear quote must survive the option catalog joining later.
    let socket = h.socket("public/linear").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "public/linear",
        r#"{"topic":"tickers.BTCUSDT","type":"delta","data":{"symbol":"BTCUSDT","bid1Price":"41234"}}"#,
    );
    h.advance(1_000.0);
    respond(&mut h, "instruments-info", "option", None, OPTIONS);
    assert!(h.engine.store().market(OPTION).is_some());
    assert_eq!(
        h.engine
            .store()
            .markets()
            .iter()
            .position(|m| m.symbol == "BTCUSDT"),
        Some(linear_index),
        "late option catalog moved existing market indexes"
    );
    respond(&mut h, "tickers", "option", Some("baseCoin=ETH"), EMPTY);
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
    assert_eq!(h.engine.store().ticker(OPTION).unwrap().bid, 1000.0);
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().bid, 41234.0);
    assert!(h.socket("public/option").is_some());
}

#[test]
fn failed_option_seed_after_other_seeds_still_publishes_working_products() {
    let mut h = Harness::started(options(false));
    for category in ["linear", "inverse"] {
        respond(
            &mut h,
            "instruments-info",
            category,
            Some("status=PreLaunch"),
            EMPTY,
        );
    }
    respond(&mut h, "instruments-info", "option", None, OPTIONS);
    respond(
        &mut h,
        "instruments-info",
        "inverse",
        None,
        &inverse_instruments(),
    );
    respond(
        &mut h,
        "instruments-info",
        "linear",
        None,
        support::INSTRUMENTS,
    );
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
    respond(&mut h, "tickers", "option", Some("baseCoin=ETH"), EMPTY);
    let failed = h
        .requests
        .iter()
        .position(|r| r.url.contains("tickers") && r.url.contains("baseCoin=BTC"))
        .unwrap();
    let failed = h.requests.remove(failed);
    h.reply(failed.id, 503, "upstream unavailable");
    assert!(
        h.engine.store().loaded().tickers,
        "option seed failure withheld tickers"
    );
    assert!(h.socket("public/linear").is_some());
    // Recovery publishes the option rows and clears the degraded state.
    h.advance(1_000.0);
    respond(&mut h, "tickers", "option", Some("baseCoin=BTC"), QUOTE);
    assert_eq!(h.engine.store().ticker(OPTION).unwrap().bid, 1000.0);
    assert!(
        h.logs
            .iter()
            .any(|m| m.contains("option markets recovered"))
    );
}

#[test]
fn unidentified_rest_neighbor_preserves_exactly_one_fresh_option_greek_and_removes_closed_greeks() {
    for close in [false, true] {
        let mut h = ready(true);
        let row = r#"{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","side":"Buy","size":"0.2","avgPrice":"1000","positionValue":"200","unrealisedPnl":"3","delta":"0.4","gamma":"0.01","vega":"12","theta":"-8"}"#;
        h.message(
            "/private",
            &format!(r#"{{"topic":"position","data":[{row}]}}"#),
        );
        // Complete the startup read that predates the stream; a fresh poll owns the repair.
        for category in ["linear", "inverse", "option"] {
            while h.requests.iter().any(|r| {
                r.url.contains("/v5/position/list")
                    && r.url.contains(&format!("category={category}"))
            }) {
                respond(&mut h, "/v5/position/list", category, None, EMPTY);
            }
        }
        h.advance(5_000.0);
        let updated = if close {
            row.replace("\"size\":\"0.2\"", "\"size\":\"0\"")
        } else {
            row.replace("\"delta\":\"0.4\"", "\"delta\":\"0.7\"")
        };
        respond(
            &mut h,
            "/v5/position/list",
            "option",
            None,
            &format!(r#"{{"retCode":0,"result":{{"list":[{{}},{updated}],"nextPageCursor":""}}}}"#),
        );
        let greeks = h.engine.store().position_greeks().unwrap();
        assert_eq!(greeks.len(), usize::from(!close));
        if !close {
            assert_eq!(greeks[0].greeks.delta, 0.7);
        }
    }
}

#[test]
fn owned_history_category_failure_keeps_sorted_globally_limited_prior_rows() {
    let mut h = ready(true);
    h.notices.clear();
    h.engine.set_log_level(safe_cex_engine::Level::Error);
    h.command(Command::Query {
        command: 811,
        query: Query::Fills {
            symbol: None,
            order_id: None,
            from_ms: None,
            to_ms: None,
            limit: Some(2),
        },
    });
    let row = |symbol: &str, id: &str, time: u64| {
        format!(
            r#"{{"symbol":"{symbol}","execId":"{id}","orderId":"o","side":"Buy","execPrice":"100","execQty":"1","execTime":"{time}"}}"#
        )
    };
    let body = |rows: Vec<String>, cursor: &str| {
        format!(
            r#"{{"retCode":0,"result":{{"list":[{}],"nextPageCursor":"{cursor}"}}}}"#,
            rows.join(",")
        )
    };
    respond(
        &mut h,
        "execution/list",
        "linear",
        None,
        &body(
            vec![
                row("BTCUSDT", "old", 1760000000000),
                row("BTCUSDT", "new", 1760000001000),
            ],
            "unused-limit-satisfied",
        ),
    );
    respond(
        &mut h,
        "execution/list",
        "inverse",
        None,
        &body(vec![row("BTCUSD", "newest", 1760000002000)], "cursor"),
    );
    respond(
        &mut h,
        "execution/list",
        "inverse",
        Some("cursor=cursor"),
        r#"{"retCode":0,"result":{"list":[false]}}"#,
    );
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(h.result(811).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["id"], "newest");
    assert_eq!(rows[1]["id"], "new");
    assert!(
        !h.has_request("execution/list"),
        "stop after failed category, never infer later coverage"
    );
    let markers: Vec<_> = h
        .notices
        .iter()
        .enumerate()
        .filter_map(|(index, notice)| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                assert_eq!(
                    event.fields,
                    [("command", safe_cex_engine::Value::Num(811.0))]
                );
                Some(index)
            }
            _ => None,
        })
        .collect();
    assert_eq!(markers.len(), 1);
    assert!(matches!(
        h.notices.get(markers[0] + 1),
        Some(safe_cex_engine::Notice::CommandDone { command: 811, .. })
    ));
}

#[test]
fn unsupported_order_memory_is_separate_and_persistent_across_product_swaps() {
    let mut h = ready(true);
    let expected = "Bybit order shared-unsupported has an unsupported type and is not shown";
    for round in 0..3 {
        if round > 0 {
            h.advance(15_000.0);
        }
        for (category, symbol) in [("linear", "BTCUSDT"), ("inverse", "BTCUSD")] {
            let before = h.logs.iter().filter(|m| m.as_str() == expected).count();
            let page = format!(
                r#"{{"retCode":0,"result":{{"list":[{{"orderId":"shared-unsupported","symbol":"{symbol}","category":"{category}","orderType":"FutureType","orderStatus":"New","qty":"1"}}],"nextPageCursor":""}}}}"#
            );
            let mut first = true;
            while h.requests.iter().any(|r| {
                r.url.contains("order/realtime") && r.url.contains(&format!("category={category}"))
            }) {
                respond(&mut h, "order/realtime", category, None, &page);
                first = false;
            }
            assert!(
                !first,
                "must exercise actual {category} account in round {round}; {:?}",
                h.paths()
            );
            assert_eq!(
                h.logs.iter().filter(|m| m.as_str() == expected).count(),
                before + usize::from(round == 0)
            );
        }
    }
    assert_eq!(h.logs.iter().filter(|m| m.as_str() == expected).count(), 2);
}

// Slice 4a original REDs: option commands run on the ordinary Plan path.

const EXISTING_OPTION: &str = r#"{"topic":"order","data":[{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","orderId":"existing-option","orderType":"Limit","orderStatus":"New","qty":"0.2","price":"1000","side":"Buy"}]}"#;

fn failed(h: &Harness, command: u32) -> safe_cex_engine::VenueError {
    h.notices
        .iter()
        .find_map(|notice| match notice {
            safe_cex_engine::Notice::CommandFailed { command: id, error } if *id == command => {
                Some(error.clone())
            }
            _ => None,
        })
        .expect("one typed failure")
}

/// O1: an option create is one admitted Plan call with its native window.
#[test]
fn option_orders_run_on_the_ordinary_plan_with_a_native_window() {
    let mut h = ready(true);
    let mut opts = order(OPTION, 0.12);
    opts.implied_volatility = Some(0.6);
    opts.client_order_id = Some("option-plan".into());
    h.command(Command::PlaceOrders {
        command: 81,
        orders: vec![opts],
    });
    let create = h.take("/v5/order/create");
    assert_eq!(create.timeout_ms, Some(20_000), "a managed Plan call");
    assert!(create.body.unwrap().contains("\"category\":\"option\""));
}

/// O2 (D4): independent option creates keep the accepted one's ID when a
/// later one is refused, as ordinary batches do.
#[test]
fn a_partially_refused_option_batch_fails_known_partial() {
    let mut h = ready(true);
    let mut put = order("ETH-25DEC26-3000-P-USDT", 0.1);
    put.client_order_id = Some("option-put".into());
    let mut call = order(OPTION, 0.12);
    call.client_order_id = Some("option-call".into());
    h.command(Command::PlaceOrders {
        command: 82,
        orders: vec![call, put],
    });
    let first = h.take("/v5/order/create");
    h.reply(
        first.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"option-accepted","orderLinkId":"option-call"},"retExtInfo":{},"time":1}"#,
    );
    let second = h.take("/v5/order/create");
    h.reply(
        second.id,
        200,
        r#"{"retCode":110007,"retMsg":"Insufficient available balance","result":{},"retExtInfo":{},"time":1}"#,
    );
    let error = failed(&h, 82);
    assert_eq!(
        error.partial.as_ref().map(|p| p.order_ids.clone()),
        Some(vec!["option-accepted".to_owned()])
    );
    assert_ne!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

/// O3: an option amend acknowledgement must name the order it amended.
#[test]
fn an_option_amend_acknowledging_another_order_is_unknown() {
    let mut h = ready(true);
    h.message("/private", EXISTING_OPTION);
    h.command(Command::AmendOrder {
        command: 83,
        id: "existing-option".into(),
        price: Some(1005.0),
        amount: None,
    });
    let amend = h.take("/v5/order/amend");
    assert!(amend.body.unwrap().contains("\"category\":\"option\""));
    h.reply(
        amend.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"orderId":"some-other-order","orderLinkId":""},"retExtInfo":{},"time":1}"#,
    );
    assert_eq!(
        failed(&h, 83).outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
}

/// O4 (D2): an absent option cancels nothing locally; one fresh read decides.
#[test]
fn an_absent_option_cancel_fences_and_reads_instead_of_cancelling_locally() {
    let mut h = ready(true);
    h.message("/private", EXISTING_OPTION);
    h.command(Command::CancelOrders {
        command: 84,
        ids: vec!["existing-option".into()],
    });
    let cancel = h.take("/v5/order/cancel-batch");
    h.reply(
        cancel.id,
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{"list":[{"category":"option","symbol":"BTC-25DEC26-100000-C-USDT","orderId":"","orderLinkId":""}]},"retExtInfo":{"list":[{"code":110001,"msg":"order not exists or too late to cancel"}]},"time":1}"#,
    );
    assert_eq!(
        h.engine.store().order("existing-option").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open,
        "no local Cancelled transition"
    );
    assert!(h.has_request("/v5/order/realtime"), "one reconcile read");
}

/// O5: an option has no leverage setting, and the refusal says so.
#[test]
fn option_leverage_is_refused_as_an_option_setting() {
    let mut h = ready(true);
    h.command(Command::SetLeverage {
        command: 85,
        symbol: OPTION.into(),
        leverage: 2.0,
        side: None,
    });
    let error = failed(&h, 85);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(error.message.contains("option"), "{}", error.message);
    assert!(!error.message.contains("spot"), "{}", error.message);
}

/// A disabled option product refuses a trade command as the account's
/// configuration, as it does for reads.
#[test]
fn a_disabled_option_product_refuses_trading_as_invalid_input() {
    let mut h = Harness::started(Options {
        variant: Variant::Linear,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.command(Command::SetLeverage {
        command: 86,
        symbol: OPTION.into(),
        leverage: 2.0,
        side: None,
    });
    let error = failed(&h, 86);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert!(h.requests.iter().all(|r| !r.url.contains("/v5/position")));
}

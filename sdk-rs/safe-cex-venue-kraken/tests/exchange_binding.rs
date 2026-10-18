//! E2E: the production KrakenExchange through the Exchange trait over a scripted transport.
#[path = "support/binding_harness.rs"]
mod support;
use safe_cex_types::{
    ExchangeName, Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts, Position,
    UpdateOrderOpts,
};
use serde_json::{Value, json};
use support::{Harness, harness, options};

const SYMBOL: &str = "PF_XBTUSD";

fn instruments() -> Value {
    json!({"result": "success", "instruments": [{
        "symbol": SYMBOL, "tradeable": true, "base": "BTC", "quote": "USD",
        "contractValueTradePrecision": 4, "tickSize": 0.5, "contractSize": 1,
        "marginLevels": [{"initialMargin": 0.02}], "contractType": "flexible_futures",
        "status": "open"
    }]})
}

fn tickers() -> Value {
    json!({"result": "success", "tickers": [{
        "symbol": SYMBOL, "bid": 30000, "ask": 30001, "last": 30000.5,
        "markPrice": 30000.25, "indexPrice": 30000.1, "vol24h": 12, "openInterest": 7,
        "fundingRate": 0.0001, "change24h": 1.5
    }]})
}

fn accounts() -> Value {
    json!({"result": "success", "accounts": {"flex": {
        "type": "multiCollateralMarginAccount", "portfolioValue": 1000, "availableMargin": 800,
        "totalUnrealized": 5, "currencies": {"USD": {"quantity": 1000, "value": 1000, "available": 800}}
    }}})
}

fn open_orders() -> Value {
    json!({"result": "success", "openOrders": [{
        "order_id": "o-1", "symbol": SYMBOL, "side": "buy", "orderType": "lmt",
        "limitPrice": 29000, "unfilledSize": 2, "filledSize": 0, "reduceOnly": false,
        "lastUpdateTime": "2023-11-14T22:13:20.000Z"
    }]})
}

fn open_positions() -> Value {
    json!({"result": "success", "openPositions": [{
        "symbol": SYMBOL, "side": "long", "size": 2, "price": 30000, "unrealizedPnl": 5
    }]})
}

fn futures(script: impl FnOnce(&support::Http)) -> Harness {
    harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", "/v3/instruments", instruments());
        http.ok("GET", "/v3/tickers", tickers());
        script(http);
    })
}

/// Markets and tickers loaded into the Store, as start() leaves them.
fn loaded(h: &Harness) {
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .expect("store update");
}

fn order(opts: Value) -> PlaceOrderOpts {
    serde_json::from_value(opts).expect("place order opts")
}

fn data(request: &support::Request) -> String {
    request.data.as_str().unwrap_or_default().to_owned()
}

#[test]
fn fetch_markets_maps_perpetual_instruments() {
    let h = futures(|_| {});
    let markets = h.drive(h.venue().fetch_markets()).expect("fetchMarkets");
    assert_eq!(markets.len(), 1);
    let m = &markets[0];
    assert_eq!(m.symbol, SYMBOL);
    assert_eq!(m.id, "BTC/USD");
    assert_eq!(m.limits.leverage.max, 50.0, "1 / initialMargin");
    assert_eq!(h.http.calls("/v3/instruments").len(), 1);
}

#[test]
fn fetch_tickers_reads_the_rest_snapshot_for_known_markets() {
    let h = futures(|_| {});
    loaded(&h);
    let tickers = h.drive(h.venue().fetch_tickers()).expect("fetchTickers");
    assert_eq!(tickers.len(), 1);
    assert_eq!(tickers[0].symbol, SYMBOL);
    assert_eq!(tickers[0].mark, 30000.25);
    assert_eq!(tickers[0].index, 30000.1);
}

#[test]
fn futures_candles_refuse_only_nan_timestamp_rows() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reads-cardinality-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let case = &source["futuresCandleBoundaries"][0];
    assert_eq!(case["label"], "mixed-NaN-timestamps");
    assert_eq!(case["result"].as_array().unwrap().len(), 4);
    let original: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
    let mut multiple = original.clone();
    multiple["candles"]
        .as_array_mut()
        .unwrap()
        .push(json!({"time":"bad","open":"99"}));
    for (body, expected_rows, expected_reports) in [
        (original, 3, 1),
        (multiple, 3, 2),
        (json!({"candles":[{"time":"NaN"},{"time":"bad"}]}), 0, 2),
    ] {
        let url = format!("https://futures.kraken.com/api/charts/v1/trade/{SYMBOL}/1m");
        let h = futures(|http| http.ok("GET", &url, body));
        let candles = h
            .drive(
                h.venue().fetch_ohlcv(
                    serde_json::from_value(json!({
                        "symbol":SYMBOL,"interval":"1m","limit":4,"to":1700000000000.0
                    }))
                    .unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(
            candles.len(),
            expected_rows,
            "valid candles survive bad timestamp rows"
        );
        for (index, candle) in candles.iter().enumerate() {
            let expected = &case["result"][index];
            for (field, actual) in [
                ("timestamp", candle.timestamp),
                ("open", candle.open),
                ("high", candle.high),
                ("low", candle.low),
                ("close", candle.close),
                ("volume", candle.volume),
            ] {
                assert_eq!(
                    format!("{:016x}", actual.to_bits()),
                    expected[field]["number"].as_str().unwrap(),
                    "native {field}"
                );
            }
        }
        let console = h.runtime.console.borrow();
        let refusals = console
            .iter()
            .filter(|(level, message)| level == "error" && message.contains("timestamp=NaN"))
            .collect::<Vec<_>>();
        assert_eq!(
            refusals.len(),
            expected_reports,
            "each refused timestamp is visible"
        );
        assert_eq!(h.http.calls(&url).len(), 1);
    }
    let url = format!("https://futures.kraken.com/api/charts/v1/trade/{SYMBOL}/1m");
    let h = futures(|http| {
        http.ok("GET", &url, json!({"candles":[{
        "time":1000,"open":"NaN","high":"Infinity","low":"-0","close":"-Infinity","volume":"5"
    }]}))
    });
    let candles = h
        .drive(
            h.venue().fetch_ohlcv(
                serde_json::from_value(json!({
                    "symbol":SYMBOL,"interval":"1m","to":1700000000000.0
                }))
                .unwrap(),
            ),
        )
        .unwrap();
    assert_eq!(candles.len(), 1, "only timestamp controls row admission");
    assert!(candles[0].open.is_nan());
    assert_eq!(candles[0].high.to_bits(), f64::INFINITY.to_bits());
    assert_eq!(candles[0].low.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(candles[0].close.to_bits(), f64::NEG_INFINITY.to_bits());
    assert!(h.runtime.console.borrow().is_empty());
}

#[test]
fn private_reads_sign_and_map_orders_positions_and_balance() {
    let h = futures(|http| {
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
    });
    loaded(&h);
    let orders = h.drive(h.venue().fetch_orders(None)).expect("fetchOrders");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0].id, "o-1");
    assert_eq!(orders[0].side, OrderSide::Buy);
    assert_eq!(orders[0].price, 29000.0);
    let signed = &h.http.calls("/v3/openorders")[0];
    assert_eq!(signed.headers["APIKey"], "fixture-key");
    assert!(
        signed.headers["Authent"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(signed.headers["Nonce"].as_str().is_some());
    assert_eq!(signed.method, "GET");
    assert_eq!(
        signed.base_url,
        "https://futures.kraken.com/derivatives/api"
    );
    assert!(
        signed.params.is_null(),
        "a signed GET carries no axios params"
    );

    let positions = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions");
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].symbol, SYMBOL);
    assert_eq!(positions[0].contracts, 2.0);
    assert_eq!(positions[0].entry_price, 30000.0);
}

#[test]
fn validate_account_returns_no_error_and_a_balance() {
    let h = futures(|http| http.ok("GET", "/v3/accounts", accounts()));
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert_eq!(result.error.as_value(), &json!(""));
    assert!(result.balance.is_some(), "best-effort balance present");
}

#[test]
fn validate_account_maps_an_auth_error_envelope() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/accounts",
            json!({"result": "error", "error": "apiKey invalid"}),
        )
    });
    let result = h
        .drive(h.venue().validate_account())
        .expect("validateAccount");
    assert!(
        result
            .error
            .as_value()
            .as_str()
            .unwrap_or("")
            .contains("Authentication failed"),
        "{:?}",
        result.error
    );
}

#[test]
fn place_order_posts_a_signed_form_and_returns_the_venue_id() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "new-1"}}),
        )
    });
    loaded(&h);
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000})),
        )))
        .expect("placeOrder");
    assert_eq!(ids, vec!["new-1"]);
    let sent = &h.http.calls("/v3/sendorder")[0];
    let body = data(sent);
    assert!(body.contains("orderType=lmt"), "{body}");
    assert!(body.contains("limitPrice=30000"), "{body}");
    assert!(body.contains("symbol=PF_XBTUSD"), "{body}");
    assert_eq!(
        sent.headers["Content-Type"],
        "application/x-www-form-urlencoded"
    );
}

#[test]
fn place_order_refuses_a_caller_client_order_id_before_sending() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "x"}}),
        )
    });
    loaded(&h);
    let result = h.drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
        h.venue(),
        order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000, "clientOrderId": "mine"})),
    )));
    assert!(result.is_err(), "Kraken has no client-order-ID rule");
    assert!(h.http.calls("/v3/sendorder").is_empty(), "nothing sent");
}

fn live_order() -> Order {
    serde_json::from_value(json!({
        "id": "o-1", "status": OrderStatus::Open, "symbol": SYMBOL, "type": OrderType::Limit,
        "side": "buy", "price": 29000, "amount": 2, "filled": 0, "remaining": 2, "reduceOnly": false, "lastUpdate": 1
    }))
    .expect("order")
}

#[test]
fn update_order_edits_a_limit_order() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/editorder",
            json!({"result": "success", "editStatus": {"status": "edited", "orderId": "o-1"}}),
        )
    });
    loaded(&h);
    let update: UpdateOrderOpts = serde_json::from_value(json!({
        "order": serde_json::to_value(live_order()).unwrap(), "update": {"price": 29500}
    }))
    .expect("update opts");
    let ids = h
        .drive(h.venue().update_order(update))
        .expect("updateOrder");
    assert_eq!(ids, vec!["o-1"]);
    let body = data(&h.http.calls("/v3/editorder")[0]);
    assert!(body.contains("limitPrice=29500"), "{body}");
    assert!(body.contains("orderId=o-1"), "{body}");
}

#[test]
fn cancel_orders_posts_one_cancel_for_a_single_order() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/cancelorder",
            json!({"result": "success", "cancelStatus": {"status": "cancelled"}}),
        )
    });
    h.drive(h.venue().cancel_orders(vec![live_order()]))
        .expect("cancelOrders");
    let body = data(&h.http.calls("/v3/cancelorder")[0]);
    assert!(body.contains("order_id=o-1"), "{body}");
}

#[test]
fn cancel_orders_surfaces_a_rejected_cancel() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/cancelorder",
            json!({"result": "success", "cancelStatus": {"status": "notFound"}}),
        )
    });
    let error = h
        .drive(h.venue().cancel_orders(vec![live_order()]))
        .expect_err("rejected cancel");
    assert!(error.message.contains("notFound"), "{}", error.message);
}

#[test]
fn set_leverage_puts_a_clamped_preference() {
    let h = futures(|http| {
        http.ok(
            "PUT",
            "/v3/leveragepreferences",
            json!({"result": "success"}),
        )
    });
    loaded(&h);
    h.drive(h.venue().set_leverage(SYMBOL.into(), 500.0, None))
        .expect("setLeverage");
    let body = data(&h.http.calls("/v3/leveragepreferences")[0]);
    assert!(
        body.contains("maxLeverage=50"),
        "clamped to market max: {body}"
    );
}

#[test]
fn change_margin_type_merges_refreshed_positions_into_the_store() {
    let h = futures(|http| {
        http.ok(
            "PUT",
            "/v3/leveragepreferences",
            json!({"result": "success"}),
        );
        http.ok("GET", "/v3/openpositions", open_positions());
    });
    loaded(&h);
    let mut other: Position = h
        .drive(h.venue().fetch_positions(None))
        .expect("fetchPositions")
        .remove(0);
    other.symbol = "PF_ETHUSD".into();
    other.id = "PF_ETHUSD-long".into();
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            positions: Some(vec![other]),
            ..Default::default()
        },
        false,
    )
    .expect("seed positions");
    h.drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
        .expect("changeMarginType");
    let positions = safe_cex_store::Store::positions(h.store.as_ref());
    let symbols: Vec<&str> = positions.iter().map(|p| p.symbol.as_str()).collect();
    assert!(
        symbols.contains(&"PF_ETHUSD") && symbols.contains(&SYMBOL),
        "refreshed positions must merge, not replace: {symbols:?}"
    );
}

#[test]
fn change_margin_type_to_isolated_sets_a_leverage_preference() {
    let h = futures(|http| {
        http.ok(
            "PUT",
            "/v3/leveragepreferences",
            json!({"result": "success"}),
        );
        http.ok(
            "GET",
            "/v3/openpositions",
            json!({"result": "success", "openPositions": []}),
        );
    });
    loaded(&h);
    h.drive(h.venue().change_margin_type(true, Some(SYMBOL.into())))
        .expect("changeMarginType");
    let body = data(&h.http.calls("/v3/leveragepreferences")[0]);
    assert!(body.contains("maxLeverage=50"), "{body}");
}

#[test]
fn start_loads_markets_tickers_connects_sockets_and_ticks() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result": "success", "leveragePreferences": []}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    h.drive(h.venue().start()).expect("start");
    for _ in 0..10 {
        h.runtime.advance(100.0);
    }
    assert_eq!(h.store.markets().len(), 1);
    assert_eq!(h.store.tickers().len(), 1);
    let loaded = h.store.loaded().clone();
    assert!(loaded.markets && loaded.tickers);
    assert!(
        loaded.orders && loaded.positions && loaded.balance,
        "{loaded:?}"
    );
    assert_eq!(h.store.orders().len(), 1);
    let urls: Vec<String> = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .map(|(u, _)| u.clone())
        .collect();
    assert!(
        urls.iter().any(|u| u == "wss://futures.kraken.com/ws/v1"),
        "{urls:?}"
    );
    let public = h.runtime.sockets.borrow()[0].1.clone();
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"feed\":\"ticker\"")),
        "ticker subscription sent on open: {:?}",
        public.sent.borrow()
    );
}

#[test]
fn private_socket_sends_the_challenge_on_open_and_subscribes_after_auth() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result": "success", "leveragePreferences": []}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(100.0);
    let private = h.runtime.sockets.borrow()[1].1.clone();
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"event\":\"challenge\"")),
        "challenge requested: {:?}",
        private.sent.borrow()
    );
    assert!(
        !private
            .sent
            .borrow()
            .iter()
            .any(|m| m.contains("\"event\":\"subscribe\"")),
        "no subscription before the challenge is signed"
    );
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                json!({"event": "challenge", "message": "challenge-text"}).to_string(),
            ),
        ));
    for _ in 0..5 {
        h.runtime.advance(100.0);
    }
    let sent = private.sent.borrow();
    let subscriptions: Vec<&String> = sent
        .iter()
        .filter(|m| m.contains("\"event\":\"subscribe\""))
        .collect();
    assert_eq!(subscriptions.len(), 4, "{sent:?}");
    assert!(subscriptions[0].contains("\"signed_challenge\""));
}

#[test]
fn private_auth_error_without_message_logs_js_json_stringify_text() {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result": "success", "leveragePreferences": []}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    let logs = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Value>::new()));
    let sink = logs.clone();
    h.venue().base().on(
        "log",
        std::rc::Rc::new(move |_, value| {
            sink.borrow_mut().push(Value::String(
                String::from_utf16(value.as_str().expect("source log message"))
                    .expect("fixture log UTF16"),
            ));
            Ok(())
        }),
    );
    h.drive(h.venue().start()).expect("start");
    h.runtime.advance(100.0);
    let private = h.runtime.sockets.borrow()[1].1.clone();
    private
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(
                r#"{"event":"error","message":"","code":1.0,"neg":-0.0,"big":1e16}"#.into(),
            ),
        ));
    for _ in 0..5 {
        h.runtime.advance(100.0);
    }
    let expected = r#"WebSocket authentication error: {"event":"error","message":"","code":1,"neg":0,"big":10000000000000000}"#;
    assert!(
        logs.borrow()
            .iter()
            .any(|log| log.as_str().is_some_and(|text| text.ends_with(expected))),
        "auth error text must match JS JSON.stringify: {:?}",
        logs.borrow()
    );
}

#[test]
fn inherited_base_members_reject_not_implemented() {
    let h = futures(|_| {});
    let msg = |e: safe_cex_exchange::runtime::ExchangeError| e.message;
    assert_eq!(
        msg(h.drive(h.venue().get_account()).unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().fetch_symbol_max_leverage(SYMBOL.into()))
            .unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().change_position_mode(true, None))
            .unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h.drive(h.venue().schedule_cancel(None)).unwrap_err()),
        "Not implemented"
    );
    assert_eq!(
        msg(h
            .drive(h.venue().fetch_public_trades(SYMBOL.into(), None))
            .unwrap_err()),
        "Not implemented"
    );
}

#[test]
fn spot_constructs_and_inherits_leverage_and_margin_rejections() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error": [], "result": {"XXBTZUSD": {
                "wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8, "ordermin": "0.0001",
                "status": "online", "altname": "XBTUSD"
            }}}),
        );
    });
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("spot fetchMarkets");
    assert_eq!(markets.len(), 1);
    assert_eq!(markets[0].symbol, "XBT/USD");
    assert_eq!(markets[0].base, "BTC", "XBT normalized");
    assert_eq!(
        h.drive(h.venue().set_leverage("XBT/USD".into(), 2.0, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        h.drive(h.venue().change_margin_type(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
}

#[test]
fn place_order_with_stop_loss_places_the_derived_algo_order_through_place_orders() {
    let h = futures(|http| {
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "main-1"}}),
        );
        http.ok(
            "POST",
            "/v3/sendorder",
            json!({"result": "success", "sendStatus": {"status": "placed", "order_id": "stop-1"}}),
        );
    });
    loaded(&h);
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": SYMBOL, "type": "limit", "side": "buy", "amount": 1, "price": 30000, "stopLoss": 29000})),
        )))
        .expect("placeOrder with stopLoss");
    assert_eq!(ids, vec!["main-1", "stop-1"]);
    let sent = h.http.calls("/v3/sendorder");
    assert_eq!(sent.len(), 2);
    let stop = data(&sent[1]);
    assert!(stop.contains("orderType=stp"), "{stop}");
    assert!(stop.contains("stopPrice=29000"), "{stop}");
    assert!(stop.contains("side=sell"), "{stop}");
}

#[test]
fn spot_place_order_spreads_transaction_ids_by_unicode_code_point() {
    for (txids, expected) in [
        (json!(["T-1", "T-2"]), vec!["T-1", "T-2"]),
        (json!("A💰B"), vec!["A", "💰", "B"]),
    ] {
        let h = harness(ExchangeName::Krakenspot, options(), |http| {
            http.ok(
                "GET",
                "/0/public/AssetPairs",
                json!({"error": [], "result": {"XXBTZUSD": {
                    "wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8,
                    "ordermin": "0.0001", "status": "online", "altname": "XBTUSD"
                }}}),
            );
            http.ok(
                "POST",
                "/0/private/AddOrder",
                json!({"error": [], "result": {"txid": txids}}),
            );
        });
        let markets = h.drive(h.venue().fetch_markets()).expect("spot markets");
        safe_cex_store::Store::update(
            h.store.as_ref(),
            safe_cex_store::StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .expect("store markets");
        let ids = h
            .drive(Box::pin(
                safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
                    h.venue(),
                    order(json!({"symbol": "XBT/USD", "type": "limit", "side": "buy",
                    "amount": 0.5, "price": 30000})),
                ),
            ))
            .expect("spot placement");
        assert_eq!(
            ids, expected,
            "AddOrder txids use JavaScript string iteration"
        );
        let requests = h.http.calls("/0/private/AddOrder");
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].headers["API-Sign"]
                .as_str()
                .is_some_and(|s| !s.is_empty())
        );
    }
}

#[test]
fn spot_place_order_posts_add_order_with_a_module_client_id() {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error": [], "result": {"XXBTZUSD": {
                "wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8, "ordermin": "0.0001",
                "status": "online", "altname": "XBTUSD"
            }}}),
        );
        http.ok(
            "POST",
            "/0/private/AddOrder",
            json!({"error": [], "result": {"txid": ["T-1"]}}),
        );
    });
    let markets = h
        .drive(h.venue().fetch_markets())
        .expect("spot fetchMarkets");
    safe_cex_store::Store::update(
        h.store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .expect("store update");
    let ids = h
        .drive(Box::pin(safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
            h.venue(),
            order(json!({"symbol": "XBT/USD", "type": "limit", "side": "buy", "amount": 0.5, "price": 30000})),
        )))
        .expect("spot placeOrder");
    assert_eq!(ids, vec!["T-1"]);
    let sent = &h.http.calls("/0/private/AddOrder")[0];
    let body = data(sent);
    assert!(body.contains("ordertype=limit"), "{body}");
    assert!(
        body.contains("nonce="),
        "spot signs the nonce into the body: {body}"
    );
    assert!(
        sent.headers["API-Sign"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
}

#[test]
fn spot_signature_rejection_replays_the_signed_form_once() {
    for status in [200.0, 401.0] {
        let h = harness(ExchangeName::Krakenspot, options(), |http| {
            http.route(
                "POST",
                "/0/private/OpenOrders",
                status,
                json!({"error": ["EAPI:Invalid signature"]}),
            );
            http.ok(
                "POST",
                "/0/private/OpenOrders",
                json!({"error": [], "result": {"open": {}}}),
            );
        });
        assert!(
            h.drive(h.venue().fetch_orders(None))
                .expect("signed replay")
                .is_empty()
        );
        let calls = h.http.calls("/0/private/OpenOrders");
        assert_eq!(calls.len(), 2, "status {status}");
        let form = |index: usize| {
            data(&calls[index])
                .split('&')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        let (first, second) = (form(0), form(1));
        let nonces = |parts: &Vec<String>| {
            parts
                .iter()
                .filter(|p| p.starts_with("nonce="))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(nonces(&first).len(), 1);
        assert_eq!(
            nonces(&second).len(),
            1,
            "old nonce removed before re-signing"
        );
        assert_ne!(nonces(&first), nonces(&second));
        assert_ne!(calls[0].headers["API-Sign"], calls[1].headers["API-Sign"]);
        assert_eq!(
            first
                .iter()
                .filter(|p| !p.starts_with("nonce="))
                .collect::<Vec<_>>(),
            second
                .iter()
                .filter(|p| !p.starts_with("nonce="))
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn spot_repeated_signature_rejection_stops_after_one_resend() {
    for status in [200.0, 401.0] {
        let h = harness(ExchangeName::Krakenspot, options(), |http| {
            http.route(
                "POST",
                "/0/private/OpenOrders",
                status,
                json!({"error": ["EAPI:Invalid signature"]}),
            );
        });
        // The source fetcher logs and returns []; observe transport calls too.
        let _ = h.drive(h.venue().fetch_orders(None));
        assert_eq!(
            h.http.calls("/0/private/OpenOrders").len(),
            2,
            "status {status}"
        );
    }
}

#[test]
fn spot_order_resend_retains_the_unsigned_order_fields() {
    for status in [200.0, 401.0] {
        let h = harness(ExchangeName::Krakenspot, options(), |http| {
            http.ok(
                "GET",
                "/0/public/AssetPairs",
                json!({"error": [], "result": {
                    "XXBTZUSD": {"wsname": "XBT/USD", "pair_decimals": 1, "lot_decimals": 8,
                        "ordermin": "0.0001", "status": "online", "altname": "XBTUSD"}
                }}),
            );
            http.route(
                "POST",
                "/0/private/AddOrder",
                status,
                json!({"error": ["EAPI:Invalid signature"]}),
            );
            http.ok(
                "POST",
                "/0/private/AddOrder",
                json!({"error": [], "result": {"txid": ["T-replayed"]}}),
            );
        });
        let markets = h.drive(h.venue().fetch_markets()).unwrap();
        safe_cex_store::Store::update(
            h.store.as_ref(),
            safe_cex_store::StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let ids = h
            .drive(Box::pin(
                safe_cex_exchange::exchange::Exchange::base(h.exchange.as_ref()).place_order(
                    h.venue(),
                    order(json!({"symbol":"XBT/USD", "type":"limit", "side":"buy",
                "amount":0.501, "price":30000})),
                ),
            ))
            .expect("write signature rejection is safe to resend");
        assert_eq!(ids, vec!["T-replayed"]);
        let calls = h.http.calls("/0/private/AddOrder");
        assert_eq!(calls.len(), 2);
        let unsigned = |request: &support::Request| {
            data(request)
                .split('&')
                .filter(|p| !p.starts_with("nonce="))
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(unsigned(&calls[0]), unsigned(&calls[1]), "status {status}");
        assert!(data(&calls[1]).contains("volume=0.501"));
        assert!(data(&calls[1]).contains("price=30000"));
        assert_eq!(
            data(&calls[1])
                .split('&')
                .filter(|p| p.starts_with("nonce="))
                .count(),
            1
        );
    }
}

#[test]
fn source_identity_flags_survive_spot_facade_construction() {
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let h = harness(name.clone(), options(), |_| {});
        assert_eq!(
            h.venue().base().is_spot,
            name == ExchangeName::Krakenspot,
            "Kraken source isSpot flag reaches typed base"
        );
        assert!(!h.venue().base().is_demo);
        h.venue().dispose().unwrap();
    }
}

fn private_frame_fixture() -> Harness {
    let h = futures(|http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            json!({"result":"success","leveragePreferences":[]}),
        );
        http.ok("GET", "/v3/openorders", open_orders());
        http.ok("GET", "/v3/openpositions", open_positions());
        http.ok("GET", "/v3/accounts", accounts());
    });
    h.drive(h.venue().start())
        .expect("production constructor and startup");
    h.runtime.pump();
    h
}
fn private_frame(h: &Harness, value: Value) {
    let socket = h.runtime.sockets.borrow()[1].1.clone();
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
    h.runtime.pump();
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
}
#[test]
fn constructed_private_orders_frame_updates_store() {
    let h = private_frame_fixture();
    private_frame(
        &h,
        json!({"feed":"open_orders_snapshot","orders":[{"order_id":"frame-order","instrument":SYMBOL,"type":"lmt","direction":0,"qty":3,"filled":0.5,"limit_price":29100}]}),
    );
    let rows = h.store.orders();
    assert_eq!(rows.len(), 1);
    let order = &rows[0];
    assert_eq!(
        (order.id.as_str(), order.symbol.as_str()),
        ("frame-order", SYMBOL)
    );
    assert_eq!(
        (
            order.price.to_number(),
            order.amount,
            order.filled,
            order.remaining
        ),
        (29100.0, 3.0, 0.5, 2.5)
    );
    assert_eq!(order.r#type, OrderType::Limit);
}
#[test]
fn constructed_private_fill_frame_updates_order_and_emits_fill() {
    let h = private_frame_fixture();
    let fills = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let saved = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, data| {
            saved.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    private_frame(
        &h,
        json!({"feed":"fills","fills":[{"order_id":"o-1","instrument":SYMBOL,"qty":0.25,"price":30002,"buy":true}]}),
    );
    let rows = h.store.orders();
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].filled, rows[0].remaining), (0.25, 1.75));
    let fills = fills.borrow();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert!(
        fill.get("id")
            .unwrap()
            .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str("o-1"))
    );
    assert!(
        fill.get("symbol")
            .unwrap()
            .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(SYMBOL))
    );
    assert!(
        fill.get("side")
            .unwrap()
            .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str("buy"))
    );
    assert_eq!(fill.get("amount").unwrap().as_f64(), Some(0.25));
    assert_eq!(fill.get("price").unwrap().as_f64(), Some(30002.0));
}
#[test]
fn constructed_private_position_frame_updates_store() {
    let h = private_frame_fixture();
    private_frame(
        &h,
        json!({"feed":"open_positions","positions":[{"instrument":SYMBOL,"balance":3,"entry_price":29000,"mark_price":30000,"pnl":7,"liquidation_threshold":25000}]}),
    );
    let rows = h.store.positions();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].symbol, SYMBOL);
    assert_eq!(
        (
            rows[0].contracts,
            rows[0].entry_price,
            rows[0].notional,
            rows[0].unrealized_pnl
        ),
        (3.0, 29000.0, 90000.0, 7.0)
    );
}
#[test]
fn constructed_private_balance_frame_updates_store() {
    let h = private_frame_fixture();
    private_frame(
        &h,
        json!({"feed":"account_balances","accounts":{"flex":{"type":"multiCollateralMarginAccount","portfolioValue":1250,"availableMargin":1100,"initialMargin":100,"maintenanceMargin":50,"pnl":9}}}),
    );
    let b = h.store.balance();
    assert_eq!(
        (b.total, b.free, b.used, b.upnl),
        (1250.0, 1100.0, 150.0, 9.0)
    );
}

fn spot_private_fixture() -> Harness {
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error":[],"result":{"XXBTZUSD":{
                "wsname":"XBT/USD","pair_decimals":1,"lot_decimals":8,
                "ordermin":"0.0001","status":"online","altname":"XBTUSD"
            }}}),
        );
        http.ok("GET", "/0/public/Ticker", json!({"error":[],"result":{}}));
        http.ok(
            "POST",
            "/0/private/OpenOrders",
            json!({"error":[],"result":{"open":{}}}),
        );
        http.ok(
            "POST",
            "/0/private/Balance",
            json!({"error":[],"result":{"ZUSD":"100"}}),
        );
        http.ok(
            "POST",
            "/0/private/GetWebSocketsToken",
            json!({"error":[],"result":{"token":"private-token"}}),
        );
    });
    h.drive(h.venue().start()).expect("Spot production startup");
    h.runtime.pump();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://ws-auth.kraken.com/v2")
        .expect("Spot private WS v2")
        .1
        .clone();
    let subscriptions: Vec<Value> = socket
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert!(
        subscriptions.contains(&json!({"method":"subscribe","params":{
            "channel":"executions","token":"private-token","snap_orders":true,
            "snap_trades":false,"order_status":true
        }}))
    );
    assert!(
        subscriptions.contains(&json!({"method":"subscribe","params":{
            "channel":"balances","token":"private-token","snapshot":true
        }}))
    );
    assert_eq!(h.http.calls("/0/private/GetWebSocketsToken").len(), 1);
    assert_eq!(h.store.balance().total, 100.0);
    assert_eq!(h.store.positions().len(), 2);
    h
}

fn spot_private_order_frame() -> Value {
    json!({"channel":"executions","type":"snapshot","data":[{
        "order_id":"spot-frame-order","symbol":"XBT/USD","side":"buy",
        "order_type":"limit","limit_price":"29100","order_qty":"3",
        "cum_qty":"0.5","exec_type":"new","timestamp":"2023-11-14T22:13:20Z"
    }]})
}

#[test]
fn constructed_spot_private_orders_frame_updates_real_store() {
    let h = spot_private_fixture();
    private_frame(&h, spot_private_order_frame());
    let orders = h.store.orders().to_vec();
    assert_eq!(orders.len(), 1);
    let order = &orders[0];
    assert_eq!(
        (order.id.as_str(), order.symbol.as_str()),
        ("spot-frame-order", "XBT/USD")
    );
    assert_eq!(order.status, OrderStatus::Open);
    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.r#type, OrderType::Limit);
    assert_eq!(
        (
            order.price.to_number(),
            order.amount,
            order.filled,
            order.remaining
        ),
        (29100.0, 3.0, 0.5, 2.5)
    );
    private_frame(
        &h,
        json!({"channel":"executions","type":"update","data":[{
            "order_id":"spot-frame-order","exec_type":"canceled"
        }]}),
    );
    assert_eq!(h.store.orders()[0].status, OrderStatus::Canceled);
}

#[test]
fn constructed_spot_private_fill_updates_store_and_emits_original_payload() {
    let h = spot_private_fixture();
    let fills = std::rc::Rc::new(std::cell::RefCell::new(vec![]));
    let saved = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, data| {
            saved.borrow_mut().push(data.clone());
            Ok(())
        }),
    );
    private_frame(&h, spot_private_order_frame());
    assert!(fills.borrow().is_empty(), "snapshot has no trade emission");
    private_frame(
        &h,
        json!({"channel":"executions","type":"update","data":[{
            "order_id":"spot-frame-order","symbol":"XBT/USD","exec_type":"trade",
            "side":"buy","order_qty":"3","cum_qty":"0.75","last_qty":"0.25",
            "avg_price":"30002"
        }]}),
    );
    assert_eq!(
        (h.store.orders()[0].filled, h.store.orders()[0].remaining),
        (0.75, 2.25)
    );
    assert_eq!(h.store.orders()[0].status, OrderStatus::Open);
    let fills = fills.borrow();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert_eq!(fill.own_keys().len(), 4);
    for (key, expected) in [("side", "buy"), ("symbol", "XBT/USD")] {
        assert!(fill.has_own(key));
        assert!(
            fill.get(key)
                .unwrap()
                .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(expected))
        );
    }
    for (key, expected) in [("price", 30002.0_f64), ("amount", 0.25_f64)] {
        assert!(fill.has_own(key));
        assert_eq!(
            fill.get(key).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn constructed_spot_private_balance_frame_refreshes_authoritative_rest_store() {
    let h = spot_private_fixture();
    let before = h.http.calls("/0/private/Balance").len();
    h.http
        .routes
        .borrow_mut()
        .retain(|(_, path, _)| path != "/0/private/Balance");
    h.http.ok(
        "POST",
        "/0/private/Balance",
        json!({"error":[],"result":{"ZUSD":"125","USDT":"7"}}),
    );
    private_frame(
        &h,
        json!({"channel":"balances","type":"update","data":[{
            "asset":"USD","balance":"999999"
        }]}),
    );
    h.runtime.advance(1000.0);
    assert_eq!(h.http.calls("/0/private/Balance").len(), before + 1);
    let b = h.store.balance();
    assert_eq!((b.free, b.used, b.total, b.upnl), (132.0, 0.0, 132.0, 0.0));
    let currencies = b.currencies.as_ref().unwrap();
    assert_eq!(currencies["USD"].as_balance().unwrap().total, 125.0);
    assert_eq!(currencies["USDT"].as_balance().unwrap().total, 7.0);
}

#[test]
fn constructed_spot_private_position_topic_preserves_rest_owned_store() {
    let h = spot_private_fixture();
    private_frame(&h, spot_private_order_frame());
    let orders = h.store.orders().to_vec();
    assert_eq!(
        orders.len(),
        1,
        "ignored-topic witness starts with a real WS order"
    );
    let positions = h.store.positions().to_vec();
    let balance = h.store.balance().clone();
    let fills = std::rc::Rc::new(std::cell::Cell::new(0));
    let saved = fills.clone();
    h.venue().base().on(
        "fill",
        std::rc::Rc::new(move |_, _| {
            saved.set(saved.get() + 1);
            Ok(())
        }),
    );
    private_frame(
        &h,
        json!({"channel":"positions","type":"update","data":[{
            "symbol":"XBT/USD","contracts":99,"unrealizedPnl":123
        }]}),
    );
    assert_eq!(&*h.store.orders(), orders.as_slice());
    assert_eq!(&*h.store.positions(), positions.as_slice());
    assert_eq!(&*h.store.balance(), &balance);
    assert_eq!(
        fills.get(),
        0,
        "Spot has no private positions/fill route for this channel"
    );
}

#[test]
fn constructed_public_book_frames_keep_source_string_iterables() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/orderbook-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, family, symbol) in [
        (ExchangeName::Kraken, "Futures", SYMBOL),
        (ExchangeName::Krakenspot, "Spot", "XBT/USD"),
    ] {
        let h = if family == "Futures" {
            futures(|_| {})
        } else {
            harness(name, options(), |http| {
                http.ok(
                    "GET",
                    "/0/public/AssetPairs",
                    json!({"error":[],"result":{
                        "XXBTZUSD":{"wsname":"XBT/USD","pair_decimals":1,"lot_decimals":8,
                        "ordermin":"0.0001","status":"online","altname":"XBTUSD"}
                    }}),
                );
                http.ok("GET", "/0/public/Ticker", json!({"error":[],"result":{}}));
            })
        };
        loaded(&h);
        let books = Rc::new(RefCell::new(Vec::new()));
        let sink = books.clone();
        let cleanup = h
            .venue()
            .listen_order_book(
                symbol.into(),
                Rc::new(move |book| {
                    sink.borrow_mut().push(book);
                }),
            )
            .unwrap();
        h.runtime.pump();
        h.runtime.advance(100.0);
        let socket = h.runtime.sockets.borrow()[0].1.clone();
        assert!(
            socket.sent.borrow().iter().any(|wire| {
                let wire: Value = serde_json::from_str(wire).unwrap();
                if family == "Futures" {
                    wire["feed"] == "book" && wire["product_ids"] == json!([symbol])
                } else {
                    wire["params"]["channel"] == "book"
                        && wire["params"]["symbol"] == json!([symbol])
                }
            }),
            "real {family} subscription"
        );
        let frame = |snapshot: bool, bids: Value, asks: Value| {
            let raw = if family == "Futures" {
                assert!(snapshot);
                json!({"feed":"book_snapshot","product_id":symbol,"bids":bids,"asks":asks})
            } else {
                json!({"channel":"book","type":if snapshot {"snapshot"} else {"update"},
                    "data":[{"symbol":symbol,"bids":bids,"asks":asks,"checksum":123}]})
            };
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(raw.to_string())));
            h.runtime.pump();
        };
        let source_snapshot = source["books"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| {
                row["family"] == family
                    && row["operation"] == "snapshot"
                    && row["config"]["bids"] == "ab"
            })
            .unwrap();
        assert!(source_snapshot["failure"].is_null());
        assert_eq!(source_snapshot["cache"], json!({"bids":[],"asks":[]}));
        frame(true, json!("ab"), json!([]));
        assert_eq!(
            books.borrow().len(),
            1,
            "{family} source emits the string-side snapshot"
        );
        assert!(books.borrow()[0].bids.is_empty() && books.borrow()[0].asks.is_empty());
        frame(
            true,
            json!([{"price":5.5,"qty":7.25},{"price":17.25,"qty":11.5}]),
            json!([{"price":41.25,"qty":43.5},{"price":37.75,"qty":47.125}]),
        );
        assert_eq!(books.borrow().len(), 2);
        assert_eq!(books.borrow()[1].bids.len(), 2);
        assert_eq!(books.borrow()[1].asks.len(), 2);
        if family == "Spot" {
            let expected = source["books"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| {
                    row["family"] == family
                        && row["operation"] == "delta"
                        && row["config"]["bids"] == "ab"
                })
                .unwrap();
            assert!(expected["failure"].is_null());
            frame(false, json!("ab"), json!([]));
            let books = books.borrow();
            assert_eq!(books.len(), 3, "Spot source emits its string-side delta");
            for (side, levels) in [("bids", &books[2].bids), ("asks", &books[2].asks)] {
                let expected = expected["cache"][side].as_array().unwrap();
                assert_eq!(levels.len(), expected.len());
                for expected in expected {
                    let price =
                        u64::from_str_radix(expected[0]["n"].as_str().unwrap(), 16).unwrap();
                    let amount =
                        u64::from_str_radix(expected[1]["n"].as_str().unwrap(), 16).unwrap();
                    let level = levels
                        .iter()
                        .find(|level| {
                            if f64::from_bits(price).is_nan() {
                                level.price.is_nan()
                            } else {
                                level.price.to_bits() == price
                            }
                        })
                        .unwrap();
                    if f64::from_bits(amount).is_nan() {
                        assert!(level.amount.is_nan());
                    } else {
                        assert_eq!(level.amount.to_bits(), amount);
                    }
                    assert_eq!(level.total.to_bits(), 0.0_f64.to_bits());
                }
            }
        }
        cleanup().unwrap();
        h.venue().dispose().unwrap();
    }
}

#[test]
fn constructed_spot_candles_keep_source_date_input_coercion() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/stream-updates-oracle.cjs"
        ))
        .arg("--date-inputs")
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["dates"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    let h = harness(ExchangeName::Krakenspot, options(), |http| {
        http.ok(
            "GET",
            "/0/public/AssetPairs",
            json!({"error":[],"result":{
                "XXBTZUSD":{"wsname":"BTC/USD","pair_decimals":1,"lot_decimals":8,
                "ordermin":"0.0001","status":"online","altname":"XBTUSD"}
            }}),
        );
        http.ok("GET", "/0/public/Ticker", json!({"error":[],"result":{}}));
    });
    loaded(&h);
    let candles = Rc::new(RefCell::new(Vec::new()));
    let sink = candles.clone();
    let cleanup = h
        .venue()
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"BTC/USD","interval":"1m"})).unwrap(),
            Rc::new(move |candle| sink.borrow_mut().push(candle)),
        )
        .unwrap();
    h.runtime.pump();
    h.runtime.advance(100.0);
    let socket = h.runtime.sockets.borrow()[0].1.clone();
    assert!(socket.sent.borrow().iter().any(|wire| {
        let wire: Value = serde_json::from_str(wire).unwrap();
        wire["params"] == json!({"channel":"ohlc","symbol":["BTC/USD"],"interval":1})
    }));
    for (index, case) in cases.iter().enumerate() {
        assert!(case["failure"].is_null(), "{case}");
        let expected = &case["trace"]
            .as_array()
            .unwrap()
            .iter()
            .find(|event| event["kind"] == "candle")
            .unwrap()["fields"];
        let frame = json!({"channel":"ohlc","type":"update","data":[{
            "symbol":"BTC/USD","interval":1,"interval_begin":case["input"],
            "open":"17.25tail","high":19.5,"low":23.75,"close":29.125,"volume":31.5
        }]});
        socket
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
        h.runtime.pump();
        let candles = candles.borrow();
        assert_eq!(candles.len(), index + 1, "source delivers {case}");
        let candle = &candles[index];
        for (name, actual) in [
            ("timestamp", candle.timestamp),
            ("open", candle.open),
            ("high", candle.high),
            ("low", candle.low),
            ("close", candle.close),
            ("volume", candle.volume),
        ] {
            let bits = u64::from_str_radix(expected[name]["number"].as_str().unwrap(), 16).unwrap();
            if f64::from_bits(bits).is_nan() {
                assert!(actual.is_nan(), "{name}: input={}", case["input"]);
            } else {
                assert_eq!(actual.to_bits(), bits, "{name}: input={}", case["input"]);
            }
        }
    }
    cleanup().unwrap();
    h.venue().dispose().unwrap();
}

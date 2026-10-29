//! Production-binding controls pinned to the retained TS Options fixtures.
mod support;
use safe_cex_engine::Venue;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts, PositionSide, ProductKind};
use safe_cex_engine::{Command, Event, Outcome};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options, keyed};

const SYMBOL: &str = "BTC-261225-85000-C";
const INFO: &str = r#"{"optionContracts":[{"underlying":"BTCUSDT","baseAsset":"BTC","settleAsset":"USDT"}],"optionSymbols":[{"symbol":"BTC-261225-85000-C","underlying":"BTCUSDT","quoteAsset":"USDT","status":"TRADING","side":"CALL","strikePrice":"85000.000","expiryDate":1798185600000,"filters":[{"filterType":"PRICE_FILTER","tickSize":"5.000","minPrice":"775"},{"filterType":"LOT_SIZE","stepSize":"0.01","minQty":"0.01","maxQty":"200"}]}]}"#;
const DAILY: &str = r#"[{"symbol":"BTC-261225-85000-C","priceChangePercent":"0.185","lastPrice":"4520","volume":"2.22","amount":"9831.45","bidPrice":"3885","askPrice":"3930","exercisePrice":"81565.34009019"}]"#;
const MARK: &str = r#"[{"symbol":"BTC-261225-85000-C","markPrice":"3845.503"}]"#;
const ACCOUNT: &str = r#"{"asset":[{"asset":"USDT","equity":"10094.44662","available":"8725.92524","initialMargin":"1084.52138","unrealizedPNL":"-5.00138"}]}"#;
const POSITIONS: &str = r#"[{"symbol":"BTC-261225-85000-C","side":"SHORT","quantity":"-0.1","entryPrice":"1000","markValue":"105.00138","unrealizedPNL":"-5.00138"}]"#;
const ORDERS: &str = r#"[{"orderId":4611875134427365000,"symbol":"BTC-261225-85000-C","side":"BUY","status":"PARTIALLY_FILLED","quantity":"0.1","executedQty":"0.02","price":"1000","clientOrderId":"existing","timeInForce":"GTC"}]"#;
fn started(private: bool) -> Harness {
    let options = if private {
        keyed(Variant::Options, "options-key", "options-secret")
    } else {
        Options {
            variant: Variant::Options,
            public_data_only: true,
            ..Options::default()
        }
    };
    let mut h = Harness::started(options);
    assert!(
        h.requests[0]
            .url
            .starts_with("https://eapi.binance.com/eapi/v1/exchangeInfo")
    );
    h.respond("/eapi/v1/exchangeInfo", 200, INFO);
    h.respond("/eapi/v1/ticker", 200, DAILY);
    h.respond("/eapi/v1/mark", 200, MARK);
    h
}
fn limit() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTC-261225-85000-C","type":"limit","side":"buy","amount":0.13,"price":1004,"timeInForce":"PostOnly","clientOrderId":"options-caller"}"#).unwrap()
}
#[test]
fn catalogue_and_tickers_keep_option_terms_and_native_contract_units() {
    let h = started(false);
    let store = h.engine.store();
    let market = &store.markets()[0];
    assert_eq!(market.symbol, SYMBOL);
    assert_eq!(market.precision.amount, 0.01);
    assert_eq!(market.precision.price, 5.);
    assert_eq!(market.contract_size, Some(1.));
    assert_eq!(market.is_expiring, Some(true));
    let product = &store.derivative_products().unwrap()[0];
    assert_eq!(product.kind, ProductKind::Call);
    assert_eq!(
        (product.underlying.as_str(), product.expiry, product.strike),
        ("BTC", Some(1798185600000.), Some(85000.))
    );
    let ticker = &store.tickers()[0];
    assert_eq!(
        (
            ticker.last,
            ticker.mark,
            ticker.percentage,
            ticker.quote_volume
        ),
        (4520., 3845.503, 18.5, 9831.45)
    );
    assert_eq!(ticker.funding_rate, 0.);
    assert!(h.requests.is_empty());
    assert!(h.sockets.is_empty());
}
#[test]
fn private_start_uses_eapi_only_and_maps_signed_positions_and_currency_balances() {
    let mut h = started(true);
    assert!(
        h.requests
            .iter()
            .all(|r| r.url.starts_with("https://eapi.binance.com/eapi/"))
    );
    assert!(
        !h.paths()
            .iter()
            .any(|p| p.contains("leverage") || p.contains("dual") || p.contains("Algo"))
    );
    h.respond("/eapi/v1/marginAccount", 200, ACCOUNT);
    h.respond("/eapi/v1/position", 200, POSITIONS);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    let store = h.engine.store();
    assert_eq!(store.balance().total, 10094.44662);
    assert_eq!(store.balance().free, 8725.92524);
    assert_eq!(store.currency_balances().unwrap()[0].currency, "USDT");
    let pos = &store.positions()[0];
    assert_eq!(pos.side, PositionSide::Short);
    assert_eq!(pos.contracts, 0.1);
    assert_eq!(pos.notional, 105.00138);
    assert_eq!(pos.leverage, 1.);
    let order = &store.orders()[0];
    assert_eq!(order.id, "4611875134427365000");
    assert_eq!(order.amount, 0.1);
    assert_eq!(order.filled, 0.02);
    assert_eq!(order.kind, OrderType::Limit);
}
#[test]
fn limit_post_only_uses_eapi_fields_and_dispatch_time_hmac() {
    let mut h = started(true);
    h.requests.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders: vec![limit()],
    }));
    let request = h.take("/eapi/v1/order?");
    assert!(request.url.contains("symbol=BTC-261225-85000-C&side=BUY&type=LIMIT&quantity=0.13&price=1005&timeInForce=GTC&postOnly=true&clientOrderId=options-caller"), "{}", request.url);
    assert!(request.url.contains("recvWindow=5000"));
    assert!(request.url.contains("signature="));
    assert!(!request.url.contains("positionSide"));
    assert!(!request.url.contains("workingType"));
    assert_eq!(
        request
            .headers
            .iter()
            .find(|(k, _)| *k == "X-MBX-APIKEY")
            .unwrap()
            .1,
        "options-key"
    );
    h.event(Event::HttpResponse {
        request: request.id,
        response: safe_cex_engine::Response {
            status: 200,
            body: r#"{"orderId":4611875134427365001}"#,
            headers: "",
        },
    });
    assert_eq!(
        h.done.last(),
        Some(&(77, Ok("[\"4611875134427365001\"]".into())))
    );
}
#[test]
fn invalid_member_refuses_the_whole_batch_before_any_write() {
    for kind in [
        OrderType::Market,
        OrderType::StopLoss,
        OrderType::TrailingStopLoss,
    ] {
        let mut h = started(true);
        h.requests.clear();
        let mut invalid = limit();
        invalid.kind = kind;
        invalid.client_order_id = None;
        h.event(Event::Command(Command::PlaceOrders {
            command: 78,
            orders: vec![limit(), invalid],
        }));
        assert!(h.requests.is_empty());
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
    }
}
#[test]
fn maximum_is_refused_without_splitting_or_reusing_client_identity() {
    let mut h = started(true);
    h.requests.clear();
    let mut order = limit();
    order.amount = 200.01;
    h.event(Event::Command(Command::PlaceOrders {
        command: 79,
        orders: vec![order],
    }));
    assert!(h.requests.is_empty());
    assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
}
#[test]
fn malformed_orders_never_publish_or_clear_a_clean_snapshot() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    let before = h.engine.store().orders().to_vec();
    let timer = h.timers.iter().find(|(_, at)| *at == 6000.).unwrap().0;
    h.now = 6000.;
    h.event(Event::TimerFired { timer });
    h.respond(
        "/eapi/v1/openOrders",
        200,
        &ORDERS.replace("PARTIALLY_FILLED", "UNRECOGNIZED"),
    );
    assert_eq!(h.engine.store().orders(), before.as_slice());
}
#[test]
fn option_streaming_refuses_without_creating_socket_or_http_work() {
    let mut h = started(false);
    h.event(Event::Command(Command::SubscribeOrderBook {
        symbol: SYMBOL.into(),
    }));
    assert!(h.sockets.is_empty());
    assert!(h.requests.is_empty());
    assert!(h.sent.is_empty());
    assert!(!h.engine.venue().books().any(|(symbol, _)| symbol == SYMBOL));
    assert!(
        h.errors
            .iter()
            .any(|e| e.contains("streaming is not supported"))
    );
}

#[test]
fn option_cancel_uses_native_id_and_quantity_ack_without_resending() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    h.requests.clear();
    h.event(Event::Command(Command::CancelOrders {
        command: 80,
        ids: vec!["4611875134427365000".into()],
    }));
    let request = h.take("/eapi/v1/order?");
    assert!(
        request
            .url
            .contains("symbol=BTC-261225-85000-C&orderId=4611875134427365000&timestamp=")
    );
    h.reply(
        request.id,
        200,
        "",
        &ORDERS
            .trim_start_matches('[')
            .trim_end_matches(']')
            .replace("PARTIALLY_FILLED", "CANCELED"),
    );
    assert!(h.failures.is_empty(), "{:?}", h.failures);
    assert_eq!(
        h.engine.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    assert!(h.requests.is_empty());
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 80).count(), 1);
}
#[test]
fn option_cancel_all_uses_only_the_single_native_family() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    h.requests.clear();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 81,
        symbol: SYMBOL.into(),
    }));
    let request = h.take("/eapi/v1/allOpenOrders?");
    assert!(h.requests.is_empty());
    h.reply(request.id, 200, "", r#"{"code":0,"msg":"success"}"#);
    assert!(h.failures.is_empty(), "{:?}", h.failures);
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 81).count(), 1);
    assert_eq!(
        h.engine.store().orders()[0].status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
}
#[test]
fn fills_preserve_native_large_ids_and_negative_fee_is_a_charge() {
    let mut h = started(true);
    h.requests.clear();
    h.event(Event::Command(Command::Query {
        command: 82,
        query: safe_cex_engine::Query::Fills {
            symbol: Some(SYMBOL.into()),
            order_id: Some("4611875134427365000".into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    }));
    h.respond("/eapi/v1/userTrades",200,r#"[{"id":"4611875134427365000","tradeId":239,"orderId":"4611875134427365000","symbol":"BTC-261225-85000-C","side":"BUY","price":"100","quantity":"1","fee":"-1.04378629","quoteAsset":"USDT","liquidity":"TAKER","time":1592465880683}]"#);
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == 82)
        .unwrap()
        .1
        .as_ref()
        .unwrap();
    assert!(result.contains("\"orderId\":\"4611875134427365000\""));
    assert!(result.contains("\"commission\":1.04378629"));
    assert!(result.contains("\"timestamp\":1592465880.683"));
    assert!(result.contains("\"maker\":false"));
}
#[test]
fn unknown_write_needs_a_fresh_clean_primary_orders_read() {
    let mut h = started(true);
    let stale = h.take("/eapi/v1/openOrders");
    h.requests.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: 90,
        orders: vec![limit()],
    }));
    let post = h.take("/eapi/v1/order?");
    h.reply(post.id, 503, "", "Service Unavailable");
    assert_eq!(
        h.failures
            .iter()
            .find(|(id, _)| *id == 90)
            .unwrap()
            .1
            .outcome,
        Some(Outcome::Unknown)
    );
    h.event(Event::Command(Command::PlaceOrders {
        command: 91,
        orders: vec![limit()],
    }));
    assert!(!h.has_request("/eapi/v1/order?"));
    assert_eq!(h.failures.last().unwrap().1.outcome, Some(Outcome::NotSent));
    h.reply(stale.id, 200, "", "[]");
    let fresh = h.take("/eapi/v1/openOrders");
    h.event(Event::Command(Command::PlaceOrders {
        command: 92,
        orders: vec![limit()],
    }));
    assert!(!h.has_request("/eapi/v1/order?"));
    h.reply(fresh.id, 200, "", "[]");
    h.event(Event::Command(Command::PlaceOrders {
        command: 93,
        orders: vec![limit()],
    }));
    assert!(h.has_request("/eapi/v1/order?"));
}
#[test]
fn incomplete_fresh_orders_does_not_release_an_uncertain_placement() {
    let mut h = started(true);
    let stale = h.take("/eapi/v1/openOrders");
    h.requests.clear();
    h.event(Event::Command(Command::PlaceOrders {
        command: 94,
        orders: vec![limit()],
    }));
    let post = h.take("/eapi/v1/order?");
    h.reply(post.id, 503, "", "Service Unavailable");
    h.reply(stale.id, 200, "", "[]");
    h.respond(
        "/eapi/v1/openOrders",
        200,
        &ORDERS.replace("PARTIALLY_FILLED", "UNKNOWN_STATUS"),
    );
    h.event(Event::Command(Command::PlaceOrders {
        command: 95,
        orders: vec![limit()],
    }));
    assert!(!h.has_request("/eapi/v1/order?"));
    assert_eq!(h.failures.last().unwrap().1.outcome, Some(Outcome::NotSent));
}
#[test]
fn history_rejects_wrong_symbol_and_never_publishes_into_live_store() {
    let mut h = started(true);
    h.requests.clear();
    let before = h.engine.store().orders().to_vec();
    h.event(Event::Command(Command::Query {
        command: 96,
        query: safe_cex_engine::Query::OrderHistory {
            symbol: Some(SYMBOL.into()),
            from_ms: None,
            to_ms: None,
            limit: Some(100),
        },
    }));
    h.respond(
        "/eapi/v1/historyOrders",
        200,
        &ORDERS.replace(SYMBOL, "OTHER-OPTION"),
    );
    assert!(h.done.iter().find(|(id, _)| *id == 96).unwrap().1.is_err());
    assert_eq!(h.engine.store().orders(), before.as_slice());
}

#[test]
fn completed_cancel_cannot_resurrect_on_a_later_stale_replica() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    h.event(Event::Command(Command::CancelOrders {
        command: 97,
        ids: vec!["4611875134427365000".into()],
    }));
    h.respond(
        "/eapi/v1/order?",
        200,
        &ORDERS
            .trim_start_matches('[')
            .trim_end_matches(']')
            .replace("PARTIALLY_FILLED", "CANCELED"),
    );
    let timer = h.timers.iter().find(|(_, at)| *at == 6000.).unwrap().0;
    h.now = 6000.;
    h.event(Event::TimerFired { timer });
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    assert!(
        !h.engine
            .store()
            .orders()
            .iter()
            .any(|o| o.id == "4611875134427365000"
                && o.status == safe_cex_engine::model::OrderStatus::Open)
    );
}

#[test]
fn option_chain_joins_two_fresh_caller_owned_observations() {
    let mut h = started(false);
    h.event(Event::Command(Command::FetchOptionChain {
        command: 101,
        query: safe_cex_engine::model::OptionChainQuery {
            underlying: "BTC".into(),
            expiry: None,
            right: None,
            strike_min: None,
            strike_max: None,
        },
    }));
    h.respond("/eapi/v1/ticker", 200, DAILY);
    assert!(!h.done.iter().any(|(id, _)| *id == 101));
    h.respond("/eapi/v1/mark",200,r#"[{"symbol":"BTC-261225-85000-C","markPrice":"3845.503","markIV":"0.36","bidIV":"0.35","askIV":"-1","delta":"0.44767474","gamma":"0.00003129","vega":"137.16680603","theta":"-37.86983988"}]"#);
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == 101)
        .unwrap()
        .1
        .as_ref()
        .unwrap();
    assert!(result.contains("\"right\":\"call\""));
    assert!(result.contains("\"askIv\":0.0"));
    assert!(result.contains("\"vega\":137.16680603"));
    assert!(result.contains("\"underlyingPrice\":81565.34009019"));
    assert!(h.requests.is_empty());
    assert!(h.sockets.is_empty());
}
#[test]
fn expired_chain_does_not_issue_a_second_observation_or_complete_a_reused_id() {
    let mut h = started(false);
    h.event(Event::Command(Command::FetchOptionChain {
        command: 102,
        query: safe_cex_engine::model::OptionChainQuery {
            underlying: "BTC".into(),
            expiry: None,
            right: None,
            strike_min: None,
            strike_max: None,
        },
    }));
    let first = h.take("/eapi/v1/ticker");
    h.now += 20_001.;
    h.reply(first.id, 200, "", DAILY);
    assert!(!h.has_request("/eapi/v1/mark"));
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 102).count(), 1);
    h.event(Event::Command(Command::FetchOptionExpiries {
        command: 102,
        underlying: "BTC".into(),
    }));
    assert_eq!(h.done.iter().filter(|(id, _)| *id == 102).count(), 2);
}
#[test]
fn account_greeks_use_the_selected_read_identity_and_exact_native_units() {
    let mut options = keyed(Variant::Options, "primary-key", "primary-secret");
    options.read_credentials = Some(safe_cex_engine::ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: None,
    });
    let mut h = Harness::started(options);
    h.respond("/eapi/v1/exchangeInfo", 200, INFO);
    h.event(Event::Command(Command::FetchAccountGreeks { command: 103 }));
    let candidates: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.url.contains("/marginAccount?"))
        .collect();
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().all(|r| {
        r.headers
            .iter()
            .any(|(k, v)| *k == "X-MBX-APIKEY" && v == "read-key")
    }));
    let request = candidates.last().unwrap().id;
    h.reply(request,200,"",r#"{"greek":[{"underlying":"BTCUSDT","delta":"-0.05","gamma":"-0.002","theta":"-0.05","vega":"-0.002"}]}"#);
    let result = h
        .done
        .iter()
        .find(|(id, _)| *id == 103)
        .unwrap()
        .1
        .as_ref()
        .unwrap();
    assert!(result.contains("\"underlying\":\"BTC\""));
    assert!(result.contains("\"delta\":-0.05"));
    assert!(result.contains("scale not measured"));
}
#[test]
fn a_new_orders_census_cannot_decrease_observed_cumulative_fill() {
    let mut h = started(true);
    h.respond("/eapi/v1/openOrders", 200, ORDERS);
    let timer = h.timers.iter().find(|(_, at)| *at == 6000.).unwrap().0;
    h.now = 6000.;
    h.event(Event::TimerFired { timer });
    h.respond("/eapi/v1/openOrders", 200, &ORDERS.replace("0.02", "0.01"));
    assert_eq!(h.engine.store().orders()[0].filled, 0.02);
}

#[test]
fn consumed_nonfinite_poll_recovers_on_a_genuine_finite_callback_without_early_io() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = started(false);
        let timer = h.timers.iter().find(|(_, at)| *at == 6000.).unwrap().0;
        h.now = bad;
        h.event(Event::TimerFired { timer });
        assert!(h.requests.is_empty());
        assert!(h.timers.iter().all(|(_, at)| at.is_finite()));
        h.now = 2000.;
        h.event(Event::Command(Command::SetTickerSymbols {
            symbols: vec![SYMBOL.into()],
        }));
        assert!(h.requests.is_empty());
        assert_eq!(h.timers.iter().filter(|(_, at)| *at == 7000.).count(), 1);
        h.event(Event::Command(Command::SetTickerSymbols {
            symbols: vec![SYMBOL.into()],
        }));
        assert_eq!(h.timers.iter().filter(|(_, at)| *at == 7000.).count(), 1);
    }
}

//! Private variants use documented Spot WS API and COIN-M account payloads.
use support::Options;
mod support;
use safe_cex_engine::model::{Balance, CurrencyBalance, MarginType, OrderStatus};
use safe_cex_engine::{Event, Method};
use safe_cex_venue_binance::Variant;
use support::Harness;

const SPOT_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.00001","minQty":"0.00001","maxQty":"9000"}]}]}"#;
const ACCOUNT: &str = r#"{"balances":[{"asset":"USDT","free":"1000","locked":"20"},{"asset":"BTC","free":"1","locked":"0.5"}]}"#;
const ORDERS: &str = r#"[{"symbol":"BTCUSDT","orderId":77,"clientOrderId":"spot-id","price":"100","origQty":"2","executedQty":"0.5","type":"LIMIT_MAKER","side":"BUY","timeInForce":"GTC"}]"#;

fn spot_start() -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/api/v3/exchangeInfo", 200, SPOT_INFO);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"2","volume":"10","quoteVolume":"1000"}]"#);
    let private = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket: private });
    h
}
fn ack(h: &mut Harness) {
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
    );
}
fn spot() -> Harness {
    let mut h = spot_start();
    h.respond("/api/v3/account", 200, ACCOUNT);
    h.respond("/api/v3/openOrders", 200, ORDERS);
    ack(&mut h);
    h.respond("/api/v3/account", 200, ACCOUNT);
    h.respond("/api/v3/openOrders", 200, ORDERS);
    h.message("ws-api/v3", r#"{"id":"tealstreet-user-data-heartbeat-11","status":200,"result":[{"subscriptionId":0}]}"#);
    h
}
#[test]
fn spot_startup_has_no_derivatives_or_retired_listen_key_requests() {
    let h = spot();
    assert!(h.requests.is_empty());
    assert_eq!(h.engine.store().positions().len(), 0);
    assert!(h.engine.store().loaded().positions);
    assert!(!h.engine.store().hedged());
    assert_eq!(
        (
            h.engine.store().balance().total,
            h.engine.store().balance().free,
            h.engine.store().balance().used
        ),
        (1170.0, 1100.0, 70.0)
    );
    let o = h.engine.store().order("spot-id").unwrap();
    assert_eq!((o.amount, o.filled, o.remaining), (2.0, 0.5, 1.5));
    assert!(h.engine.store().order("77").is_none());
    assert!(h.sent[0].1.contains("userDataStream.subscribe.signature"));
    assert!(
        h.sent
            .iter()
            .any(|(_, s)| s.contains("session.subscriptions"))
    );
    assert!(!h.sent.iter().any(|(_, s)| s.contains("session.status")));
}
#[test]
fn spot_subscription_hmac_is_raw_sorted_parameters_and_ids_are_correlated() {
    let mut h = spot_start();
    #[derive(serde::Deserialize)]
    struct Subscribe {
        id: String,
        params: Params,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Params {
        api_key: String,
        recv_window: u32,
        timestamp: f64,
        signature: String,
    }
    let auth: Subscribe = serde_json::from_str(&h.sent[0].1).unwrap();
    assert_eq!(auth.id, "tealstreet-user-data-10");
    assert_eq!(
        (
            auth.params.api_key.as_str(),
            auth.params.recv_window,
            auth.params.timestamp
        ),
        ("api-key", 5000, 1000.0)
    );
    let expected = safe_cex_crypto::hmac_sha256(
        b"secret",
        b"apiKey=api-key&recvWindow=5000&timestamp=1000",
        safe_cex_crypto::Encoding::Hex,
    );
    assert_eq!(auth.params.signature, expected);
    h.message(
        "ws-api/v3",
        r#"{"id":"unrelated","status":400,"error":{"msg":"ignore"}}"#,
    );
    assert!(h.closed.is_empty());
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":"wrong"}}"#,
    );
    assert_eq!(h.closed, [h.socket("ws-api/v3").unwrap()]);
}
#[test]
fn spot_balance_events_are_partial_and_stale_snapshots_do_not_roll_them_back() {
    let mut h = spot();
    // Live traffic inside each 20 s idle window keeps the stream's epoch current.
    h.now += 15_000.0;
    h.message("ws-api/v3", r#"{"id":"unrelated","status":200}"#);
    h.now += 15_000.0;
    let health = h.timers.iter().find(|(_, at)| *at == 4000.0).unwrap().0;
    h.event(Event::TimerFired { timer: health });
    let read = h.take("/api/v3/account");
    h.message("ws-api/v3", r#"{"subscriptionId":999,"event":{"e":"outboundAccountPosition","B":[{"a":"USDT","f":"9","l":"0"}]}}"#);
    assert_eq!(h.engine.store().balance().free, 1100.0);
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","B":[{"a":"USDT","f":"900","l":"10"}]}}"#);
    assert_eq!(
        (
            h.engine.store().balance().total,
            h.engine.store().balance().free,
            h.engine.store().balance().used
        ),
        (1060.0, 1000.0, 60.0)
    );
    h.reply(read.id, 200, "", ACCOUNT);
    assert_eq!(h.engine.store().balance().free, 1000.0);
    assert_eq!(
        h.engine.store().currency_balances(),
        Some(
            [
                CurrencyBalance {
                    currency: "BTC".into(),
                    balance: Balance {
                        free: 1.0,
                        used: 0.5,
                        total: 1.5,
                        upnl: 0.0
                    },
                },
                CurrencyBalance {
                    currency: "USDT".into(),
                    balance: Balance {
                        free: 900.0,
                        used: 10.0,
                        total: 910.0,
                        upnl: 0.0
                    },
                },
            ]
            .as_slice()
        )
    );
    assert!(h.has_request("/api/v3/account"));
}

#[test]
fn spot_currencies_keep_native_slices_zero_rows_and_authoritative_snapshot_removals() {
    let mut h = spot();
    let expected = |currency: &str, free, used| CurrencyBalance {
        currency: currency.into(),
        balance: Balance {
            free,
            used,
            total: free + used,
            upnl: 0.0,
        },
    };
    assert_eq!(
        h.engine.store().currency_balances(),
        Some([expected("BTC", 1.0, 0.5), expected("USDT", 1000.0, 20.0)].as_slice())
    );
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","B":[{"a":"BTC","f":"0","l":"0"}]}}"#);
    assert_eq!(
        h.engine.store().currency_balances(),
        Some([expected("BTC", 0.0, 0.0), expected("USDT", 1000.0, 20.0)].as_slice())
    );
    assert_eq!(h.engine.store().balance().total, 1020.0);
    h.now += 30_000.0;
    let health = h.timers.iter().find(|(_, at)| *at == 4000.0).unwrap().0;
    h.event(Event::TimerFired { timer: health });
    h.respond(
        "/api/v3/account",
        200,
        r#"{"balances":[{"asset":"USDT","free":"0","locked":"0"}]}"#,
    );
    assert_eq!(
        h.engine.store().currency_balances(),
        Some([expected("USDT", 0.0, 0.0)].as_slice())
    );
    assert_eq!(h.engine.store().balance().total, 0.0);
}
#[test]
fn spot_execution_uses_last_fill_price_and_terminal_status_before_reconciliation() {
    let mut h = spot();
    let _ = h.engine.take_frame(h.now);
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":77,"s":"BTCUSDT","c":"spot-id","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"100","L":"99","q":"2","z":"1.0","l":"0.5","f":"GTC"}}"#);
    let o = h.engine.store().order("spot-id").unwrap();
    assert_eq!(
        (o.filled, o.remaining, o.status),
        (1.0, 1.0, OrderStatus::Open)
    );
    let frame = h.engine.take_frame(h.now).unwrap();
    let mut at = 16;
    let mut found = false;
    for _ in 0..u16::from_le_bytes(frame[6..8].try_into().unwrap()) {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            assert_eq!(
                f64::from_le_bytes(frame[at + 24..at + 32].try_into().unwrap()),
                99.0
            );
            found = true;
        }
        at += 8 + len;
    }
    assert!(found);
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":77,"s":"BTCUSDT","c":"spot-id","S":"BUY","o":"LIMIT","X":"EXPIRED_IN_MATCH","x":"EXPIRED","p":"100","q":"2","z":"1","l":"0"}}"#);
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Canceled
    );
}
#[test]
fn spot_reconnect_ignores_old_auth_and_subscription_and_repairs_after_ack() {
    let mut h = spot();
    let old = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.socket("ws-api/v3").unwrap();
    assert_ne!(socket, old);
    h.event(Event::WsOpened { socket });
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
    );
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","B":[{"a":"USDT","f":"1","l":"0"}]}}"#);
    assert_eq!(h.engine.store().balance().free, 1100.0);
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-12","status":200,"result":{"subscriptionId":5}}"#,
    );
    assert!(h.has_request("/api/v3/account"));
    assert!(h.has_request("/api/v3/openOrders"));
}

const INVERSE_POSITION_RISK: &str = r#"[{"symbol":"BTCUSD_PERP","positionSide":"BOTH","positionAmt":"2","entryPrice":"9000","markPrice":"10000","unRealizedProfit":"0.001","liquidationPrice":"7000","leverage":"20","marginType":"isolated","isolatedMargin":"0.003"}]"#;
fn inverse() -> Harness {
    inverse_with("", INVERSE_POSITION_RISK)
}
fn inverse_with(extra_catalogue_rows: &str, position_risk: &str) -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::CoinM,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/dapi/v1/exchangeInfo",200,&r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"120"}]}__EXTRA__]}"#.replace("__EXTRA__", extra_catalogue_rows));
    h.respond(
        "/dapi/v2/leverageBracket",
        200,
        r#"[{"symbol":"BTCUSD_PERP","brackets":[{"initialLeverage":125}]}]"#,
    );
    h.respond("/dapi/v1/ticker/24hr",200,r#"[{"symbol":"BTCUSD_PERP","lastPrice":"10000","priceChangePercent":"1","volume":"100","baseVolume":"1"}]"#);
    h.respond(
        "/dapi/v1/ticker/bookTicker",
        200,
        r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
    );
    h.respond("/dapi/v1/premiumIndex",200,r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0.0001","nextFundingTime":100000}]"#);
    h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"coin-key"}"#);
    h.respond(
        "/dapi/v1/positionSide/dual",
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond("/dapi/v1/positionRisk", 200, position_risk);
    h.respond("/dapi/v1/account",200,r#"{"assets":[{"asset":"BTC","walletBalance":"1","availableBalance":"0.8","initialMargin":"0.1","unrealizedProfit":"0.01"}]}"#);
    h.respond("/dapi/v1/openOrders",200,r#"[{"symbol":"BTCUSD_PERP","orderId":12,"clientOrderId":"native","price":"9000","origQty":"5","executedQty":"2","type":"LIMIT","side":"BUY"}]"#);
    let private = h.socket("/ws/coin-key").unwrap();
    h.event(Event::WsOpened { socket: private });
    h
}
#[test]
fn inverse_contracts_positions_coin_margin_and_usd_balance_are_distinct() {
    let h = inverse();
    assert!(h.requests.is_empty());
    let p = &h.engine.store().positions()[0];
    assert_eq!(
        (p.contracts, p.notional, p.unrealized_pnl, p.margin_size),
        (0.02, 200.0, 0.001, Some(0.003))
    );
    assert_eq!(p.margin_type, Some(MarginType::Isolated));
    assert_eq!(p.leverage, 20.0);
    let b = h.engine.store().balance();
    assert_eq!(
        (b.total, b.free, b.used, b.upnl),
        (10000.0, 8000.0, 1000.0, 100.0)
    );
    let o = h.engine.store().order("12").unwrap();
    assert_eq!((o.amount, o.filled, o.remaining), (5.0, 2.0, 3.0));
    assert_eq!(o.extra.as_ref().unwrap().remaining_native, Some(3.0));
    assert_eq!(h.engine.store().coin_balances().unwrap()[0].amount, 1.0);
    assert_eq!(
        h.engine.store().currency_balances().unwrap(),
        [CurrencyBalance {
            currency: "BTC".into(),
            balance: Balance {
                total: 1.0,
                free: 0.8,
                used: 0.1,
                upnl: 0.01,
            },
        }]
    );
}
#[test]
fn inverse_balance_info_marks_coin_margined_like_ts() {
    let mut h = inverse();
    h.event(Event::Command(safe_cex_engine::Command::Venue {
        command: 90,
        action: safe_cex_engine::VenueAction::FetchBalanceInfo,
    }));
    assert!(h.requests.is_empty());
    let info: serde_json::Value =
        serde_json::from_str(h.result(90).unwrap().as_ref().unwrap()).unwrap();
    assert_eq!(info, serde_json::json!({ "coinMargined": true }));
}
#[test]
fn inverse_order_failures_and_stale_reads_preserve_stream_state() {
    let mut h = inverse();
    h.message("/ws/coin-key", r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"NEW","p":"9000","q":"5","z":"2"}}"#);
    h.advance(10_000.0);
    let read = h.take("/dapi/v1/openOrders");
    assert_eq!(read.method, Method::Get);
    h.message("/ws/coin-key",r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"9000","q":"5","z":"3","l":"1","L":"9000"}}"#);
    h.reply(read.id,200,"",r#"[{"symbol":"BTCUSD_PERP","orderId":12,"price":"9000","origQty":"5","executedQty":"2","type":"LIMIT","side":"BUY"}]"#);
    assert_eq!(h.engine.store().order("12").unwrap().filled, 3.0);
}

fn fill_prices(frame: &[u8]) -> Vec<f64> {
    let mut at = 16;
    for _ in 0..u16::from_le_bytes(frame[6..8].try_into().unwrap()) {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let body = &frame[at + 8..at + 8 + len];
            let count = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
            let numbers = (4 + 12 * count).next_multiple_of(8);
            return (0..count)
                .map(|i| {
                    f64::from_le_bytes(
                        body[numbers + 16 * i..numbers + 16 * i + 8]
                            .try_into()
                            .unwrap(),
                    )
                })
                .collect();
        }
        at += 8 + len;
    }
    Vec::new()
}

#[test]
fn spot_execution_identity_survives_duplicate_out_of_order_and_client_id_reuse() {
    let mut h = spot();
    let _ = h.engine.take_frame(h.now);
    let event = r#"{"subscriptionId":0,"event":{"e":"executionReport","i":77,"s":"BTCUSDT","c":"spot-id","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"100","L":"99","q":"2","z":"1","l":"0.5"}}"#;
    h.message("ws-api/v3", event);
    h.message("ws-api/v3", event);
    h.message("ws-api/v3", &event.replace(r#""z":"1""#, r#""z":"0.75""#));
    assert_eq!(h.engine.store().order("spot-id").unwrap().filled, 1.0);
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [99.0]);
    let terminal = event
        .replace("PARTIALLY_FILLED", "FILLED")
        .replace(r#""z":"1""#, r#""z":"2""#)
        .replace(r#""l":"0.5""#, r#""l":"1""#);
    h.message("ws-api/v3", &terminal);
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Closed
    );
    h.message(
        "ws-api/v3",
        &event
            .replace(r#""z":"1""#, r#""z":"2""#)
            .replace("PARTIALLY_FILLED", "NEW"),
    );
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [99.0]);
    // Same caller ID can identify a new native order after the previous fills.
    h.message("ws-api/v3", &event.replace(r#""i":77"#, r#""i":78"#));
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [99.0]);
    h.message("ws-api/v3", &terminal);
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn spot_unseen_partial_execution_and_original_cancel_client_id_are_applied() {
    let mut h = spot();
    let event = r#"{"subscriptionId":0,"event":{"e":"executionReport","i":78,"s":"BTCUSDT","c":"new-client","S":"SELL","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"100","L":"101","q":"2","z":"1","l":"1"}}"#;
    h.message("ws-api/v3", event);
    assert_eq!(h.engine.store().order("new-client").unwrap().filled, 1.0);
    h.message(
        "ws-api/v3",
        &event
            .replace(
                r#""c":"new-client""#,
                r#""c":"cancel-request","C":"new-client""#,
            )
            .replace("PARTIALLY_FILLED", "CANCELED")
            .replace("TRADE", "CANCELED"),
    );
    assert_eq!(
        h.engine.store().order("new-client").unwrap().status,
        OrderStatus::Canceled
    );
    assert!(h.engine.store().order("cancel-request").is_none());
}

#[test]
fn malformed_inverse_order_snapshot_preserves_the_last_good_orders() {
    let mut h = inverse();
    h.message("/ws/coin-key", r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"NEW","p":"9000","q":"5","z":"2"}}"#);
    h.advance(10_000.0);
    h.respond("/dapi/v1/openOrders", 200, r#"{"unexpected":"envelope"}"#);
    assert_eq!(h.engine.store().orders().len(), 1);
    assert_eq!(h.engine.store().order("12").unwrap().filled, 2.0);
}

#[test]
fn spot_collateral_valuation_tries_live_usdc_and_usd_pairs_without_derivative_pegs() {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let info = format!(
        r#"{},{{"symbol":"ETHUSDT","status":"BREAK","baseAsset":"ETH","quoteAsset":"USDT","filters":[]}},{{"symbol":"ETHUSDC","status":"TRADING","baseAsset":"ETH","quoteAsset":"USDC","filters":[]}},{{"symbol":"ALTUSD","status":"TRADING","baseAsset":"ALT","quoteAsset":"USD","filters":[]}}]}}"#,
        SPOT_INFO.strip_suffix("]}").unwrap()
    );
    h.respond("/api/v3/exchangeInfo", 200, &info);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"0","volume":"1","quoteVolume":"100"},{"symbol":"ETHUSDC","lastPrice":"50","bidPrice":"49","askPrice":"51","priceChangePercent":"0","volume":"1","quoteVolume":"50"},{"symbol":"ALTUSD","lastPrice":"20","bidPrice":"19","askPrice":"21","priceChangePercent":"0","volume":"1","quoteVolume":"20"}]"#);
    h.respond("/api/v3/account", 200, r#"{"balances":[{"asset":"ETH","free":"2","locked":"0"},{"asset":"ALT","free":"3","locked":"0"},{"asset":"BFUSD","free":"100","locked":"0"},{"asset":"USD","free":"1","locked":"0"}]}"#);
    assert_eq!(h.engine.store().balance().total, 161.0);
}

#[test]
fn spot_missing_heartbeat_ack_times_out_even_while_user_events_continue() {
    let mut h = spot();
    let socket = h.socket("ws-api/v3").unwrap();
    for _ in 0..5 {
        h.message(
            "ws-api/v3",
            r#"{"subscriptionId":0,"event":{"e":"outboundAccountPosition","B":[]}}"#,
        );
        h.advance(5_000.0);
    }
    assert!(h.closed.contains(&socket));
    assert!(
        h.logs
            .iter()
            .any(|s| s.contains("acknowledgement timed out"))
    );
}

#[test]
fn spot_reconnect_replayed_execution_does_not_emit_a_second_fill() {
    let mut h = spot();
    let event = r#"{"subscriptionId":0,"event":{"e":"executionReport","i":77,"s":"BTCUSDT","c":"spot-id","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"100","L":"99","q":"2","z":"1","l":"0.5"}}"#;
    h.message("ws-api/v3", event);
    let _ = h.engine.take_frame(h.now);
    let old = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    let socket = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket });
    h.message(
        "ws-api/v3",
        r#"{"id":"tealstreet-user-data-12","status":200,"result":{"subscriptionId":5}}"#,
    );
    h.message(
        "ws-api/v3",
        &event.replace("\"subscriptionId\":0", "\"subscriptionId\":5"),
    );
    assert_eq!(h.engine.store().order("spot-id").unwrap().filled, 1.0);
    if let Some(frame) = h.engine.take_frame(h.now) {
        assert!(fill_prices(frame).is_empty());
    }
}

#[test]
fn spot_rest_replica_cannot_resurrect_terminal_orders_even_for_a_later_read() {
    let mut h = spot();
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":77,"s":"BTCUSDT","c":"spot-id","S":"BUY","o":"LIMIT","X":"FILLED","x":"TRADE","p":"100","L":"99","q":"2","z":"2","l":"1.5"}}"#);
    h.advance(10_000.0);
    h.respond("/api/v3/openOrders", 200, ORDERS);
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(h.engine.store().order("spot-id").unwrap().filled, 2.0);
}

#[test]
fn inverse_snapshot_baseline_and_native_execution_cache_fence_duplicate_fills() {
    let mut h = inverse();
    let _ = h.engine.take_frame(h.now);
    let event = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"9000","q":"5","z":"2","l":"1","L":"9100"}}"#;
    // The startup snapshot already included this execution.
    h.message("/ws/coin-key", event);
    if let Some(frame) = h.engine.take_frame(h.now) {
        assert!(fill_prices(frame).is_empty());
    }
    let next = event.replace(r#""z":"2""#, r#""z":"3""#);
    h.message("/ws/coin-key", &next);
    h.message("/ws/coin-key", &next);
    h.message("/ws/coin-key", event);
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [9100.0]);
    let order = h.engine.store().order("12").unwrap();
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (5.0, 3.0, 2.0)
    );
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(2.0));
}

#[test]
fn inverse_unseen_partial_order_keeps_native_counts_and_terminal_replays_cannot_reopen_it() {
    let mut h = inverse();
    let event = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":13,"s":"BTCUSD_PERP","S":"SELL","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"9000","q":"5","z":"1","l":"1","L":"9200"}}"#;
    let _ = h.engine.take_frame(h.now);
    h.message("/ws/coin-key", event);
    let order = h.engine.store().order("13").unwrap();
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (5.0, 1.0, 4.0)
    );
    assert_eq!(order.extra.as_ref().unwrap().remaining_native, Some(4.0));
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [9200.0]);
    let terminal = event
        .replace("PARTIALLY_FILLED", "FILLED")
        .replace(r#""z":"1""#, r#""z":"5""#)
        .replace(r#""l":"1""#, r#""l":"4""#);
    h.message("/ws/coin-key", &terminal);
    h.message("/ws/coin-key", &terminal);
    h.message("/ws/coin-key", &terminal.replace("FILLED", "NEW"));
    h.message("/ws/coin-key", event);
    assert_eq!(
        h.engine.store().order("13").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(fill_prices(h.engine.take_frame(h.now).unwrap()), [9200.0]);
}

#[test]
fn inverse_reconnect_and_later_replica_snapshot_cannot_replay_or_roll_back_execution() {
    let mut h = inverse();
    let event = r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"PARTIALLY_FILLED","x":"TRADE","p":"9000","q":"5","z":"3","l":"1","L":"9100"}}"#;
    h.message("/ws/coin-key", event);
    let _ = h.engine.take_frame(h.now);
    let old = h.socket("/ws/coin-key").unwrap();
    h.event(Event::WsClosed {
        socket: old,
        code: 1006,
    });
    h.advance(500.0);
    h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"new-coin-key"}"#);
    let socket = h.socket("/ws/new-coin-key").unwrap();
    h.event(Event::WsOpened { socket });
    h.message("/ws/new-coin-key", event);
    if let Some(frame) = h.engine.take_frame(h.now) {
        assert!(fill_prices(frame).is_empty());
    }
    h.advance(10_000.0);
    h.respond("/dapi/v1/openOrders", 200, r#"[{"symbol":"BTCUSD_PERP","orderId":12,"price":"9000","origQty":"5","executedQty":"2","type":"LIMIT","side":"BUY"}]"#);
    assert_eq!(h.engine.store().order("12").unwrap().filled, 3.0);
}

#[test]
fn spot_rest_seeded_limit_maker_amend_keeps_post_only_semantics() {
    let mut h = spot();
    assert_eq!(
        h.engine.store().order("spot-id").unwrap().time_in_force,
        Some(safe_cex_engine::model::TimeInForce::PostOnly)
    );
    h.event(Event::Command(safe_cex_engine::Command::AmendOrder {
        command: 91,
        id: "spot-id".into(),
        price: Some(101.0),
        amount: None,
    }));
    let cancel = h.take("/api/v3/order");
    assert_eq!(cancel.method, Method::Delete);
    assert!(cancel.url.contains("origClientOrderId=spot-id"));
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"orderId":77,"symbol":"BTCUSDT","clientOrderId":"spot-id","origQty":"2","executedQty":"0.5","status":"CANCELED"}"#,
    );
    let replacement = h.take("/api/v3/order");
    assert_eq!(replacement.method, Method::Post);
    assert!(replacement.url.contains("type=LIMIT_MAKER"));
    assert!(!replacement.url.contains("timeInForce="));
    assert!(!replacement.url.contains("newClientOrderId=spot-id"));
}

#[test]
fn inverse_unpriced_collateral_keeps_its_native_amount_without_inventing_usd() {
    let mut h = inverse();
    h.now += 30_000.0;
    let health = h.timers.iter().find(|(_, at)| *at == 4000.0).unwrap().0;
    h.event(Event::TimerFired { timer: health });
    h.respond("/dapi/v1/account", 200, r#"{"assets":[{"asset":"BTC","walletBalance":"1","availableBalance":"0.8","initialMargin":"0.1","unrealizedProfit":"0.01"},{"asset":"XYZ","walletBalance":"42","availableBalance":"40","initialMargin":"2","unrealizedProfit":"1"}]}"#);
    let balance = h.engine.store().balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (10000.0, 8000.0, 1000.0, 100.0)
    );
    let xyz = h
        .engine
        .store()
        .coin_balances()
        .unwrap()
        .iter()
        .find(|coin| coin.base == "XYZ")
        .unwrap();
    assert_eq!((xyz.amount, xyz.notional), (42.0, 0.0));
}

#[test]
fn inverse_owned_position_outside_partial_catalogue_never_guesses_contract_size() {
    let mut h = inverse_with(
        r#",{"symbol":7}"#,
        &INVERSE_POSITION_RISK.replace("BTCUSD_PERP", "MISSINGUSD_PERP"),
    );
    assert!(!h.engine.store().loaded().positions);
    h.message("/ws/coin-key", r#"{"e":"ACCOUNT_UPDATE","E":1564745798939,"T":1564745798938,"a":{"m":"ORDER","B":[],"P":[{"s":"MISSINGUSD_PERP","pa":"2","ep":"9000","cr":"0","up":"0.001","mt":"cross","iw":"0","ps":"BOTH"}]}}"#);
    assert!(h.engine.store().positions().is_empty());
    assert!(
        h.logs
            .iter()
            .any(|log| log.contains("owned position market unavailable"))
    );
}

#[test]
fn inverse_delivery_position_excluded_from_complete_catalogue_does_not_block_perpetuals() {
    let delivery = r#"{"symbol":"BTCUSD_260925","positionSide":"BOTH","positionAmt":"3","entryPrice":"9500","markPrice":"10000","unRealizedProfit":"0.002","liquidationPrice":"0","leverage":"10","marginType":"cross","isolatedMargin":"0"}"#;
    let mut h = inverse_with(
        "",
        &INVERSE_POSITION_RISK.replace("}]", &format!("}},{delivery}]")),
    );
    assert!(h.engine.store().loaded().positions);
    let symbols: Vec<&str> = h
        .engine
        .store()
        .positions()
        .iter()
        .map(|position| position.symbol.as_str())
        .collect();
    assert_eq!(symbols, ["BTCUSD_PERP"]);
    h.message("/ws/coin-key", r#"{"e":"ACCOUNT_UPDATE","E":1564745798939,"T":1564745798938,"a":{"m":"ORDER","B":[],"P":[{"s":"BTCUSD_PERP","pa":"4","ep":"9000","cr":"0","up":"0.001","mt":"isolated","iw":"0.003","ps":"BOTH"},{"s":"BTCUSD_260925","pa":"5","ep":"9500","cr":"0","up":"0.002","mt":"cross","iw":"0","ps":"BOTH"}]}}"#);
    let positions = h.engine.store().positions();
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].notional, 400.0);
}

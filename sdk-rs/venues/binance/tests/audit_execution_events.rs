//! ORDER_TRADE_UPDATE examples and execution/expiration semantics:
//! https://developers.binance.com/en/docs/products/derivatives-trading-coin-futures/user-data-streams
//! https://developers.binance.com/en/docs/products/derivatives-trading-usds-futures/user-data-streams
use support::Options;
mod support;

use safe_cex_engine::Event;
use safe_cex_engine::model::OrderStatus;
use safe_cex_venue_binance::Variant;
use support::Harness;

fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    if variant == Variant::CoinM {
        h.respond("/dapi/v1/exchangeInfo", 200, r#"{"symbols":[{"symbol":"BTCUSD_PERP","contractStatus":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"1000"}]}]}"#);
        h.respond("/dapi/v1/ticker/24hr", 200, r#"[{"symbol":"BTCUSD_PERP","lastPrice":"9000","priceChangePercent":"0","volume":"1","baseVolume":"1"}]"#);
        h.respond(
            "/dapi/v1/ticker/bookTicker",
            200,
            r#"[{"symbol":"BTCUSD_PERP","bidPrice":"8999","askPrice":"9001"}]"#,
        );
        h.respond("/dapi/v1/premiumIndex", 200, r#"[{"symbol":"BTCUSD_PERP","markPrice":"9000","indexPrice":"9000","lastFundingRate":"0","nextFundingTime":100000}]"#);
    } else {
        h.load_public();
    }
    let listen = match variant {
        Variant::CoinM => "/dapi/v1/listenKey",
        Variant::PortfolioMargin => "/papi/v1/listenKey",
        _ => "/fapi/v1/listenKey",
    };
    h.respond(listen, 200, r#"{"listenKey":"audit-key"}"#);
    let socket = h.socket(private_path(variant)).unwrap();
    h.event(Event::WsOpened { socket });
    let _ = h.engine.take_frame(h.now);
    h
}

fn private_path(variant: Variant) -> &'static str {
    match variant {
        Variant::CoinM => "/ws/audit-key",
        Variant::PortfolioMargin => "/pm/ws",
        _ => "/private/ws",
    }
}

fn event(variant: Variant, status: &str, execution: &str, filled: &str, trade: i64) -> String {
    let symbol = if variant == Variant::CoinM {
        "BTCUSD_PERP"
    } else {
        "BTCUSDT"
    };
    format!(
        r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"{symbol}","c":"audit-client","S":"SELL","ot":"STOP_MARKET","o":"MARKET","f":"GTC","q":"3","p":"0","ap":"9100","sp":"9000","x":"{execution}","X":"{status}","i":77,"l":"1","z":"{filled}","L":"9200","t":{trade},"R":true,"wt":"CONTRACT_PRICE","er":"8"}}}}"#
    )
}

fn fills(h: &mut Harness) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return Vec::new();
    };
    let mut at = 16;
    for _ in 0..u16::from_le_bytes(frame[6..8].try_into().unwrap()) {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let length = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 11 {
            let body = &frame[at + 8..at + 8 + length];
            let count = u32::from_le_bytes(body[..4].try_into().unwrap()) as usize;
            let start = (4 + 12 * count).next_multiple_of(8);
            return (0..count)
                .map(|i| {
                    let offset = start + 16 * i;
                    (
                        f64::from_le_bytes(body[offset..offset + 8].try_into().unwrap()),
                        f64::from_le_bytes(body[offset + 8..offset + 16].try_into().unwrap()),
                    )
                })
                .collect();
        }
        at += length + 8;
    }
    Vec::new()
}

#[test]
fn coin_m_stop_expiration_does_not_drop_triggered_execution() {
    let v = Variant::CoinM;
    let mut h = ready(v);
    let path = private_path(v);
    h.message(path, &event(v, "NEW", "NEW", "0", 0));
    h.message(path, &event(v, "EXPIRED", "EXPIRED", "0", 0));
    assert_eq!(
        h.engine.store().order("77").unwrap().status,
        OrderStatus::Canceled
    );
    h.message(path, &event(v, "PARTIALLY_FILLED", "TRADE", "1", 10));
    assert_eq!(fills(&mut h), [(9200.0, 1.0)]);
    assert_eq!(
        h.engine.store().order("77").unwrap().status,
        OrderStatus::Canceled
    );
    let final_trade = event(v, "FILLED", "TRADE", "3", 11)
        .replace(r#""l":"1""#, r#""l":"2""#)
        .replace(r#""L":"9200""#, r#""L":"9300""#);
    h.message(path, &final_trade);
    h.message(path, &final_trade);
    assert_eq!(fills(&mut h), [(9300.0, 2.0)]);
    let order = h.engine.store().order("77").unwrap();
    assert_eq!(
        (order.status, order.filled, order.remaining),
        (OrderStatus::Closed, 3.0, 0.0)
    );
    h.message(path, &event(v, "EXPIRED", "EXPIRED", "0", 0));
    assert_eq!(
        h.engine.store().order("77").unwrap().status,
        OrderStatus::Closed
    );
}

#[test]
fn derivative_fills_require_trade_and_dedupe_by_execution_identity() {
    for v in [Variant::UsdM, Variant::PortfolioMargin, Variant::CoinM] {
        let mut h = ready(v);
        let path = private_path(v);
        let id = if v == Variant::PortfolioMargin {
            "audit-client"
        } else {
            "77"
        };
        h.message(path, &event(v, "NEW", "NEW", "0", 0));
        for execution in ["AMENDMENT", "CANCELED"] {
            h.message(path, &event(v, "PARTIALLY_FILLED", execution, "1", 10));
        }
        let missing =
            event(v, "PARTIALLY_FILLED", "TRADE", "1", 10).replace("\"x\":\"TRADE\",", "");
        assert!(
            serde_json::from_str::<safe_cex_venue_binance::wire::OrderUpdateEvent<'_>>(&missing)
                .unwrap()
                .o
                .x
                .is_none()
        );
        h.message(path, &missing);
        assert!(
            fills(&mut h).is_empty(),
            "lifecycle packets do not execute: {v:?}"
        );
        let trade = event(v, "PARTIALLY_FILLED", "TRADE", "2", 10);
        h.message(path, &trade);
        h.message(path, &trade);
        // Distinct identities remain distinct even with equal or reordered z.
        h.message(path, &event(v, "PARTIALLY_FILLED", "TRADE", "2", 11));
        h.message(path, &event(v, "PARTIALLY_FILLED", "TRADE", "1", 12));
        assert_eq!(fills(&mut h), [(9200.0, 1.0); 3], "{v:?}");
        assert_eq!(h.engine.store().order(id).unwrap().filled, 2.0);
        for invalid in [
            event(v, "PARTIALLY_FILLED", "TRADE", "3", -1),
            event(v, "PARTIALLY_FILLED", "TRADE", "3", 20).replace(r#""l":"1""#, r#""l":"0""#),
            event(v, "PARTIALLY_FILLED", "TRADE", "3", 21).replace(r#""L":"9200""#, r#""L":"0""#),
        ] {
            h.message(path, &invalid);
        }
        assert!(fills(&mut h).is_empty());
    }
}

#[test]
fn liquidation_calculated_execution_emits_one_fill() {
    for v in [Variant::UsdM, Variant::PortfolioMargin, Variant::CoinM] {
        let mut h = ready(v);
        let path = private_path(v);
        let liquidation = event(v, "FILLED", "CALCULATED", "1", 30)
            .replace(r#""c":"audit-client""#, r#""c":"autoclose-1""#)
            .replace(r#""ot":"STOP_MARKET""#, r#""ot":"LIQUIDATION""#)
            .replace(r#""q":"3""#, r#""q":"1""#);
        h.message(path, &liquidation);
        assert_eq!(fills(&mut h), [(9200.0, 1.0)], "{v:?}");
        h.message(path, &liquidation);
        assert!(fills(&mut h).is_empty(), "{v:?}");
    }
}

#[test]
fn canceled_lifecycle_before_trade_preserves_fill_without_reopening() {
    for v in [Variant::UsdM, Variant::PortfolioMargin, Variant::CoinM] {
        let mut h = ready(v);
        let path = private_path(v);
        let id = if v == Variant::PortfolioMargin {
            "audit-client"
        } else {
            "77"
        };
        h.message(path, &event(v, "NEW", "NEW", "0", 0));
        h.message(path, &event(v, "CANCELED", "CANCELED", "1", 0));
        let trade = event(v, "PARTIALLY_FILLED", "TRADE", "1", 10);
        h.message(path, &trade);
        h.message(path, &trade);
        h.message(path, &event(v, "NEW", "NEW", "0", 0));
        assert_eq!(fills(&mut h), [(9200.0, 1.0)]);
        let order = h.engine.store().order(id).unwrap();
        assert_eq!((order.status, order.filled), (OrderStatus::Canceled, 1.0));
    }
}

#[test]
fn execution_identity_survives_private_socket_reconnect() {
    for v in [Variant::UsdM, Variant::PortfolioMargin, Variant::CoinM] {
        let mut h = ready(v);
        let path = private_path(v);
        let trade = event(v, "PARTIALLY_FILLED", "TRADE", "1", 10);
        h.message(path, &trade);
        assert_eq!(fills(&mut h), [(9200.0, 1.0)]);
        let socket = h.socket(path).unwrap();
        h.event(Event::WsClosed { socket, code: 1006 });
        h.advance(500.0);
        let listen = match v {
            Variant::CoinM => "/dapi/v1/listenKey",
            Variant::PortfolioMargin => "/papi/v1/listenKey",
            _ => "/fapi/v1/listenKey",
        };
        h.respond(listen, 200, r#"{"listenKey":"audit-key"}"#);
        let socket = h.socket(path).unwrap();
        h.event(Event::WsOpened { socket });
        h.message(path, &trade);
        assert!(fills(&mut h).is_empty(), "{v:?}");
    }
}

#[test]
fn pm_client_id_reuse_accepts_new_native_order_and_fences_old_generation() {
    let v = Variant::PortfolioMargin;
    let mut h = ready(v);
    let path = private_path(v);
    h.message(path, &event(v, "NEW", "NEW", "0", 0));
    h.message(path, &event(v, "CANCELED", "CANCELED", "0", 0));
    assert_eq!(
        h.engine.store().order("audit-client").unwrap().status,
        OrderStatus::Canceled
    );
    let new = event(v, "NEW", "NEW", "0", 0).replace(r#""i":77"#, r#""i":78"#);
    h.message(path, &new);
    assert_eq!(
        h.engine.store().order("audit-client").unwrap().status,
        OrderStatus::Open
    );
    // A replay from the old generation must not change the new order.
    h.message(path, &event(v, "NEW", "NEW", "0", 0));
    h.message(path, &event(v, "CANCELED", "CANCELED", "0", 0));
    h.message(path, &event(v, "FILLED", "TRADE", "3", 12));
    let current = h.engine.store().order("audit-client").unwrap();
    assert_eq!((current.status, current.filled), (OrderStatus::Open, 0.0));
}

#[test]
fn pm_strategy_incarnation_and_regular_native_namespaces_are_independent() {
    let v = Variant::PortfolioMargin;
    let mut h = ready(v);
    let path = private_path(v);
    let strategy = |native, status: &str| {
        format!(
            r#"{{"e":"CONDITIONAL_ORDER_TRADE_UPDATE","so":{{"s":"BTCUSDT","c":"strategy-client","si":{native},"S":"SELL","st":"STOP_MARKET","os":"{status}","q":"3","p":"0","sp":"9000","R":true}}}}"#
        )
    };
    h.message(path, &strategy(77, "NEW"));
    h.message(path, &strategy(77, "CANCELED"));
    h.message(path, &strategy(78, "NEW"));
    for status in ["NEW", "CANCELED", "FILLED"] {
        h.message(path, &strategy(77, status));
    }
    assert_eq!(
        h.engine.store().order("strategy-client").unwrap().status,
        OrderStatus::Open
    );
    // Strategy 77's terminal fence must not suppress regular native order 77.
    h.message(path, &event(v, "NEW", "NEW", "0", 0));
    assert_eq!(
        h.engine.store().order("audit-client").unwrap().status,
        OrderStatus::Open
    );
    h.message(path, &event(v, "CANCELED", "CANCELED", "0", 0));
    h.message(path, &strategy(79, "NEW"));
    assert_eq!(
        h.engine.store().order("strategy-client").unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn malformed_execution_does_not_consume_trade_identity() {
    for v in [Variant::UsdM, Variant::PortfolioMargin, Variant::CoinM] {
        let mut h = ready(v);
        let path = private_path(v);
        let trade = event(v, "PARTIALLY_FILLED", "TRADE", "1", 50);
        for (from, to) in [
            (r#""l":"1""#, r#""l":"NaN""#),
            (r#""l":"1""#, r#""l":"inf""#),
            (r#""L":"9200""#, r#""L":"NaN""#),
            (r#""L":"9200""#, r#""L":"inf""#),
            (r#""z":"1""#, r#""z":"NaN""#),
            (r#""z":"1""#, r#""z":"inf""#),
        ] {
            h.message(path, &trade.replace(from, to));
            assert!(fills(&mut h).is_empty(), "{v:?}: {to}");
        }
        h.message(path, &trade);
        assert_eq!(fills(&mut h), [(9200.0, 1.0)], "{v:?}");
        h.message(path, &trade);
        assert!(fills(&mut h).is_empty());
    }
}

#[test]
fn pm_superseded_native_cannot_rebind_when_old_terminal_event_was_delayed() {
    let v = Variant::PortfolioMargin;
    for snapshot_binding in [false, true] {
        let mut h = ready(v);
        let path = private_path(v);
        h.respond("/papi/v1/um/openOrders", 200, "[]");
        h.respond("/papi/v1/um/conditional/openOrders", 200, "[]");
        h.message(path, &event(v, "NEW", "NEW", "0", 0));
        if snapshot_binding {
            h.advance(10_000.0);
            h.respond("/papi/v1/um/openOrders", 200, r#"[{"symbol":"BTCUSDT","orderId":78,"clientOrderId":"audit-client","price":"9000","origQty":"3","executedQty":"0","type":"LIMIT","side":"SELL"}]"#);
            h.respond("/papi/v1/um/conditional/openOrders", 200, "[]");
        } else {
            h.message(
                path,
                &event(v, "NEW", "NEW", "0", 0).replace(r#""i":77"#, r#""i":78"#),
            );
        }
        let current = h.engine.store().order("audit-client").unwrap().clone();
        for (status, execution, filled) in [
            ("CANCELED", "CANCELED", "0"),
            ("FILLED", "TRADE", "3"),
            ("NEW", "NEW", "0"),
        ] {
            h.message(path, &event(v, status, execution, filled, 10));
            assert_eq!(
                h.engine.store().order("audit-client"),
                Some(&current),
                "snapshot={snapshot_binding}: {status}"
            );
        }
    }
}

#[test]
fn usd_m_algo_and_regular_terminal_namespaces_are_independent() {
    let v = Variant::UsdM;
    let mut h = ready(v);
    let path = private_path(v);
    let algo = |status: &str| {
        format!(
            r#"{{"e":"ALGO_UPDATE","o":{{"aid":77,"s":"BTCUSDT","S":"SELL","o":"STOP_MARKET","X":"{status}","q":"3","tp":"9000"}}}}"#
        )
    };
    h.message(path, &algo("EXPIRED"));
    h.message(path, &event(v, "NEW", "NEW", "0", 0));
    assert_eq!(
        h.engine.store().order("77").unwrap().status,
        OrderStatus::Open
    );
    h.message(path, &event(v, "CANCELED", "CANCELED", "0", 0));
    // Use a different algo id to avoid the earlier actual algo expiration.
    h.message(path, &algo("NEW").replace(r#""aid":77"#, r#""aid":78"#));
    assert_eq!(
        h.engine.store().order("78").unwrap().status,
        OrderStatus::Open
    );
}

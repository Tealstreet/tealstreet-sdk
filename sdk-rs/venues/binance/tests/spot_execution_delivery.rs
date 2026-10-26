//! Spot executionReport delivery is independent of quantified cancellation proof.
//! https://developers.binance.com/en/docs/products/spot/user-data-stream
use support::Options;
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn orders(filled: &str) -> String {
    format!(
        r#"[{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"{filled}","type":"LIMIT","side":"BUY","timeInForce":"GTC"}}]"#
    )
}
fn ready(rows: &str) -> Harness {
    let mut h = Harness::started(Options {
        variant: Variant::Spot,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.respond("/api/v3/exchangeInfo", 200, r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]},{"symbol":"ETHUSDT","status":"TRADING","baseAsset":"ETH","quoteAsset":"USDT","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}]}]}"#);
    h.respond("/api/v3/ticker/24hr", 200, r#"[{"symbol":"BTCUSDT","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000"},{"symbol":"ETHUSDT","lastPrice":"100","bidPrice":"99","askPrice":"101","priceChangePercent":"0","volume":"1","quoteVolume":"100"}]"#);
    let socket = h.socket("ws-api/v3").unwrap();
    h.event(Event::WsOpened { socket });
    h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
    h.respond("/api/v3/openOrders", 200, rows);
    subscribe(&mut h);
    h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
    h.respond("/api/v3/openOrders", 200, rows);
    take_fills(&mut h);
    h
}
fn subscribe(h: &mut Harness) {
    #[derive(serde::Deserialize)]
    struct Sent {
        id: String,
    }
    let text = h
        .sent
        .iter()
        .rev()
        .find(|(_, text)| text.contains("userDataStream.subscribe.signature"))
        .unwrap()
        .1
        .clone();
    let sent: Sent = serde_json::from_str(&text).unwrap();
    h.message(
        "ws-api/v3",
        &format!(
            r#"{{"id":"{}","status":200,"result":{{"subscriptionId":0}}}}"#,
            sent.id
        ),
    );
}
fn event(
    identity: (u64, &str, &str),
    status: &str,
    execution: &str,
    cumulative: &str,
    last: &str,
    trade: Option<i64>,
) -> String {
    let (native, symbol, client) = identity;
    let trade = trade.map_or(String::new(), |t| format!(",\"t\":{t}"));
    format!(
        r#"{{"subscriptionId":0,"event":{{"e":"executionReport","i":{native},"s":"{symbol}","c":"{client}","S":"BUY","o":"LIMIT","f":"GTC","X":"{status}","x":"{execution}","p":"9000","L":"9100","q":"7","z":"{cumulative}","l":"{last}"{trade}}}}}"#
    )
}
fn trade(cumulative: &str, id: Option<i64>) -> String {
    event(
        (12, "BTCUSDT", "maker"),
        "PARTIALLY_FILLED",
        "TRADE",
        cumulative,
        "1",
        id,
    )
}
fn take_fills(h: &mut Harness) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return vec![];
    };
    let Some((_, payload)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
    else {
        return vec![];
    };
    let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
    let start = (4 + 12 * count).next_multiple_of(8);
    (0..count)
        .map(|i| {
            let at = start + 16 * i;
            (
                f64::from_le_bytes(payload[at..at + 8].try_into().unwrap()),
                f64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}
fn ack(filled: &str, status: &str) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"cancel-request","origClientOrderId":"maker","origQty":"7","executedQty":"{filled}","status":"{status}"}}"#
    )
}
fn cancel(h: &mut Harness, filled: &str, status: &str) {
    h.event(Event::Command(Command::CancelOrders {
        command: 71,
        ids: vec!["maker".into()],
    }));
    let request = h.take("/api/v3/order?");
    assert_eq!(request.method, Method::Delete);
    h.reply(request.id, 200, "", &ack(filled, status));
    assert!(h.result(71).unwrap().is_ok());
}

#[test]
fn quantified_cancel_ack_does_not_consume_a_later_execution() {
    for native_id in [None, Some(100)] {
        let mut h = ready(&orders("2"));
        cancel(&mut h, "3", "CANCELED");
        assert!(take_fills(&mut h).is_empty());
        h.message("ws-api/v3", &trade("3", native_id));
        h.message("ws-api/v3", &trade("3", native_id));
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
        let current = h.engine.store().order("maker").unwrap();
        assert_eq!(
            (current.status, current.filled, current.remaining),
            (OrderStatus::Canceled, 3.0, 4.0)
        );
    }
}

#[test]
fn quantified_amend_ack_preserves_replacement_sizing_and_late_fill_delivery() {
    let mut h = ready(&orders("2"));
    h.event(Event::Command(Command::AmendOrder {
        command: 72,
        id: "maker".into(),
        price: Some(10001.0),
        amount: None,
    }));
    let request = h.take("/api/v3/order?");
    h.reply(request.id, 200, "", &ack("3", "CANCELED"));
    let replacement = h.take("/api/v3/order?");
    assert_eq!(replacement.method, Method::Post);
    assert!(replacement.url.contains("quantity=4&"));
    assert!(take_fills(&mut h).is_empty());
    h.message("ws-api/v3", &trade("3", Some(100)));
    h.message("ws-api/v3", &trade("3", Some(100)));
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
    assert_eq!(h.engine.store().order("maker").unwrap().filled, 3.0);
}

fn refresh(h: &mut Harness, rows: &str) {
    h.message(
        "ws-api/v3",
        &event((99, "BTCUSDT", "other"), "NEW", "NEW", "0", "0", None),
    );
    h.advance(10_000.0);
    h.respond("/api/v3/openOrders", 200, rows);
}

#[test]
fn initial_snapshot_floor_is_fixed_and_later_rest_cannot_consume_live_delivery() {
    let mut h = ready(&orders("2"));
    h.message("ws-api/v3", &trade("2", Some(1)));
    h.message("ws-api/v3", &trade("1", Some(2)));
    assert!(take_fills(&mut h).is_empty());
    refresh(&mut h, &orders("3"));
    assert_eq!(h.engine.store().order("maker").unwrap().filled, 3.0);
    h.message("ws-api/v3", &trade("3", Some(3)));
    h.message("ws-api/v3", &trade("3", Some(3)));
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
}

#[test]
fn stream_first_lifecycle_and_trade_observations_keep_a_zero_bootstrap_floor() {
    for first_trade in [false, true] {
        let mut h = ready("[]");
        h.message(
            "ws-api/v3",
            &event(
                (12, "BTCUSDT", "maker"),
                if first_trade {
                    "PARTIALLY_FILLED"
                } else {
                    "NEW"
                },
                if first_trade { "TRADE" } else { "NEW" },
                "3",
                if first_trade { "1" } else { "0" },
                Some(10),
            ),
        );
        assert_eq!(take_fills(&mut h).len(), usize::from(first_trade));
        refresh(&mut h, &orders("3"));
        h.message("ws-api/v3", &trade("3", Some(11)));
        h.message("ws-api/v3", &trade("2", Some(12)));
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0); 2]);
        assert_eq!(h.engine.store().order("maker").unwrap().filled, 3.0);
    }
}

#[test]
fn distinct_native_trade_ids_survive_equal_and_reordered_cumulative_quantities() {
    let mut h = ready(&orders("2"));
    for (id, filled) in [(10, "4"), (11, "3"), (12, "3"), (10, "4"), (11, "3")] {
        h.message("ws-api/v3", &trade(filled, Some(id)));
    }
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0); 3]);
    assert_eq!(
        (
            h.engine.store().order("maker").unwrap().filled,
            h.engine.store().order("maker").unwrap().remaining
        ),
        (4.0, 3.0)
    );
}

#[test]
fn malformed_execution_does_not_consume_a_valid_retry_identity() {
    for (from, to) in [
        (r#""L":"9100""#, r#""L":"0""#),
        (r#""L":"9100""#, r#""L":"-1""#),
        (r#""L":"9100""#, r#""L":"NaN""#),
        (r#""L":"9100""#, r#""L":"inf""#),
        (r#""L":"9100""#, r#""L":"bad""#),
        (r#""l":"1""#, r#""l":"0""#),
        (r#""l":"1""#, r#""l":"-1""#),
        (r#""l":"1""#, r#""l":"NaN""#),
        (r#""l":"1""#, r#""l":"inf""#),
        (r#""l":"1""#, r#""l":"9""#),
        (r#""z":"3""#, r#""z":"-1""#),
        (r#""z":"3""#, r#""z":"NaN""#),
        (r#""z":"3""#, r#""z":"inf""#),
        (r#""z":"3""#, r#""z":"bad""#),
        (r#""z":"3""#, r#""z":"8""#),
        (r#""q":"7""#, r#""q":"bad""#),
        (r#""S":"BUY""#, r#""S":"unknown""#),
        (r#""t":100"#, r#""t":-1"#),
    ] {
        let mut h = ready(&orders("2"));
        let current = h.engine.store().order("maker").unwrap().clone();
        let valid = trade("3", Some(100));
        h.message("ws-api/v3", &valid.replace(from, to));
        assert!(take_fills(&mut h).is_empty(), "{from} -> {to}");
        assert_eq!(h.engine.store().order("maker"), Some(&current));
        h.message("ws-api/v3", &valid);
        h.message("ws-api/v3", &valid);
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
    }
}

#[test]
fn rejected_snapshots_cannot_seed_history_for_an_unseen_native_order() {
    for failure in ["http", "syntax", "row", "stale", "terminal"] {
        let mut h = ready("[]");
        h.message(
            "ws-api/v3",
            &event((99, "BTCUSDT", "other"), "NEW", "NEW", "0", "0", None),
        );
        if failure == "terminal" {
            h.message(
                "ws-api/v3",
                &event(
                    (99, "BTCUSDT", "other"),
                    "CANCELED",
                    "CANCELED",
                    "0",
                    "0",
                    None,
                ),
            );
        }
        h.advance(10_000.0);
        let read = h.take("/api/v3/openOrders");
        if failure == "stale" {
            h.message(
                "ws-api/v3",
                &event((99, "BTCUSDT", "other"), "NEW", "NEW", "0", "0", None),
            );
        }
        let rows = match failure {
            "syntax" => "[bad]".into(),
            // A rejected row seeds nothing for its own order; valid peers still commit.
            "row" => orders("3").replace(r#""9000""#, r#""NaN""#),
            "terminal" => format!(
                "{},{{\"symbol\":\"BTCUSDT\",\"orderId\":99,\"clientOrderId\":\"other\",\"price\":\"9000\",\"origQty\":\"7\",\"executedQty\":\"0\",\"type\":\"LIMIT\",\"side\":\"BUY\"}}]",
                orders("3").trim_end_matches(']')
            ),
            _ => orders("3"),
        };
        h.reply(
            read.id,
            if failure == "http" { 500 } else { 200 },
            "",
            &rows,
        );
        h.message("ws-api/v3", &trade("3", Some(100)));
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0)], "{failure}");
    }
}

#[test]
fn stream_first_ack_never_creates_a_historical_delivery_floor() {
    let mut h = ready("[]");
    h.message(
        "ws-api/v3",
        &event((12, "BTCUSDT", "maker"), "NEW", "NEW", "2", "0", None),
    );
    cancel(&mut h, "3", "CANCELED");
    h.message("ws-api/v3", &trade("1", Some(100)));
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
    assert_eq!(h.engine.store().order("maker").unwrap().filled, 3.0);
}

#[test]
fn terminal_totals_and_closed_rows_do_not_consume_late_or_reordered_native_trades() {
    for closed in [false, true] {
        let mut h = ready(&orders("2"));
        cancel(
            &mut h,
            if closed { "7" } else { "3" },
            if closed { "FILLED" } else { "CANCELED" },
        );
        if closed {
            h.message(
                "ws-api/v3",
                &event(
                    (12, "BTCUSDT", "maker"),
                    "FILLED",
                    "TRADE",
                    "7",
                    "1",
                    Some(100),
                ),
            );
        } else {
            h.message("ws-api/v3", &trade("4", Some(100)));
        }
        h.message("ws-api/v3", &trade("3", Some(101)));
        h.message("ws-api/v3", &trade("3", Some(101)));
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0); 2]);
        let current = h.engine.store().order("maker").unwrap();
        assert_eq!(
            (current.status, current.filled, current.remaining),
            if closed {
                (OrderStatus::Closed, 7.0, 0.0)
            } else {
                (OrderStatus::Canceled, 4.0, 3.0)
            }
        );
        let before = current.clone();
        h.message(
            "ws-api/v3",
            &event(
                (12, "BTCUSDT", "maker"),
                "CANCELED",
                "CANCELED",
                "3",
                "0",
                None,
            ),
        );
        h.message(
            "ws-api/v3",
            &event((12, "BTCUSDT", "maker"), "NEW", "NEW", "0", "0", None),
        );
        assert_eq!(h.engine.store().order("maker"), Some(&before));
    }
}

#[test]
fn trade_before_quantified_ack_cannot_replay_or_roll_back_sizing() {
    let mut h = ready(&orders("2"));
    h.message("ws-api/v3", &trade("3", Some(100)));
    cancel(&mut h, "2", "CANCELED");
    h.message("ws-api/v3", &trade("3", Some(100)));
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
    let current = h.engine.store().order("maker").unwrap();
    assert_eq!((current.filled, current.remaining), (3.0, 4.0));
}

#[test]
fn lifecycle_ack_permutations_deliver_once_for_current_and_reused_client_rows() {
    for symbol in ["BTCUSDT", "ETHUSDT"] {
        for reused in [false, true] {
            for ack_first in [false, true] {
                for quantified in [false, true] {
                    let mut h = ready(&orders("2"));
                    h.event(Event::Command(Command::CancelSymbolOrders {
                        command: 73,
                        symbol: "BTCUSDT".into(),
                    }));
                    let cancel = h.take("/api/v3/openOrders");
                    if reused {
                        h.message(
                            "ws-api/v3",
                            &event((44, symbol, "maker"), "NEW", "NEW", "0", "0", None)
                                .replace(r#""S":"BUY""#, r#""S":"SELL""#)
                                .replace(r#""p":"9000""#, r#""p":"100""#),
                        );
                    }
                    let current = h.engine.store().order("maker").unwrap().clone();
                    let reply = if quantified {
                        format!("[{}]", ack("3", "CANCELED"))
                    } else {
                        "[]".into()
                    };
                    if ack_first {
                        h.reply(cancel.id, 200, "", &reply);
                    }
                    let lifecycle = event(
                        (12, "BTCUSDT", "cancel-request"),
                        "CANCELED",
                        "CANCELED",
                        "3",
                        "0",
                        None,
                    )
                    .replace(
                        r#""c":"cancel-request""#,
                        r#""c":"cancel-request","C":"maker""#,
                    );
                    h.message("ws-api/v3", &lifecycle);
                    assert!(take_fills(&mut h).is_empty());
                    h.message("ws-api/v3", &trade("3", Some(100)));
                    h.message("ws-api/v3", &trade("3", Some(100)));
                    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
                    if !ack_first {
                        h.reply(cancel.id, 200, "", &reply);
                    }
                    h.message("ws-api/v3", &trade("3", Some(100)));
                    assert!(take_fills(&mut h).is_empty());
                    if reused {
                        assert_eq!(h.engine.store().order("maker"), Some(&current));
                    } else {
                        assert_eq!(
                            h.engine.store().order("maker").unwrap().status,
                            OrderStatus::Canceled
                        );
                    }
                    assert!(h.result(73).unwrap().is_ok());
                }
            }
        }
    }
}

#[test]
fn execution_identity_includes_symbol_and_native_order() {
    let mut h = ready(&orders("2"));
    for identity in [
        (12, "BTCUSDT", "maker"),
        (13, "BTCUSDT", "second"),
        (12, "ETHUSDT", "third"),
    ] {
        let packet = event(identity, "PARTIALLY_FILLED", "TRADE", "3", "1", Some(100));
        h.message("ws-api/v3", &packet);
        h.message("ws-api/v3", &packet);
    }
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0); 3]);
}

#[test]
fn legacy_idless_fallback_tracks_delivery_not_authoritative_totals() {
    let mut h = ready(&orders("2"));
    cancel(&mut h, "4", "CANCELED");
    for total in ["3", "3", "2.5", "4", "4"] {
        h.message("ws-api/v3", &trade(total, None));
    }
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0); 2]);
    assert_eq!(h.engine.store().order("maker").unwrap().filled, 4.0);
}

#[test]
fn reconnect_preserves_pending_delivery_and_native_trade_identity() {
    let mut h = ready(&orders("2"));
    cancel(&mut h, "3", "CANCELED");
    for delivered in [false, true] {
        let old = h.socket("ws-api/v3").unwrap();
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        h.advance(1000.0);
        let socket = h.socket("ws-api/v3").unwrap();
        assert_ne!(old, socket);
        h.event(Event::WsOpened { socket });
        subscribe(&mut h);
        h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
        h.respond("/api/v3/openOrders", 200, "[]");
        h.message("ws-api/v3", &trade("3", Some(100)));
        assert_eq!(
            take_fills(&mut h),
            if delivered {
                vec![]
            } else {
                vec![(9100.0, 1.0)]
            }
        );
    }
}

#[test]
fn native_trade_identity_window_evicts_oldest_without_a_lifetime_exactly_once_claim() {
    let mut h = ready(&orders("2"));
    // Deterministic cache-boundary regression; no timers, network or load measurement.
    for id in 0..=16_384 {
        h.message("ws-api/v3", &trade("3", Some(id)));
    }
    assert_eq!(take_fills(&mut h).len(), 16_385);
    h.message("ws-api/v3", &trade("3", Some(16_384)));
    assert!(take_fills(&mut h).is_empty());
    // A previously evicted execution can be observed again outside the retained window.
    h.message("ws-api/v3", &trade("3", Some(0)));
    assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
}

#[test]
fn flat_and_nested_quantified_cancel_all_acknowledgements_preserve_trade_delivery() {
    for nested in [false, true] {
        let mut h = ready(&orders("2"));
        h.message(
            "ws-api/v3",
            &event((13, "BTCUSDT", "second"), "NEW", "NEW", "0", "0", None),
        );
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 74,
            symbol: "BTCUSDT".into(),
        }));
        let cancel = h.take("/api/v3/openOrders");
        let first = ack("3", "CANCELED");
        let second = ack("0", "CANCELED")
            .replace(r#""orderId":12"#, r#""orderId":13"#)
            .replace("maker", "second");
        let body = if nested {
            format!(
                r#"[{{"orderListId":5,"symbol":"BTCUSDT","listStatusType":"ALL_DONE","listOrderStatus":"ALL_DONE","orders":[{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker"}},{{"symbol":"BTCUSDT","orderId":13,"clientOrderId":"second"}}],"orderReports":[{first},{second}]}}]"#
            )
        } else {
            format!("[{first},{second}]")
        };
        h.reply(cancel.id, 200, "", &body);
        assert!(h.result(74).unwrap().is_ok());
        assert!(take_fills(&mut h).is_empty());
        h.message("ws-api/v3", &trade("3", Some(100)));
        h.message("ws-api/v3", &trade("3", Some(100)));
        assert_eq!(take_fills(&mut h), [(9100.0, 1.0)]);
        assert_eq!(
            h.engine.store().order("maker").unwrap().status,
            OrderStatus::Canceled
        );
    }
}

#[test]
fn one_unmappable_spot_order_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let order = |client: &str, native: u64, kind: &str, price: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","orderId":{native},"clientOrderId":"{client}","price":"{price}","origQty":"7","executedQty":"0","type":"{kind}","side":"BUY","timeInForce":"GTC"}}"#
        )
    };
    let read = |h: &mut Harness, poke: u64, rows: &str| {
        let client = format!("poke{poke}");
        h.message(
            "ws-api/v3",
            &event((poke, "BTCUSDT", &client), "NEW", "NEW", "0", "0", None),
        );
        h.advance(10_000.0);
        h.respond("/api/v3/openOrders", 200, rows);
        // Acknowledge the stream heartbeat so the next read is not a resubscribe.
        if let Some((_, sent)) = h.sent.iter().rev().find(|(_, s)| s.contains("heartbeat")) {
            let id = sent.split('"').nth(3).unwrap().to_owned();
            h.message(
                "ws-api/v3",
                &format!(r#"{{"id":"{id}","status":200,"result":[{{"subscriptionId":0}}]}}"#),
            );
        }
    };
    let mut h = ready(&format!(
        "[{},{}]",
        order("a", 10, "LIMIT", "9000"),
        order("b", 11, "LIMIT", "9001")
    ));
    let stored = h.engine.store().order("b").unwrap().clone();
    h.logs.clear();

    read(
        &mut h,
        97,
        &format!(
            "[{},{}]",
            order("a", 10, "LIMIT", "9002"),
            order("b", 11, "FUTURE_KIND", "1")
        ),
    );
    let store = h.engine.store();
    assert_eq!(store.order("a").unwrap().price, 9002.0);
    assert_eq!(store.order("b"), Some(&stored));
    assert!(
        h.logs
            .iter()
            .any(|l| l == "Binance open order b has an unsupported type")
    );
    assert!(!h.logs.iter().any(|l| l.contains("FUTURE_KIND")));

    read(
        &mut h,
        98,
        &format!("[{}]", order("a", 10, "LIMIT", "9002")),
    );
    let ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert_eq!(ids, ["a"]);
}

#[test]
fn a_rejected_row_keeps_only_a_stored_order_on_its_own_symbol() {
    let order = |symbol: &str, client: &str, native: u64, kind: &str| {
        format!(
            r#"{{"symbol":"{symbol}","orderId":{native},"clientOrderId":"{client}","price":"9000","origQty":"7","executedQty":"0","type":"{kind}","side":"BUY","timeInForce":"GTC"}}"#
        )
    };
    let mut h = ready(&format!(
        "[{},{}]",
        order("BTCUSDT", "a", 10, "LIMIT"),
        order("ETHUSDT", "x", 20, "LIMIT")
    ));
    assert_eq!(h.engine.store().order("x").unwrap().symbol, "ETHUSDT");
    h.message(
        "ws-api/v3",
        &event((97, "BTCUSDT", "poke97"), "NEW", "NEW", "0", "0", None),
    );
    h.advance(10_000.0);
    // A BTCUSDT row reusing client id x says nothing about the ETHUSDT order x.
    h.respond(
        "/api/v3/openOrders",
        200,
        &format!(
            "[{},{}]",
            order("BTCUSDT", "a", 10, "LIMIT"),
            order("BTCUSDT", "x", 30, "FUTURE_KIND")
        ),
    );
    let ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    assert_eq!(ids, ["a"]);
}

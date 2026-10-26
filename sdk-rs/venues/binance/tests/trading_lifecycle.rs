//! Adversarial cancel/replace cases with documented full cancellation replies.
use support::Options;
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn ready(variant: Variant) -> Harness {
    let spot = variant == Variant::Spot;
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let symbol = if spot { "BTCUSDT" } else { "BTCUSD_PERP" };
    let prefix = if spot { "/api/v3" } else { "/dapi/v1" };
    h.respond(&format!("{prefix}/exchangeInfo"), 200, &format!(r#"{{"symbols":[{{"symbol":"{symbol}","status":"TRADING","baseAsset":"BTC","quoteAsset":"USD","marginAsset":"BTC","contractType":"PERPETUAL","contractSize":100,"filters":[{{"filterType":"PRICE_FILTER","tickSize":"0.1"}},{{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}},{{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"100"}}]}}]}}"#));
    if !spot {
        h.respond("/dapi/v2/leverageBracket", 200, "[]");
    }
    h.respond(&format!("{prefix}/ticker/24hr"), 200, &format!(r#"[{{"symbol":"{symbol}","lastPrice":"10000","bidPrice":"9999","askPrice":"10001","priceChangePercent":"0","volume":"1","quoteVolume":"10000","baseVolume":"1"}}]"#));
    if !spot {
        h.respond(
            "/dapi/v1/ticker/bookTicker",
            200,
            r#"[{"symbol":"BTCUSD_PERP","bidPrice":"9999","askPrice":"10001"}]"#,
        );
        h.respond("/dapi/v1/premiumIndex", 200, r#"[{"symbol":"BTCUSD_PERP","markPrice":"10000","indexPrice":"10000","lastFundingRate":"0","nextFundingTime":100000}]"#);
        h.respond("/dapi/v1/listenKey", 200, r#"{"listenKey":"coin-key"}"#);
        h.respond(
            "/dapi/v1/positionSide/dual",
            200,
            r#"{"dualSidePosition":false}"#,
        );
        h.respond("/dapi/v1/positionRisk", 200, r#"[{"symbol":"BTCUSD_PERP","positionSide":"BOTH","positionAmt":"7","entryPrice":"9000","markPrice":"10000","unRealizedProfit":"0","liquidationPrice":"7000","leverage":"20","marginType":"cross","isolatedMargin":"0"}]"#);
    }
    h.respond(
        &format!("{prefix}/account"),
        200,
        if spot {
            r#"{"balances":[]}"#
        } else {
            r#"{"assets":[]}"#
        },
    );
    let orders = format!(
        r#"[{{"symbol":"{symbol}","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","timeInForce":"GTC"}},{{"symbol":"{symbol}","orderId":13,"clientOrderId":"second","price":"9000","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY"}}]"#
    );
    h.respond(&format!("{prefix}/openOrders"), 200, &orders);
    let socket = h
        .socket(if spot { "ws-api/v3" } else { "/ws/coin-key" })
        .unwrap();
    h.event(Event::WsOpened { socket });
    if spot {
        h.message(
            "ws-api/v3",
            r#"{"id":"tealstreet-user-data-10","status":200,"result":{"subscriptionId":0}}"#,
        );
        h.respond("/api/v3/account", 200, r#"{"balances":[]}"#);
        h.respond("/api/v3/openOrders", 200, &orders);
        h.message("ws-api/v3", r#"{"id":"tealstreet-user-data-heartbeat-11","status":200,"result":[{"subscriptionId":0}]}"#);
    }
    h
}
fn id(variant: Variant) -> &'static str {
    if variant == Variant::Spot {
        "maker"
    } else {
        "12"
    }
}
fn path(variant: Variant) -> &'static str {
    if variant == Variant::Spot {
        "/api/v3/order"
    } else {
        "/dapi/v1/order"
    }
}
fn ack(variant: Variant, native: u64, filled: &str, status: &str) -> String {
    let symbol = if variant == Variant::Spot {
        "BTCUSDT"
    } else {
        "BTCUSD_PERP"
    };
    let client = if native == 12 { "maker" } else { "second" };
    format!(
        r#"{{"symbol":"{symbol}","orderId":{native},"clientOrderId":"cancel-request","origClientOrderId":"{client}","origQty":"7","executedQty":"{filled}","status":"{status}"}}"#
    )
}
fn amend(h: &mut Harness, variant: Variant) {
    h.event(Event::Command(Command::AmendOrder {
        command: 91,
        id: id(variant).into(),
        price: Some(10001.0),
        amount: None,
    }));
}
fn fill(h: &mut Harness, variant: Variant, filled: &str, status: &str) {
    if variant == Variant::Spot {
        h.message("ws-api/v3", &format!(r#"{{"subscriptionId":0,"event":{{"e":"executionReport","i":12,"s":"BTCUSDT","c":"maker","S":"BUY","o":"LIMIT","X":"{status}","x":"TRADE","p":"9000","L":"9100","q":"7","z":"{filled}","l":"1"}}}}"#));
    } else {
        h.message("/ws/coin-key", &format!(r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"i":12,"s":"BTCUSD_PERP","S":"BUY","o":"LIMIT","X":"{status}","x":"TRADE","p":"9000","q":"7","z":"{filled}","l":"1","L":"9100"}}}}"#));
    }
}
fn quantity(url: &str) -> Option<&str> {
    url.split('?')
        .nth(1)?
        .split('&')
        .find_map(|p| p.strip_prefix("quantity="))
}

#[test]
fn replacement_uses_remaining_after_snapshot_fills_and_authoritative_cancel_ack() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        amend(&mut h, variant);
        let cancel = h.take(path(variant));
        assert!(h.progress.is_empty(), "replacement is not yet dispatched");
        assert_eq!(cancel.method, Method::Delete);
        h.reply(cancel.id, 200, "", &ack(variant, 12, "3", "CANCELED"));
        let place = h.take(path(variant));
        assert_eq!(place.method, Method::Post);
        assert_eq!(h.progress.len(), 1);
        assert_eq!(h.progress[0].0, 91);
        assert_eq!(h.progress[0].1.len(), 1);
        assert!(place.url.contains(&h.progress[0].1[0].identity));
        assert_eq!(
            h.progress_at_request
                .iter()
                .find(|(id, _)| *id == place.id)
                .unwrap()
                .1,
            1
        );
        assert_eq!(quantity(&place.url), Some("4"), "{variant:?}");
        assert_eq!(h.engine.store().order(id(variant)).unwrap().filled, 3.0);
    }
}
#[test]
fn racing_fill_recomputes_remaining_and_closed_wins_over_late_cancel() {
    for variant in [Variant::Spot, Variant::CoinM] {
        for (filled, expected) in [("4", Some("3")), ("7", None)] {
            let mut h = ready(variant);
            amend(&mut h, variant);
            let cancel = h.take(path(variant));
            fill(
                &mut h,
                variant,
                filled,
                if filled == "7" {
                    "FILLED"
                } else {
                    "PARTIALLY_FILLED"
                },
            );
            h.reply(cancel.id, 200, "", &ack(variant, 12, "2", "CANCELED"));
            if let Some(expected) = expected {
                assert_eq!(quantity(&h.take(path(variant)).url), Some(expected));
            } else {
                assert!(h.requests.is_empty());
                assert_eq!(
                    h.engine.store().order(id(variant)).unwrap().status,
                    OrderStatus::Closed
                );
                assert!(h.result(91).unwrap().is_ok());
            }
        }
    }
}
#[test]
fn missing_rejected_mismatched_or_unknown_cancel_never_places_a_replacement() {
    for variant in [Variant::Spot, Variant::CoinM] {
        for body in [
            "{}".to_owned(),
            ack(variant, 999, "2", "CANCELED"),
            ack(variant, 12, "2", "NEW"),
            ack(variant, 12, "2", "CANCELED").replace("BTC", "ETH"),
            r#"{"code":-2010,"msg":"rejected"}"#.to_owned(),
            r#"{"code":-2011,"msg":"unknown order"}"#.to_owned(),
        ] {
            let mut h = ready(variant);
            amend(&mut h, variant);
            let cancel = h.take(path(variant));
            h.reply(cancel.id, 200, "", &body);
            assert!(h.only_reconcile_reads(), "{variant:?} {body}");
            assert!(h.result(91).unwrap().is_err(), "{variant:?} {body}");
            assert_eq!(
                h.engine.store().order(id(variant)).unwrap().status,
                OrderStatus::Open
            );
        }
    }
}
#[test]
fn cancel_ack_filled_is_authoritative_without_a_stream_execution() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        amend(&mut h, variant);
        let cancel = h.take(path(variant));
        h.reply(cancel.id, 200, "", &ack(variant, 12, "7", "FILLED"));
        assert!(h.requests.is_empty());
        assert_eq!(
            h.engine.store().order(id(variant)).unwrap().status,
            OrderStatus::Closed
        );
        assert_eq!(h.engine.store().order(id(variant)).unwrap().filled, 7.0);
    }
}
#[test]
fn inverse_batch_cancel_marks_only_confirmed_rows_and_reports_short_or_mixed_results() {
    for mixed in [true, false] {
        let mut h = ready(Variant::CoinM);
        h.event(Event::Command(Command::CancelOrders {
            command: 92,
            ids: vec!["12".into(), "13".into()],
        }));
        let cancel = h.take("/dapi/v1/batchOrders");
        let body = format!(
            "[{}{}]",
            ack(Variant::CoinM, 12, "3", "CANCELED"),
            if mixed {
                r#",{"code":-2010,"msg":"cancel rejected"}"#
            } else {
                ""
            }
        );
        h.reply(cancel.id, 200, "", &body);
        assert_eq!(
            h.engine.store().order("12").unwrap().status,
            OrderStatus::Canceled
        );
        assert_eq!(
            h.engine.store().order("13").unwrap().status,
            OrderStatus::Open
        );
        assert!(h.result(92).unwrap().is_err());
        assert!(h.only_reconcile_reads());
    }
}
#[test]
fn cancel_all_validates_its_distinct_response_and_preserves_filled_rows() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let symbol = if variant == Variant::Spot {
            "BTCUSDT"
        } else {
            "BTCUSD_PERP"
        };
        for valid in [true, false] {
            let mut h = ready(variant);
            fill(&mut h, variant, "7", "FILLED");
            h.event(Event::Command(Command::CancelSymbolOrders {
                command: 93,
                symbol: symbol.into(),
            }));
            let cancel = h.take(if variant == Variant::Spot {
                "/api/v3/openOrders"
            } else {
                "/dapi/v1/allOpenOrders"
            });
            let body = if !valid {
                "{}"
            } else if variant == Variant::Spot {
                "[]"
            } else {
                r#"{"code":200,"msg":"success"}"#
            };
            h.reply(cancel.id, 200, "", body);
            assert_eq!(
                h.engine.store().order(id(variant)).unwrap().status,
                OrderStatus::Closed
            );
            let second = if variant == Variant::Spot {
                "second"
            } else {
                "13"
            };
            assert_eq!(
                h.engine.store().order(second).unwrap().status,
                if valid {
                    OrderStatus::Canceled
                } else {
                    OrderStatus::Open
                }
            );
            assert_eq!(h.result(93).unwrap().is_ok(), valid);
        }
    }
}
#[test]
fn inverse_full_close_stop_price_amend_recreates_a_zero_quantity_close_position_stop() {
    let mut h = ready(Variant::CoinM);
    h.message("/ws/coin-key", r#"{"e":"ORDER_TRADE_UPDATE","o":{"i":14,"s":"BTCUSD_PERP","S":"SELL","o":"STOP_MARKET","X":"NEW","p":"0","sp":"8000","q":"0","z":"0","cp":true,"wt":"MARK_PRICE"}}"#);
    h.event(Event::Command(Command::AmendOrder {
        command: 94,
        id: "14".into(),
        price: Some(8100.0),
        amount: None,
    }));
    let cancel = h.take("/dapi/v1/order");
    let body =
        ack(Variant::CoinM, 14, "0", "CANCELED").replace(r#""origQty":"7""#, r#""origQty":"0""#);
    h.reply(cancel.id, 200, "", &body);
    let place = h.take("/dapi/v1/order");
    assert!(place.url.contains("closePosition=true"));
    assert!(place.url.contains("stopPrice=8100"));
    assert!(!place.url.contains("quantity="));
}

#[test]
fn removed_original_order_fences_amendment_instead_of_replacing_the_original_total() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        amend(&mut h, variant);
        let cancel = h.take(path(variant));
        h.engine.store_mut().set_orders(Vec::new());
        h.reply(cancel.id, 200, "", &ack(variant, 12, "3", "CANCELED"));
        assert!(h.requests.is_empty());
        assert!(h.result(91).unwrap().is_err());
    }
}
fn reuse_spot_id(h: &mut Harness) {
    h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":15,"s":"BTCUSDT","c":"maker","S":"BUY","o":"LIMIT","X":"NEW","x":"NEW","p":"9200","q":"8","z":"0","l":"0"}}"#);
}
#[test]
fn reused_spot_client_id_is_not_canceled_or_amended_by_an_old_native_ack() {
    let mut h = ready(Variant::Spot);
    amend(&mut h, Variant::Spot);
    let cancel = h.take("/api/v3/order");
    reuse_spot_id(&mut h);
    h.reply(cancel.id, 200, "", &ack(Variant::Spot, 12, "3", "CANCELED"));
    assert!(h.only_reconcile_reads());
    assert!(h.result(91).unwrap().is_err());
    let order = h.engine.store().order("maker").unwrap();
    assert_eq!(
        (order.status, order.amount, order.filled),
        (OrderStatus::Open, 8.0, 0.0)
    );
}
#[test]
fn spot_cancel_all_preserves_a_new_incarnation_outside_its_original_native_targets() {
    for body in [
        "[]".to_owned(),
        format!("[{}]", ack(Variant::Spot, 12, "3", "CANCELED")),
    ] {
        let mut h = ready(Variant::Spot);
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 95,
            symbol: "BTCUSDT".into(),
        }));
        let cancel = h.take("/api/v3/openOrders");
        reuse_spot_id(&mut h);
        h.reply(cancel.id, 200, "", &body);
        let order = h.engine.store().order("maker").unwrap();
        assert_eq!(
            (order.status, order.amount, order.filled),
            (OrderStatus::Open, 8.0, 0.0)
        );
        assert!(h.result(95).unwrap().is_ok());
    }
}

#[test]
fn concurrent_amendment_cannot_cancel_and_replace_the_same_original_twice() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        amend(&mut h, variant);
        h.event(Event::Command(Command::AmendOrder {
            command: 96,
            id: id(variant).into(),
            price: Some(10002.0),
            amount: None,
        }));
        assert_eq!(h.requests.len(), 1);
        assert!(h.result(96).unwrap().is_err());
        assert!(h.result(91).is_none());
    }
}

#[test]
fn cancel_all_ack_fences_an_open_order_snapshot_issued_before_the_cancel() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        fill(&mut h, variant, "3", "PARTIALLY_FILLED");
        h.advance(10_000.0);
        let open_path = if variant == Variant::Spot {
            "/api/v3/openOrders"
        } else {
            "/dapi/v1/openOrders"
        };
        let read = h.take(open_path);
        let symbol = if variant == Variant::Spot {
            "BTCUSDT"
        } else {
            "BTCUSD_PERP"
        };
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 97,
            symbol: symbol.into(),
        }));
        let cancel = h.take(if variant == Variant::Spot {
            open_path
        } else {
            "/dapi/v1/allOpenOrders"
        });
        h.reply(
            cancel.id,
            200,
            "",
            if variant == Variant::Spot {
                "[]"
            } else {
                r#"{"code":200,"msg":"success"}"#
            },
        );
        h.reply(read.id, 200, "", &format!(r#"[{{"symbol":"{symbol}","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"3","type":"LIMIT","side":"BUY"}}]"#));
        assert_eq!(
            h.engine.store().order(id(variant)).unwrap().status,
            OrderStatus::Canceled
        );
    }
}

fn spot_order_list(report: &str) -> String {
    format!(
        r#"{{"orderListId":1929,"symbol":"BTCUSDT","listStatusType":"ALL_DONE","listOrderStatus":"ALL_DONE","orders":[{{"symbol":"BTCUSDT","orderId":13,"clientOrderId":"second"}}],"orderReports":[{report}]}}"#
    )
}
#[test]
fn spot_cancel_all_accepts_mixed_flat_and_order_list_acknowledgements() {
    let mut h = ready(Variant::Spot);
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 98,
        symbol: "BTCUSDT".into(),
    }));
    let cancel = h.take("/api/v3/openOrders");
    let body = format!(
        "[{},{}]",
        ack(Variant::Spot, 12, "2", "CANCELED"),
        spot_order_list(&ack(Variant::Spot, 13, "3", "CANCELED"))
    );
    h.reply(cancel.id, 200, "", &body);
    assert!(h.result(98).unwrap().is_ok());
    assert_eq!(
        h.engine.store().order("maker").unwrap().status,
        OrderStatus::Canceled
    );
    let second = h.engine.store().order("second").unwrap();
    assert_eq!((second.status, second.filled), (OrderStatus::Canceled, 3.0));
}
#[test]
fn malformed_nested_spot_cancel_all_reply_cannot_partially_mutate_a_flat_row() {
    for list in [
        spot_order_list(&ack(Variant::Spot, 999, "2", "CANCELED")),
        spot_order_list(&ack(Variant::Spot, 13, "2", "NEW")),
        spot_order_list(&ack(Variant::Spot, 13, "2", "CANCELED"))
            .replace("orderReports", "missingReports"),
    ] {
        let mut h = ready(Variant::Spot);
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 99,
            symbol: "BTCUSDT".into(),
        }));
        let cancel = h.take("/api/v3/openOrders");
        h.reply(
            cancel.id,
            200,
            "",
            &format!("[{},{}]", ack(Variant::Spot, 12, "2", "CANCELED"), list),
        );
        assert!(h.result(99).unwrap().is_err());
        assert_eq!(
            h.engine.store().order("maker").unwrap().status,
            OrderStatus::Open
        );
        assert_eq!(
            h.engine.store().order("second").unwrap().status,
            OrderStatus::Open
        );
        assert!(h.only_reconcile_reads());
    }
}

#[test]
fn cancel_all_tombstone_rejects_a_later_replica_and_keeps_late_executions_without_reopening() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        let symbol = if variant == Variant::Spot {
            "BTCUSDT"
        } else {
            "BTCUSD_PERP"
        };
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 100,
            symbol: symbol.into(),
        }));
        let cancel = h.take(if variant == Variant::Spot {
            "/api/v3/openOrders"
        } else {
            "/dapi/v1/allOpenOrders"
        });
        h.reply(
            cancel.id,
            200,
            "",
            if variant == Variant::Spot {
                "[]"
            } else {
                r#"{"code":200}"#
            },
        );
        fill(&mut h, variant, "3", "PARTIALLY_FILLED");
        fill(&mut h, variant, "3", "PARTIALLY_FILLED");
        let order = h.engine.store().order(id(variant)).unwrap();
        assert_eq!((order.status, order.filled), (OrderStatus::Canceled, 3.0));
        h.advance(10_000.0);
        let read = h.take(if variant == Variant::Spot {
            "/api/v3/openOrders"
        } else {
            "/dapi/v1/openOrders"
        });
        h.reply(read.id, 200, "", &format!(r#"[{{"symbol":"{symbol}","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"3","type":"LIMIT","side":"BUY"}}]"#));
        assert_eq!(
            h.engine.store().order(id(variant)).unwrap().status,
            OrderStatus::Canceled
        );
        fill(&mut h, variant, "7", "FILLED");
        assert_eq!(
            h.engine.store().order(id(variant)).unwrap().status,
            OrderStatus::Closed
        );
    }
}

#[test]
fn unquantified_old_spot_execution_emits_once_without_mutating_a_reused_client_order() {
    let mut h = ready(Variant::Spot);
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 101,
        symbol: "BTCUSDT".into(),
    }));
    let cancel = h.take("/api/v3/openOrders");
    h.reply(cancel.id, 200, "", "[]");
    reuse_spot_id(&mut h);
    let _ = h.engine.take_frame(h.now);
    fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
    fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
    let current = h.engine.store().order("maker").unwrap();
    assert_eq!(
        (current.status, current.amount, current.filled),
        (OrderStatus::Open, 8.0, 0.0)
    );
    let frame = h.engine.take_frame(h.now).unwrap();
    let mut at = 16;
    let mut fills = 0;
    for _ in 0..u16::from_le_bytes(frame[6..8].try_into().unwrap()) {
        if u16::from_le_bytes(frame[at..at + 2].try_into().unwrap()) == 11 {
            fills += u32::from_le_bytes(frame[at + 8..at + 12].try_into().unwrap());
        }
        at += 8 + u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
    }
    assert_eq!(fills, 1);
}

#[test]
fn original_order_cannot_be_amended_again_while_its_replacement_post_is_in_flight() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant);
        amend(&mut h, variant);
        let cancel = h.take(path(variant));
        h.reply(cancel.id, 200, "", &ack(variant, 12, "3", "CANCELED"));
        assert_eq!(h.requests.len(), 1);
        assert_eq!(h.requests[0].method, Method::Post);
        h.event(Event::Command(Command::AmendOrder {
            command: 102,
            id: id(variant).into(),
            price: Some(10002.0),
            amount: None,
        }));
        assert_eq!(h.requests.len(), 1);
        assert!(h.result(102).unwrap().is_err());
        assert!(h.result(91).is_none());
    }
}

#[test]
fn inverse_closed_order_stays_closed_after_unknown_cancel_and_delayed_partial() {
    let variant = Variant::CoinM;
    let mut h = ready(variant);
    fill(&mut h, variant, "7", "FILLED");
    h.event(Event::Command(Command::CancelOrders {
        command: 97,
        ids: vec![id(variant).into()],
    }));
    let cancel = h.take(path(variant));
    h.reply(
        cancel.id,
        400,
        "",
        r#"{"code":-2011,"msg":"Unknown order sent."}"#,
    );
    fill(&mut h, variant, "4", "PARTIALLY_FILLED");
    let order = h.engine.store().order(id(variant)).unwrap();
    assert_eq!(
        (order.status, order.filled, order.remaining),
        (OrderStatus::Closed, 7.0, 0.0)
    );
}

#[test]
fn spot_cancel_all_keeps_cross_symbol_client_reuse_and_fences_the_old_native() {
    for empty_ack in [false, true] {
        let mut h = ready(Variant::Spot);
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 98,
            symbol: "BTCUSDT".into(),
        }));
        let cancel = h.take("/api/v3/openOrders");
        h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":44,"s":"ETHUSDT","c":"maker","S":"BUY","o":"LIMIT","X":"NEW","x":"NEW","p":"100","L":"0","q":"9","z":"0","l":"0"}}"#);
        let before = h.engine.store().order("maker").unwrap().clone();
        assert_eq!(before.symbol, "ETHUSDT");
        h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":12,"s":"BTCUSDT","c":"maker","S":"BUY","o":"LIMIT","X":"NEW","x":"NEW","p":"9000","L":"0","q":"7","z":"2","l":"0"}}"#);
        assert_eq!(h.engine.store().order("maker"), Some(&before));
        let reply = if empty_ack {
            "[]".to_owned()
        } else {
            format!(
                "[{},{}]",
                ack(Variant::Spot, 12, "3", "CANCELED"),
                ack(Variant::Spot, 13, "2", "CANCELED")
            )
        };
        h.reply(cancel.id, 200, "", &reply);
        assert!(h.result(98).unwrap().is_ok());
        assert_eq!(h.engine.store().order("maker"), Some(&before));
        assert_eq!(
            h.engine.store().order("second").unwrap().status,
            OrderStatus::Canceled
        );
        let _ = h.engine.take_frame(h.now);
        fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
        fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
        assert_eq!(h.engine.store().order("maker"), Some(&before));
        let frame = h.engine.take_frame(h.now);
        let count = frame.map_or(0, |frame| {
            safe_cex_engine::wire::sections(frame)
                .find(|(kind, _)| *kind == 11)
                .map_or(0, |(_, payload)| {
                    u32::from_le_bytes(payload[..4].try_into().unwrap())
                })
        });
        assert_eq!(
            count, 1,
            "quantified and unquantified ACKs preserve delivery"
        );
        h.advance(10_000.0);
        h.respond("/api/v3/openOrders", 200, r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED"}]"#);
        assert_eq!(h.engine.store().order("maker"), Some(&before));
    }
}

#[test]
fn historical_spot_lifecycle_before_trade_keeps_delivery_progress_and_the_new_row() {
    let emitted = |h: &mut Harness| {
        let Some(frame) = h.engine.take_frame(h.now) else {
            return Vec::new();
        };
        let Some((_, payload)) =
            safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
        else {
            return Vec::new();
        };
        let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
        let start = (4 + 12 * count).next_multiple_of(8);
        (0..count)
            .map(|index| {
                let at = start + 16 * index;
                (
                    f64::from_le_bytes(payload[at..at + 8].try_into().unwrap()),
                    f64::from_le_bytes(payload[at + 8..at + 16].try_into().unwrap()),
                )
            })
            .collect::<Vec<_>>()
    };
    for ack_first in [false, true] {
        for same_symbol in [false, true] {
            let mut h = ready(Variant::Spot);
            h.event(Event::Command(Command::CancelSymbolOrders {
                command: 103,
                symbol: "BTCUSDT".into(),
            }));
            let cancel = h.take("/api/v3/openOrders");
            if same_symbol {
                reuse_spot_id(&mut h);
            } else {
                h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":44,"s":"ETHUSDT","c":"maker","S":"BUY","o":"LIMIT","X":"NEW","x":"NEW","p":"100","L":"0","q":"9","z":"0","l":"0"}}"#);
            }
            let before = h.engine.store().order("maker").unwrap().clone();
            let _ = emitted(&mut h);
            if ack_first {
                h.reply(cancel.id, 200, "", "[]");
            }
            // The cumulative cancellation total includes a trade whose actual
            // execution report arrives later; this lifecycle frame is not a fill.
            h.message("ws-api/v3", r#"{"subscriptionId":0,"event":{"e":"executionReport","i":12,"s":"BTCUSDT","c":"cancel-request","C":"maker","S":"BUY","o":"LIMIT","X":"CANCELED","x":"CANCELED","p":"9000","L":"0","q":"7","z":"3","l":"0"}}"#);
            assert!(emitted(&mut h).is_empty());
            assert_eq!(h.engine.store().order("maker"), Some(&before));
            fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
            fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
            assert_eq!(
                emitted(&mut h),
                vec![(9100.0, 1.0)],
                "ack_first={ack_first} same_symbol={same_symbol}"
            );
            if !ack_first {
                h.reply(cancel.id, 200, "", "[]");
            }
            fill(&mut h, Variant::Spot, "3", "PARTIALLY_FILLED");
            assert!(emitted(&mut h).is_empty());
            assert_eq!(h.engine.store().order("maker"), Some(&before));
            assert!(h.result(103).unwrap().is_ok());
            h.advance(10_000.0);
            h.respond("/api/v3/openOrders", 200, r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"9000","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED"}]"#);
            assert_eq!(h.engine.store().order("maker"), Some(&before));
        }
    }
}

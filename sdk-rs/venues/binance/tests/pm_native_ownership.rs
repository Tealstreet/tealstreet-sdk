//! PM source ownership and business-unit isolation from private stream contracts.
use support::Options;
mod support;

use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::Harness;

fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    let pm = variant == Variant::PortfolioMargin;
    h.respond(
        if pm {
            "/papi/v1/listenKey"
        } else {
            "/fapi/v1/listenKey"
        },
        200,
        r#"{"listenKey":"owner-key"}"#,
    );
    let socket = h.socket(if pm { "/pm/ws" } else { "/private/ws" }).unwrap();
    h.event(Event::WsOpened { socket });
    h.respond(
        if pm {
            "/papi/v1/um/openOrders"
        } else {
            "/fapi/v1/openOrders"
        },
        200,
        "[]",
    );
    h.respond(
        if pm {
            "/papi/v1/um/conditional/openOrders"
        } else {
            "/fapi/v1/openAlgoOrders"
        },
        200,
        "[]",
    );
    h
}

fn regular(symbol: &str, native: u64, status: &str, trade: bool) -> String {
    format!(
        r#"{{"e":"ORDER_TRADE_UPDATE","o":{{"s":"{symbol}","c":"shared","i":{native},"S":"BUY","o":"LIMIT","ot":"LIMIT","f":"GTC","q":"9","p":"101","x":"{}","X":"{status}","l":"{}","z":"{}","L":"102","t":77}}}}"#,
        if trade { "TRADE" } else { status },
        if trade { "2" } else { "0" },
        if trade && status == "FILLED" {
            "9"
        } else if trade {
            "2"
        } else {
            "0"
        }
    )
}

fn strategy(symbol: &str, native: u64, status: &str) -> String {
    format!(
        r#"{{"e":"CONDITIONAL_ORDER_TRADE_UPDATE","so":{{"s":"{symbol}","c":"shared","si":{native},"S":"SELL","st":"STOP_MARKET","os":"{status}","q":"3","p":"0","sp":"90","R":true}}}}"#
    )
}

fn snapshot(h: &mut Harness, regular: &str, strategy: &str) {
    h.advance(15_000.0);
    h.respond("/papi/v1/um/openOrders", 200, regular);
    h.respond("/papi/v1/um/conditional/openOrders", 200, strategy);
}
fn regular_rows(symbol: &str, native: u64) -> String {
    format!(
        r#"[{{"symbol":"{symbol}","orderId":{native},"clientOrderId":"shared","price":"101","origQty":"9","executedQty":"0","type":"LIMIT","side":"BUY"}}]"#
    )
}
fn strategy_rows(symbol: &str, native: u64) -> String {
    format!(
        r#"[{{"symbol":"{symbol}","strategyId":{native},"newClientStrategyId":"shared","price":"0","origQty":"3","strategyType":"STOP_MARKET","side":"SELL","stopPrice":"90"}}]"#
    )
}
fn fills(h: &mut Harness) -> Vec<(f64, f64)> {
    let Some(frame) = h.engine.take_frame(h.now) else {
        return Vec::new();
    };
    let Some((_, data)) = safe_cex_engine::wire::sections(frame).find(|(kind, _)| *kind == 11)
    else {
        return Vec::new();
    };
    let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
    let start = (4 + count * 12).next_multiple_of(8);
    (0..count)
        .map(|i| {
            let at = start + i * 16;
            (
                f64::from_le_bytes(data[at..at + 8].try_into().unwrap()),
                f64::from_le_bytes(data[at + 8..at + 16].try_into().unwrap()),
            )
        })
        .collect()
}

#[test]
fn explicit_cm_account_events_do_not_create_or_modify_pm_positions() {
    for variant in [Variant::PortfolioMargin, Variant::UsdM] {
        let mut h = ready(variant);
        let path = if variant == Variant::PortfolioMargin {
            "/pm/ws"
        } else {
            "/private/ws"
        };
        for (family, amount) in [(Some("CM"), 5), (Some("UM"), 2), (None, 3), (Some("CM"), 7)] {
            let fs = family.map_or(String::new(), |f| format!(r#", "fs":"{f}""#));
            h.message(path, &format!(r#"{{"e":"ACCOUNT_UPDATE"{fs},"a":{{"P":[{{"s":"BTCUSDT","pa":"{amount}","ep":"100","up":"1","ps":"BOTH"}}]}}}}"#));
            if variant == Variant::PortfolioMargin && family == Some("CM") {
                if amount == 5 {
                    assert!(h.engine.store().positions().is_empty());
                } else {
                    assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
                }
            } else {
                assert_eq!(h.engine.store().positions()[0].contracts, f64::from(amount));
            }
        }
    }
}

#[test]
fn cross_family_terminal_replays_preserve_the_new_owner_and_deliver_old_fills() {
    for strategy_first in [false, true] {
        for new_symbol in ["BTCUSDT", "ETHUSDT"] {
            for new_native in [42, 78] {
                for old_terminal_seen in [false, true] {
                    let mut h = ready(Variant::PortfolioMargin);
                    let old = if strategy_first {
                        strategy("BTCUSDT", 42, "NEW")
                    } else {
                        regular("BTCUSDT", 42, "NEW", false)
                    };
                    h.message("/pm/ws", &old);
                    if old_terminal_seen {
                        h.message(
                            "/pm/ws",
                            &if strategy_first {
                                strategy("BTCUSDT", 42, "CANCELED")
                            } else {
                                regular("BTCUSDT", 42, "CANCELED", false)
                            },
                        );
                    }
                    // NEW establishes the new native source even if the old terminal event is delayed.
                    h.message(
                        "/pm/ws",
                        &if strategy_first {
                            regular(new_symbol, new_native, "NEW", false)
                        } else {
                            strategy(new_symbol, new_native, "NEW")
                        },
                    );
                    let current = h.engine.store().order("shared").unwrap().clone();
                    fills(&mut h);
                    for status in ["CANCELED", "NEW", "FILLED", "TRIGGERED", "EXPIRED"] {
                        let old = if strategy_first {
                            strategy("BTCUSDT", 42, status)
                        } else {
                            regular("BTCUSDT", 42, status, status == "FILLED")
                        };
                        h.message("/pm/ws", &old);
                        h.message("/pm/ws", &old);
                        assert_eq!(
                            h.engine.store().order("shared"),
                            Some(&current),
                            "strategy_first={strategy_first}, symbol={new_symbol}, native={new_native}, status={status}"
                        );
                    }
                    assert_eq!(
                        fills(&mut h),
                        if strategy_first {
                            vec![]
                        } else {
                            vec![(102.0, 2.0)]
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn snapshot_source_binding_fences_old_cross_family_streams_and_replicas() {
    for strategy_first in [false, true] {
        let mut h = ready(Variant::PortfolioMargin);
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "NEW")
            } else {
                regular("BTCUSDT", 42, "NEW", false)
            },
        );
        let rows = if strategy_first {
            regular_rows("BTCUSDT", 78)
        } else {
            strategy_rows("BTCUSDT", 78)
        };
        snapshot(
            &mut h,
            if strategy_first { &rows } else { "[]" },
            if strategy_first { "[]" } else { &rows },
        );
        let current = h.engine.store().order("shared").unwrap().clone();
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "CANCELED")
            } else {
                regular("BTCUSDT", 42, "CANCELED", false)
            },
        );
        assert_eq!(h.engine.store().order("shared"), Some(&current));
        let old = if strategy_first {
            strategy_rows("BTCUSDT", 42)
        } else {
            regular_rows("BTCUSDT", 42)
        };
        snapshot(
            &mut h,
            if strategy_first { "[]" } else { &old },
            if strategy_first { &old } else { "[]" },
        );
        assert_eq!(h.engine.store().order("shared"), Some(&current));
    }
}

#[test]
fn triggered_regular_stop_uses_regular_cancel_contract_and_accepts_late_trades() {
    let mut h = ready(Variant::PortfolioMargin);
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "NEW"));
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "TRIGGERED"));
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Closed
    );
    let triggered =
        regular("BTCUSDT", 78, "NEW", false).replace(r#""ot":"LIMIT""#, r#""ot":"STOP_MARKET""#);
    h.message("/pm/ws", &triggered);
    h.event(Event::Command(Command::CancelOrders {
        command: 80,
        ids: vec!["shared".into()],
    }));
    let cancel = h.take("/papi/v1/um/order?");
    assert_eq!(cancel.method, Method::Delete);
    assert!(cancel.url.contains("origClientOrderId=shared"));
    h.reply(cancel.id, 200, "", r#"{"symbol":"BTCUSDT","orderId":78,"clientOrderId":"shared","origQty":"9","executedQty":"0","status":"CANCELED"}"#);
    assert!(h.result(80).unwrap().is_ok());
    let fill = regular("BTCUSDT", 78, "PARTIALLY_FILLED", true)
        .replace(r#""ot":"LIMIT""#, r#""ot":"STOP_MARKET""#);
    h.message("/pm/ws", &fill);
    assert_eq!(fills(&mut h), [(102.0, 2.0)]);
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn captured_cancel_replies_preserve_a_cross_family_owner() {
    for strategy_first in [false, true] {
        for bulk in [false, true] {
            for unknown in [false, true] {
                for new_symbol in ["BTCUSDT", "ETHUSDT"] {
                    let mut h = ready(Variant::PortfolioMargin);
                    h.message(
                        "/pm/ws",
                        &if strategy_first {
                            strategy("BTCUSDT", 42, "NEW")
                        } else {
                            regular("BTCUSDT", 42, "NEW", false)
                        },
                    );
                    h.event(Event::Command(if bulk {
                        Command::CancelSymbolOrders {
                            command: 90,
                            symbol: "BTCUSDT".into(),
                        }
                    } else {
                        Command::CancelOrders {
                            command: 90,
                            ids: vec!["shared".into()],
                        }
                    }));
                    let regular_cancel = (!strategy_first || bulk).then(|| {
                        h.take(if bulk {
                            "/papi/v1/um/allOpenOrders"
                        } else {
                            "/papi/v1/um/order?"
                        })
                    });
                    let strategy_cancel = (strategy_first || bulk).then(|| {
                        h.take(if bulk {
                            "/papi/v1/um/conditional/allOpenOrders"
                        } else {
                            "/papi/v1/um/conditional/order?"
                        })
                    });
                    h.message(
                        "/pm/ws",
                        &if strategy_first {
                            regular(new_symbol, 78, "NEW", false)
                        } else {
                            strategy(new_symbol, 78, "NEW")
                        },
                    );
                    let current = h.engine.store().order("shared").unwrap().clone();
                    for (cancel, strategy_reply) in
                        [(regular_cancel, false), (strategy_cancel, true)]
                    {
                        if let Some(cancel) = cancel {
                            let body = if unknown {
                                r#"{"code":-2011,"msg":"Unknown order sent."}"#
                            } else if bulk {
                                r#"{"code":200,"msg":"success"}"#
                            } else if strategy_reply {
                                r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"shared","strategyStatus":"CANCELED"}"#
                            } else {
                                r#"{"symbol":"BTCUSDT","orderId":42,"clientOrderId":"shared","origQty":"9","executedQty":"0","status":"CANCELED"}"#
                            };
                            h.reply(cancel.id, if unknown { 400 } else { 200 }, "", body);
                            assert_eq!(h.engine.store().order("shared"), Some(&current));
                        }
                    }
                    let rows = if strategy_first {
                        regular_rows(new_symbol, 78)
                    } else {
                        strategy_rows(new_symbol, 78)
                    };
                    snapshot(
                        &mut h,
                        if strategy_first { &rows } else { "[]" },
                        if strategy_first { "[]" } else { &rows },
                    );
                    let order = h.engine.store().order("shared").unwrap();
                    assert_eq!(order.status, OrderStatus::Open);
                    assert_eq!(order.symbol, new_symbol);
                }
            }
        }
    }
}

#[test]
fn old_amend_cancellation_cannot_authorize_cross_family_replacement() {
    for strategy_first in [false, true] {
        let mut h = ready(Variant::PortfolioMargin);
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "NEW")
            } else {
                regular("BTCUSDT", 42, "NEW", false)
            },
        );
        h.event(Event::Command(Command::AmendOrder {
            command: 92,
            id: "shared".into(),
            price: Some(91.0),
            amount: None,
        }));
        let cancel = h.take(if strategy_first {
            "/papi/v1/um/conditional/order?"
        } else {
            "/papi/v1/um/order?"
        });
        h.message(
            "/pm/ws",
            &if strategy_first {
                regular("BTCUSDT", 78, "NEW", false)
            } else {
                strategy("BTCUSDT", 78, "NEW")
            },
        );
        let current = h.engine.store().order("shared").unwrap().clone();
        h.reply(cancel.id, 200, "", if strategy_first { r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"shared","strategyStatus":"CANCELED"}"# } else { r#"{"symbol":"BTCUSDT","orderId":42,"clientOrderId":"shared","origQty":"9","executedQty":"0","status":"CANCELED"}"# });
        assert_eq!(h.engine.store().order("shared"), Some(&current));
        assert!(h.result(92).unwrap().is_err());
        assert!(
            !h.requests
                .iter()
                .any(|request| request.method == Method::Post)
        );
    }
}

#[test]
fn missing_native_strategy_identity_cannot_retire_a_regular_owner() {
    let mut h = ready(Variant::PortfolioMargin);
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "NEW"));
    h.message("/pm/ws", &regular("BTCUSDT", 78, "NEW", false));
    let current = h.engine.store().order("shared").unwrap().clone();
    let missing = strategy("BTCUSDT", 42, "CANCELED").replace(",\"si\":42", "");
    assert!(!missing.contains("\"si\""));
    h.message("/pm/ws", &missing);
    assert_eq!(h.engine.store().order("shared"), Some(&current));
    let rows = strategy_rows("BTCUSDT", 99).replace(",\"strategyId\":99", "");
    snapshot(&mut h, "[]", &rows);
    assert_eq!(h.engine.store().order("shared"), Some(&current));
}

#[test]
fn ambiguous_or_in_flight_snapshot_cannot_partially_change_ownership() {
    for in_flight in [false, true] {
        let mut h = ready(Variant::PortfolioMargin);
        h.message("/pm/ws", &regular("BTCUSDT", 78, "NEW", false));
        h.advance(15_000.0);
        if in_flight {
            h.message("/pm/ws", &regular("BTCUSDT", 79, "NEW", false));
        }
        let current = h.engine.store().order("shared").unwrap().clone();
        h.respond("/papi/v1/um/openOrders", 200, &regular_rows("BTCUSDT", 80));
        h.respond(
            "/papi/v1/um/conditional/openOrders",
            200,
            &strategy_rows("BTCUSDT", 43),
        );
        assert_eq!(h.engine.store().order("shared"), Some(&current));
        // Rejected snapshots must not consume or retire the unseen strategy candidate.
        h.message("/pm/ws", &strategy("BTCUSDT", 43, "NEW"));
        assert_eq!(
            h.engine.store().order("shared").unwrap().side,
            safe_cex_engine::model::OrderSide::Sell
        );
    }
}

#[test]
fn trigger_trade_before_new_is_delivered_and_snapshot_establishes_regular_ownership() {
    let mut h = ready(Variant::PortfolioMargin);
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "NEW"));
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "TRIGGERED"));
    let fill = regular("BTCUSDT", 78, "PARTIALLY_FILLED", true)
        .replace(r#""ot":"LIMIT""#, r#""ot":"STOP_MARKET""#);
    h.message("/pm/ws", &fill);
    h.message("/pm/ws", &fill);
    assert_eq!(fills(&mut h), [(102.0, 2.0)]);
    let rows = regular_rows("BTCUSDT", 78)
        .replace(r#""executedQty":"0""#, r#""executedQty":"2""#)
        .replace(r#""type":"LIMIT""#, r#""type":"STOP_MARKET""#);
    snapshot(&mut h, &rows, "[]");
    assert_eq!(h.engine.store().order("shared").unwrap().filled, 2.0);
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Open
    );
    let current = h.engine.store().order("shared").unwrap().clone();
    h.message("/pm/ws", &strategy("BTCUSDT", 42, "FINISHED"));
    assert_eq!(h.engine.store().order("shared"), Some(&current));
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 93,
        symbol: "BTCUSDT".into(),
    }));
    let regular_cancel = h.take("/papi/v1/um/allOpenOrders");
    let strategy_cancel = h.take("/papi/v1/um/conditional/allOpenOrders");
    h.reply(strategy_cancel.id, 200, "", r#"{"code":200}"#);
    assert_eq!(h.engine.store().order("shared"), Some(&current));
    h.reply(regular_cancel.id, 200, "", r#"{"code":200}"#);
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn removed_foreign_symbol_owner_still_fences_old_captured_cancel() {
    let mut h = ready(Variant::PortfolioMargin);
    h.message("/pm/ws", &regular("BTCUSDT", 42, "NEW", false));
    h.event(Event::Command(Command::CancelOrders {
        command: 94,
        ids: vec!["shared".into()],
    }));
    let cancel = h.take("/papi/v1/um/order?");
    h.message("/pm/ws", &strategy("ETHUSDT", 78, "NEW"));
    snapshot(&mut h, "[]", "[]");
    assert!(h.engine.store().orders().is_empty());
    h.reply(cancel.id, 200, "", r#"{"symbol":"BTCUSDT","orderId":42,"clientOrderId":"shared","origQty":"9","executedQty":"0","status":"CANCELED"}"#);
    assert!(h.result(94).unwrap().is_err());
    h.message("/pm/ws", &strategy("ETHUSDT", 78, "NEW"));
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(h.engine.store().order("shared").unwrap().symbol, "ETHUSDT");
}

#[test]
fn cross_family_ownership_survives_private_socket_reconnect() {
    for strategy_first in [false, true] {
        let mut h = ready(Variant::PortfolioMargin);
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "NEW")
            } else {
                regular("BTCUSDT", 42, "NEW", false)
            },
        );
        h.message(
            "/pm/ws",
            &if strategy_first {
                regular("BTCUSDT", 78, "NEW", false)
            } else {
                strategy("BTCUSDT", 78, "NEW")
            },
        );
        let current = h.engine.store().order("shared").unwrap().clone();
        let old = h.socket("/pm/ws").unwrap();
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        h.advance(500.0);
        h.respond("/papi/v1/listenKey", 200, r#"{"listenKey":"owner-key"}"#);
        let fresh = h.socket("/pm/ws").unwrap();
        assert_ne!(old, fresh);
        h.event(Event::WsOpened { socket: fresh });
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "CANCELED")
            } else {
                regular("BTCUSDT", 42, "CANCELED", false)
            },
        );
        assert_eq!(h.engine.store().order("shared"), Some(&current));
    }
}

#[test]
fn malformed_new_source_cannot_retire_the_current_owner() {
    for strategy_first in [false, true] {
        let mut h = ready(Variant::PortfolioMargin);
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "NEW")
            } else {
                regular("BTCUSDT", 42, "NEW", false)
            },
        );
        let current = h.engine.store().order("shared").unwrap().clone();
        let invalid = if strategy_first {
            regular("BTCUSDT", 78, "NEW", false).replace("LIMIT", "UNKNOWN")
        } else {
            strategy("BTCUSDT", 78, "NEW").replace("STOP_MARKET", "UNKNOWN")
        };
        h.message("/pm/ws", &invalid);
        assert_eq!(h.engine.store().order("shared"), Some(&current));
        h.message(
            "/pm/ws",
            &if strategy_first {
                strategy("BTCUSDT", 42, "CANCELED")
            } else {
                regular("BTCUSDT", 42, "CANCELED", false)
            },
        );
        assert_eq!(
            h.engine.store().order("shared").unwrap().status,
            OrderStatus::Canceled
        );
    }
}

fn invalid_handoff_preserves_owner(strategy_first: bool, field: &str, invalid: &str) {
    let mut h = ready(Variant::PortfolioMargin);
    let old = if strategy_first {
        strategy("BTCUSDT", 42, "NEW")
    } else {
        regular("BTCUSDT", 42, "NEW", false)
    };
    h.message("/pm/ws", &old);
    let current = h.engine.store().order("shared").unwrap().clone();
    let valid = if strategy_first {
        regular("BTCUSDT", 78, "NEW", false)
    } else {
        strategy("BTCUSDT", 78, "NEW")
    };
    let original = match (strategy_first, field) {
        (true, "q") => "9",
        (false, "q") => "3",
        (true, "p") => "101",
        (false, "p") | (true, "z") => "0",
        (false, "sp") => "90",
        _ => panic!("unsupported handoff field"),
    };
    let invalid = valid.replace(
        &format!(r#""{field}":"{original}""#),
        &format!(r#""{field}":"{invalid}""#),
    );
    assert_ne!(invalid, valid);
    h.message("/pm/ws", &invalid);
    assert_eq!(
        h.engine.store().order("shared"),
        Some(&current),
        "invalid {field} must not replace the current owner: {invalid}"
    );
    // Rejecting the malformed candidate must not retire the old native order.
    h.message(
        "/pm/ws",
        &if strategy_first {
            strategy("BTCUSDT", 42, "CANCELED")
        } else {
            regular("BTCUSDT", 42, "CANCELED", false)
        },
    );
    assert_eq!(
        h.engine.store().order("shared").unwrap().status,
        OrderStatus::Canceled
    );
    // A corrected NEW with the same candidate native ID still establishes ownership.
    h.message("/pm/ws", &valid);
    let corrected = h.engine.store().order("shared").unwrap();
    assert_eq!(corrected.status, OrderStatus::Open);
    assert_eq!(corrected.amount, if strategy_first { 9.0 } else { 3.0 });
}

#[test]
fn invalid_new_handoff_quantities_cannot_retire_a_live_native_owner() {
    for strategy_first in [false, true] {
        for invalid in ["bad", "-1", "NaN", "Infinity", "1e999"] {
            invalid_handoff_preserves_owner(strategy_first, "q", invalid);
        }
    }
}

#[test]
fn invalid_new_handoff_prices_cannot_retire_a_live_native_owner() {
    for strategy_first in [false, true] {
        for invalid in ["bad", "-1", "NaN", "Infinity", "1e999"] {
            invalid_handoff_preserves_owner(strategy_first, "p", invalid);
            if !strategy_first {
                invalid_handoff_preserves_owner(strategy_first, "sp", invalid);
            }
        }
    }
}

#[test]
fn new_handoff_execution_cannot_exceed_its_original_quantity() {
    invalid_handoff_preserves_owner(true, "z", "10");
}

#[test]
fn invalid_snapshot_handoff_cannot_retire_the_current_owner() {
    for strategy_first in [false, true] {
        for (field, original, invalid) in [
            ("origQty", if strategy_first { "9" } else { "3" }, "bad"),
            ("origQty", if strategy_first { "9" } else { "3" }, "-1"),
            ("origQty", if strategy_first { "9" } else { "3" }, "1e999"),
            ("price", if strategy_first { "101" } else { "0" }, "bad"),
            ("price", if strategy_first { "101" } else { "0" }, "-1"),
            ("price", if strategy_first { "101" } else { "0" }, "1e999"),
        ] {
            let mut h = ready(Variant::PortfolioMargin);
            h.message(
                "/pm/ws",
                &if strategy_first {
                    strategy("BTCUSDT", 42, "NEW")
                } else {
                    regular("BTCUSDT", 42, "NEW", false)
                },
            );
            let before = h.engine.store().order("shared").unwrap().clone();
            let valid = if strategy_first {
                regular_rows("BTCUSDT", 78)
            } else {
                strategy_rows("BTCUSDT", 78)
            };
            let malformed = valid.replace(
                &format!(r#""{field}":"{original}""#),
                &format!(r#""{field}":"{invalid}""#),
            );
            assert_ne!(malformed, valid);
            snapshot(
                &mut h,
                if strategy_first { &malformed } else { "[]" },
                if strategy_first { "[]" } else { &malformed },
            );
            assert_eq!(
                h.engine.store().order("shared"),
                Some(&before),
                "invalid snapshot must not transfer ownership: {malformed}"
            );
            // A valid stream candidate with the same native identity must remain usable.
            h.message(
                "/pm/ws",
                &if strategy_first {
                    regular("BTCUSDT", 78, "NEW", false)
                } else {
                    strategy("BTCUSDT", 78, "NEW")
                },
            );
            assert_eq!(
                h.engine.store().order("shared").unwrap().side,
                if strategy_first {
                    safe_cex_engine::model::OrderSide::Buy
                } else {
                    safe_cex_engine::model::OrderSide::Sell
                }
            );
        }
    }
}

#[test]
fn one_unmappable_pm_order_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let order = |client: &str, native: u64, kind: &str, price: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","orderId":{native},"clientOrderId":"{client}","price":"{price}","origQty":"9","executedQty":"0","type":"{kind}","side":"BUY"}}"#
        )
    };
    let strategy = |client: &str, native: u64, kind: &str, stop: &str| {
        format!(
            r#"{{"symbol":"BTCUSDT","strategyId":{native},"newClientStrategyId":"{client}","price":"0","origQty":"3","strategyType":"{kind}","side":"SELL","stopPrice":"{stop}"}}"#
        )
    };
    let read = |h: &mut Harness, poke: u64, orders: &[String], strategies: &[String]| {
        h.message("/pm/ws", &regular("BTCUSDT", poke, "NEW", false));
        snapshot(
            h,
            &format!("[{}]", orders.join(",")),
            &format!("[{}]", strategies.join(",")),
        );
    };
    let mut h = ready(Variant::PortfolioMargin);
    read(
        &mut h,
        1,
        &[
            order("a", 10, "LIMIT", "100"),
            order("b", 11, "LIMIT", "101"),
        ],
        &[
            strategy("s", 20, "STOP_MARKET", "90"),
            strategy("t", 21, "STOP_MARKET", "91"),
        ],
    );
    let stored = ["b", "t"].map(|id| h.engine.store().order(id).unwrap().clone());
    h.logs.clear();

    read(
        &mut h,
        2,
        &[
            order("a", 10, "LIMIT", "102"),
            order("b", 11, "FUTURE_KIND", "1"),
        ],
        &[
            strategy("s", 20, "STOP_MARKET", "92"),
            strategy("t", 21, "STOP_MARKET", "-1"),
        ],
    );
    let store = h.engine.store();
    assert_eq!(store.order("a").unwrap().price, 102.0);
    assert_eq!(store.order("s").unwrap().price, 92.0);
    for row in &stored {
        assert_eq!(store.order(&row.id), Some(row));
    }
    for message in [
        "Binance open order b has an unsupported type",
        "Binance conditional order t has invalid economics",
    ] {
        assert!(h.logs.iter().any(|l| l == message), "{message}");
    }
    assert!(!h.logs.iter().any(|l| l.contains("FUTURE_KIND")));

    read(
        &mut h,
        3,
        &[order("a", 10, "LIMIT", "102")],
        &[strategy("s", 20, "STOP_MARKET", "92")],
    );
    let mut ids: Vec<_> = h
        .engine
        .store()
        .orders()
        .iter()
        .map(|o| o.id.clone())
        .collect();
    ids.sort();
    assert_eq!(ids, ["a", "s"]);
}

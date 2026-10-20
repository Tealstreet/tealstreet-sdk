//! Cancel/replace races from the USD-M and portfolio-margin cancel-order contracts.
//! https://developers.binance.com/docs/derivatives/usds-margined-futures/trade/rest-api/Cancel-Order
//! https://developers.binance.com/docs/derivatives/portfolio-margin/trade/Cancel-UM-Order
mod support;

use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::{Options, Variant};
use support::Harness;

fn root(variant: Variant) -> &'static str {
    if variant == Variant::PortfolioMargin {
        "/papi/v1/um"
    } else {
        "/fapi/v1"
    }
}
fn id(variant: Variant) -> &'static str {
    if variant == Variant::PortfolioMargin {
        "maker"
    } else {
        "12"
    }
}
fn socket(variant: Variant) -> &'static str {
    if variant == Variant::PortfolioMargin {
        "/pm/ws"
    } else {
        "/private/ws"
    }
}
fn ready(variant: Variant) -> Harness {
    ready_orders(variant, None)
}
fn ready_orders(variant: Variant, full_close: Option<bool>) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    let root = root(variant);
    h.respond("/fapi/v1/exchangeInfo", 200, support::EXCHANGE_INFO);
    h.respond(&format!("{root}/leverageBracket"), 200, "[]");
    h.respond("/fapi/v1/ticker/24hr", 200, support::DAILY);
    h.respond("/fapi/v1/ticker/bookTicker", 200, support::BOOK_TICKERS);
    h.respond("/fapi/v1/premiumIndex", 200, support::PREMIUM);
    h.respond(
        if variant == Variant::PortfolioMargin {
            "/papi/v1/listenKey"
        } else {
            "/fapi/v1/listenKey"
        },
        200,
        r#"{"listenKey":"private-key"}"#,
    );
    h.respond(
        &format!("{root}/positionSide/dual"),
        200,
        r#"{"dualSidePosition":false}"#,
    );
    h.respond(
        if variant == Variant::PortfolioMargin {
            "/papi/v1/um/positionRisk"
        } else {
            "/fapi/v3/positionRisk"
        },
        200,
        r#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"7","entryPrice":"100","markPrice":"100","unRealizedProfit":"0","leverage":"20","marginType":"cross","notional":"700","liquidationPrice":"50"}]"#,
    );
    if variant == Variant::PortfolioMargin {
        h.respond("/papi/v1/balance", 200, "[]");
    } else {
        h.respond("/fapi/v1/symbolConfig", 200, "[]");
        h.respond("/fapi/v3/account", 200, r#"{"assets":[]}"#);
    }
    h.respond(&format!("{root}/openOrders"), 200, r#"[{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","price":"100","origQty":"7","executedQty":"2","type":"LIMIT","side":"BUY","status":"PARTIALLY_FILLED","timeInForce":"GTC"}]"#);
    h.respond(
        &format!(
            "{root}/{}",
            if variant == Variant::PortfolioMargin {
                "conditional/openOrders"
            } else {
                "openAlgoOrders"
            }
        ),
        200,
        &match full_close {
            None => "[]".to_owned(),
            Some(close) if variant == Variant::PortfolioMargin => format!(r#"[{{"newClientStrategyId":"stop","strategyId":42,"strategyStatus":"NEW","strategyType":"STOP_MARKET","origQty":"7","price":"0","side":"SELL","stopPrice":"90","symbol":"BTCUSDT","workingType":"MARK_PRICE","closePosition":{close}}}]"#),
            Some(close) => format!(r#"[{{"algoId":42,"clientAlgoId":"stop","algoType":"CONDITIONAL","orderType":"STOP_MARKET","symbol":"BTCUSDT","side":"SELL","quantity":"{}","algoStatus":"NEW","triggerPrice":"90","workingType":"MARK_PRICE","closePosition":{close}}}]"#, if close {"0"} else {"7"}),
        },
    );
    let private = h.socket(socket(variant)).unwrap();
    h.event(Event::WsOpened { socket: private });
    h
}
fn amend(h: &mut Harness, variant: Variant, command: u32, amount: Option<f64>) {
    h.event(Event::Command(Command::AmendOrder {
        command,
        id: id(variant).into(),
        price: Some(101.0),
        amount,
    }));
}
fn ack(filled: u32, status: &str) -> String {
    format!(
        r#"{{"symbol":"BTCUSDT","orderId":12,"clientOrderId":"maker","origQty":"7","executedQty":"{filled}","status":"{status}"}}"#
    )
}
fn fill(h: &mut Harness, variant: Variant, filled: u32, status: &str) {
    h.message(socket(variant), &format!(r#"{{"e":"ORDER_TRADE_UPDATE","T":1000,"E":1000,"o":{{"s":"BTCUSDT","c":"maker","i":12,"S":"BUY","o":"LIMIT","f":"GTC","q":"7","p":"100","x":"TRADE","X":"{status}","l":"1","z":"{filled}","L":"100","t":{filled}}}}}"#));
}
fn quantity(url: &str) -> Option<&str> {
    url.split_once('?')?
        .1
        .split('&')
        .find_map(|field| field.strip_prefix("quantity="))
}

#[test]
fn cancel_ack_and_stream_race_replace_only_unfilled_target_quantity() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for (racing_fill, target, expected) in [
            (None, None, "4"),
            (Some(4), None, "3"),
            (Some(4), Some(6.0), "2"),
            (None, Some(9.0), "6"),
        ] {
            let mut h = ready(variant);
            amend(&mut h, variant, 91, target);
            let path = format!("{}/order", root(variant));
            let cancel = h.take(&path);
            assert_eq!(cancel.method, Method::Delete);
            if let Some(filled) = racing_fill {
                fill(&mut h, variant, filled, "PARTIALLY_FILLED");
            }
            h.reply(cancel.id, 200, "", &ack(3, "CANCELED"));
            let replacement = h.take(&path);
            assert_eq!(replacement.method, Method::Post);
            assert_eq!(quantity(&replacement.url), Some(expected), "{variant:?}");
        }
    }
}

#[test]
fn unknown_cancel_after_full_or_partial_fill_never_places_again() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for filled in [None, Some(4), Some(7)] {
            let mut h = ready(variant);
            amend(&mut h, variant, 91, None);
            let cancel = h.take(&format!("{}/order", root(variant)));
            if let Some(filled) = filled {
                fill(
                    &mut h,
                    variant,
                    filled,
                    if filled == 7 {
                        "FILLED"
                    } else {
                        "PARTIALLY_FILLED"
                    },
                );
            }
            h.reply(
                cancel.id,
                400,
                "",
                r#"{"code":-2011,"msg":"Unknown order sent."}"#,
            );
            assert!(h.requests.is_empty(), "{variant:?} {filled:?}");
            assert!(h.result(91).unwrap().is_err());
            let order = h.engine.store().order(id(variant)).unwrap();
            assert_eq!(
                order.status,
                if filled == Some(7) {
                    OrderStatus::Closed
                } else {
                    OrderStatus::Open
                }
            );
        }
    }
}

#[test]
fn full_fill_beats_late_cancel_and_reduced_target_cannot_overfill() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for (stream, ack_filled, target) in
            [(true, 2, None), (false, 7, None), (false, 3, Some(3.0))]
        {
            let mut h = ready(variant);
            amend(&mut h, variant, 91, target);
            let cancel = h.take(&format!("{}/order", root(variant)));
            if stream {
                fill(&mut h, variant, 7, "FILLED");
            }
            h.reply(
                cancel.id,
                200,
                "",
                &ack(
                    ack_filled,
                    if ack_filled == 7 {
                        "FILLED"
                    } else {
                        "CANCELED"
                    },
                ),
            );
            assert!(h.requests.is_empty(), "{variant:?}");
            assert!(h.result(91).unwrap().is_ok());
            if target.is_none() {
                assert_eq!(
                    h.engine.store().order(id(variant)).unwrap().status,
                    OrderStatus::Closed
                );
            }
        }
    }
}

#[test]
fn malformed_or_mismatched_cancellation_fences_amendment() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for body in [
            "{}".to_owned(),
            ack(2, "NEW"),
            ack(2, "CANCELED").replace("BTCUSDT", "ETHUSDT"),
            ack(2, "CANCELED").replace("12", "99"),
            ack(8, "CANCELED"),
        ] {
            let mut h = ready(variant);
            amend(&mut h, variant, 91, None);
            let cancel = h.take(&format!("{}/order", root(variant)));
            h.reply(cancel.id, 200, "", &body);
            assert!(h.requests.is_empty(), "{variant:?} {body}");
            assert!(h.result(91).unwrap().is_err(), "{variant:?} {body}");
        }
    }
}

#[test]
fn duplicate_amendment_does_not_cancel_twice() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        let mut h = ready(variant);
        amend(&mut h, variant, 91, None);
        amend(&mut h, variant, 92, None);
        assert_eq!(h.requests.len(), 1);
        assert!(h.result(92).unwrap().is_err());
    }
}

fn conditional_path(variant: Variant) -> &'static str {
    if variant == Variant::PortfolioMargin {
        "/papi/v1/um/conditional/order"
    } else {
        "/fapi/v1/algoOrder"
    }
}
fn conditional_id(variant: Variant) -> &'static str {
    if variant == Variant::PortfolioMargin {
        "stop"
    } else {
        "42"
    }
}
fn amend_conditional(h: &mut Harness, variant: Variant) {
    h.event(Event::Command(Command::AmendOrder {
        command: 93,
        id: conditional_id(variant).into(),
        price: Some(91.0),
        amount: None,
    }));
}

#[test]
fn conditional_amend_accepts_its_distinct_cancellation_contract_and_preserves_full_close() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        for full_close in [false, true] {
            let mut h = ready_orders(variant, Some(full_close));
            amend_conditional(&mut h, variant);
            let cancel = h.take(conditional_path(variant));
            assert_eq!(cancel.method, Method::Delete);
            let ack = if variant == Variant::PortfolioMargin {
                r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"stop","strategyStatus":"CANCELED","origQty":"7"}"#
            } else {
                r#"{"algoId":42,"clientAlgoId":"stop","code":"200","msg":"success"}"#
            };
            h.reply(cancel.id, 200, "", ack);
            let replacement = h.take(conditional_path(variant));
            assert_eq!(
                replacement.method,
                Method::Post,
                "{variant:?} full_close={full_close}"
            );
            if full_close && variant == Variant::UsdM {
                assert_eq!(quantity(&replacement.url), None);
                assert!(replacement.url.contains("closePosition=true"));
            } else {
                assert_eq!(quantity(&replacement.url), Some("7"));
            }
        }
    }
}

#[test]
fn malformed_unknown_or_mismatched_conditional_cancellation_never_replaces() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        let mut bad = vec!["{}", r#"{"code":-2011,"msg":"Unknown order sent."}"#];
        if variant == Variant::PortfolioMargin {
            bad.extend([
                r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"other","strategyStatus":"CANCELED"}"#,
                r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"stop","strategyStatus":"TRIGGERED"}"#,
            ]);
        } else {
            bad.extend([
                r#"{"algoId":99,"code":200,"msg":"success"}"#,
                r#"{"algoId":42,"code":400,"msg":"failure"}"#,
            ]);
        }
        for body in bad {
            let mut h = ready_orders(variant, Some(true));
            amend_conditional(&mut h, variant);
            let cancel = h.take(conditional_path(variant));
            h.reply(cancel.id, 200, "", body);
            assert!(h.requests.is_empty(), "{variant:?} {body}");
            assert!(h.result(93).unwrap().is_err(), "{variant:?} {body}");
        }
    }
}

#[test]
fn pm_reused_client_id_is_not_changed_by_an_old_incarnations_cancel_ack() {
    for amend_order in [false, true] {
        for unknown in [false, true] {
            let variant = Variant::PortfolioMargin;
            let mut h = ready(variant);
            if amend_order {
                amend(&mut h, variant, 91, None);
            } else {
                h.event(Event::Command(Command::CancelOrders {
                    command: 91,
                    ids: vec!["maker".into()],
                }));
            }
            let cancel = h.take("/papi/v1/um/order");
            h.message("/pm/ws", r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"maker","i":13,"S":"BUY","o":"LIMIT","f":"GTC","q":"9","p":"101","x":"NEW","X":"NEW","l":"0","z":"0","L":"0","t":0}}"#);
            let before = h.engine.store().order("maker").unwrap().clone();
            let reply = if unknown {
                r#"{"code":-2011,"msg":"Unknown order sent."}"#.to_owned()
            } else {
                ack(3, "CANCELED")
            };
            h.reply(cancel.id, if unknown { 400 } else { 200 }, "", &reply);
            assert_eq!(h.engine.store().order("maker"), Some(&before));
            assert!(h.requests.is_empty());
            if amend_order || !unknown {
                assert!(h.result(91).unwrap().is_err());
            }
        }
    }
}

#[test]
fn pm_reused_strategy_id_is_not_changed_by_an_old_cancellation() {
    let variant = Variant::PortfolioMargin;
    for amend_order in [false, true] {
        for unknown in [false, true] {
            let mut h = ready_orders(variant, Some(false));
            if amend_order {
                amend_conditional(&mut h, variant);
            } else {
                h.event(Event::Command(Command::CancelOrders {
                    command: 93,
                    ids: vec!["stop".into()],
                }));
            }
            let cancel = h.take(conditional_path(variant));
            h.message("/pm/ws", r#"{"e":"CONDITIONAL_ORDER_TRADE_UPDATE","fs":"UM","so":{"s":"BTCUSDT","c":"stop","si":43,"S":"SELL","st":"STOP_MARKET","q":"9","p":"0","sp":"89","os":"NEW","R":true,"wt":"MARK_PRICE","ps":"BOTH","cp":false,"i":0}}"#);
            let before = h.engine.store().order("stop").unwrap().clone();
            h.reply(cancel.id, 200, "", if unknown {
                r#"{"code":-2011,"msg":"Unknown order sent."}"#
            } else {
                r#"{"symbol":"BTCUSDT","strategyId":42,"newClientStrategyId":"stop","strategyStatus":"CANCELED","origQty":"7"}"#
            });
            assert_eq!(h.engine.store().order("stop"), Some(&before));
            assert!(h.requests.is_empty());
            if amend_order || !unknown {
                assert!(h.result(93).unwrap().is_err());
            }
        }
    }
}

#[test]
fn pm_symbol_cancel_applies_only_to_captured_native_incarnations() {
    let variant = Variant::PortfolioMargin;
    for (reuse_regular, reuse_strategy) in
        [(true, false), (false, true), (true, true), (false, false)]
    {
        for unknown in [false, true] {
            let mut h = ready_orders(variant, Some(false));
            h.event(Event::Command(Command::CancelSymbolOrders {
                command: 94,
                symbol: "BTCUSDT".into(),
            }));
            let regular = h.take("/papi/v1/um/allOpenOrders");
            let strategy = h.take("/papi/v1/um/conditional/allOpenOrders");
            if reuse_regular {
                h.message("/pm/ws", r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"maker","i":13,"S":"BUY","o":"LIMIT","f":"GTC","q":"9","p":"101","x":"NEW","X":"NEW","l":"0","z":"0","L":"0","t":0}}"#);
            }
            if reuse_strategy {
                h.message("/pm/ws", r#"{"e":"CONDITIONAL_ORDER_TRADE_UPDATE","fs":"UM","so":{"s":"BTCUSDT","c":"stop","si":43,"S":"SELL","st":"STOP_MARKET","q":"9","p":"0","sp":"89","os":"NEW","R":true,"wt":"MARK_PRICE","ps":"BOTH","cp":false,"i":0}}"#);
            }
            h.message("/pm/ws", r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"new-client","i":99,"S":"BUY","o":"LIMIT","f":"GTC","q":"5","p":"102","x":"NEW","X":"NEW","l":"0","z":"0","L":"0","t":0}}"#);
            let new_regular = h.engine.store().order("maker").unwrap().clone();
            let new_strategy = h.engine.store().order("stop").unwrap().clone();
            let later = h.engine.store().order("new-client").unwrap().clone();
            let reply = if unknown {
                r#"{"code":-2011,"msg":"Unknown order sent."}"#
            } else {
                r#"{"code":200,"msg":"The operation of cancel all open order is done."}"#
            };
            h.reply(regular.id, if unknown { 400 } else { 200 }, "", reply);
            h.reply(strategy.id, if unknown { 400 } else { 200 }, "", reply);
            assert!(h.result(94).unwrap().is_ok());
            for (id, reused, before) in [
                ("maker", reuse_regular, new_regular),
                ("stop", reuse_strategy, new_strategy),
            ] {
                let current = h.engine.store().order(id).unwrap();
                if reused {
                    assert_eq!(current, &before, "{id} unknown={unknown}");
                } else {
                    assert_eq!(current.status, OrderStatus::Canceled);
                }
            }
            assert_eq!(h.engine.store().order("new-client"), Some(&later));
            // A later successful replica must be able to retain the new native rows.
            h.advance(10_000.0);
            let mut regular_rows = vec![
                r#"{"symbol":"BTCUSDT","orderId":99,"clientOrderId":"new-client","price":"102","origQty":"5","executedQty":"0","type":"LIMIT","side":"BUY","status":"NEW"}"#,
            ];
            if reuse_regular {
                regular_rows.push(r#"{"symbol":"BTCUSDT","orderId":13,"clientOrderId":"maker","price":"101","origQty":"9","executedQty":"0","type":"LIMIT","side":"BUY","status":"NEW"}"#);
            }
            h.respond(
                "/papi/v1/um/openOrders",
                200,
                &format!("[{}]", regular_rows.join(",")),
            );
            h.respond("/papi/v1/um/conditional/openOrders", 200, if reuse_strategy {
                r#"[{"newClientStrategyId":"stop","strategyId":43,"strategyStatus":"NEW","strategyType":"STOP_MARKET","origQty":"9","price":"0","side":"SELL","stopPrice":"89","symbol":"BTCUSDT","workingType":"MARK_PRICE"}]"#
            } else {"[]"});
            assert_eq!(
                h.engine.store().orders().len(),
                1 + usize::from(reuse_regular) + usize::from(reuse_strategy)
            );
            assert!(
                h.engine
                    .store()
                    .orders()
                    .iter()
                    .all(|order| order.status == OrderStatus::Open)
            );
        }
    }
}

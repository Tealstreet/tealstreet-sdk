use crate::support::FixtureOptions as Options;
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, Method};

use serde_json::{Value, json};
use support::*;

fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        &POSITION
            .replace("\"data\":[", "\"data\":{\"list\":[")
            .replace("]}", "]}}"),
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-strategy-orders"))
    {
        h.respond(
            "/unfilled-strategy-orders",
            200,
            r#"{"code":"00000","data":[]}"#,
        );
    }
    h.login();
    h
}

fn leverage_fixture(uta_account: bool, hedged: bool, margin: MarginType) -> Harness {
    let mut h = if uta_account {
        uta()
    } else {
        Harness::classic()
    };
    h.engine.store_mut().set_hedged(hedged);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.margin_type = Some(margin);
        s.leverage = 9.0;
        s.buy_leverage = Some(10.0);
        s.sell_leverage = Some(11.0);
    });
    let mut long = h.engine.store().positions()[0].clone();
    long.id = "long".into();
    long.side = PositionSide::Long;
    long.leverage = 10.0;
    long.margin_type = Some(margin);
    let mut short = long.clone();
    short.id = "short".into();
    short.side = PositionSide::Short;
    short.leverage = 11.0;
    h.engine.store_mut().set_positions(vec![long, short]);
    h
}

fn set_leverage(h: &mut Harness, side: Option<PositionSide>, leverage: f64) -> Value {
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage,
        side,
    });
    let request = h.take("/set-leverage");
    let body = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    h.reply(request.id, 200, r#"{"code":"00000","data":{}}"#);
    assert!(h.done.last().unwrap().1.is_ok());
    body
}

#[test]
fn isolated_hedge_routes_requested_or_default_leg_and_preserves_other_settings() {
    for uta in [false, true] {
        for side in [None, Some(PositionSide::Long), Some(PositionSide::Short)] {
            let mut h = leverage_fixture(uta, true, MarginType::Isolated);
            let payload = set_leverage(&mut h, side, 25.0);
            let short = side == Some(PositionSide::Short);
            assert_eq!(
                payload[if uta { "posSide" } else { "holdSide" }],
                if short { "short" } else { "long" }
            );
            assert_eq!(
                payload["leverage"],
                if uta { json!("25") } else { json!(25.0) }
            );
            assert_eq!(
                h.engine.store().position("long").unwrap().leverage,
                if short { 10.0 } else { 25.0 }
            );
            assert_eq!(
                h.engine.store().position("short").unwrap().leverage,
                if short { 25.0 } else { 11.0 }
            );
            let setting = h.engine.store().setting("BTCUSDT").unwrap();
            assert_eq!(setting.leverage, if short { 9.0 } else { 25.0 });
            assert_eq!(setting.buy_leverage, Some(if short { 10.0 } else { 25.0 }));
            assert_eq!(setting.sell_leverage, Some(if short { 25.0 } else { 11.0 }));
        }
    }
}

#[test]
fn cross_or_oneway_ignores_side_and_updates_coupled_settings() {
    for uta in [false, true] {
        for (hedged, margin) in [(true, MarginType::Cross), (false, MarginType::Isolated)] {
            let mut h = leverage_fixture(uta, hedged, margin);
            let p = set_leverage(&mut h, Some(PositionSide::Short), 25.0);
            assert!(p.get("holdSide").is_none());
            assert!(p.get("posSide").is_none());
            assert!(p.get("productType").is_some());
            assert!(
                h.engine
                    .store()
                    .positions()
                    .iter()
                    .all(|p| p.leverage == 25.0)
            );
            let s = h.engine.store().setting("BTCUSDT").unwrap();
            assert_eq!(
                (s.leverage, s.buy_leverage, s.sell_leverage),
                (25.0, Some(25.0), Some(25.0))
            );
        }
    }
}

fn order(amount: f64, caller: Option<&str>) -> PlaceOrderOpts {
    let mut value =
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":90});
    if let Some(caller) = caller {
        value["clientOrderId"] = json!(caller);
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn zero_lot_row_is_skipped_but_caller_id_contract_still_preflights_entire_batch() {
    for uta_account in [false, true] {
        let mut h = if uta_account {
            uta()
        } else {
            Harness::classic()
        };
        h.command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(0.0001, None), order(0.002, Some("valid"))],
        });
        let request = h.take(if uta_account {
            "/place-order"
        } else {
            "/batch-place-order"
        });
        let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        if uta_account {
            assert_eq!(payload["qty"], "0.002");
            assert_eq!(payload["clientOid"], "valid");
        } else {
            assert_eq!(payload["orderList"].as_array().unwrap().len(), 1);
            assert_eq!(payload["orderList"][0]["clientOid"], "valid");
        }
        h.reply(
            request.id,
            200,
            if uta_account {
                r#"{"code":"00000","data":{"orderId":"valid-id"}}"#
            } else {
                r#"{"code":"00000","data":{"successList":[{"orderId":"valid-id"}]}}"#
            },
        );
        assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[\"valid-id\"]");
        h.command(Command::PlaceOrders {
            command: 2,
            orders: vec![order(0.002, None), order(0.0001, Some("cannot-drop"))],
        });
        assert!(h.done.last().unwrap().1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
    let mut h = Harness::classic();
    h.command(Command::PlaceOrders {
        command: 1,
        orders: vec![order(0.0, None), order(0.0001, None)],
    });
    assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[]");
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn trading_waits_for_probe_then_uses_detected_route_once() {
    for uta_account in [false, true] {
        let mut h = Harness::private_started();
        h.command(Command::SetPositionMode {
            command: 71,
            hedged: true,
            symbol: None,
        });
        h.command(Command::SetPositionMode {
            command: 72,
            hedged: false,
            symbol: None,
        });
        assert!(h.done.is_empty());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
        h.respond(
            "/account/settings",
            200,
            if uta_account {
                r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#
            } else {
                r#"{"code":"40084","msg":"Classic Account mode","data":null}"#
            },
        );
        let path = if uta_account {
            "/set-hold-mode"
        } else {
            "/set-position-mode"
        };
        let first = h.take(path);
        assert_eq!(
            h.requests
                .iter()
                .filter(|r| r.method == Method::Post)
                .count(),
            0
        );
        h.reply(first.id, 200, r#"{"code":"00000","data":{}}"#);
        let second = h.take(path);
        h.reply(second.id, 200, r#"{"code":"00000","data":{}}"#);
        assert_eq!(
            h.done.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [71, 72]
        );
        assert!(h.done.iter().all(|(_, r)| r.is_ok()));
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
        assert!(!h.engine.store().hedged());
    }
}

#[test]
fn pending_probe_timeout_fails_queued_commands_without_writes() {
    let mut h = Harness::private_started();
    h.command(Command::SetPositionMode {
        command: 71,
        hedged: true,
        symbol: None,
    });
    h.command(Command::SetPositionMode {
        command: 72,
        hedged: false,
        symbol: None,
    });
    h.timer_after(10_000.0);
    assert_eq!(h.done.len(), 2);
    assert!(
        h.done
            .iter()
            .all(|(_, r)| r.as_ref().unwrap_err().contains("detection"))
    );
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    assert!(h.requests.iter().all(|r| r.method == Method::Get));
}

#[test]
fn sized_stop_downgrade_logs_but_explicit_full_stop_does_not() {
    for uta_account in [false, true] {
        for amount in [0.0, 0.5] {
            let mut h = if uta_account {
                uta()
            } else {
                Harness::classic()
            };
            h.engine.store_mut().update_positions(|p| p.contracts = 0.0);
            h.command(Command::PlaceOrders { command: 1, orders: vec![serde_json::from_value(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":amount,"price":80})).unwrap()] });
            let path = if uta_account {
                "/place-strategy-order"
            } else {
                "/place-tpsl-order"
            };
            for _ in 0..4 {
                if h.requests.iter().any(|r| r.url.contains(path)) {
                    break;
                }
                if h.requests
                    .iter()
                    .any(|r| r.url.contains("/account/account?"))
                {
                    h.respond("/account/account?", 200, r#"{"code":"00000","data":{"posMode":"one_way_mode","marginMode":"crossed","crossedMarginLeverage":"20"}}"#);
                } else if h
                    .requests
                    .iter()
                    .any(|r| r.url.contains("/account/settings"))
                {
                    h.respond(
                        "/account/settings",
                        200,
                        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
                    );
                } else {
                    h.respond(
                        if uta_account {
                            "/current-position"
                        } else {
                            "/all-position"
                        },
                        200,
                        if uta_account {
                            r#"{"code":"00000","data":{"list":[]}}"#
                        } else {
                            r#"{"code":"00000","data":[]}"#
                        },
                    );
                }
            }
            let request = h.take(path);
            let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(
                payload[if uta_account { "tpslMode" } else { "planType" }],
                if uta_account { "full" } else { "pos_loss" }
            );
            assert_eq!(
                h.logs
                    .iter()
                    .filter(|line| line.contains("became a full-position stop"))
                    .count(),
                usize::from(amount > 0.0)
            );
        }
    }
}

#[test]
fn zero_leverage_limits_use_ts_defaults_and_inverted_limits_cannot_panic() {
    let mut h = Harness::started(Options {
        public_data_only: true,
        ..Options::default()
    });
    h.respond(
        "/contracts",
        200,
        &CONTRACTS.replace(
            "\"maxLever\":\"125\"",
            "\"minLever\":\"0\",\"maxLever\":\"0\"",
        ),
    );
    let limits = &h.engine.store().market("BTCUSDT").unwrap().limits.leverage;
    assert_eq!((limits.min, limits.max), (1.0, 20.0));
    for (min, max, requested, expected) in [(0.0, 0.0, 80.0, 20.0), (0.0, 0.0, 0.5, 1.0)] {
        let mut h = Harness::classic();
        h.engine.store_mut().update_markets(|m| {
            m.limits.leverage.min = min;
            m.limits.leverage.max = max;
        });
        let payload = set_leverage(&mut h, None, requested);
        assert_eq!(payload["leverage"], expected);
    }
    for (min, max) in [(30.0, 20.0), (-1.0, 20.0), (f64::NAN, f64::INFINITY)] {
        let mut h = Harness::classic();
        h.engine.store_mut().update_markets(|m| {
            m.limits.leverage.min = min;
            m.limits.leverage.max = max;
        });
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
            side: None,
        });
        assert!(h.done.last().unwrap().1.is_err());
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
    }
}

#[test]
fn inverse_cross_hedge_preserves_ts_hold_side_but_updates_coupled_leverage() {
    let mut h = Harness::started(Options {
        product: safe_cex_venue_bitget::Product::Inverse,
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    });
    h.respond(
        "/contracts",
        200,
        &CONTRACTS
            .replace("BTCUSDT", "BTCUSD")
            .replace("\"quoteCoin\":\"USDT\"", "\"quoteCoin\":\"USD\""),
    );
    h.respond("/tickers", 200, &TICKERS.replace("BTCUSDT", "BTCUSD"));
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    h.respond(
        "/all-position",
        200,
        &POSITION
            .replace("BTCUSDT", "BTCUSD")
            .replace("one_way_mode", "hedge_mode"),
    );
    h.engine.store_mut().set_hedged(true);
    let mut long = h.engine.store().positions()[0].clone();
    long.id = "long".into();
    let mut short = long.clone();
    short.id = "short".into();
    short.side = PositionSide::Short;
    h.engine.store_mut().set_positions(vec![long, short]);
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSD".into(),
        leverage: 25.0,
        side: Some(PositionSide::Short),
    });
    let request = h.take("/set-leverage");
    let p: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(p["holdSide"], "short");
    assert_eq!(p["marginCoin"], "BTC");
    h.reply(request.id, 200, r#"{"code":"00000","data":{}}"#);
    assert!(
        h.engine
            .store()
            .positions()
            .iter()
            .all(|p| p.leverage == 25.0)
    );
    let s = h.engine.store().setting("BTCUSD").unwrap();
    assert_eq!(
        (s.leverage, s.buy_leverage, s.sell_leverage),
        (25.0, Some(25.0), Some(25.0))
    );
}

#[test]
fn uta_margin_change_preserves_long_leverage_after_side_writes_and_short_stream_rows() {
    for side in [PositionSide::Long, PositionSide::Short] {
        for refresh_metadata in [false, true] {
            let mut h = leverage_fixture(true, true, MarginType::Isolated);
            // Scalar callers historically use the long rail.
            h.engine
                .store_mut()
                .update_setting("BTCUSDT", |setting| setting.leverage = 10.0);
            set_leverage(&mut h, Some(side), 25.0);
            let expected = if side == PositionSide::Long {
                25.0
            } else {
                10.0
            };
            // A later position row for the other hedge leg cannot change the scalar.
            h.private_message(r#"{"arg":{"topic":"position"},"data":[{"symbol":"BTCUSDT","posSide":"short","size":"1","leverage":"30","marginMode":"isolated","holdMode":"hedge_mode"}]}"#);
            let setting = h.engine.store().setting("BTCUSDT").unwrap();
            assert_eq!(setting.leverage, expected);
            assert_eq!(setting.buy_leverage, Some(expected));
            assert_eq!(setting.sell_leverage, Some(30.0));
            if refresh_metadata {
                h.engine.store_mut().set_positions(vec![]);
            }
            h.command(Command::SetMarginType {
                command: 2,
                symbol: "BTCUSDT".into(),
                isolated: false,
            });
            if refresh_metadata {
                h.respond("/account/settings", 200, r#"{"code":"00000","data":{"holdMode":"hedge_mode","symbolConfigList":[{"symbol":"BTCUSDT","category":"USDT-FUTURES","marginMode":"isolated","leverage":"3"}]}}"#);
            }
            let request = h.take("/set-leverage");
            let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload["leverage"], expected.to_string());
            assert_eq!(payload["marginMode"], "crossed");
            assert!(payload.get("posSide").is_none());
        }
    }
}

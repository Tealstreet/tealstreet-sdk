mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_woo::{Options, Woo};
use support::*;
fn engine() -> TestEngine {
    let (seed, _) = started();
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        application_id: Some("fixture-app".into()),
        ..Default::default()
    }));
    e.store_mut().set_markets(seed.store().markets().to_vec());
    e.store_mut().set_tickers(seed.store().tickers().to_vec());
    e
}
fn opts() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.01,"price":59000}"#,
    )
    .unwrap()
}
fn placed(e: &mut TestEngine, orders: Vec<PlaceOrderOpts>) -> Vec<Action> {
    e.handle(
        1000.0,
        Event::Command(Command::PlaceOrders { command: 1, orders }),
    );
    e.drain_actions().collect()
}

#[test]
fn caller_identity_precedes_write_and_omission_is_preserved() {
    for supplied in [false, true] {
        let mut e = engine();
        let mut o = opts();
        o.client_order_id = supplied.then(|| "123456789".into());
        let out = placed(&mut e, vec![o]);
        let http = out
            .iter()
            .position(|a| matches!(a, Action::Http(_)))
            .unwrap();
        let receipts: Vec<_> = out
            .iter()
            .enumerate()
            .filter_map(|(i, action)| {
                if let Action::Notice(Notice::CommandProgress {
                    command: 1,
                    receipts,
                }) = action
                {
                    assert!(i < http);
                    Some(receipts)
                } else {
                    None
                }
            })
            .flatten()
            .collect();
        let Action::Http(request) = &out[http] else {
            unreachable!()
        };
        if supplied {
            assert_eq!(receipts.len(), 1);
            assert_eq!(receipts[0].symbol, "BTCUSDT");
            assert_eq!(
                receipts[0].kind,
                safe_cex_engine::OrderIdentityKind::ClientOrderId
            );
            assert!(
                request
                    .body
                    .as_deref()
                    .unwrap()
                    .contains(&format!("client_order_id={}", receipts[0].identity))
            );
        } else {
            assert!(receipts.is_empty());
            assert!(
                !request
                    .body
                    .as_deref()
                    .unwrap()
                    .contains("client_order_id=")
            );
        }
    }
}
fn req(actions: &[Action], method: Method, path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                (r.method == method && r.url.split('?').next().unwrap().ends_with(path))
                    .then(|| r.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| panic!("missing {method:?} {path}: {actions:?}"))
}
fn reply(e: &mut TestEngine, r: &HttpRequest, now: f64, body: &str) -> Vec<Action> {
    response(e, r, now, body);
    e.drain_actions().collect()
}
fn tick(e: &mut TestEngine, actions: &[Action]) -> Vec<Action> {
    let (timer, at) = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                Some((*timer, *at_ms))
            } else {
                None
            }
        })
        .unwrap();
    e.handle(at, Event::TimerFired { timer });
    e.drain_actions().collect()
}
fn result(actions: &[Action]) -> Result<String, String> {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: 1, result }) => Some(Ok(result.clone())),
            // Failures as the plain text the host renders.
            Action::Notice(Notice::CommandFailed { command: 1, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .unwrap()
}
fn failure(actions: &[Action]) -> &safe_cex_engine::VenueError {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .unwrap()
}
fn order(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: OrderSide::Sell,
        price: 61000.0,
        amount: 0.01,
        filled: 0.0,
        remaining: 0.01,
        reduce_only: algo_kind(kind),
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn algo_kind(kind: OrderType) -> bool {
    matches!(
        kind,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    )
}
fn position() -> Position {
    Position {
        id: "BTCUSDT:long".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 60000.0,
        notional: 1200.0,
        leverage: 10.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 0.02,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }
}
#[test]
fn caller_id_batch_preflight_rejects_every_invalid_or_nonplain_leg_before_reads_or_writes() {
    for id in ["0", "01", "-1", "1.0", "9007199254740992", ""] {
        let mut e = engine();
        let mut bad = opts();
        bad.client_order_id = Some(id.into());
        let a = placed(&mut e, vec![opts(), bad]);
        assert!(result(&a).is_err());
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    }
    for variant in 0..4 {
        let mut e = engine();
        let mut o = opts();
        o.client_order_id = Some("9007199254740991".into());
        match variant {
            0 => o.amount = 201.001,
            1 => o.stop_loss = Some(58000.0),
            2 => {
                o.kind = OrderType::StopLoss;
                o.side = OrderSide::Sell;
            }
            _ => (),
        };
        let orders = if variant == 3 {
            vec![o.clone(), o]
        } else {
            vec![o]
        };
        let a = placed(&mut e, orders);
        assert!(result(&a).is_err());
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

fn assert_bybit_control_rejected_before_batch_io(bad: PlaceOrderOpts) {
    let mut e = engine();
    let actions = placed(&mut e, vec![opts(), bad]);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_))),
        "unsupported controls must cause no HTTP"
    );
    assert!(result(&actions).is_err());
    assert!(e.store().orders().is_empty());
}

#[test]
fn bybit_bbo_on_final_batch_member_prevents_every_http_request() {
    let mut bad = opts();
    bad.bybit_bbo = Some(BybitBbo {
        side_type: "Queue".into(),
        level: 1.0,
    });
    assert_bybit_control_rejected_before_batch_io(bad);
}

#[test]
fn bybit_ticks_on_final_batch_member_prevents_every_http_request() {
    let mut bad = opts();
    bad.bybit_slippage_tolerance_ticks = Some(2.0);
    assert_bybit_control_rejected_before_batch_io(bad);
}

#[test]
fn plain_safe_client_id_and_broker_are_sent_exactly_once() {
    let mut e = engine();
    let mut o = opts();
    o.client_order_id = Some("9007199254740991".into());
    let a = placed(&mut e, vec![o]);
    let r = req(&a, Method::Post, "/v1/order");
    let b = r.body.as_deref().unwrap();
    assert!(b.contains("client_order_id=9007199254740991"));
    assert!(b.contains("broker_id=ab82cb09-cfec-4473-80a3-b740779d0644"));
    let a = reply(
        &mut e,
        &r,
        1010.0,
        r#"{"success":true,"order_id":9007199254740990}"#,
    );
    assert_eq!(result(&a).unwrap(), r#"["9007199254740990"]"#);
}
#[test]
fn split_lots_preserve_every_tick_and_respect_native_maximum() {
    let mut e = engine();
    let mut o = opts();
    o.amount = 201.001;
    let mut a = placed(&mut e, vec![o]);
    let mut sum = 0.0;
    for id in 1..=3 {
        let r = req(&a, Method::Post, "/v1/order");
        let b = r.body.as_deref().unwrap();
        let q = b
            .split('&')
            .find_map(|p| p.strip_prefix("order_quantity="))
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!(q > 0.0 && q <= 100.0);
        sum += q;
        a = reply(
            &mut e,
            &r,
            1000.0 + id as f64,
            r#"{"success":true,"order_id":12}"#,
        );
        if id < 3 {
            a = tick(&mut e, &a);
        }
    }
    assert!((sum - 201.001).abs() < 1e-10);
    assert_eq!(result(&a).unwrap(), r#"["12","12","12"]"#);
}
#[test]
fn market_reads_touch_and_crosses_inside_cap_when_default_slippage_would_miss() {
    let mut e = engine();
    e.store_mut().update_ticker("BTCUSDT", |t| {
        t.mark = 60000.0;
        t.index = 60000.0;
    });
    let mut o = opts();
    o.kind = OrderType::Market;
    o.price = None;
    let a = placed(&mut e, vec![o]);
    let touch = req(&a, Method::Get, "/v1/public/orderbook/PERP_BTC_USDT");
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
    let a = reply(
        &mut e,
        &touch,
        1010.0,
        r#"{"asks":[{"price":"60580"}],"bids":[{"price":59990}]}"#,
    );
    let r = req(&a, Method::Post, "/v1/order");
    assert!(r.body.as_ref().unwrap().contains("order_price=60580"));
    assert!(r.body.as_ref().unwrap().contains("order_type=IOC"));
}
#[test]
fn market_ignores_out_of_cap_touch_and_preserves_caller_slippage() {
    let mut e = engine();
    e.store_mut().update_ticker("BTCUSDT", |t| {
        t.mark = 60000.0;
        t.index = 60000.0;
    });
    let mut o = opts();
    o.kind = OrderType::Market;
    o.price = None;
    o.max_slippage_pct = Some(0.2);
    let a = placed(&mut e, vec![o]);
    let touch = req(&a, Method::Get, "/v1/public/orderbook/PERP_BTC_USDT");
    let a = reply(
        &mut e,
        &touch,
        1010.0,
        r#"{"asks":[{"price":62000}],"bids":[]}"#,
    );
    let r = req(&a, Method::Post, "/v1/order");
    assert!(r.body.as_ref().unwrap().contains("order_price=60120"));
}
#[test]
fn postonly_batch_touch_failure_detected_before_earlier_plain_order_can_write() {
    let mut e = engine();
    let mut o = opts();
    o.price = Some(59000.0);
    o.time_in_force = Some(TimeInForce::PostOnly);
    let a = placed(&mut e, vec![opts(), o]);
    let touch = req(&a, Method::Get, "/v1/public/orderbook/PERP_BTC_USDT");
    let a = reply(
        &mut e,
        &touch,
        1010.0,
        r#"{"asks":[{"price":58999}],"bids":[]}"#,
    );
    assert!(result(&a).unwrap_err().contains("touch"));
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
}
#[test]
fn fair_crossing_postonly_and_unimplemented_opening_conditional_reject_locally() {
    let mut e = engine();
    let mut o = opts();
    o.price = Some(60002.0);
    o.time_in_force = Some(TimeInForce::PostOnly);
    assert!(
        result(&placed(&mut e, vec![o]))
            .unwrap_err()
            .contains("fair")
    );
    let mut o = opts();
    o.kind = OrderType::Conditional;
    assert!(
        result(&placed(&mut e, vec![o]))
            .unwrap_err()
            .contains("conditional")
    );
}
#[test]
fn sized_stop_and_bracket_use_closing_hedge_side_and_native_trigger_sources() {
    let mut e = engine();
    e.store_mut().set_hedged(true);
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.sl_trigger_type = Some(StopTriggerType::LastPrice);
    let a = placed(&mut e, vec![o]);
    let r = req(&a, Method::Post, "/v3/algo/order");
    let b = r.body.as_ref().unwrap();
    assert!(b.contains(r#""algoType":"STOP""#));
    assert!(b.contains(r#""positionSide":"LONG""#));
    assert!(b.contains(r#""triggerPriceType":"MARKET_PRICE""#));
    assert!(b.contains(r#""quantity":"0.01""#));
    let mut e = engine();
    e.store_mut().set_hedged(true);
    let mut o = opts();
    o.stop_loss = Some(58000.0);
    o.take_profit = Some(62000.0);
    o.tp_trigger_type = Some(StopTriggerType::LastPrice);
    let a = placed(&mut e, vec![o]);
    let r = req(&a, Method::Post, "/v1/order");
    let a = reply(&mut e, &r, 1010.0, r#"{"success":true,"order_id":1}"#);
    let a = tick(&mut e, &a);
    let r = req(&a, Method::Post, "/v3/algo/order");
    let b = r.body.as_ref().unwrap();
    assert!(b.contains(r#""algoType":"POSITIONAL_TP_SL""#));
    assert!(b.contains(r#""side":"SELL""#));
    assert!(b.contains(r#""positionSide":"LONG""#));
    assert!(b.contains(r#""triggerPrice":"58000""#));
    assert!(b.contains(r#""triggerPrice":"62000""#));
}
#[test]
fn trailing_uses_full_position_and_absolute_callback_with_sl_reference() {
    let mut e = engine();
    e.store_mut().set_positions(vec![position()]);
    let mut o = opts();
    o.kind = OrderType::TrailingStopLoss;
    o.side = OrderSide::Sell;
    o.price = Some(59000.0);
    o.amount = 0.001;
    o.sl_trigger_type = Some(StopTriggerType::LastPrice);
    let a = placed(&mut e, vec![o]);
    let r = req(&a, Method::Post, "/v3/algo/order");
    let b = r.body.as_ref().unwrap();
    assert!(b.contains(r#""quantity":"0.02""#));
    assert!(b.contains(r#""callbackValue":"1000""#));
    assert!(b.contains(r#""triggerPriceType":"MARKET_PRICE""#));
}
#[test]
fn regular_price_only_amend_always_sends_total_quantity() {
    let mut e = engine();
    let mut o = order("12", OrderType::Limit);
    o.filled = 0.004;
    o.remaining = 0.006;
    e.store_mut().set_orders(vec![o]);
    e.handle(
        1000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "12".into(),
            price: Some(61010.0),
            amount: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Put, "/v3/order/12");
    assert!(r.body.as_ref().unwrap().contains(r#""quantity":"0.01""#));
    let a = reply(
        &mut e,
        &r,
        1010.0,
        r#"{"success":true,"data":{"success":true,"status":"EDIT_SENT"}}"#,
    );
    assert_eq!(result(&a).unwrap(), r#"["12"]"#);
    assert_eq!(e.store().order("12").unwrap().filled, 0.004);
}
#[test]
fn parent_cancel_deduplicates_siblings_and_symbol_cancel_covers_both_families() {
    let mut e = engine();
    let mut sl = order("12", OrderType::StopLoss);
    sl.parent_id = Some("10".into());
    let mut tp = order("13", OrderType::TakeProfit);
    tp.parent_id = Some("10".into());
    e.store_mut().set_orders(vec![sl, tp]);
    e.handle(
        1000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["12".into(), "13".into()],
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Delete, "/v3/algo/order/10");
    let a = reply(
        &mut e,
        &r,
        1010.0,
        r#"{"success":true,"status":"CANCEL_SENT"}"#,
    );
    assert_eq!(result(&a).unwrap(), "[]");
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: "BTCUSDT".into(),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    req(&a, Method::Delete, "/v3/merge/orders/pending/PERP_BTC_USDT");
}
#[test]
fn global_cancel_covers_regular_and_algo_families() {
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: String::new(),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Delete, "/v3/orders/pending");
    let a = reply(&mut e, &r, 1010.0, r#"{"success":true}"#);
    let a = tick(&mut e, &a);
    req(&a, Method::Delete, "/v3/algo/orders/pending");
}
#[test]
fn global_cancel_attempts_both_families_after_failures_without_retrying() {
    for failure in ["http", "transport", "nested"] {
        let mut e = engine();
        e.handle(
            1000.0,
            Event::Command(Command::CancelSymbolOrders {
                command: 1,
                symbol: String::new(),
            }),
        );
        let a: Vec<_> = e.drain_actions().collect();
        let regular = req(&a, Method::Delete, "/v3/orders/pending");
        match failure {
            "transport" => e.handle(
                1010.0,
                Event::HttpFailed {
                    request: regular.id,
                    error: "connection lost",
                },
            ),
            _ => e.handle(
                1010.0,
                Event::HttpResponse {
                    request: regular.id,
                    response: Response {
                        status: if failure == "http" { 503 } else { 200 },
                        headers: "",
                        body: if failure == "http" {
                            r#"{"success":false,"message":"regular failed"}"#
                        } else {
                            r#"{"success":true,"data":{"success":false}}"#
                        },
                    },
                },
            ),
        }
        let a: Vec<_> = e.drain_actions().collect();
        assert!(!a.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )));
        let a = tick(&mut e, &a);
        let algo = req(&a, Method::Delete, "/v3/algo/orders/pending");
        let a = reply(
            &mut e,
            &algo,
            1120.0,
            r#"{"success":false,"message":"algo failed"}"#,
        );
        let error = result(&a).unwrap_err();
        assert!(error.contains("/v3/orders/pending"), "{error}");
        assert!(error.contains("/v3/algo/orders/pending"), "{error}");
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method == Method::Delete))
        );
        // Finishing the failed command releases the FIFO for the next command.
        let a = placed(&mut e, vec![opts()]);
        let a = if a.iter().any(|a| matches!(a, Action::SetTimer { .. })) {
            tick(&mut e, &a)
        } else {
            a
        };
        req(&a, Method::Post, "/v1/order");
    }
}
#[test]
fn global_cancel_reports_algo_failure_after_regular_success() {
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 1,
            symbol: String::new(),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let regular = req(&a, Method::Delete, "/v3/orders/pending");
    let a = reply(&mut e, &regular, 1010.0, r#"{"success":true}"#);
    let a = tick(&mut e, &a);
    let algo = req(&a, Method::Delete, "/v3/algo/orders/pending");
    let a = reply(
        &mut e,
        &algo,
        1120.0,
        r#"{"success":false,"message":"algo failed"}"#,
    );
    assert!(result(&a).unwrap_err().contains("algo failed"));
}
#[test]
fn ambiguous_write_failure_never_retries_and_fifo_advances_to_next_command() {
    let mut e = engine();
    let a = placed(&mut e, vec![opts()]);
    let first = req(&a, Method::Post, "/v1/order");
    e.handle(
        1001.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![opts()],
        }),
    );
    assert_eq!(e.drain_actions().count(), 0);
    e.handle(
        1010.0,
        Event::HttpResponse {
            request: first.id,
            response: Response {
                status: 503,
                headers: "",
                body: r#"{"success":false,"message":"ambiguous write"}"#,
            },
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert!(result(&a).unwrap_err().contains("ambiguous"));
    let a = tick(&mut e, &a);
    let second = req(&a, Method::Post, "/v1/order");
    assert_ne!(first.id, second.id);
}
#[test]
fn leverage_commits_only_after_matching_authoritative_readback() {
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Post, "/v1/client/futures_leverage");
    let a = reply(&mut e, &r, 1010.0, r#"{"success":true}"#);
    let r = req(&a, Method::Get, "/v1/client/futures_leverage");
    assert!(
        e.store()
            .setting("BTCUSDT")
            .is_none_or(|s| s.leverage != 20.0)
    );
    let a = reply(
        &mut e,
        &r,
        1020.0,
        r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","margin_mode":"CROSS","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"20"}]}}"#,
    );
    assert!(result(&a).is_ok());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}
#[test]
fn mode_readback_retries_reads_only_and_never_invents_confirmation() {
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::SetPositionMode {
            command: 1,
            hedged: true,
            symbol: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Post, "/v1/client/position_mode");
    let mut a = reply(&mut e, &r, 1010.0, r#"{"success":true}"#);
    for attempt in 0..3 {
        let r = req(&a, Method::Get, "/v3/accountinfo");
        a = reply(
            &mut e,
            &r,
            1020.0 + attempt as f64 * 300.0,
            r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"ONE_WAY"}}"#,
        );
        if attempt < 2 {
            a = tick(&mut e, &a);
            assert!(
                !a.iter()
                    .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
            );
        }
    }
    assert!(result(&a).is_err());
    assert!(!e.store().hedged());
}
fn positional_engine() -> TestEngine {
    let mut e = engine();
    e.start(0.0);
    let r = prepared_seed_requests(&mut e, 0.).remove(0);
    response(&mut e, &r, 10.0, MARKETS);
    let mut early = prepared_seed_actions(&mut e, 10.);
    let ticker = early
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/public/futures") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    response(&mut e, &ticker, 20.0, TICKERS);
    early.extend(e.drain_actions());
    let a = early;
    let regular = a
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.contains("/v1/orders?").then(|| r.clone())
            } else {
                None
            }
        })
        .unwrap();
    let algos = a
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.contains("/v3/algo/orders?").then(|| r.clone())
            } else {
                None
            }
        })
        .unwrap();
    reply(
        &mut e,
        &regular,
        30.0,
        r#"{"success":true,"rows":[],"meta":{"total":0,"records_per_page":100,"current_page":1}}"#,
    );
    reply(
        &mut e,
        &algos,
        40.0,
        r#"{"success":true,"data":{"rows":[{"symbol":"PERP_BTC_USDT","rootAlgoOrderId":100,"algoType":"POSITIONAL_TP_SL","childOrders":[{"algoOrderId":101,"algoType":"STOP_LOSS","algoStatus":"NEW","side":"SELL","quantity":0,"triggerPrice":58000,"triggerPriceType":"MARK_PRICE","totalExecutedQuantity":0,"reduceOnly":true},{"algoOrderId":102,"algoType":"TAKE_PROFIT","algoStatus":"NEW","side":"SELL","quantity":0,"triggerPrice":65000,"triggerPriceType":"MARKET_PRICE","totalExecutedQuantity":0,"reduceOnly":true}]}],"meta":{"total":1,"records_per_page":100,"current_page":1}}}"#,
    );
    e
}
#[test]
fn full_position_stop_replacement_retains_sibling_and_its_independent_trigger_source() {
    let mut e = positional_engine();
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.amount = 0.0;
    o.price = Some(58500.0);
    let a = placed(&mut e, vec![o]);
    let r = req(&a, Method::Delete, "/v3/algo/order/100");
    let a = reply(&mut e, &r, 1010.0, r#"{"success":true}"#);
    let a = tick(&mut e, &a);
    let r = req(&a, Method::Post, "/v3/algo/order");
    let body = r.body.as_ref().unwrap();
    assert!(body.contains(r#""triggerPrice":"58500""#));
    assert!(body.contains(r#""triggerPrice":"65000""#));
    assert!(body.contains(r#""triggerPriceType":"MARKET_PRICE""#));
}
#[test]
fn positional_amend_targets_child_under_parent_and_sized_algo_amount_is_native_quantity() {
    let mut e = positional_engine();
    e.handle(
        1000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "101".into(),
            price: Some(58500.0),
            amount: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Put, "/v3/algo/order/100");
    let body = r.body.as_ref().unwrap();
    assert!(body.contains(r#""childOrders":[{"algoOrderId":"101","triggerPrice":"58500"}]"#));
    let mut e = engine();
    let mut o = order("22", OrderType::StopLoss);
    o.parent_id = Some("22".into());
    e.store_mut().set_orders(vec![o]);
    e.handle(
        1000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "22".into(),
            price: None,
            amount: Some(0.02),
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Put, "/v3/algo/order/22");
    assert_eq!(r.body.as_deref(), Some(r#"{"quantity":"0.02"}"#));
}
#[test]
fn native_snowflake_ids_are_preserved_for_results_amend_and_cancel() {
    let mut e = engine();
    let a = placed(&mut e, vec![opts()]);
    let r = req(&a, Method::Post, "/v1/order");
    let a = reply(
        &mut e,
        &r,
        1010.0,
        r#"{"success":true,"order_id":9007199254740993}"#,
    );
    assert_eq!(result(&a).unwrap(), r#"["9007199254740993"]"#);
    e.store_mut()
        .set_orders(vec![order("9007199254740993", OrderType::Limit)]);
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["9007199254740993".into()],
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Delete, "/v1/order");
    assert!(
        r.body
            .as_ref()
            .unwrap()
            .contains("order_id=9007199254740993")
    );
}
#[test]
fn nested_mutation_failure_is_an_error_and_does_not_send_later_batch_legs() {
    let mut e = engine();
    let a = placed(&mut e, vec![opts(), opts()]);
    let r = req(&a, Method::Post, "/v1/order");
    let a = reply(
        &mut e,
        &r,
        1010.0,
        r#"{"success":true,"data":{"success":false},"order_id":123}"#,
    );
    assert!(result(&a).is_err());
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.method==Method::Post))
    );
}
#[test]
fn rounded_zero_prices_and_zero_trailing_distance_reject_before_mutation() {
    let mut e = engine();
    let mut o = opts();
    o.price = Some(0.00001);
    assert!(result(&placed(&mut e, vec![o])).is_err());
    e.store_mut().set_positions(vec![position()]);
    let mut o = opts();
    o.kind = OrderType::TrailingStopLoss;
    o.side = OrderSide::Sell;
    o.price = Some(60000.0);
    assert!(
        result(&placed(&mut e, vec![o]))
            .unwrap_err()
            .contains("distance")
    );
}
#[test]
fn amendments_reject_rounded_zero_prices_and_sized_algo_invalid_lots_without_write() {
    for (kind, price, amount) in [
        (OrderType::Limit, Some(0.01), None),
        (OrderType::StopLoss, Some(0.01), None),
        (OrderType::StopLoss, None, Some(0.0001)),
        (OrderType::StopLoss, None, Some(200.0)),
    ] {
        let mut e = engine();
        e.store_mut().set_orders(vec![order("12", kind)]);
        e.handle(
            1000.0,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: "12".into(),
                price,
                amount,
            }),
        );
        let a: Vec<_> = e.drain_actions().collect();
        assert!(result(&a).is_err());
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
#[test]
fn trailing_amend_refreshes_full_position_quantity_after_cancel_ack() {
    let mut e = engine();
    e.store_mut().set_positions(vec![position()]);
    let mut trailing = order("12", OrderType::TrailingStopLoss);
    trailing.price = 59000.0;
    e.store_mut().set_orders(vec![trailing]);
    e.handle(
        1000.0,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: "12".into(),
            price: Some(59500.0),
            amount: None,
        }),
    );
    let a: Vec<_> = e.drain_actions().collect();
    let r = req(&a, Method::Delete, "/v3/algo/order/12");
    e.store_mut().update_positions(|p| p.contracts = 0.03);
    let a = reply(&mut e, &r, 1010.0, r#"{"success":true}"#);
    let a = tick(&mut e, &a);
    let r = req(&a, Method::Post, "/v3/algo/order");
    assert!(r.body.as_ref().unwrap().contains(r#""quantity":"0.03""#));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified) in [
        (
            503,
            r#"{"success":false,"message":"ambiguous write"}"#,
            unknown,
        ),
        (
            400,
            r#"{"success":false,"code":-1005,"message":"invalid params"}"#,
            (Some(ErrorKind::Rejected), Some(Outcome::Rejected)),
        ),
        (
            429,
            r#"{"success":false,"code":-1003,"message":"too many"}"#,
            (Some(ErrorKind::RateLimited), Some(Outcome::Rejected)),
        ),
        (
            200,
            r#"{"success":false,"code":-1012,"message":"rpc exception"}"#,
            unknown,
        ),
        (
            200,
            r#"{"success":false,"code":-1007,"message":"duplicate"}"#,
            (None, None),
        ),
        (200, "<html>", unknown),
        (200, r#"{"message":"ok"}"#, unknown),
        (200, r#"{"success":true,"data":{}}"#, unknown),
    ] {
        let mut e = engine();
        let a = placed(&mut e, vec![opts()]);
        let r = req(&a, Method::Post, "/v1/order");
        e.handle(
            1010.0,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let a: Vec<_> = e.drain_actions().collect();
        let error = failure(&a);
        assert_eq!(
            (error.kind, error.outcome),
            classified,
            "{status} {body}: {}",
            error.message
        );
        assert_eq!(result(&a), Err(error.legacy_text()));
    }
    // Transport loss on a write is Unknown.
    let mut e = engine();
    let a = placed(&mut e, vec![opts()]);
    let r = req(&a, Method::Post, "/v1/order");
    e.handle(
        1010.0,
        Event::HttpFailed {
            request: r.id,
            error: "reset",
        },
    );
    let a: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        (failure(&a).kind, failure(&a).outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

#[test]
fn accepted_leverage_write_reports_unconfirmed_readback_without_resending() {
    let mut e = engine();
    e.store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 10.0);
    e.handle(
        1000.0,
        Event::Command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: None,
        }),
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    let write = req(&a, Method::Post, "/v1/client/futures_leverage");
    let mut a = reply(&mut e, &write, 1010.0, r#"{"success":true}"#);
    for attempt in 0..3 {
        let read = req(&a, Method::Get, "/v1/client/futures_leverage");
        a = reply(
            &mut e,
            &read,
            1020.0 + attempt as f64 * 300.0,
            r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","margin_mode":"CROSS","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"10"}]}}"#,
        );
        if attempt < 2 {
            a = tick(&mut e, &a);
        }
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r) if r.method==Method::Post))
        );
    }
    let error = failure(&a);
    assert!(
        error.message.contains("write was accepted"),
        "{}",
        error.message
    );
    assert!(error.message.contains("unconfirmed"));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 10.0);
}

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

fn allocation_engine(maximum: f64, step: f64) -> TestEngine {
    let mut e = engine();
    let mut markets = e.store().markets().to_vec();
    let market = markets.iter_mut().find(|m| m.symbol == "BTCUSDT").unwrap();
    market.precision.amount = step;
    market.limits.amount.min = step;
    market.limits.amount.max = maximum;
    e.store_mut().set_markets(markets);
    e
}

#[test]
fn legacy_split_lots_match_ts_sequences_and_sums() {
    // TS woo.exchange.ts:2244/2303 normalizes before woo.order-state.ts:5-19.
    let cases: &[(f64, f64, f64, &[f64], u64)] = &[
        (26.0, 5.0, 1.0, &[5.0, 5.0, 4.0, 4.0, 4.0, 4.0], 26),
        (11.0, 5.0, 1.0, &[4.0, 4.0, 3.0], 11),
        (12.0, 5.0, 1.0, &[4.0, 4.0, 4.0], 12),
        (10.0, 5.0, 1.0, &[5.0, 5.0], 10),
        (7.0, 3.0, 0.5, &[2.5, 2.5, 2.0], 14),
        // KNOWN-ISSUE: legacy TS under-allocates (sum 1 of requested 1.0004); kept for exact TS parity per Sam's decision
        (1.0004, 0.3, 0.001, &[0.25, 0.25, 0.25, 0.25], 1000),
        // KNOWN-ISSUE: legacy TS rounds up (sum 1.001 of requested 1.0005); kept for exact TS parity per Sam's decision
        (1.0005, 0.3, 0.001, &[0.251, 0.25, 0.25, 0.25], 1001),
    ];
    for &(amount, maximum, step, expected, expected_ticks) in cases {
        let mut e = allocation_engine(maximum, step);
        let mut o = opts();
        o.amount = amount;
        let mut actions = placed(&mut e, vec![o]);
        let mut lots = Vec::new();
        for index in 0..expected.len() {
            let request = req(&actions, Method::Post, "/v1/order");
            let quantity = request
                .body
                .as_deref()
                .unwrap()
                .split('&')
                .find_map(|part| part.strip_prefix("order_quantity="))
                .unwrap()
                .parse::<f64>()
                .unwrap();
            assert!(quantity > 0.0 && quantity <= maximum);
            lots.push(quantity);
            actions = reply(
                &mut e,
                &request,
                1001.0 + index as f64,
                r#"{"success":true,"order_id":12}"#,
            );
            if index + 1 < expected.len() {
                actions = tick(&mut e, &actions);
            }
        }
        assert_eq!(lots, expected, "{amount}/{maximum}/{step}");
        let allocated_ticks = lots
            .iter()
            .map(|lot| (lot / step).round() as u64)
            .sum::<u64>();
        assert_eq!(allocated_ticks, expected_ticks, "{amount}/{maximum}/{step}");
        let ids: Vec<String> = serde_json::from_str(&result(&actions).unwrap()).unwrap();
        assert_eq!(ids.len(), expected.len());
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
        );
    }
}

#[test]
fn legacy_sized_stop_normalizes_before_splitting() {
    // TS woo.exchange.ts:2390/2417 uses the same normalization for sized stops.
    // KNOWN-ISSUE: legacy TS rounds up (sum 1.001 of requested 1.0005); kept for exact TS parity per Sam's decision
    let mut e = allocation_engine(0.3, 0.001);
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.amount = 1.0005;
    let mut actions = placed(&mut e, vec![o]);
    let mut lots = Vec::new();
    for index in 0..4 {
        let request = req(&actions, Method::Post, "/v3/algo/order");
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        lots.push(body["quantity"].as_str().unwrap().parse::<f64>().unwrap());
        assert_eq!(body["algoType"], "STOP");
        actions = reply(
            &mut e,
            &request,
            1001.0 + f64::from(index),
            r#"{"success":true,"data":{"rows":[{"orderId":"12"}]}}"#,
        );
        if index < 3 {
            actions = tick(&mut e, &actions);
        }
    }
    assert_eq!(lots, [0.251, 0.25, 0.25, 0.25]);
    assert_eq!(
        lots.iter()
            .map(|lot| (lot * 1000.0).round() as u64)
            .sum::<u64>(),
        1001
    );
    assert!(result(&actions).is_ok());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
    );
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
fn leverage_readback_without_margin_mode_confirms_only_leverage() {
    for margin_type in [None, Some(MarginType::Cross), Some(MarginType::Isolated)] {
        let mut e = engine();
        e.store_mut().update_setting("BTCUSDT", |s| {
            s.leverage = 10.0;
            s.margin_type = margin_type;
        });
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
        let a = reply(&mut e, &write, 1010.0, r#"{"success":true}"#);
        let read = req(&a, Method::Get, "/v1/client/futures_leverage");
        let a = reply(
            &mut e,
            &read,
            1020.0,
            r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"20"}]}}"#,
        );
        assert!(
            a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. }))),
            "{a:?}"
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
        );
        let setting = e.store().setting("BTCUSDT").unwrap();
        assert_eq!(setting.leverage, 20.0);
        assert_eq!(setting.buy_leverage, Some(20.0));
        assert_eq!(setting.sell_leverage, Some(20.0));
        assert_eq!(
            setting.margin_type, margin_type,
            "missing mode is not confirmation"
        );
    }
}

#[test]
fn absent_margin_mode_does_not_relax_leverage_readback_scope_or_value() {
    let valid = r#"{"success":true,"data":{"symbol":"PERP_BTC_USDT","position_mode":"ONE_WAY","details":[{"position_side":"BOTH","leverage":"20"}]}}"#;
    for body in [
        valid.replace("PERP_BTC_USDT", "PERP_ETH_USDT"),
        valid.replace(
            "\"position_mode\"",
            "\"margin_mode\":\"ISOLATED\",\"position_mode\"",
        ),
        valid.replace("ONE_WAY", "HEDGE_MODE"),
        valid.replace("BOTH", "LONG"),
        valid.replace("\"20\"", "\"10\""),
        valid.replace("\"20\"", "\"0\""),
        valid.replace("\"leverage\":\"20\"", "\"other\":\"20\""),
        valid.replace("\"position_mode\":\"ONE_WAY\",", ""),
        valid.replace(r#"{"position_side":"BOTH","leverage":"20"}"#, ""),
    ] {
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
            a = reply(&mut e, &read, 1020.0 + attempt as f64 * 300.0, &body);
            if attempt < 2 {
                a = tick(&mut e, &a);
            }
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
            );
        }
        let error = failure(&a);
        assert_eq!(
            error.outcome,
            Some(safe_cex_engine::Outcome::Unknown),
            "{body}"
        );
        assert!(
            error.message.contains("readback remains unconfirmed"),
            "{error:?}"
        );
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 10.0);
    }
}

#[test]
fn margin_mode_mutation_is_not_confirmed_by_a_leverage_only_reply() {
    let mut e = engine();
    e.handle(
        1000.0,
        Event::Command(Command::SetMarginType {
            command: 1,
            symbol: "BTCUSDT".into(),
            isolated: true,
        }),
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(failure(&a).outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert!(
        failure(&a)
            .message
            .contains("isolated margin mutation is not implemented")
    );
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
fn ordinary_write_rate_limits_are_unknown_without_resigning_or_resending() {
    assert_write_rate_limits_are_unknown(false);
}

#[test]
fn protective_write_rate_limits_are_unknown_without_resigning_or_resending() {
    assert_write_rate_limits_are_unknown(true);
}

fn assert_write_rate_limits_are_unknown(protective: bool) {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (status, body, code) in [
        (
            429,
            r#"{"success":false,"code":-1003,"message":"too many"}"#,
            Some("-1003"),
        ),
        (
            200,
            r#"{"success":false,"code":-1003,"message":"too many"}"#,
            Some("-1003"),
        ),
        (
            401,
            r#"{"success":false,"code":-1003,"message":"too many"}"#,
            Some("-1003"),
        ),
        (429, r#"{"success":false,"message":"too many"}"#, None),
        (429, "<html>too many</html>", None),
        (
            429,
            r#"{"success":false,"code":-1001,"message":"too many"}"#,
            Some("-1001"),
        ),
    ] {
        let mut e = engine();
        let mut o = opts();
        let path = if protective {
            o.kind = OrderType::StopLoss;
            o.side = OrderSide::Sell;
            o.reduce_only = Some(true);
            "/v3/algo/order"
        } else {
            "/v1/order"
        };
        let mut actions = placed(&mut e, vec![o]);
        let write = req(&actions, Method::Post, path);
        e.handle(
            1010.0,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let response_actions: Vec<_> = e.drain_actions().collect();
        let error = failure(&response_actions).clone();
        assert_eq!(
            error.message,
            if body.starts_with('<') {
                "WOO response: expected value at line 1 column 1"
            } else {
                "WOO request failed: too many"
            }
        );
        assert_eq!(error.venue_code.as_deref(), code);
        assert_eq!(result(&response_actions), Err(error.legacy_text()));
        actions.extend(response_actions);

        // Exercise any queued work and a late duplicate answer, not just the immediate reply.
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        for (timer, at) in timers {
            e.handle(at.max(1011.0), Event::TimerFired { timer });
            actions.extend(e.drain_actions());
        }
        e.handle(
            60_000.0,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        actions.extend(e.drain_actions());
        let physical: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(physical.len(), 1, "no second physical request: {actions:?}");
        assert_eq!(
            physical
                .iter()
                .flat_map(|r| &r.headers)
                .filter(|(name, value)| *name == "x-api-signature" && !value.is_empty())
                .count(),
            1,
            "exactly one signed packet"
        );
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
                .count(),
            1
        );
        assert_eq!(
            (error.kind, error.outcome),
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            "{status} {body}"
        );
    }
}

#[test]
fn rate_limited_confirmation_reads_still_retry_without_repeating_the_write() {
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
    let a = reply(&mut e, &write, 1010.0, r#"{"success":true}"#);
    let read = req(&a, Method::Get, "/v1/client/futures_leverage");
    e.handle(
        1020.0,
        Event::HttpResponse {
            request: read.id,
            response: Response {
                status: 429,
                headers: "",
                body: r#"{"success":false,"code":-1003,"message":"too many"}"#,
            },
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    let a = tick(&mut e, &a);
    req(&a, Method::Get, "/v1/client/futures_leverage");
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
    );
}

#[test]
fn invalid_orders_remain_not_sent_before_any_signed_request() {
    for protective in [false, true] {
        let mut e = engine();
        let mut o = opts();
        o.amount = -1.0;
        if protective {
            o.kind = OrderType::StopLoss;
            o.side = OrderSide::Sell;
        }
        let a = placed(&mut e, vec![o]);
        assert_eq!(failure(&a).outcome, Some(safe_cex_engine::Outcome::NotSent));
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn unconfirmed_rate_limited_mode_write_keeps_unknown_kind_and_code() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for status in [200, 429, 408, 503] {
        for readback in [
            "mismatch",
            "wrong_account",
            "transport",
            "rejected",
            "malformed",
        ] {
            let mut e = engine();
            e.handle(
                1000.0,
                Event::Command(Command::SetPositionMode {
                    command: 1,
                    hedged: true,
                    symbol: None,
                }),
            );
            let mut actions = e.drain_actions().collect::<Vec<_>>();
            let write = req(&actions, Method::Post, "/v1/client/position_mode");
            e.handle(1010.0, Event::HttpResponse {
                request: write.id,
                response: Response { status, headers: "", body: r#"{"success":false,"code":-1003,"message":"position mode remains unchanged"}"# },
            });
            let mut answer = e.drain_actions().collect::<Vec<_>>();
            actions.extend(answer.iter().cloned());
            for attempt in 0..3 {
                let read = req(&answer, Method::Get, "/v3/accountinfo");
                let now = 1020.0 + attempt as f64 * 300.0;
                if readback == "transport" {
                    e.handle(
                        now,
                        Event::HttpFailed {
                            request: read.id,
                            error: "connection lost",
                        },
                    );
                } else {
                    let (status, body) = match readback {
                        "mismatch" => (
                            200,
                            r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"ONE_WAY"}}"#,
                        ),
                        "wrong_account" => (
                            200,
                            r#"{"success":true,"data":{"accountMode":"SPOT","positionMode":"HEDGE_MODE"}}"#,
                        ),
                        "rejected" => (
                            429,
                            r#"{"success":false,"code":-1003,"message":"too many"}"#,
                        ),
                        _ => (200, "<html>"),
                    };
                    e.handle(
                        now,
                        Event::HttpResponse {
                            request: read.id,
                            response: Response {
                                status,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                answer = e.drain_actions().collect();
                actions.extend(answer.iter().cloned());
                if attempt < 2 {
                    answer = tick(&mut e, &answer);
                    actions.extend(answer.iter().cloned());
                }
            }
            // A mode setter may complete on verified end state, never on unconfirmed text.
            let error = failure(&answer);
            assert_eq!(
                (error.kind, error.outcome, error.venue_code.as_deref()),
                (
                    Some(ErrorKind::RateLimited),
                    Some(Outcome::Unknown),
                    Some("-1003")
                ),
                "{status} {readback}"
            );
            assert!(error.message.contains("position mode remains unchanged"));
            assert!(error.message.contains("readback remains unconfirmed"));
            assert!(!e.store().hedged());
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
                    .count(),
                1
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Http(r) if r.method == Method::Get))
                    .count(),
                3
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
                    .count(),
                1
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
            );
        }
    }
}

#[test]
fn verified_mode_end_state_can_complete_even_after_rate_limit() {
    for (status, response) in [
        (200, "accepted"),
        (200, "unchanged"),
        (200, "rate_limited"),
        (429, "rate_limited"),
        (408, "rate_limited"),
        (503, "rate_limited"),
    ] {
        let mut e = engine();
        e.handle(
            1000.0,
            Event::Command(Command::SetPositionMode {
                command: 1,
                hedged: true,
                symbol: None,
            }),
        );
        let a = e.drain_actions().collect::<Vec<_>>();
        let write = req(&a, Method::Post, "/v1/client/position_mode");
        let body = match response {
            "accepted" => r#"{"success":true}"#,
            "unchanged" => {
                r#"{"success":false,"code":-1005,"message":"position mode remains unchanged"}"#
            }
            _ => r#"{"success":false,"code":-1003,"message":"position mode remains unchanged"}"#,
        };
        e.handle(
            1010.0,
            Event::HttpResponse {
                request: write.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let a = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. })))
        );
        let read = req(&a, Method::Get, "/v3/accountinfo");
        // SetPositionMode's end state is verified by a successful, exact mode read.
        let a = reply(
            &mut e,
            &read,
            1020.0,
            r#"{"success":true,"data":{"accountMode":"FUTURES","positionMode":"HEDGE_MODE"}}"#,
        );
        assert!(result(&a).is_ok());
        assert!(e.store().hedged());
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        );
    }
}

#[test]
fn cancel_all_preserves_rate_limit_kind_code_and_unknown_across_both_legs() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for rate_on_regular in [false, true] {
        for peer in ["accepted", "rejected", "unknown", "rate_limited"] {
            let mut e = engine();
            e.handle(
                1000.0,
                Event::Command(Command::CancelSymbolOrders {
                    command: 1,
                    symbol: String::new(),
                }),
            );
            let mut actions = e.drain_actions().collect::<Vec<_>>();
            let regular = req(&actions, Method::Delete, "/v3/orders/pending");
            let mut answer = Vec::new();
            let mut request = regular;
            for index in 0..2 {
                if index == 1 {
                    let next = tick(&mut e, &answer);
                    request = req(&next, Method::Delete, "/v3/algo/orders/pending");
                    actions.extend(next);
                }
                let is_rate = (index == 0) == rate_on_regular || peer == "rate_limited";
                if !is_rate && peer == "unknown" {
                    e.handle(
                        1010.0 + index as f64 * 110.0,
                        Event::HttpFailed {
                            request: request.id,
                            error: "connection lost",
                        },
                    );
                } else {
                    let (status, body) = if is_rate {
                        (
                            429,
                            r#"{"success":false,"code":-1003,"message":"too many"}"#,
                        )
                    } else if peer == "rejected" {
                        (
                            400,
                            r#"{"success":false,"code":-1005,"message":"invalid params"}"#,
                        )
                    } else {
                        (200, r#"{"success":true}"#)
                    };
                    e.handle(
                        1010.0 + index as f64 * 110.0,
                        Event::HttpResponse {
                            request: request.id,
                            response: Response {
                                status,
                                headers: "",
                                body,
                            },
                        },
                    );
                }
                answer = e.drain_actions().collect();
                if index == 0 {
                    assert!(
                        !answer
                            .iter()
                            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
                    );
                }
                actions.extend(answer.iter().cloned());
            }
            let error = failure(&answer);
            assert_eq!(
                (error.kind, error.outcome, error.venue_code.as_deref()),
                (
                    Some(ErrorKind::RateLimited),
                    Some(Outcome::Unknown),
                    Some("-1003")
                ),
                "{rate_on_regular} {peer}"
            );
            let path = if rate_on_regular {
                "/v3/orders/pending"
            } else {
                "/v3/algo/orders/pending"
            };
            assert!(
                error
                    .message
                    .contains(&format!("{path}: WOO request failed: too many"))
            );
            if peer != "accepted" {
                assert!(error.message.contains("/v3/orders/pending:"));
                assert!(error.message.contains("/v3/algo/orders/pending:"));
            }
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Http(r) if r.method == Method::Delete))
                    .count(),
                2
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandFailed { command: 1, .. })
                    ))
                    .count(),
                1
            );
            assert!(!answer.iter().any(|a| matches!(a, Action::Http(_))));
        }
    }
}

#[test]
fn ambiguous_http_status_keeps_native_rate_limit_kind_without_resending() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for protective in [false, true] {
        for status in [408, 503] {
            let mut e = engine();
            let mut o = opts();
            if protective {
                o.kind = OrderType::StopLoss;
                o.side = OrderSide::Sell;
            }
            let a = placed(&mut e, vec![o]);
            let path = if protective {
                "/v3/algo/order"
            } else {
                "/v1/order"
            };
            let write = req(&a, Method::Post, path);
            e.handle(
                1010.0,
                Event::HttpResponse {
                    request: write.id,
                    response: Response {
                        status,
                        headers: "",
                        body: r#"{"success":false,"code":-1003,"message":"too many"}"#,
                    },
                },
            );
            let a = e.drain_actions().collect::<Vec<_>>();
            let error = failure(&a);
            assert_eq!(
                (error.kind, error.outcome, error.venue_code.as_deref()),
                (
                    Some(ErrorKind::RateLimited),
                    Some(Outcome::Unknown),
                    Some("-1003")
                )
            );
            assert!(
                !a.iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
            );
        }
    }
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
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
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

#[test]
fn a_refused_lot_reports_the_accepted_lot_as_partial() {
    let mut e = engine();
    let mut o = opts();
    o.amount = 201.001;
    let a = placed(&mut e, vec![o]);
    let first = req(&a, Method::Post, "/v1/order");
    let a = reply(&mut e, &first, 1001.0, r#"{"success":true,"order_id":1}"#);
    let a = tick(&mut e, &a);
    let second = req(&a, Method::Post, "/v1/order");
    let a = reply(
        &mut e,
        &second,
        1002.0,
        r#"{"success":false,"code":-1101,"message":"too small"}"#,
    );
    let error = a
        .iter()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { error, .. }) => {
                Some(error.clone())
            }
            _ => None,
        })
        .expect("the command failed");
    assert_eq!(error.partial.unwrap().order_ids, ["1"]);
}

#[test]
fn split_unknown_contract_ordinary() {
    assert_split_unknown_contract(false);
}

#[test]
fn split_unknown_contract_sized_stop() {
    assert_split_unknown_contract(true);
}

#[test]
fn split_definite_refusal_keeps_classification_and_emits_no_unknown() {
    for sized_stop in [false, true] {
        let mut e = allocation_engine(0.3, 0.001);
        let mut order = opts();
        order.amount = 1.;
        if sized_stop {
            order.kind = OrderType::StopLoss;
            order.side = OrderSide::Sell;
        }
        let path = if sized_stop {
            "/v3/algo/order"
        } else {
            "/v1/order"
        };
        let actions = placed(&mut e, vec![order]);
        let request = req(&actions, Method::Post, path);
        let actions = reply(
            &mut e,
            &request,
            1001.,
            r#"{"success":false,"code":-1101,"message":"too small"}"#,
        );
        let error = actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Rejected));
        assert_eq!(error.venue_code.as_deref(), Some("-1101"));
        let journal = error.elements.as_ref().unwrap();
        assert_eq!(journal.len(), 4);
        assert!(
            matches!(&journal[0].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
        );
        assert!(journal[1..].iter().all(|row| matches!(
            row.result,
            safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
        )));
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandError { .. })) || matches!(a, Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")));
    }
}

fn assert_split_unknown_contract(sized_stop: bool) {
    use safe_cex_engine::write_plan::ElementResult;
    use safe_cex_engine::{Outcome, UNKNOWN_OUTCOME_TEXT};
    // KNOWN-ISSUE: legacy TS rounds up (sum 1.001 of requested 1.0005); kept for exact TS parity per Sam's decision
    for failed_index in [1, 0, 3] {
        for delivery in ["native", "malformed", "transport", "503"] {
            let mut e = allocation_engine(0.3, 0.001);
            let mut order = opts();
            order.amount = 1.0005;
            if sized_stop {
                order.kind = OrderType::StopLoss;
                order.side = OrderSide::Sell;
            }
            let path = if sized_stop {
                "/v3/algo/order"
            } else {
                "/v1/order"
            };
            let mut actions = placed(&mut e, vec![order]);
            let mut history = Vec::new();
            let mut quantities = Vec::new();
            let mut placed_ids = Vec::new();
            let failed_request = loop {
                let request = req(&actions, Method::Post, path);
                let quantity = if sized_stop {
                    serde_json::from_str::<serde_json::Value>(request.body.as_deref().unwrap())
                        .unwrap()["quantity"]
                        .as_str()
                        .unwrap()
                        .parse::<f64>()
                        .unwrap()
                } else {
                    request
                        .body
                        .as_deref()
                        .unwrap()
                        .split('&')
                        .find_map(|p| p.strip_prefix("order_quantity="))
                        .unwrap()
                        .parse()
                        .unwrap()
                };
                quantities.push(quantity);
                history.extend(actions);
                if placed_ids.len() == failed_index {
                    break request;
                }
                let id = (12 + placed_ids.len()).to_string();
                let body = if sized_stop {
                    format!(r#"{{"success":true,"data":{{"rows":[{{"orderId":"{id}"}}]}}}}"#)
                } else {
                    format!(r#"{{"success":true,"order_id":{id}}}"#)
                };
                actions = reply(&mut e, &request, 1001. + placed_ids.len() as f64, &body);
                placed_ids.push(id);
                history.extend(actions.clone());
                actions = tick(&mut e, &actions);
            };
            assert_eq!(quantities, [0.251, 0.25, 0.25, 0.25][..=failed_index]);
            assert_eq!(
                (quantities.iter().sum::<f64>() * 1000.).round() as u64,
                251 + 250 * failed_index as u64
            );
            if delivery == "transport" {
                e.handle(
                    2000.,
                    Event::HttpFailed {
                        request: failed_request.id,
                        error: "synthetic https://venue.test/order?signature=payload body={}",
                    },
                );
            } else {
                e.handle(2000., Event::HttpResponse {
                    request: failed_request.id,
                    response: Response {
                        status: if delivery == "503" { 503 } else { 200 },
                        headers: "",
                        body: if delivery == "malformed" { "{}" } else { r#"{"success":false,"code":-1000,"message":"synthetic ambiguous outcome"}"# },
                    },
                });
            }
            let actions: Vec<_> = e.drain_actions().collect();
            let errors: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(errors.len(), 1);
            let error = errors[0];
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(
                error
                    .partial
                    .as_ref()
                    .map(|p| p.order_ids.clone())
                    .unwrap_or_default(),
                placed_ids
            );
            assert!(error.message.contains(UNKNOWN_OUTCOME_TEXT), "{error:?}");
            let rows = error.elements.as_ref().expect("indexed split journal");
            assert_eq!(rows.len(), 4);
            for (index, row) in rows.iter().enumerate() {
                assert_eq!(row.element.index, index);
                assert_eq!(row.element.client_order_id, None);
                match &row.result {
                    ElementResult::Placed { order_ids } if index < failed_index => {
                        assert_eq!(order_ids, &[placed_ids[index].clone()])
                    }
                    ElementResult::Unknown { error } if index == failed_index => {
                        assert_eq!(error.outcome, Some(Outcome::Unknown))
                    }
                    ElementResult::NotSent { error: None } if index > failed_index => (),
                    row => panic!("incorrect lot{index}: {row:?}"),
                }
            }
            let diagnostics: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandError {
                        command: 1,
                        message,
                    }) => Some(message),
                    _ => None,
                })
                .collect();
            assert_eq!(diagnostics.len(), 1);
            assert!(diagnostics[0].contains(UNKNOWN_OUTCOME_TEXT));
            let events: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::Event(event))
                        if event.code == "order.outcome_unknown" =>
                    {
                        Some(event)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].message, UNKNOWN_OUTCOME_TEXT);
            assert!(events[0].fields.contains(&("outcome", "unknown".into())));
            assert!(events[0].fields.contains(&("command", "woo-1".into())));
            assert!(
                events[0]
                    .fields
                    .contains(&("placedOrderIds", placed_ids.join(",").into()))
            );
            assert!(
                events[0]
                    .fields
                    .contains(&("unknownIndices", failed_index.to_string().into()))
            );
            let request_id = failed_request.id.0.to_string();
            // ^[0-9]{1,20}$: retain the physical request ID without float conversion.
            assert!(
                (1..=20).contains(&request_id.len())
                    && request_id.bytes().all(|b| b.is_ascii_digit())
            );
            assert!(events[0].fields.contains(&("request", request_id.into())));
            let status = events[0]
                .fields
                .iter()
                .find(|(key, _)| *key == "status")
                .map(|(_, value)| value.clone());
            assert_eq!(
                status,
                (delivery != "transport")
                    .then(|| if delivery == "503" { 503u32 } else { 200u32 }.into())
            );
            for (key, value) in &events[0].fields {
                assert!(
                    [
                        "command",
                        "outcome",
                        "placedOrderIds",
                        "unknownIndices",
                        "status",
                        "request"
                    ]
                    .contains(key)
                );
                let text = format!("{key} {value:?}").to_lowercase();
                for forbidden in [
                    "http",
                    "/v1",
                    "/v3",
                    "?",
                    "&",
                    "=",
                    "body",
                    "payload",
                    "signature",
                    "fixture",
                    "synthetic",
                ] {
                    assert!(!text.contains(forbidden), "unsafe field {key}");
                }
            }
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
            );
            history.extend(actions);
            e.handle(
                3000.,
                Event::HttpFailed {
                    request: failed_request.id,
                    error: "late failure",
                },
            );
            response(
                &mut e,
                &failed_request,
                3001.,
                r#"{"success":true,"order_id":999}"#,
            );
            for timer in history.iter().filter_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(*timer),
                _ => None,
            }) {
                e.handle(20000., Event::TimerFired { timer });
            }
            let late: Vec<_> = e.drain_actions().collect();
            assert!(!late.iter().any(|a| matches!(a,
                Action::Http(HttpRequest { method: Method::Post, .. }) |
                Action::Notice(Notice::CommandFailed { command: 1, .. } | Notice::CommandError { command: 1, .. })
            ) || matches!(a, Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")));
        }
    }
}

#[test]
fn confirmed_parent_cancel_is_applied_before_unknown_replacement() {
    let mut e = positional_engine();
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.amount = 0.0;
    o.price = Some(58500.0);
    let actions = placed(&mut e, vec![o]);
    let cancel = req(&actions, Method::Delete, "/v3/algo/order/100");
    let actions = reply(&mut e, &cancel, 1010., r#"{"success":true}"#);
    let actions = tick(&mut e, &actions);
    let replacement = req(&actions, Method::Post, "/v3/algo/order");
    let actions = reply(
        &mut e,
        &replacement,
        1020.,
        r#"{"success":false,"code":-1000,"message":"ambiguous"}"#,
    );
    let error = failure(&actions);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let rows = error.elements.as_ref().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(matches!(
        &rows[1].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
    assert!(
        matches!(&rows[0].result,
        safe_cex_engine::write_plan::ElementResult::Applied {
            effect: safe_cex_engine::write_plan::AppliedEffect::Cancel,
            target_order_id: Some(target), ..
        } if target == "100"),
        "accepted DELETE /v3/algo/order/100 was misreported: {rows:?}"
    );
}

#[test]
fn parent_cancel_unknown_stops_before_replacement() {
    let mut e = positional_engine();
    let mut o = opts();
    o.kind = OrderType::StopLoss;
    o.side = OrderSide::Sell;
    o.amount = 0.0;
    o.price = Some(58500.0);
    let actions = placed(&mut e, vec![o]);
    let cancel = req(&actions, Method::Delete, "/v3/algo/order/100");
    let actions = reply(
        &mut e,
        &cancel,
        1010.,
        r#"{"success":false,"code":-1000,"message":"ambiguous"}"#,
    );
    let error = failure(&actions);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let rows = error.elements.as_ref().unwrap();
    assert_eq!(rows.len(), 2);
    assert!(matches!(
        rows[0].result,
        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
    ));
    assert!(matches!(
        rows[1].result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
    ));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
    );
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown")).count(), 1);
}

//! Oracles: KuCoin Classic Futures Add Order, Add TP/SL Order, Cancel Order,
//! Switch Position Mode; safe-cex native-lot and client-ID contracts.
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_kucoin::{Kucoin, Options};

struct Harness {
    engine: Engine<Kucoin>,
    actions: Vec<Action>,
    failures: Vec<safe_cex_engine::VenueError>,
    now: f64,
}
/// Records a failure and renders it as the plain text the host shows.
fn plain(action: Action, failures: &mut Vec<safe_cex_engine::VenueError>) -> Action {
    match action {
        Action::Notice(Notice::CommandFailed { command, error }) => {
            failures.push(error.clone());
            Action::Notice(Notice::CommandFailed { command, error })
        }
        action => action,
    }
}

impl Harness {
    fn new() -> Self {
        Self::with_source(17)
    }
    fn with_source(seed: u64) -> Self {
        let mut engine = Engine::new(
            Kucoin::new(Options {
                key: Some("documented-test-key".into()),
                secret: Some("documented-test-secret".into()),
                passphrase: Some("documented-test-passphrase".into()),
                ..Default::default()
            }),
            Some(safe_cex_engine::test_support::seeded_entropy(seed)),
        )
        .unwrap();
        engine.store_mut().set_markets(vec![Market {
            id: "XBTUSDTM".into(),
            symbol: "BTCUSDT".into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 1.0,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 1.0,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 100.0,
                },
                price: None,
            },
            contract_size: Some(0.001),
            is_expiring: Some(false),
            is_inverse: Some(false),
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }]);
        Self {
            engine,
            actions: vec![],
            failures: vec![],
            now: 1_700_000_000_000.0,
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        let mut receipts = Vec::new();
        for action in self.engine.drain_actions() {
            if let Action::Notice(Notice::CommandProgress {
                receipts: actual, ..
            }) = &action
            {
                receipts.extend(actual.clone());
            }
            if let Action::Http(request) = &action {
                let body = request
                    .body
                    .as_deref()
                    .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
                let expected: Vec<_> = body
                    .as_ref()
                    .and_then(|body| body["clientOid"].as_str())
                    .map(|id| OrderIdentityReceipt {
                        symbol: "BTCUSDT".into(),
                        kind: OrderIdentityKind::ClientOrderId,
                        identity: id.into(),
                    })
                    .into_iter()
                    .collect();
                assert_eq!(
                    std::mem::take(&mut receipts),
                    expected,
                    "actual KuCoin clientOid must be receipted before dispatch"
                );
            }
            let action = plain(action, &mut self.failures);
            self.actions.push(action);
        }
    }
    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        let index = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r) if r.url.contains(path)))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.actions));
        let Action::Http(request) = self.actions.remove(index) else {
            unreachable!()
        };
        request
    }
    fn has(&self, path: &str) -> bool {
        self.actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains(path)))
    }
    fn response(&mut self, request: &HttpRequest, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn reply(&mut self, path: &str, body: &str) {
        let request = self.take(path);
        self.response(&request, 200, body);
    }
    fn pace(&mut self) {
        self.advance(20.0);
    }
    fn advance(&mut self, delay: f64) {
        let index = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==self.now+delay))
            .unwrap_or_else(|| panic!("missing pacing timer: {:?}", self.actions));
        let Action::SetTimer { timer, at_ms } = self.actions.remove(index) else {
            unreachable!()
        };
        self.now = at_ms;
        self.event(Event::TimerFired { timer });
    }
    fn metadata(&mut self, hedged: bool, margin: &str) {
        assert!(!self.has("/api/v1/orders"));
        self.reply(
            "getMarginMode",
            &format!(r#"{{"code":"200000","data":{{"marginMode":"{margin}"}}}}"#),
        );
        self.reply(
            "getPositionMode",
            if hedged {
                r#"{"code":"200000","data":{"positionMode":1}}"#
            } else {
                r#"{"code":"200000","data":{"positionMode":0}}"#
            },
        );
        if margin == "CROSS" {
            self.reply(
                "getCrossUserLeverage",
                r#"{"code":"200000","data":{"leverage":5}}"#,
            );
        }
        self.pace();
    }
    fn done(&self, command: u32) -> Result<String, String> {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: id,
                    result,
                }) if *id == command => Some(Ok(result.clone())),
                Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                    Some(Err(error.legacy_text()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("no result for {command}: {:?}", self.actions))
    }
    fn set_order(&mut self, id: &str, kind: OrderType, remaining_native: f64) {
        self.engine.store_mut().upsert_order(Order {
            id: id.into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind,
            side: OrderSide::Buy,
            price: 100.0,
            amount: 0.01,
            filled: 0.004,
            remaining: remaining_native * 0.001,
            reduce_only: false,
            close: Some(false),
            client_order_id: Some("original-caller".into()),
            time_in_force: Some(TimeInForce::GoodTillCancel),
            last_update: self.now,
            trigger_type: Some(StopTriggerType::MarkPrice),
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(remaining_native),
                ..Default::default()
            }),
        });
    }
    fn confirm_cancel(&mut self, id: &str, size: f64, filled: f64) {
        let request = self.take(&format!("/api/v1/orders/{id}"));
        assert_eq!(request.method, Method::Get);
        self.response(&request, 200, &format!(
            r#"{{"code":"200000","data":{{"id":"{id}","isActive":false,"cancelExist":true,"status":"done","size":{size},"filledSize":{filled}}}}}"#
        ));
        self.pace();
    }
}
fn order() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":10,"price":100.04,"clientOrderId":"caller_ABC-123"}"#).unwrap()
}
fn place(h: &mut Harness, orders: Vec<PlaceOrderOpts>) {
    h.command(Command::PlaceOrders { command: 1, orders });
}

#[test]
fn caller_id_native_lots_and_headless_metadata_are_preserved() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(true, "CROSS");
    let request = h.take("/api/v1/orders");
    let body = request.body.as_deref().unwrap();
    assert_eq!(request.method, Method::Post);
    for expected in [
        r#""symbol":"XBTUSDTM""#,
        r#""clientOid":"caller_ABC-123""#,
        r#""size":"10""#,
        r#""price":"100""#,
        r#""marginMode":"CROSS""#,
        r#""positionSide":"LONG""#,
        r#""leverage":5"#,
    ] {
        assert!(body.contains(expected), "{body} missing {expected}");
    }
    h.response(
        &request,
        200,
        r#"{"code":"200000","data":{"orderId":"9007199254740993"}}"#,
    );
    h.pace();
    assert_eq!(h.done(1), Ok(r#"["9007199254740993"]"#.into()));
}
#[test]
fn futures_native_alias_and_caller_or_generated_oid_have_exact_receipt_ownership() {
    for generated in [false, true] {
        let mut h = Harness::new();
        let mut opts = order();
        if generated {
            opts.client_order_id = None;
        }
        h.command(Command::PlaceOrders {
            command: 91,
            orders: vec![opts],
        });
        h.metadata(true, "CROSS");
        let receipt_at = h
            .actions
            .iter()
            .position(|action| {
                matches!(
                    action,
                    Action::Notice(Notice::CommandProgress { command: 91, .. })
                )
            })
            .unwrap();
        let write_at = h.actions.iter().position(|action| matches!(action, Action::Http(request) if request.url.ends_with("/api/v1/orders"))).unwrap();
        assert!(receipt_at < write_at);
        let Action::Notice(Notice::CommandProgress { receipts, .. }) = &h.actions[receipt_at]
        else {
            unreachable!()
        };
        let Action::Http(request) = &h.actions[write_at] else {
            unreachable!()
        };
        let payload: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload["symbol"], "XBTUSDTM");
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].symbol, "BTCUSDT");
        assert_eq!(receipts[0].identity, payload["clientOid"].as_str().unwrap());
        assert_eq!(receipts[0].kind, OrderIdentityKind::ClientOrderId);
    }
}
#[test]
fn caller_id_preflight_rejects_the_whole_batch_before_any_transport() {
    for invalid in [
        "",
        "bad space",
        "é",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "caller_ABC-123",
    ] {
        let mut h = Harness::new();
        let mut second = order();
        second.client_order_id = Some(invalid.into());
        place(&mut h, vec![order(), second]);
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(h.done(1).is_err());
    }
    let mut h = Harness::new();
    let mut second = order();
    second.stop_loss = Some(90.0);
    place(&mut h, vec![order(), second]);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(h.done(1).is_err());
}
#[test]
fn unsupported_slippage_protection_rejects_the_entire_batch_before_transport() {
    for kind in [OrderType::Market, OrderType::Limit] {
        let mut h = Harness::new();
        let mut protected = order();
        protected.kind = kind;
        protected.client_order_id = None;
        protected.max_slippage_pct = Some(1.0);
        place(&mut h, vec![order(), protected]);
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(h.done(1).as_ref().unwrap_err().contains("slippage"));
    }
}
#[test]
fn bybit_execution_controls_reject_the_whole_batch_before_transport() {
    for bbo in [true, false] {
        let mut h = Harness::new();
        let mut protected = order();
        protected.client_order_id = None;
        if bbo {
            protected.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        } else {
            protected.bybit_slippage_tolerance_ticks = Some(5.0);
        }
        place(&mut h, vec![order(), protected]);
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(h.done(1).as_ref().unwrap_err().contains("Bybit"));
    }
}
#[test]
fn late_batch_format_failure_sends_no_writes() {
    let mut h = Harness::new();
    let mut second = order();
    second.client_order_id = None;
    second.amount = 2000.0;
    place(&mut h, vec![order(), second]);
    h.metadata(false, "CROSS");
    assert!(!h.has("/api/v1/orders"));
    assert!(h.done(1).is_err());
}
#[test]
fn bracket_post_only_uses_st_orders_and_one_trigger_source() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.client_order_id = None;
    opts.stop_loss = Some(90.0);
    opts.take_profit = Some(110.0);
    opts.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    opts.tp_trigger_type = Some(StopTriggerType::MarkPrice);
    opts.time_in_force = Some(TimeInForce::PostOnly);
    place(&mut h, vec![opts]);
    h.metadata(false, "ISOLATED");
    let request = h.take("/api/v1/st-orders");
    let body = request.body.unwrap();
    for expected in [
        r#""triggerStopUpPrice":"110""#,
        r#""triggerStopDownPrice":"90""#,
        r#""stopPriceType":"MP""#,
        r#""postOnly":true"#,
        r#""timeInForce":"GTC""#,
    ] {
        assert!(body.contains(expected), "{body}");
    }
    assert!(!body.contains("positionSide"));
}
#[test]
fn mixed_bracket_trigger_sources_fail_before_cancellation_or_placement() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.client_order_id = None;
    opts.stop_loss = Some(90.0);
    opts.take_profit = Some(110.0);
    opts.sl_trigger_type = Some(StopTriggerType::MarkPrice);
    opts.tp_trigger_type = Some(StopTriggerType::IndexPrice);
    place(&mut h, vec![opts]);
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(h.done(1).is_err());
}
#[test]
fn full_position_stop_omits_size_and_closes_the_correct_hedged_side() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.client_order_id = None;
    opts.kind = OrderType::StopLoss;
    opts.amount = 0.0;
    opts.price = Some(110.0);
    opts.sl_trigger_type = Some(StopTriggerType::IndexPrice);
    place(&mut h, vec![opts]);
    h.metadata(true, "CROSS");
    let request = h.take("/api/v1/orders");
    let body = request.body.unwrap();
    for expected in [
        r#""type":"market""#,
        r#""closeOrder":true"#,
        r#""reduceOnly":true"#,
        r#""stop":"up""#,
        r#""stopPriceType":"IP""#,
        r#""positionSide":"SHORT""#,
    ] {
        assert!(body.contains(expected), "{body}");
    }
    assert!(!body.contains("\"size\""));
}
#[test]
fn ambiguous_order_response_is_not_retried_and_fails_the_job() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "connection reset after acceptance",
    });
    h.pace();
    assert!(!h.has("/api/v1/orders"));
    assert!(h.done(1).is_err());
}
#[test]
fn explicit_signature_rejection_retries_only_once() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let first = h.take("/api/v1/orders");
    h.now += 1.0;
    h.response(
        &first,
        400,
        r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#,
    );
    let second = h.take("/api/v1/orders");
    assert_eq!(first.body, second.body);
    assert_ne!(first.headers, second.headers);
    h.response(
        &second,
        400,
        r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#,
    );
    h.pace();
    assert!(!h.has("/api/v1/orders"));
    assert!(h.done(1).is_err());
}
#[test]
fn cancel_replace_uses_remaining_native_once_and_a_new_client_id() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    let cancel = h.take("/api/v1/orders/old");
    assert_eq!(cancel.method, Method::Delete);
    assert!(!h.has("/api/v1/orders"));
    h.response(
        &cancel,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 4.0);
    let create = h.take("/api/v1/orders");
    let body = create.body.unwrap();
    assert!(body.contains(r#""size":"6""#));
    assert!(body.contains(r#""price":"101""#));
    assert!(!body.contains("original-caller"));
}
#[test]
fn ambiguous_cancel_prevents_replacement() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    let cancel = h.take("/api/v1/orders/old");
    h.event(Event::HttpFailed {
        request: cancel.id,
        error: "lost cancel response",
    });
    h.pace();
    assert!(!h.has("/api/v1/orders"));
    assert!(h.done(1).is_err());
}
#[test]
fn virtual_leg_amend_keeps_both_protections_and_uses_st_orders() {
    let mut h = Harness::new();
    h.set_order("parent", OrderType::Limit, 6.0);
    h.set_order("parent__stop_loss", OrderType::StopLoss, 6.0);
    h.set_order("parent__take_profit", OrderType::TakeProfit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "parent__stop_loss".into(),
        price: Some(90.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    h.reply(
        "/api/v1/orders/parent",
        r#"{"code":"200000","data":{"cancelledOrderIds":["parent"]}}"#,
    );
    h.pace();
    h.confirm_cancel("parent", 10.0, 4.0);
    let request = h.take("/api/v1/st-orders");
    let body = request.body.unwrap();
    for expected in [
        r#""triggerStopDownPrice":"90""#,
        r#""triggerStopUpPrice":"100""#,
        r#""stopPriceType":"MP""#,
        r#""size":"6""#,
    ] {
        assert!(body.contains(expected), "{body}");
    }
}
#[test]
fn virtual_cancel_deduplicates_parent_and_falls_back_only_on_404() {
    let mut h = Harness::new();
    h.set_order("parent", OrderType::Limit, 6.0);
    h.set_order("parent__stop_loss", OrderType::StopLoss, 6.0);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["parent".into(), "parent__stop_loss".into()],
    });
    let first = h.take("/api/v1/orders/parent");
    h.response(&first, 404, r#"{"code":"404000","msg":"Not found"}"#);
    let second = h.take("/api/v1/stopOrders/parent");
    h.response(
        &second,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["parent"]}}"#,
    );
    h.pace();
    assert_eq!(h.done(1), Ok("[]".into()));
    assert!(!h.has("/orders/parent"));
}
#[test]
fn symbol_cancel_uses_v3_and_account_cancel_covers_both_families() {
    let mut h = Harness::new();
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "BTCUSDT".into(),
    });
    let request = h.take("/api/v3/orders?symbol=XBTUSDTM");
    assert_eq!(request.method, Method::Delete);
    h.response(&request, 200, r#"{"code":"200000","data":[]}"#);
    h.pace();
    assert!(!h.has("/api/v1/stopOrders"));
    assert_eq!(h.done(1), Ok("[]".into()));
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "".into(),
    });
    h.reply("/api/v3/orders", r#"{"code":"200000","data":[]}"#);
    h.pace();
    h.reply("/api/v1/stopOrders", r#"{"code":"200000","data":[]}"#);
    h.pace();
    assert_eq!(h.done(2), Ok("[]".into()));
}

#[test]
fn account_cancel_cannot_report_success_when_the_stop_family_rejects() {
    let mut h = Harness::new();
    h.command(Command::CancelSymbolOrders {
        command: 1,
        symbol: "".into(),
    });
    h.reply("/api/v3/orders", r#"{"code":"200000","data":[]}"#);
    h.pace();
    let stop = h.take("/api/v1/stopOrders");
    h.response(
        &stop,
        400,
        r#"{"code":"100001","msg":"Stop cancellation rejected"}"#,
    );
    h.pace();
    assert!(h.done(1).is_err());
    assert!(!h.has("/api/v1/stopOrders"));
}
#[test]
fn leverage_requires_cross_metadata_and_clamps_to_market_limits() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 150.0,
        side: None,
    });
    h.metadata(false, "CROSS");
    let request = h.take("changeCrossUserLeverage");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains(r#""leverage":"100""#)
    );
    h.response(&request, 200, r#"{"code":"200000","data":true}"#);
    h.pace();
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 100.0);
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 5.0,
        side: None,
    });
    h.metadata(false, "ISOLATED");
    assert!(!h.has("changeCrossUserLeverage"));
    assert!(h.done(1).is_err());
}
#[test]
fn margin_requires_symbol_and_updates_only_after_success() {
    let mut h = Harness::new();
    h.command(Command::SetMarginType {
        command: 1,
        symbol: "".into(),
        isolated: true,
    });
    assert!(h.done(1).is_err());
    assert!(!h.has("changeMarginMode"));
    h.command(Command::SetMarginType {
        command: 2,
        symbol: "BTCUSDT".into(),
        isolated: true,
    });
    let request = h.take("changeMarginMode");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains(r#""marginMode":"ISOLATED""#)
    );
    h.response(&request, 400, r#"{"code":"100001","msg":"rejected"}"#);
    h.pace();
    assert!(h.done(2).is_err());
    assert!(h.engine.store().setting("BTCUSDT").is_none());
}
#[test]
fn mode_switch_reads_current_positions_and_rejects_open_or_malformed_quantities() {
    for quantity in ["2", "null", r#""invalid""#] {
        let mut h = Harness::new();
        h.command(Command::SetPositionMode {
            command: 1,
            hedged: true,
            symbol: None,
        });
        h.reply(
            "/api/v1/positions",
            &format!(r#"{{"code":"200000","data":[{{"currentQty":{quantity}}}]}}"#),
        );
        h.pace();
        assert!(!h.has("switchPositionMode"));
        assert!(h.done(1).is_err());
    }
    let mut h = Harness::new();
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
        symbol: None,
    });
    h.reply("/api/v1/positions", r#"{"code":"200000","data":[]}"#);
    h.pace();
    let request = h.take("switchPositionMode");
    assert_eq!(request.body.as_deref(), Some(r#"{"positionMode":"1"}"#));
    h.response(&request, 200, r#"{"code":"200000","data":true}"#);
    h.pace();
    assert!(h.engine.store().hedged());
    assert_eq!(h.done(1), Ok("[]".into()));
}
#[test]
fn malformed_success_without_order_id_is_a_failure() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    h.reply("/api/v1/orders", r#"{"code":"200000","data":{}}"#);
    h.pace();
    assert!(h.done(1).is_err());
}
#[test]
fn opening_triggers_are_refused_before_writes_instead_of_silently_dropped() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.client_order_id = None;
    opts.trigger_price = Some(110.0);
    place(&mut h, vec![opts]);
    assert!(h.done(1).is_err());
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn standalone_stop_honors_the_generic_trigger_source() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.client_order_id = None;
    opts.kind = OrderType::TakeProfit;
    opts.trigger_type = Some(StopTriggerType::MarkPrice);
    place(&mut h, vec![opts]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    assert!(request.body.unwrap().contains(r#""stopPriceType":"MP""#));
}

#[test]
fn confirmed_margin_change_invalidates_leverage_before_the_next_order() {
    let mut h = Harness::new();
    h.command(Command::SetMarginType {
        command: 2,
        symbol: "BTCUSDT".into(),
        isolated: false,
    });
    h.reply("changeMarginMode", r#"{"code":"200000","data":true}"#);
    h.pace();
    assert!(
        h.engine
            .store()
            .setting("BTCUSDT")
            .unwrap()
            .leverage
            .is_nan()
    );
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    assert!(request.body.unwrap().contains(r#""leverage":5"#));
}

#[test]
fn replacement_rejects_a_trigger_rounded_to_zero_before_cancelling() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::StopLoss, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(0.01),
        amount: None,
    });
    h.metadata(false, "CROSS");
    assert!(h.done(1).is_err());
    assert!(!h.has("/api/v1/orders"));
}

#[test]
fn isolated_hedged_placement_uses_the_target_sides_leverage() {
    for (side, reduce_only, expected) in [
        (OrderSide::Buy, false, 10),
        (OrderSide::Sell, false, 20),
        (OrderSide::Buy, true, 20),
    ] {
        let mut h = Harness::new();
        h.engine.store_mut().set_positions(
            [PositionSide::Long, PositionSide::Short]
                .into_iter()
                .map(|side| Position {
                    id: format!("BTCUSDT:{side:?}"),
                    symbol: "BTCUSDT".into(),
                    side,
                    entry_price: 100.0,
                    notional: 1.0,
                    leverage: if side == PositionSide::Long {
                        10.0
                    } else {
                        20.0
                    },
                    margin_type: Some(MarginType::Isolated),
                    position_mode: Some(PositionMode::Hedged),
                    unrealized_pnl: 0.0,
                    realized_pnl: None,
                    contracts: 0.01,
                    liquidation_price: 0.0,
                    margin_size: None,
                    last_update: None,
                })
                .collect(),
        );
        let mut opts = order();
        opts.side = side;
        opts.reduce_only = Some(reduce_only);
        place(&mut h, vec![opts]);
        h.metadata(true, "ISOLATED");
        let request = h.take("/api/v1/orders");
        assert!(
            request
                .body
                .unwrap()
                .contains(&format!(r#""leverage":{expected}"#))
        );
    }
}

#[test]
fn cancel_replace_waits_for_terminal_proof_and_subtracts_intervening_fills() {
    for (amount, expected) in [(None, "2"), (Some(9.0), "5")] {
        let mut h = Harness::new();
        h.set_order("old", OrderType::Limit, 6.0);
        h.command(Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(101.0),
            amount,
        });
        h.metadata(false, "CROSS");
        h.reply(
            "/orders/old",
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        let pending = h.take("/orders/old");
        assert_eq!(pending.method, Method::Get);
        assert!(!h.has("/api/v1/orders"));
        h.response(&pending, 200, r#"{"code":"200000","data":{"id":"old","isActive":true,"cancelExist":false,"status":"open","size":10,"filledSize":6}}"#);
        assert!(!h.has("/api/v1/orders"));
        h.advance(100.0);
        h.confirm_cancel("old", 10.0, 8.0);
        let request = h.take("/api/v1/orders");
        assert_eq!(request.method, Method::Post);
        assert!(
            request
                .body
                .unwrap()
                .contains(&format!(r#""size":"{expected}""#))
        );
        assert!(h.engine.store().order("old").is_none());
    }
}

#[test]
fn fully_filled_order_during_cancel_is_not_replaced_by_price_only_amend() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    h.reply(
        "/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 10.0);
    assert!(!h.has("/api/v1/orders"));
    assert_eq!(h.done(1), Ok("[]".into()));
}

#[test]
fn failed_confirmation_reads_retry_without_sending_a_replacement() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    h.reply(
        "/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    for attempt in 0..3 {
        let request = h.take("/orders/old");
        assert_eq!(request.method, Method::Get);
        h.event(Event::HttpFailed {
            request: request.id,
            error: "read failed",
        });
        assert!(!h.has("/api/v1/orders"));
        if attempt < 2 {
            h.advance(100.0);
        } else {
            h.pace();
        }
    }
    assert!(h.done(1).is_err());
}

#[test]
fn active_cancellation_proof_has_a_bounded_poll_count() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    h.reply(
        "/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    for attempt in 0..33 {
        let request = h.take("/orders/old");
        assert_eq!(request.method, Method::Get);
        h.response(
            &request,
            200,
            r#"{"code":"200000","data":{"id":"old","isActive":true,"size":10,"filledSize":4}}"#,
        );
        assert!(!h.has("/api/v1/orders"));
        if attempt < 32 {
            h.advance(100.0);
        } else {
            h.pace();
        }
    }
    assert!(h.done(1).is_err());
}

#[test]
fn explicit_active_flag_overrides_a_contradictory_canceled_status() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "old".into(),
        price: Some(101.0),
        amount: None,
    });
    h.metadata(false, "CROSS");
    h.reply(
        "/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    let request = h.take("/orders/old");
    h.response(&request, 200, r#"{"code":"200000","data":{"id":"old","status":"canceled","isActive":true,"cancelExist":true,"size":10,"filledSize":4}}"#);
    assert!(!h.has("/api/v1/orders"));
    h.advance(100.0);
    h.confirm_cancel("old", 10.0, 4.0);
    let body = h.take("/api/v1/orders").body.unwrap();
    assert!(body.contains(r#""size":"6""#), "{body}");
}

#[test]
fn generated_client_ids_do_not_collide_across_instances_at_the_same_timestamp() {
    let mut ids = std::collections::HashSet::new();
    // Model the independent entropy of production hosts with distinct explicit sources.
    for seed in 1..=16 {
        let mut h = Harness::with_source(seed);
        let mut o = order();
        o.client_order_id = None;
        place(&mut h, vec![o]);
        h.metadata(false, "CROSS");
        let r = h.take("/api/v1/orders");
        let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
        let id = body["clientOid"].as_str().unwrap();
        assert!(!id.is_empty() && id.len() <= 40);
        assert!(
            ids.insert(id.to_owned()),
            "new sessions must not reuse an exchange idempotency key"
        );
    }
}

#[test]
fn uncached_cancel_ids_send_directly_and_do_not_block_known_ids_after_rejection() {
    for reject_unknown in [false, true] {
        let mut h = Harness::new();
        h.set_order("known", OrderType::Limit, 2.0);
        h.command(Command::CancelOrders {
            command: 1,
            ids: vec!["unknown".into(), "known".into()],
        });
        let unknown = h.take("/api/v1/orders/unknown");
        assert!(unknown.url.ends_with("/api/v1/orders/unknown"));
        assert_eq!(unknown.method, Method::Delete);
        if reject_unknown {
            h.response(&unknown, 400, r#"{"code":"100001","msg":"Unknown order"}"#);
        } else {
            h.response(
                &unknown,
                200,
                r#"{"code":"200000","data":{"cancelledOrderIds":["unknown"]}}"#,
            );
        }
        h.pace();
        let known = h.take("/api/v1/orders/known?symbol=XBTUSDTM");
        h.response(
            &known,
            200,
            r#"{"code":"200000","data":{"cancelledOrderIds":["known"]}}"#,
        );
        h.pace();
        assert_eq!(h.done(1).is_err(), reject_unknown);
        assert!(!h.has("/api/v1/orders/unknown"));
    }
}

#[test]
fn an_empty_native_cancel_id_never_becomes_a_bulk_delete() {
    for id in ["", "__stop_loss"] {
        let mut h = Harness::new();
        h.command(Command::CancelOrders {
            command: 1,
            ids: vec![id.into()],
        });
        assert!(!h.has("/api/v1/orders"));
        assert!(h.done(1).is_err());
    }
}

#[test]
fn invalid_dynamic_leverage_limits_reject_without_native_write() {
    for (min, max) in [
        (30.0, 20.0),
        (f64::NAN, 20.0),
        (1.0, 0.0),
        (1.0, f64::INFINITY),
    ] {
        let mut h = Harness::new();
        h.engine.store_mut().update_markets(|market| {
            market.limits.leverage.min = min;
            market.limits.leverage.max = max;
        });
        h.command(Command::SetLeverage {
            command: 99,
            symbol: "BTCUSDT".into(),
            leverage: 5.0,
            side: None,
        });
        h.metadata(false, "CROSS");
        assert!(!h.actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.method != Method::Get)
        ));
        assert!(h.done(99).as_ref().unwrap_err().contains("leverage limits"));
    }
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified, text) in [
        (503, "<html>", unknown, "Invalid KuCoin response (HTTP 503)"),
        (
            503,
            r#"{"code":"500000","msg":"busy"}"#,
            unknown,
            "KuCoin API Error 500000: busy (HTTP 503)",
        ),
        (
            429,
            r#"{"code":"429000","msg":"too many"}"#,
            rejected(ErrorKind::RateLimited),
            "KuCoin API Error 429000: too many (HTTP 429)",
        ),
        (
            200,
            r#"{"code":"300003","msg":"Balance insufficient"}"#,
            rejected(ErrorKind::Rejected),
            "KuCoin API Error 300003: Balance insufficient (HTTP 200)",
        ),
        (
            200,
            r#"{"code":"400007","msg":"Access denied"}"#,
            rejected(ErrorKind::AuthPermission),
            "KuCoin API Error 400007: Access denied (HTTP 200)",
        ),
        (
            200,
            r#"{"code":"999999","msg":"new"}"#,
            (None, None),
            "KuCoin API Error 999999: new (HTTP 200)",
        ),
        (
            200,
            r#"{"code":"200000","data":{}}"#,
            unknown,
            "Missing KuCoin order id",
        ),
    ] {
        let mut h = Harness::new();
        place(&mut h, vec![order()]);
        h.metadata(false, "CROSS");
        let request = h.take("/api/v1/orders");
        h.response(&request, status, body);
        h.pace();
        let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
        assert_eq!(
            ((error.kind, error.outcome), error.legacy_text().as_str()),
            (classified, text),
            "{status} {body}"
        );
        if status == 200 && body.contains("300003") {
            assert_eq!(error.venue_code.as_deref(), Some("300003"));
        }
    }
    // Transport loss on a write is Unknown.
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    h.event(Event::HttpFailed {
        request: request.id,
        error: "reset",
    });
    h.pace();
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    // Preflight refusals were never sent.
    let mut h = Harness::new();
    let mut invalid = order();
    invalid.client_order_id = Some("bad id!".into());
    place(&mut h, vec![invalid]);
    assert_eq!(h.failures[0].outcome, Some(Outcome::NotSent));
}

#[test]
fn a_refusal_after_an_accepted_placement_is_never_rejected() {
    let mut h = Harness::new();
    let mut second = order();
    second.client_order_id = Some("caller_second".into());
    place(&mut h, vec![order(), second]);
    h.metadata(false, "CROSS");
    let first = h.take("/api/v1/orders");
    h.response(&first, 200, r#"{"code":"200000","data":{"orderId":"one"}}"#);
    h.pace();
    let second = h.take("/api/v1/orders");
    h.response(
        &second,
        200,
        r#"{"code":"300003","msg":"Balance insufficient"}"#,
    );
    h.pace();
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].outcome, None);
}

#[test]
fn cancel_legs_combine_outcomes_without_overclaiming() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (first, second, outcome) in [
        // A refused first leg then an undocumented answer: the outcome is not proven.
        (
            (400, r#"{"code":"100001","msg":"Unknown order"}"#),
            (200, r#"{"code":"999999","msg":"new"}"#),
            (Some(ErrorKind::Rejected), None),
        ),
        // A refused leg then a lost answer: Unknown, with the lost leg's kind.
        (
            (400, r#"{"code":"100001","msg":"Unknown order"}"#),
            (0, ""),
            (Some(ErrorKind::Network), Some(Outcome::Unknown)),
        ),
    ] {
        let mut h = Harness::new();
        h.command(Command::CancelOrders {
            command: 1,
            ids: vec!["a".into(), "b".into()],
        });
        let a = h.take("/api/v1/orders/a");
        h.response(&a, first.0, first.1);
        h.pace();
        let b = h.take("/api/v1/orders/b");
        if second.0 == 0 {
            h.event(Event::HttpFailed {
                request: b.id,
                error: "reset",
            });
        } else {
            h.response(&b, second.0, second.1);
        }
        h.pace();
        assert_eq!(
            h.done(1).as_ref().unwrap_err(),
            "KuCoin API Error 100001: Unknown order (HTTP 400)"
        );
        let [error] = std::mem::take(&mut h.failures).try_into().unwrap();
        assert_eq!((error.kind, error.outcome), outcome);
        if outcome.1.is_some() {
            assert_eq!(error.venue_code, None);
        }
    }
}

#[cfg(feature = "private-adoption-originals")]
#[path = "support/trading_context.rs"]
mod trading_context;

fn leverage_result(h: &Harness, command: u32) -> serde_json::Value {
    serde_json::from_str(&h.done(command).expect("the read must succeed")).unwrap()
}

#[test]
fn a_flat_cross_symbol_reads_margin_then_leverage() {
    let mut h = Harness::new();
    assert_eq!(
        h.engine.capabilities().leverage_query,
        safe_cex_engine::LeverageQuery::OnReadAndMetadata
    );
    h.command(Command::FetchLeverage {
        command: 70,
        symbol: "BTCUSDT".into(),
    });
    let margin = h.take("/api/v2/position/getMarginMode");
    h.response(
        &margin,
        200,
        r#"{"code":"200000","data":{"symbol":"XBTUSDTM","marginMode":"CROSS"}}"#,
    );
    let leverage = h.take("/api/v2/getCrossUserLeverage");
    h.response(
        &leverage,
        200,
        r#"{"code":"200000","data":{"symbol":"XBTUSDTM","leverage":"7"}}"#,
    );
    let value = leverage_result(&h, 70);
    assert_eq!(value["buyLeverage"], 7.0);
    assert_eq!(value["sellLeverage"], 7.0);
    assert_eq!(value["marginType"], "cross");
    assert_eq!(h.engine.store().setting("BTCUSDT").unwrap().leverage, 7.0);
}

#[test]
fn a_flat_isolated_symbol_is_one_as_in_ts_with_no_cross_read() {
    let mut h = Harness::new();
    h.command(Command::FetchLeverage {
        command: 71,
        symbol: "BTCUSDT".into(),
    });
    let margin = h.take("/api/v2/position/getMarginMode");
    h.response(
        &margin,
        200,
        r#"{"code":"200000","data":{"symbol":"XBTUSDTM","marginMode":"ISOLATED"}}"#,
    );
    assert!(!h.has("getCrossUserLeverage"));
    let value = leverage_result(&h, 71);
    assert_eq!(value["buyLeverage"], 1.0);
    assert_eq!(value["marginType"], "isolated");
}

#[test]
fn a_failed_or_invalid_leverage_read_fails_its_command() {
    let mut h = Harness::new();
    h.command(Command::FetchLeverage {
        command: 72,
        symbol: "BTCUSDT".into(),
    });
    let margin = h.take("/api/v2/position/getMarginMode");
    h.response(&margin, 200, r#"{"code":"400100","msg":"denied"}"#);
    assert!(h.done(72).unwrap_err().contains("denied"));

    h.command(Command::FetchLeverage {
        command: 73,
        symbol: "BTCUSDT".into(),
    });
    let margin = h.take("/api/v2/position/getMarginMode");
    h.response(
        &margin,
        200,
        r#"{"code":"200000","data":{"marginMode":"CROSS"}}"#,
    );
    let leverage = h.take("/api/v2/getCrossUserLeverage");
    h.response(
        &leverage,
        200,
        r#"{"code":"200000","data":{"leverage":"0"}}"#,
    );
    assert!(h.done(73).unwrap_err().contains("invalid leverage"));
}

#[test]
fn an_unknown_symbol_leverage_read_sends_nothing() {
    let mut h = Harness::new();
    h.command(Command::FetchLeverage {
        command: 74,
        symbol: "NOPEUSDT".into(),
    });
    assert!(h.done(74).unwrap_err().contains("unknown market"));
    assert!(!h.has("getMarginMode"));
}

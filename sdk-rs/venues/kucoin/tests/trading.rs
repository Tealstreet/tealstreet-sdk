//! Oracles: KuCoin Classic Futures Add Order, Add TP/SL Order, Cancel Order,
//! Switch Position Mode; safe-cex native-lot and client-ID contracts.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_kucoin::{Kucoin, Options};

struct Harness {
    engine: Engine<Kucoin>,
    actions: Vec<Action>,
    now: f64,
    known_mode: Option<bool>,
}
impl Harness {
    fn new() -> Self {
        let mut engine = Engine::new(Kucoin::new(Options {
            key: Some("documented-test-key".into()),
            secret: Some("documented-test-secret".into()),
            passphrase: Some("documented-test-passphrase".into()),
            ..Default::default()
        }));
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
            now: 1_700_000_000_000.0,
            known_mode: None,
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.actions.extend(self.engine.drain_actions());
    }
    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    fn take(&mut self, path: &str) -> HttpRequest {
        support::wait_for_http(
            &mut self.engine,
            &mut self.actions,
            &mut self.now,
            Some(path),
            (path == "/api/v1/orders").then_some(Method::Post),
        );
        let index = self
            .actions
            .iter()
            .position(|a| matches!(a,Action::Http(r) if r.url.contains(path) && (path != "/api/v1/orders" || r.method == Method::Post)))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.actions));
        let Action::Http(request) = self.actions.remove(index) else {
            unreachable!()
        };
        request
    }
    fn has_write(&self, path: &str) -> bool {
        self.actions
            .iter()
            .any(|a| matches!(a,Action::Http(r) if r.method != Method::Get && r.url.contains(path)))
    }
    fn response(&mut self, request: &HttpRequest, status: u16, body: &str) {
        if status == 200 && request.url.contains("getPositionMode") {
            #[derive(serde::Deserialize)]
            struct Mode {
                #[serde(rename = "positionMode")]
                value: u8,
            }
            #[derive(serde::Deserialize)]
            struct Envelope {
                data: Mode,
            }
            let mode: Envelope = serde_json::from_str(body).unwrap();
            assert!(mode.data.value <= 1);
            self.known_mode = Some(mode.data.value == 1);
        }
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
        let target = self.now + delay;
        let mut fired = false;
        loop {
            let next = self.actions.iter().enumerate().filter_map(|(index, action)| {
                if let Action::SetTimer { timer, at_ms } = action
                    && *at_ms <= target
                    && !self.actions.iter().any(|a| matches!(a, Action::ClearTimer { timer: cancelled } if cancelled == timer)) {
                    Some((index, *at_ms))
                } else { None }
            }).min_by(|a, b| a.1.total_cmp(&b.1));
            let Some((index, _)) = next else {
                break;
            };
            let Action::SetTimer { timer, at_ms } = self.actions.remove(index) else {
                unreachable!()
            };
            self.now = self.now.max(at_ms);
            self.event(Event::TimerFired { timer });
            fired = true;
        }
        assert!(fired, "missing pacing timer: {:?}", self.actions);
        self.now = target;
    }
    fn metadata(&mut self, hedged: bool, margin: &str) {
        assert!(!self.has_write("/api/v1/orders"));
        self.reply(
            "getMarginMode",
            &format!(r#"{{"code":"200000","data":{{"marginMode":"{margin}"}}}}"#),
        );
        if let Some(mode) = self.known_mode {
            assert_eq!(
                mode, hedged,
                "fixture must not skip a different position mode"
            );
        } else {
            self.reply(
                "getPositionMode",
                if hedged {
                    r#"{"code":"200000","data":{"positionMode":1}}"#
                } else {
                    r#"{"code":"200000","data":{"positionMode":0}}"#
                },
            );
        }
        if margin == "CROSS" {
            self.reply(
                "getCrossUserLeverage",
                r#"{"code":"200000","data":{"leverage":5}}"#,
            );
        }
        self.pace();
    }
    fn done(&self, command: u32) -> &Result<String, String> {
        self.actions
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone {
                    command: id,
                    result,
                }) if *id == command => Some(result),
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
    assert_eq!(h.done(1), &Ok(r#"["9007199254740993"]"#.into()));
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
    assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
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
    assert_eq!(h.done(1), &Ok("[]".into()));
    assert!(!h.has_write("/orders/parent"));
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
    assert!(!h.actions.iter().any(
        |action| matches!(action, Action::Http(request) if request.url.contains("/api/v1/stopOrders"))
    ));
    assert_eq!(h.done(1), &Ok("[]".into()));
    h.command(Command::CancelSymbolOrders {
        command: 2,
        symbol: "".into(),
    });
    h.reply("/api/v3/orders", r#"{"code":"200000","data":[]}"#);
    h.pace();
    h.reply("/api/v1/stopOrders", r#"{"code":"200000","data":[]}"#);
    h.pace();
    assert_eq!(h.done(2), &Ok("[]".into()));
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
    assert!(!h.has_write("/api/v1/stopOrders"));
}
#[test]
fn leverage_requires_cross_metadata_and_clamps_to_market_limits() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 150.0,
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
    });
    h.metadata(false, "ISOLATED");
    assert!(!h.has_write("changeCrossUserLeverage"));
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
    assert!(!h.has_write("changeMarginMode"));
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
        });
        h.reply(
            "/api/v1/positions",
            &format!(r#"{{"code":"200000","data":[{{"currentQty":{quantity}}}]}}"#),
        );
        h.pace();
        assert!(!h.has_write("switchPositionMode"));
        assert!(h.done(1).is_err());
    }
    let mut h = Harness::new();
    h.command(Command::SetPositionMode {
        command: 1,
        hedged: true,
    });
    h.reply("/api/v1/positions", r#"{"code":"200000","data":[]}"#);
    h.pace();
    let request = h.take("switchPositionMode");
    assert_eq!(request.body.as_deref(), Some(r#"{"positionMode":"1"}"#));
    h.response(&request, 200, r#"{"code":"200000","data":true}"#);
    h.pace();
    assert!(h.engine.store().hedged());
    assert_eq!(h.done(1), &Ok("[]".into()));
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
    assert!(!h.has_write("/api/v1/orders"));
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
        assert!(!h.has_write("/api/v1/orders"));
        h.response(&pending, 200, r#"{"code":"200000","data":{"id":"old","isActive":true,"cancelExist":false,"status":"open","size":10,"filledSize":6}}"#);
        assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
    assert_eq!(h.done(1), &Ok("[]".into()));
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
        assert!(!h.has_write("/api/v1/orders"));
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
        assert!(!h.has_write("/api/v1/orders"));
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
    assert!(!h.has_write("/api/v1/orders"));
    h.advance(100.0);
    h.confirm_cancel("old", 10.0, 4.0);
    let body = h.take("/api/v1/orders").body.unwrap();
    assert!(body.contains(r#""size":"6""#), "{body}");
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
        assert!(!h.actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("/api/v1/orders/unknown"))
        ));
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
        assert!(!h.actions.iter().any(
            |action| matches!(action, Action::Http(request) if request.url.contains("/api/v1/orders"))
        ));
        assert!(h.done(1).is_err());
    }
}

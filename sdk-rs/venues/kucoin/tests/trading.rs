//! Oracles: KuCoin Classic Futures Add Order, Add TP/SL Order, Cancel Order,
//! Switch Position Mode; safe-cex native-lot and client-ID contracts.
use safe_cex_engine::error::Protection;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Outcome, Response,
};
mod support;
use safe_cex_venue_kucoin::Kucoin;
use support::credentials::Options;

#[path = "trading/allocation.rs"]
mod allocation;

struct Harness {
    engine: Engine<Kucoin>,
    actions: Vec<Action>,
    failures: Vec<safe_cex_engine::VenueError>,
    now: f64,
    quote_socket: Option<safe_cex_engine::SocketId>,
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
        let mut engine = support::credentials::with_seed(
            Options {
                key: Some("documented-test-key".into()),
                secret: Some("documented-test-secret".into()),
                passphrase: Some("documented-test-passphrase".into()),
                ..Default::default()
            },
            Kucoin::new,
            seed,
        );
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
            quote_socket: None,
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        let mut receipts = Vec::new();
        for action in
            safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now).into_iter()
        {
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
        if !self
            .actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == self.now + 20.0))
            && self.actions.iter().any(|a| {
                matches!(
                    a,
                    Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
                )
            })
        {
            return; // Managed Plan terminals finish in their final callback.
        }
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
        self.metadata_replies(hedged, margin);
        self.pace();
    }
    /// The metadata reads answered; the write they gate is paced next.
    fn metadata_replies(&mut self, hedged: bool, margin: &str) {
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
    /// Real native field receipts, including unchanged prices, on a public socket.
    fn fresh_ticker(&mut self, price: f64) {
        if self.quote_socket.is_none() {
            let orders = self.engine.store().orders().to_vec();
            let positions = self.engine.store().positions().to_vec();
            let setting = self.engine.store().setting("BTCUSDT");
            *self.engine.store_mut() = safe_cex_engine::Store::default();
            self.engine.start(self.now);
            self.event(Event::PollCeilingChanged { ceiling_ms: None });
            self.reply("/contracts/active", r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"BTC","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":1000,"maxLeverage":100}]}"#);
            self.reply(
                "/allTickers",
                r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100"}]}"#,
            );
            let request = self.take("/bullet-public");
            self.response(&request, 200, r#"{"code":"200000","data":{"token":"public-test-token","instanceServers":[{"endpoint":"wss://example.test/public","pingInterval":18000,"pingTimeout":10000}]}}"#);
            let socket = self
                .actions
                .iter()
                .find_map(|a| match a {
                    Action::WsConnect { socket, .. } => Some(*socket),
                    _ => None,
                })
                .expect("real public socket");
            self.event(Event::WsOpened { socket });
            self.event(Event::WsMessage {
                socket,
                text: r#"{"type":"welcome"}"#,
            });
            self.quote_socket = Some(socket);
            for order in orders {
                self.engine.store_mut().upsert_order(order);
            }
            self.engine.store_mut().set_positions(positions);
            if let Some(setting) = setting {
                self.engine
                    .store_mut()
                    .replace_settings([("BTCUSDT", setting)]);
            }
            self.actions.clear();
        }
        let socket = self.quote_socket.unwrap();
        let instrument = format!(
            r#"{{"type":"message","topic":"/contract/instrument:XBTUSDTM","data":{{"markPrice":{price},"indexPrice":{price}}}}}"#
        );
        self.event(Event::WsMessage {
            socket,
            text: &instrument,
        });
        let snapshot = format!(
            r#"{{"type":"message","topic":"/contractMarket/snapshot:XBTUSDTM","data":{{"symbol":"XBTUSDTM","lastPrice":{price}}}}}"#
        );
        self.event(Event::WsMessage {
            socket,
            text: &snapshot,
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
fn provider_refusal_is_not_sent_and_releases_the_native_write_job() {
    let mut h = Harness::new();
    let provider = h.engine.signer().unwrap();
    h.engine
        .set_signer(Some(std::sync::Arc::new(
            support::credentials::RefuseWrites(provider.clone()),
        )))
        .unwrap();
    place(&mut h, vec![order()]);
    h.metadata(true, "CROSS");
    h.pace();
    assert!(!h.has("/api/v1/orders"));
    assert_eq!(
        h.failures.last().unwrap().outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    h.engine.set_signer(Some(provider)).unwrap();
    h.command(Command::PlaceOrders {
        command: 2,
        orders: vec![order()],
    });
    assert_eq!(h.take("/api/v1/orders").method, Method::Post);
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
    second.amount = 0.1; // Rounds to zero; above-max sizes are sent for TS parity.
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
fn timestamp_refusal_does_not_resend_a_placement_or_cancel() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    assert_eq!(request.method, Method::Post);
    h.response(
        &request,
        400,
        r#"{"code":"400002","msg":"Invalid KC-API-TIMESTAMP"}"#,
    );
    h.pace();
    assert!(h.done(1).is_err());
    assert!(!h.has("/timestamp") && !h.has("/api/v1/orders"));

    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["unknown".into()],
    });
    let request = h.take("/api/v1/orders/unknown");
    assert_eq!(request.method, Method::Delete);
    h.response(
        &request,
        400,
        r#"{"code":"400002","msg":"Invalid KC-API-TIMESTAMP"}"#,
    );
    h.pace();
    assert!(h.done(1).is_err());
    assert!(!h.has("/timestamp") && !h.has("/api/v1/orders"));
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
fn a_limit_whose_replacement_is_refused_says_the_order_is_gone() {
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
    h.response(
        &cancel,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 4.0);
    let create = h.take("/api/v1/orders");
    h.response(
        &create,
        200,
        r#"{"code":"300003","msg":"Balance insufficient"}"#,
    );
    h.pace();
    let error = h.done(1).unwrap_err();
    assert!(error.contains("order is gone"), "{error}");
}
#[test]
fn a_partly_filled_standalone_stop_is_refused_before_any_cancel() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    h.engine
        .store_mut()
        .update_order("old", |o| o.filled = 0.001);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    assert!(writes(&h).is_empty());
    assert_eq!(failure(&h).outcome, Some(Outcome::NotSent));
    assert_eq!(failure(&h).protection, None);
    assert!(h.engine.store().order("old").is_some());
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
    let mut position = short_position();
    position.side = PositionSide::Long;
    h.engine.store_mut().set_positions(vec![position]);
    h.set_order("parent__take_profit", OrderType::TakeProfit, 6.0);
    h.engine
        .store_mut()
        .update_order("parent__take_profit", |o| o.side = OrderSide::Sell);
    h.engine
        .store_mut()
        .update_order("parent__stop_loss", |o| o.side = OrderSide::Sell);
    h.fresh_ticker(95.0);
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
    let mut position = short_position();
    position.side = PositionSide::Long;
    h.engine.store_mut().set_positions(vec![position]);
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
    h.reply(
        "/api/v1/stopOrders?symbol=XBTUSDTM",
        r#"{"code":"200000","data":[]}"#,
    );
    h.pace();
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
    for (amount, expected) in [(None, "2"), (Some(9.0), "1")] {
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
fn write_failures_preserve_definitive_refusals_and_explain_ambiguity() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    let unknown = (None, Some(Outcome::Unknown));
    for (status, body, classified, text) in [
        (
            503,
            "<html>",
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
            "KuCoin write outcome is unknown. Check Open Orders",
        ),
        (
            503,
            r#"{"code":"500000","msg":"busy"}"#,
            (Some(ErrorKind::VenueUnavailable), Some(Outcome::Unknown)),
            "KuCoin write outcome is unknown. Check Open Orders",
        ),
        (
            429,
            r#"{"code":"429000","msg":"too many"}"#,
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            "KuCoin write outcome is unknown. Check Open Orders",
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
        // An IP the key's whitelist excludes, at any 4xx: what to do, as an auth refusal.
        (
            400,
            r#"{"code":"400006","msg":"The requested IP address is not in the API whitelist"}"#,
            rejected(ErrorKind::AuthPermission),
            "KuCoin API Error 400006: The requested IP address is not in the API whitelist (HTTP 400). The API key's IP whitelist does not include this server: add the new Tealstreet IPs to the key on the exchange",
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
            "KuCoin write outcome is unknown. Check Open Orders",
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
            if outcome.1 == Some(Outcome::Unknown) {
                "KuCoin write outcome is unknown. Check Open Orders"
            } else {
                "KuCoin API Error 100001: Unknown order (HTTP 400)"
            }
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

/// A buy stop of 6 lots at 100 (it closes a short), whose trigger moves to
/// 120 against a fresh mark of 110; nothing has filled, so the original can
/// be put back exactly.
fn resting_stop(h: &mut Harness) {
    h.set_order("old", OrderType::StopLoss, 6.0);
    h.engine.store_mut().update_order("old", |o| o.filled = 0.0);
    h.fresh_ticker(110.0);
    h.engine.store_mut().set_positions(vec![short_position()]);
}
fn amend_stop(h: &mut Harness, id: &str, price: f64) {
    h.command(Command::AmendOrder {
        command: 1,
        id: id.into(),
        price: Some(price),
        amount: None,
    });
}
fn writes(h: &Harness) -> Vec<String> {
    h.actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.url.clone()),
            _ => None,
        })
        .collect()
}
/// The cancel lands, reads as cancelled with `filled` lots, and the
/// replacement is the next write.
fn to_replacement(h: &mut Harness, id: &str, filled: f64) -> HttpRequest {
    h.metadata(false, "CROSS");
    let cancel = h.take(&format!("/api/v1/orders/{id}"));
    assert_eq!(cancel.method, Method::Delete);
    h.response(
        &cancel,
        200,
        &format!(r#"{{"code":"200000","data":{{"cancelledOrderIds":["{id}"]}}}}"#),
    );
    h.pace();
    h.confirm_cancel(id, 10.0, filled);
    let index = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
        .unwrap_or_else(|| panic!("no replacement: {:?}", h.actions));
    let Action::Http(request) = h.actions.remove(index) else {
        unreachable!()
    };
    request
}
fn short_position() -> Position {
    Position {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Short,
        entry_price: 110.0,
        notional: 0.66,
        leverage: 5.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 6.0,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }
}
const REFUSED: &str = r#"{"code":"300003","msg":"Balance insufficient"}"#;
fn failure(h: &Harness) -> safe_cex_engine::VenueError {
    h.failures.last().cloned().expect("a typed failure")
}

#[test]
/// Planning rejects an unsafe edit; managed Claim checks it again at release.
fn a_stale_crossed_or_flat_stop_at_the_cancel_cancels_nothing() {
    for case in ["stale", "crossed", "flat"] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        h.engine
            .store_mut()
            .update_order("old", |o| o.reduce_only = true);
        h.engine.store_mut().set_positions(vec![short_position()]);
        if case == "stale" {
            // Fresh when planned, 5.01 s old when the cancel is sent 20 ms later.
            h.now += 4_990.0;
        }
        amend_stop(&mut h, "old", 120.0);
        h.metadata_replies(false, "CROSS");
        // The cancel is planned and sent on the next pacing tick.
        match case {
            "stale" => {}
            // The mark rose through the new trigger before the cancel left.
            "crossed" => h
                .engine
                .store_mut()
                .update_ticker("BTCUSDT", |t| t.mark = 125.0),
            _ => h.engine.store_mut().set_positions(vec![]),
        }
        h.pace();
        assert!(writes(&h).is_empty(), "{case}: {:?}", writes(&h));
        let error = failure(&h);
        let why = match case {
            "stale" => "mark price reference is stale",
            "crossed" => "already crossed the mark price",
            _ => "no position for it to protect",
        };
        assert!(
            error.message.contains("was not changed") && error.message.contains(why),
            "{case}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{case}");
        assert_eq!(error.protection, None, "{case}: the stop still stands");
        assert!(h.engine.store().order("old").is_some(), "{case}");
    }
}

#[test]
fn a_refused_replacement_puts_the_original_stop_back_once() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    assert!(
        replacement
            .body
            .as_deref()
            .unwrap()
            .contains(r#""stopPrice":"120""#)
    );
    h.fresh_ticker(95.0);
    h.response(&replacement, 200, REFUSED);
    h.pace();
    let restore = h.take("/api/v1/");
    let body = restore.body.clone().unwrap();
    for expected in [r#""stopPrice":"100""#, r#""size":"6""#, r#""side":"buy""#] {
        assert!(
            body.contains(expected),
            "the original, not the edit: {body}"
        );
    }
    h.response(
        &restore,
        200,
        r#"{"code":"200000","data":{"orderId":"restored"}}"#,
    );
    h.pace();
    let error = failure(&h);
    assert!(
        error.message.contains("is unchanged") && error.message.contains("put back"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Restored));
    assert!(writes(&h).is_empty(), "one put-back only: {:?}", writes(&h));
}

#[test]
fn a_refused_put_back_is_typed_unprotected_and_never_retried() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    h.fresh_ticker(95.0);
    h.response(&replacement, 200, REFUSED);
    h.pace();
    let restore = h.take("/api/v1/");
    h.response(&restore, 200, REFUSED);
    h.pace();
    let error = failure(&h);
    assert!(
        error.message.contains("both refused") && error.message.contains("re-place"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
}

const RATE_LIMITED: &str = r#"{"code":"429000","msg":"Too Many Requests"}"#;

/// A 429 may have placed the order, so it is unknown: never "refused" or Unprotected.
fn assert_unknown_stop(h: &Harness) {
    let error = failure(h);
    assert!(
        error
            .message
            .contains("outcome is unknown. Check Open Orders"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("both refused"), "{}", error.message);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(writes(h).is_empty(), "{:?}", writes(h));
}

#[test]
fn a_rate_limited_put_back_is_unknown_not_unprotected() {
    for (status, body) in [(429, RATE_LIMITED), (200, RATE_LIMITED)] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        let replacement = to_replacement(&mut h, "old", 0.0);
        h.fresh_ticker(95.0);
        h.response(&replacement, 200, REFUSED);
        h.pace();
        let restore = h.take("/api/v1/");
        h.response(&restore, status, body);
        h.pace();
        assert_unknown_stop(&h);
    }
}

#[test]
fn a_rate_limited_replacement_is_unknown_and_never_put_back() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    h.response(&replacement, 429, RATE_LIMITED);
    h.pace();
    assert_unknown_stop(&h);
}

#[test]
fn a_stop_that_filled_during_its_cancel_is_not_put_back() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    h.reply(
        "/api/v1/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 2.0);
    assert!(
        writes(&h).is_empty(),
        "a newly filled target never gets a replacement"
    );
    let error = failure(&h);
    assert!(
        error.message.contains("stop was removed") && error.message.contains("please re-place it"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
}

#[test]
fn a_leg_of_a_partly_filled_entry_says_its_stop_was_removed() {
    let mut h = Harness::new();
    h.set_order("parent", OrderType::Limit, 6.0);
    h.set_order("parent__stop_loss", OrderType::StopLoss, 6.0);
    let mut position = short_position();
    position.side = PositionSide::Long;
    h.engine.store_mut().set_positions(vec![position]);
    h.engine
        .store_mut()
        .update_order("parent__stop_loss", |o| o.side = OrderSide::Sell);
    h.fresh_ticker(95.0);
    amend_stop(&mut h, "parent__stop_loss", 90.0);
    let replacement = to_replacement(&mut h, "parent", 4.0);
    h.response(&replacement, 200, REFUSED);
    h.pace();
    let error = failure(&h);
    assert!(
        error.message.contains("stop was removed") && error.message.contains("please re-place it"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
}

#[test]
fn an_unknown_replacement_is_never_put_back_or_resent() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    h.event(Event::HttpFailed {
        request: replacement.id,
        error: "lost create response",
    });
    h.pace();
    let error = failure(&h);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(
        writes(&h).is_empty(),
        "no put-back over an unknown: {:?}",
        writes(&h)
    );
}

/// Lost, or refused outright, a confirmation read leaves the accepted cancel
/// unproven: never a refusal that would invite a put-back.
#[test]
fn a_lost_cancel_confirmation_is_unknown_with_no_put_back_or_resend() {
    for refused in [false, true] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        h.reply(
            "/api/v1/orders/old",
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        for attempt in 0..3 {
            let read = h.take("/api/v1/orders/old");
            assert_eq!(read.method, Method::Get);
            if refused {
                h.response(&read, 400, r#"{"code":"100001","msg":"Parameter error"}"#);
            } else {
                h.event(Event::HttpFailed {
                    request: read.id,
                    error: "lost confirmation read",
                });
            }
            if attempt < 2 {
                h.advance(100.0);
            } else {
                h.pace();
            }
        }
        let error = failure(&h);
        assert_eq!(
            error.outcome,
            Some(Outcome::Unknown),
            "refused read: {refused}"
        );
        assert_eq!(error.protection, None);
        assert!(
            error.message.contains("could not be confirmed")
                && error.message.contains("unknown")
                && error.message.contains("no replacement was sent"),
            "{}",
            error.message
        );
        assert!(writes(&h).is_empty(), "{:?}", writes(&h));
        assert!(!h.has("/api/v1/orders/old"), "no further reads");
    }
}

/// A leg whose row carries no source is replaced on its entry's (mark here),
/// so the veto checks the mark, not a last-price default.
#[test]
fn a_leg_crossed_on_the_mark_but_not_the_last_cancels_nothing() {
    let mut h = Harness::new();
    h.set_order("parent", OrderType::Limit, 6.0);
    h.set_order("parent__stop_loss", OrderType::StopLoss, 6.0);
    let mut position = short_position();
    position.side = PositionSide::Long;
    h.engine.store_mut().set_positions(vec![position]);
    h.engine.store_mut().update_order("parent__stop_loss", |o| {
        o.side = OrderSide::Sell;
        o.trigger_type = None;
    });
    h.fresh_ticker(95.0);
    amend_stop(&mut h, "parent__stop_loss", 90.0);
    // The mark falls through the new stop before the cancel; the last does not.
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.mark = 89.0);
    h.metadata(false, "CROSS");
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
    let error = failure(&h);
    assert!(
        error.message.contains("already crossed the mark price"),
        "{}",
        error.message
    );
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.protection, None);
}

/// A cancel refused for its signature is re-signed a round trip later; the
/// stop is checked again before that resend.
#[test]
fn a_stop_that_filled_before_its_resigned_cancel_is_left_alone() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    let cancel = h.take("/api/v1/orders/old");
    assert_eq!(cancel.method, Method::Delete);
    h.engine
        .store_mut()
        .update_order("old", |o| o.filled = 0.002);
    h.response(
        &cancel,
        400,
        r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#,
    );
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
    h.pace();
    let error = failure(&h);
    assert!(
        error.message.contains("was not changed")
            && error
                .message
                .contains("changed or filled before cancellation"),
        "{}",
        error.message
    );
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
}

/// A cancel answered 404 falls back to the stop family a round trip later;
/// that fallback, and its re-signed resend, are checked like the first.
#[test]
fn a_stop_family_fallback_cancel_is_vetoed_like_the_first() {
    for case in ["crossed", "filled", "resigned"] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        let cancel = h.take("/api/v1/orders/old");
        let cross = |h: &mut Harness| {
            h.engine
                .store_mut()
                .update_ticker("BTCUSDT", |t| t.mark = 125.0)
        };
        match case {
            "crossed" => cross(&mut h),
            "filled" => {
                h.engine
                    .store_mut()
                    .update_order("old", |o| o.filled = 0.002);
            }
            _ => {}
        }
        h.response(&cancel, 404, r#"{"code":"404","msg":"Not Found"}"#);
        if case == "resigned" {
            let fallback = h.take("/api/v1/stopOrders/old");
            assert_eq!(fallback.method, Method::Delete);
            cross(&mut h);
            h.response(
                &fallback,
                400,
                r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#,
            );
        }
        assert!(!h.has("/api/v1/stopOrders"), "{case}: {:?}", writes(&h));
        h.pace();
        let error = failure(&h);
        let why = if case == "filled" {
            "changed or filled before cancellation"
        } else {
            "already crossed the mark price"
        };
        assert!(
            error.message.contains("was not changed") && error.message.contains(why),
            "{case}: {}",
            error.message
        );
        assert_eq!(
            error.outcome,
            Some(if case == "resigned" {
                Outcome::Unknown
            } else {
                Outcome::Rejected
            }),
            "{case}"
        );
        assert_eq!(error.protection, None, "{case}");
        assert!(writes(&h).is_empty(), "{case}: {:?}", writes(&h));
    }
}

fn resting_stop_limit(h: &mut Harness, kind: OrderType, close: bool) {
    resting_stop(h);
    h.engine.store_mut().update_order("old", |o| {
        o.kind = kind;
        o.close = Some(close);
        o.reduce_only = true;
        o.extra.as_mut().unwrap().conditional_order_price = Some(130.0);
    });
    h.engine.store_mut().set_positions(vec![short_position()]);
}

#[test]
fn stop_limit_replacement_preserves_price_type_and_native_lots() {
    for (kind, trigger) in [(OrderType::StopLoss, 120.0), (OrderType::TakeProfit, 90.0)] {
        let mut h = Harness::new();
        resting_stop_limit(&mut h, kind, false);
        amend_stop(&mut h, "old", trigger);
        let request = to_replacement(&mut h, "old", 0.0);
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["type"], "limit");
        assert_eq!(body["price"], "130");
        assert_eq!(body["stopPrice"], trigger.to_string());
        assert_eq!(body["size"], "6");
        assert_ne!(
            body["closeOrder"], true,
            "omitted closeOrder defaults to false"
        );
        assert_eq!(body["stopPriceType"], "MP");
        assert_eq!(body["timeInForce"], "GTC");
    }
}

#[test]
fn stop_limit_put_back_preserves_original_type_price_trigger_and_size() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut h = Harness::new();
        resting_stop_limit(&mut h, kind, false);
        h.command(Command::AmendOrder {
            command: 1,
            id: "old".into(),
            price: Some(if kind == OrderType::StopLoss {
                120.0
            } else {
                90.0
            }),
            amount: Some(8.0),
        });
        let replacement = to_replacement(&mut h, "old", 0.0);
        h.fresh_ticker(original_safe_quote(kind, OrderSide::Buy));
        h.response(&replacement, 200, REFUSED);
        h.pace();
        let restore = h.take("/api/v1/orders");
        let body: serde_json::Value =
            serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["type"], "limit");
        assert_eq!(body["price"], "130");
        assert_eq!(body["stopPrice"], "100");
        assert_eq!(
            body["size"], "6",
            "put-back uses original lots, not the edit"
        );
        assert_ne!(
            body["closeOrder"], true,
            "omitted closeOrder defaults to false"
        );
        h.response(
            &restore,
            200,
            r#"{"code":"200000","data":{"orderId":"restored"}}"#,
        );
        h.pace();
        assert_eq!(failure(&h).protection, Some(Protection::Restored));
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn stop_close_flag_is_preserved_even_when_the_native_row_has_a_size() {
    let mut h = Harness::new();
    resting_stop_limit(&mut h, OrderType::StopLoss, true);
    amend_stop(&mut h, "old", 120.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["closeOrder"], true);
    assert!(
        body.get("size").is_none(),
        "closeOrder owns the full-position quantity"
    );
    h.fresh_ticker(95.0);
    h.response(&replacement, 200, REFUSED);
    h.pace();
    let restore = h.take("/api/v1/orders");
    let body: serde_json::Value = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["closeOrder"], true);
    assert!(body.get("size").is_none());
}

#[test]
fn stop_limit_with_unusable_limit_price_is_refused_before_cancel() {
    for price in [0.0, f64::NAN, 0.01] {
        let mut h = Harness::new();
        resting_stop_limit(&mut h, OrderType::StopLoss, false);
        h.engine.store_mut().update_order("old", |o| {
            o.extra.as_mut().unwrap().conditional_order_price = Some(price);
        });
        amend_stop(&mut h, "old", 120.0);
        if h.has("getMarginMode") {
            h.metadata(false, "CROSS");
        }
        assert!(writes(&h).is_empty(), "invalid native limit: {price}");
        assert_eq!(failure(&h).outcome, Some(Outcome::NotSent));
        assert!(h.engine.store().order("old").is_some());
    }
}

#[test]
fn new_app_protection_with_redundant_trigger_remains_stop_market() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut h = Harness::new();
        let mut opts = order();
        opts.kind = kind;
        opts.client_order_id = None;
        opts.price = Some(110.0);
        opts.trigger_price = Some(110.0);
        place(&mut h, vec![opts]);
        h.metadata(false, "CROSS");
        let request = h.take("/api/v1/orders");
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["type"], "market");
        assert_eq!(body["stopPrice"], "110");
        assert!(body.get("price").is_none());
        assert!(body.get("timeInForce").is_none());
    }
}

#[test]
fn new_redundant_trigger_stop_cannot_bypass_post_only_refusal() {
    let mut h = Harness::new();
    let mut opts = order();
    opts.kind = OrderType::StopLoss;
    opts.client_order_id = None;
    opts.trigger_price = opts.price;
    opts.time_in_force = Some(TimeInForce::PostOnly);
    place(&mut h, vec![opts]);
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. }))),
        "new PostOnly stop must refuse synchronously"
    );
    assert!(writes(&h).is_empty());
    assert_eq!(failure(&h).outcome, Some(Outcome::NotSent));
}

#[test]
fn new_sized_close_market_or_stop_keeps_the_explicit_native_size() {
    for kind in [
        OrderType::Market,
        OrderType::StopLoss,
        OrderType::TakeProfit,
    ] {
        let mut h = Harness::new();
        let mut opts = order();
        opts.kind = kind;
        opts.client_order_id = None;
        opts.close = Some(true);
        opts.amount = 10.0;
        place(&mut h, vec![opts]);
        h.metadata(false, "CROSS");
        let request = h.take("/api/v1/orders");
        let body: serde_json::Value =
            serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["size"], "10");
        assert_ne!(body["closeOrder"], true);
        assert_eq!(body["type"], "market");
    }
}

#[test]
fn native_stop_limit_equal_to_its_trigger_stays_limit_on_replacement_and_put_back() {
    let mut h = Harness::new();
    resting_stop_limit(&mut h, OrderType::StopLoss, false);
    h.engine.store_mut().update_order("old", |o| {
        o.price = 130.0;
    });
    h.fresh_ticker(110.0);
    amend_stop(&mut h, "old", 130.0);
    let replacement = to_replacement(&mut h, "old", 0.0);
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["type"], "limit");
    assert_eq!(body["price"], "130");
    assert_eq!(body["stopPrice"], "130");
    h.response(&replacement, 200, REFUSED);
    h.pace();
    let restore = h.take("/api/v1/orders");
    let body: serde_json::Value = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["type"], "limit");
    assert_eq!(body["price"], "130");
    assert_eq!(body["stopPrice"], "130");
}

#[test]
fn an_ordinary_limit_amend_keeps_the_original_close_order_wire_shape() {
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
        "/api/v1/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 4.0);
    let replacement = h.take("/api/v1/orders");
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["type"], "limit");
    assert_eq!(body["size"], "6");
    assert!(
        body.get("closeOrder").is_none(),
        "native default false needs no new field"
    );
}

fn hold_protective_cancel(h: &mut Harness) -> safe_cex_engine::request_context::Preparation {
    h.metadata_replies(false, "CROSS");
    let index = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(index) else {
        unreachable!()
    };
    h.now = at_ms;
    h.engine.handle(h.now, Event::TimerFired { timer });
    next_cancel_prepare(
        h,
        "https://api-futures.kucoin.com/api/v1/orders/old?symbol=XBTUSDTM",
    )
}
fn next_cancel_prepare(
    h: &mut Harness,
    expected_url: &str,
) -> safe_cex_engine::request_context::Preparation {
    use safe_cex_engine::request_context::HttpAction;
    h.engine.prepare_dispatch(h.now);
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Delete)),
        "no legacy cancel: {actions:?}"
    );
    let preparation = actions.iter().find_map(|a| match a {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if request.method == Method::Delete => {
                assert_eq!(request.url, expected_url);
                Some(*preparation)
            }
            _ => None,
        },
        _ => None,
    });
    assert!(
        preparation.is_some(),
        "the genuine cancel must enter Prepare: {actions:?}"
    );
    preparation.unwrap()
}
fn release_held(h: &mut Harness, preparation: safe_cex_engine::request_context::Preparation) {
    h.engine.http_prepared(
        h.now,
        preparation,
        safe_cex_engine::request_context::Prepared {
            id: preparation.attempt().request.0,
            expires_at: None,
        },
    );
    for action in h.engine.drain_actions().collect::<Vec<_>>() {
        match action {
            Action::ManagedHttp(action) => match *action {
                safe_cex_engine::request_context::HttpAction::Bind {
                    preparation: bound,
                    permit,
                    request,
                } => {
                    assert_eq!(bound, preparation);
                    if matches!(
                        h.engine.claim_http(h.now, permit),
                        Ok(safe_cex_engine::request_context::Claim::Released(_))
                    ) {
                        h.actions.push(Action::Http(request));
                    }
                }
                other => panic!("expected exact bind, got {other:?}"),
            },
            other => h.actions.push(plain(other, &mut h.failures)),
        }
    }
    for action in safe_cex_engine::test_support::prepared_actions(&mut h.engine, h.now) {
        h.actions.push(plain(action, &mut h.failures));
    }
}
#[test]
fn managed_stop_cancel_checks_every_final_claim_after_prepare() {
    for case in ["live", "equal", "stale", "filled", "flat", "new_sibling"] {
        let mut h = Harness::new();
        resting_stop_limit(&mut h, OrderType::StopLoss, false);
        let seen = h.now;
        amend_stop(&mut h, "old", 120.0);
        let preparation = hold_protective_cancel(&mut h);
        match case {
            "live" => h.now = seen + 5_000.0,
            "equal" => h
                .engine
                .store_mut()
                .update_ticker("BTCUSDT", |t| t.mark = 120.0),
            "stale" => h.now = seen + 5_000.25,
            "filled" => {
                h.engine
                    .store_mut()
                    .update_order("old", |o| o.filled = 0.001);
            }
            "flat" => h.engine.store_mut().set_positions(vec![]),
            _ => h.set_order("old__take_profit", OrderType::TakeProfit, 6.0),
        }
        release_held(&mut h, preparation);
        if case == "live" {
            let cancel = h.take("/api/v1/orders/old");
            assert_eq!(cancel.method, Method::Delete);
            assert!(h.failures.is_empty());
        } else {
            assert!(writes(&h).is_empty(), "{case}: {:?}", writes(&h));
            assert_eq!(h.failures.len(), 1, "{case}");
            assert_eq!(failure(&h).outcome, Some(Outcome::NotSent), "{case}");
            assert_eq!(failure(&h).protection, None, "{case}");
            assert!(h.engine.store().order("old").is_some());
        }
    }
}
#[test]
fn managed_stop_field_receipts_do_not_borrow_another_source_or_store_write() {
    for case in ["same_mark", "only_last", "only_index", "store_write"] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        h.now += 4_990.0;
        let socket = h.quote_socket.unwrap();
        match case {
            "same_mark" => h.event(Event::WsMessage { socket, text: r#"{"type":"message","topic":"/contract/instrument:XBTUSDTM","data":{"markPrice":110}}"# }),
            "only_index" => h.event(Event::WsMessage { socket, text: r#"{"type":"message","topic":"/contract/instrument:XBTUSDTM","data":{"indexPrice":110}}"# }),
            "only_last" => h.event(Event::WsMessage { socket, text: r#"{"type":"message","topic":"/contractMarket/snapshot:XBTUSDTM","data":{"symbol":"XBTUSDTM","lastPrice":110}}"# }),
            _ => h.engine.store_mut().update_ticker("BTCUSDT", |t| t.mark = 111.0),
        }
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        if case == "same_mark" {
            assert!(
                h.has("/api/v1/orders/old"),
                "an actual unchanged mark event refreshes that field"
            );
            assert_eq!(h.take("/api/v1/orders/old").method, Method::Delete);
        } else {
            assert!(writes(&h).is_empty(), "{case}");
            assert_eq!(failure(&h).outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn managed_stop_ambiguous_writes_never_restore_or_resend() {
    for restore in [false, true] {
        for (status, body) in [
            (408, REFUSED),
            (500, REFUSED),
            (429, REFUSED),
            (200, RATE_LIMITED),
            (200, "{"),
            (200, r#"{"code":"200000","data":{}}"#),
            (200, r#"{"code":"900000","msg":"unlisted"}"#),
        ] {
            let mut h = Harness::new();
            resting_stop(&mut h);
            amend_stop(&mut h, "old", 120.0);
            let mut request = to_replacement(&mut h, "old", 0.0);
            if restore {
                h.fresh_ticker(95.0);
                h.response(&request, 200, REFUSED);
                h.pace();
                request = h.take("/api/v1/orders");
            }
            h.response(&request, status, body);
            h.pace();
            assert_eq!(
                h.failures.len(),
                1,
                "restore={restore} status={status} {body}"
            );
            assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
            assert_eq!(failure(&h).protection, None);
            assert!(writes(&h).is_empty());
            assert!(failure(&h).message.contains("outcome is unknown"));
        }
    }
}
#[test]
fn managed_stop_cancellation_proof_requires_exact_terminal_identity_and_fills() {
    for data in [
        r#"{"id":"foreign","isActive":false,"cancelExist":true,"filledSize":0}"#,
        r#"{"id":"old","isActive":false,"cancelExist":true,"filledSize":null}"#,
        r#"{"id":"old","isActive":false,"cancelExist":true,"filledSize":""}"#,
        r#"{"id":"old","isActive":false,"cancelExist":true,"filledSize":-1}"#,
        r#"{"id":"old","isActive":false,"cancelExist":true,"filledSize":0,"dealSize":1}"#,
    ] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        h.reply(
            "/api/v1/orders/old",
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        for attempt in 0..3 {
            let read = h.take("/api/v1/orders/old");
            assert_eq!(read.method, Method::Get);
            h.response(&read, 200, &format!(r#"{{"code":"200000","data":{data}}}"#));
            if h.actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
            {
                h.pace();
            }
            assert!(
                writes(&h).is_empty(),
                "unproven target/fills cannot authorize a create: {data}"
            );
            if attempt < 2 {
                h.advance(100.0);
            }
        }
        assert_eq!(h.failures.len(), 1, "{data}");
        assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
        assert_eq!(failure(&h).protection, None);
        assert!(h.engine.store().order("old").is_some());
        assert!(writes(&h).is_empty());
    }
}
#[test]
fn managed_stop_final_create_claim_rechecks_flatness_and_new_fills() {
    for case in ["flat", "fill", "new_order"] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        let original = h.engine.store().order("old").unwrap().clone();
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        h.reply(
            "/api/v1/orders/old",
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        let read = h.take("/api/v1/orders/old");
        h.response(&read, 200, r#"{"code":"200000","data":{"id":"old","isActive":false,"cancelExist":true,"filledSize":0}}"#);
        match case {
            "flat" => h.engine.store_mut().set_positions(vec![]),
            "new_order" => h.set_order("old", OrderType::StopLoss, 6.0),
            _ => h.engine.store_mut().upsert_order(Order {
                filled: 0.001,
                ..original
            }),
        }
        h.pace();
        assert!(writes(&h).is_empty(), "{case}");
        assert_eq!(
            failure(&h).protection,
            Some(Protection::Unprotected),
            "{case}"
        );
    }
}

#[test]
fn managed_stop_retirement_and_command_id_reuse_ignore_the_old_callback() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let held = hold_protective_cancel(&mut h);
    h.engine.store_mut().set_positions(vec![]);
    release_held(&mut h, held);
    assert_eq!(h.failures.len(), 1);
    assert_eq!(failure(&h).outcome, Some(Outcome::NotSent));
    h.engine.store_mut().set_positions(vec![short_position()]);
    amend_stop(&mut h, "old", 120.0);
    h.pace();
    let cancel = h.take("/api/v1/orders/old");
    h.event(Event::HttpResponse {
        request: held.attempt().request,
        response: Response {
            status: 200,
            headers: "",
            body: r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        },
    });
    assert!(
        !h.has("/api/v1/orders/old"),
        "retired generation creates no proof leg"
    );
    assert_eq!(h.failures.len(), 1);
    assert!(h.engine.store().order("old").is_some());
    h.response(
        &cancel,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.confirm_cancel("old", 10.0, 0.0);
    let replacement = h.take("/api/v1/orders");
    h.response(
        &replacement,
        200,
        r#"{"code":"200000","data":{"orderId":"replacement"}}"#,
    );
    assert_eq!(
        h.done(1),
        Err(h.failures[0].legacy_text()),
        "the earlier generation retains its own failure notice"
    );
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, .. })))
            .count(),
        1
    );
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, result }) if result == r#"["replacement"]"#)));
}

#[test]
fn managed_stop_signature_refusals_are_bounded_and_put_back_is_one_shot() {
    let mut h = Harness::new();
    resting_stop(&mut h);
    amend_stop(&mut h, "old", 120.0);
    let first = to_replacement(&mut h, "old", 0.0);
    let bad_signature = r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#;
    h.response(&first, 400, bad_signature);
    let second = h.take("/api/v1/orders");
    assert_eq!(
        first.body, second.body,
        "one retry preserves the exact clientOid/payload"
    );
    h.response(&second, 400, bad_signature);
    h.fresh_ticker(95.0);
    h.pace();
    let restore = h.take("/api/v1/orders");
    assert!(
        restore
            .body
            .as_deref()
            .unwrap()
            .contains(r#""stopPrice":"100""#)
    );
    h.response(&restore, 400, bad_signature);
    assert!(writes(&h).is_empty(), "the put-back is never re-sent");
    h.pace();
    assert_eq!(failure(&h).protection, Some(Protection::Unprotected));
    assert!(failure(&h).message.contains("re-place"));
    assert!(writes(&h).is_empty(), "no re-sign or second put-back");
}

#[test]
fn managed_stop_fallback_and_resign_claims_recheck_after_their_own_prepare() {
    for fallback in [false, true] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        let cancel = h.take("/api/v1/orders/old");
        let (status, body, url) = if fallback {
            (
                404,
                r#"{"code":"404000","msg":"URL Not Found"}"#,
                "https://api-futures.kucoin.com/api/v1/stopOrders/old",
            )
        } else {
            (
                400,
                r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#,
                "https://api-futures.kucoin.com/api/v1/orders/old?symbol=XBTUSDTM",
            )
        };
        h.now += 1.0;
        h.engine.handle(
            h.now,
            Event::HttpResponse {
                request: cancel.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let held = next_cancel_prepare(&mut h, url);
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.mark = 120.0);
        release_held(&mut h, held);
        assert!(writes(&h).is_empty());
        assert_eq!(h.failures.len(), 1);
        assert!(failure(&h).message.contains("already crossed"));
        assert_eq!(
            failure(&h).outcome,
            Some(if fallback {
                Outcome::Rejected
            } else {
                Outcome::Unknown
            })
        );
        assert!(h.engine.store().order("old").is_some());
    }
}

#[test]
fn managed_stop_confirmed_fallback_and_resigned_cancel_complete_once() {
    for fallback in [false, true] {
        let mut h = Harness::new();
        resting_stop(&mut h);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        let first = h.take("/api/v1/orders/old");
        h.response(
            &first,
            if fallback { 404 } else { 400 },
            if fallback {
                r#"{"code":"404000","msg":"URL Not Found"}"#
            } else {
                r#"{"code":"400005","msg":"Invalid KC-API-SIGN"}"#
            },
        );
        let cancel = h.take(if fallback {
            "/api/v1/stopOrders/old"
        } else {
            "/api/v1/orders/old"
        });
        assert_eq!(cancel.method, Method::Delete);
        h.response(
            &cancel,
            200,
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        h.confirm_cancel("old", 10.0, 0.0);
        let replacement = h.take("/api/v1/orders");
        h.response(
            &replacement,
            200,
            r#"{"code":"200000","data":{"orderId":"replacement"}}"#,
        );
        h.pace();
        let results: Vec<_> = h
            .actions
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandDone { command: 1, result }) => Some(result.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            results,
            [r#"["replacement"]"#],
            "fallback={fallback}: {:?}",
            h.failures
        );
        assert!(h.failures.is_empty());
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn managed_stop_discovery_receipts_keep_row_time_and_require_a_valid_native_field() {
    for case in ["live", "stale", "fallback", "rejected"] {
        let mut h = Harness::new();
        *h.engine.store_mut() = safe_cex_engine::Store::default();
        h.engine.start(h.now);
        h.event(Event::PollCeilingChanged { ceiling_ms: None });
        h.reply("/contracts/active", r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","baseCurrency":"BTC","quoteCurrency":"USDT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"multiplier":0.001,"maxOrderQty":1000,"maxLeverage":100}]}"#);
        let tickers = h.take("/allTickers");
        assert_eq!(tickers.method, Method::Get);
        h.event(Event::HttpStart {
            request: tickers.id,
            status: 200,
            headers: "",
        });
        let body = if case == "fallback" {
            r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"110"}]"#
        } else if case == "rejected" {
            r#"{"code":"400100","data":[{"symbol":"XBTUSDTM","price":"110","markPrice":"110"}]"#
        } else {
            r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"110","markPrice":"110"}]"#
        };
        h.event(Event::HttpChunk {
            request: tickers.id,
            data: body.as_bytes(),
        });
        // The row was staged now; the end of the envelope grants no new quote age.
        h.now += if case == "stale" { 4_980.25 } else { 4_980.0 };
        h.event(Event::HttpChunk {
            request: tickers.id,
            data: b"}",
        });
        h.event(Event::HttpEnd {
            request: tickers.id,
        });
        if case == "rejected" {
            h.engine
                .store_mut()
                .update_ticker("BTCUSDT", |ticker| ticker.mark = 110.0);
        }
        h.actions.clear();
        h.set_order("old", OrderType::StopLoss, 6.0);
        h.engine.store_mut().update_order("old", |o| o.filled = 0.0);
        h.engine.store_mut().set_positions(vec![short_position()]);
        amend_stop(&mut h, "old", 120.0);
        h.metadata(false, "CROSS");
        if case == "live" {
            assert_eq!(h.take("/api/v1/orders/old").method, Method::Delete);
        } else {
            assert!(writes(&h).is_empty(), "{case}: {:?}", writes(&h));
            assert_eq!(failure(&h).outcome, Some(Outcome::NotSent), "{case}");
            assert!(h.engine.store().order("old").is_some());
        }
    }
}

fn protective_kind(h: &mut Harness, kind: OrderType, side: OrderSide) -> f64 {
    resting_stop_limit(h, kind, false);
    h.engine
        .store_mut()
        .update_order("old", |order| order.side = side);
    let mut position = short_position();
    position.side = if side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    h.engine.store_mut().set_positions(vec![position]);
    if (kind == OrderType::StopLoss) == (side == OrderSide::Buy) {
        120.0
    } else {
        90.0
    }
}
fn original_safe_quote(kind: OrderType, side: OrderSide) -> f64 {
    if (kind == OrderType::StopLoss) == (side == OrderSide::Buy) {
        95.0
    } else {
        110.0
    }
}
fn confirm_protective_target(h: &mut Harness) {
    h.metadata(false, "CROSS");
    h.reply(
        "/api/v1/orders/old",
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    let read = h.take("/api/v1/orders/old");
    assert_eq!(read.method, Method::Get);
    h.response(&read, 200, r#"{"code":"200000","data":{"id":"old","isActive":false,"cancelExist":true,"filledSize":0}}"#);
}
fn hold_protective_create(
    h: &mut Harness,
    trigger: f64,
) -> safe_cex_engine::request_context::Preparation {
    use safe_cex_engine::request_context::HttpAction;
    let index = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(index) else {
        unreachable!()
    };
    h.now = at_ms;
    h.engine.handle(h.now, Event::TimerFired { timer });
    h.engine.prepare_dispatch(h.now);
    let actions: Vec<_> = h.engine.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method == Method::Post))
    );
    let preparation = actions.iter().find_map(|action| match action {
        Action::ManagedHttp(action) => match action.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if request.method == Method::Post => {
                assert_eq!(request.url, "https://api-futures.kucoin.com/api/v1/orders");
                let body: serde_json::Value =
                    serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                assert_eq!(body["stopPrice"], trigger.to_string());
                assert_eq!(body["type"], "limit");
                assert_eq!(body["price"], "130");
                Some(*preparation)
            }
            _ => None,
        },
        _ => None,
    });
    assert!(
        preparation.is_some(),
        "a genuine prepared create: {actions:?}"
    );
    preparation.unwrap()
}

#[test]
fn replacement_claim_checks_its_own_fresh_trigger_for_both_kinds_and_sides() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            for case in ["equal", "stale", "live"] {
                let mut h = Harness::new();
                let trigger = protective_kind(&mut h, kind, side);
                let seen = h.now;
                amend_stop(&mut h, "old", trigger);
                confirm_protective_target(&mut h);
                let held = hold_protective_create(&mut h, trigger);
                match case {
                    "equal" => h.fresh_ticker(trigger),
                    "stale" => h.now = seen + 5_000.25,
                    _ => h.now = seen + 5_000.0,
                }
                release_held(&mut h, held);
                if case == "live" {
                    assert_eq!(h.take("/api/v1/orders").method, Method::Post);
                    assert!(h.failures.is_empty());
                } else {
                    assert!(
                        writes(&h).is_empty(),
                        "{kind:?} {side:?} {case}: {:?}",
                        writes(&h)
                    );
                    assert_eq!(h.failures.len(), 1);
                    assert_eq!(failure(&h).protection, Some(Protection::Unprotected));
                    assert!(failure(&h).message.contains(if case == "equal" {
                        "crossed"
                    } else {
                        "stale"
                    }));
                    assert!(h.engine.store().order("old").is_none());
                }
            }
        }
    }
}

#[test]
fn put_back_claim_checks_the_original_trigger_for_both_kinds_and_sides() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            for case in ["equal", "stale", "live"] {
                let mut h = Harness::new();
                let trigger = protective_kind(&mut h, kind, side);
                amend_stop(&mut h, "old", trigger);
                let replacement = to_replacement(&mut h, "old", 0.0);
                h.fresh_ticker(original_safe_quote(kind, side));
                let seen = h.now;
                h.response(&replacement, 200, REFUSED);
                let held = hold_protective_create(&mut h, 100.0);
                match case {
                    "equal" => h.fresh_ticker(100.0),
                    "stale" => h.now = seen + 5_000.25,
                    _ => h.now = seen + 5_000.0,
                }
                release_held(&mut h, held);
                if case == "live" {
                    assert_eq!(h.take("/api/v1/orders").method, Method::Post);
                    assert!(h.failures.is_empty());
                } else {
                    assert!(
                        writes(&h).is_empty(),
                        "{kind:?} {side:?} {case}: {:?}",
                        writes(&h)
                    );
                    assert_eq!(h.failures.len(), 1);
                    assert_eq!(failure(&h).protection, Some(Protection::Unprotected));
                    assert!(failure(&h).message.contains(if case == "equal" {
                        "crossed"
                    } else {
                        "stale"
                    }));
                    assert!(h.engine.store().order("old").is_none());
                }
            }
        }
    }
}

#[test]
fn replacement_crossed_during_proof_polling_sends_no_create_or_put_back() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut h = Harness::new();
        let trigger = protective_kind(&mut h, kind, OrderSide::Buy);
        amend_stop(&mut h, "old", trigger);
        h.metadata(false, "CROSS");
        h.reply(
            "/api/v1/orders/old",
            r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
        );
        h.pace();
        let first = h.take("/api/v1/orders/old");
        h.response(
            &first,
            200,
            r#"{"code":"200000","data":{"id":"old","isActive":true,"filledSize":0}}"#,
        );
        h.fresh_ticker(trigger);
        h.advance(100.0);
        let proof = h.take("/api/v1/orders/old");
        h.response(&proof, 200, r#"{"code":"200000","data":{"id":"old","isActive":false,"cancelExist":true,"filledSize":0}}"#);
        h.pace();
        assert!(writes(&h).is_empty(), "{kind:?}: {:?}", writes(&h));
        assert_eq!(h.failures.len(), 1);
        assert_eq!(failure(&h).protection, Some(Protection::Unprotected));
        assert!(failure(&h).message.contains("crossed"));
    }
}

#[test]
fn protective_failure_words_keep_the_kind_and_protected_position_side() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        for side in [OrderSide::Buy, OrderSide::Sell] {
            for result in [
                "restored",
                "refused",
                "unknown_cancel",
                "unknown_proof",
                "unknown_place",
                "unknown_restore",
            ] {
                let mut h = Harness::new();
                let trigger = protective_kind(&mut h, kind, side);
                amend_stop(&mut h, "old", trigger);
                if result == "unknown_cancel" {
                    h.metadata(false, "CROSS");
                    let cancel = h.take("/api/v1/orders/old");
                    h.event(Event::HttpFailed {
                        request: cancel.id,
                        error: "lost cancel",
                    });
                } else if result == "unknown_proof" {
                    h.metadata(false, "CROSS");
                    h.reply(
                        "/api/v1/orders/old",
                        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
                    );
                    h.pace();
                    for attempt in 0..3 {
                        let read = h.take("/api/v1/orders/old");
                        h.event(Event::HttpFailed {
                            request: read.id,
                            error: "lost proof",
                        });
                        if attempt < 2 {
                            h.advance(100.0);
                        }
                    }
                } else {
                    let replacement = to_replacement(&mut h, "old", 0.0);
                    if result == "unknown_place" {
                        h.event(Event::HttpFailed {
                            request: replacement.id,
                            error: "lost replacement",
                        });
                    } else {
                        h.fresh_ticker(original_safe_quote(kind, side));
                        h.response(&replacement, 200, REFUSED);
                        h.pace();
                        let restore = h.take("/api/v1/orders");
                        match result {
                            "restored" => h.response(
                                &restore,
                                200,
                                r#"{"code":"200000","data":{"orderId":"restored"}}"#,
                            ),
                            "refused" => h.response(&restore, 200, REFUSED),
                            _ => h.event(Event::HttpFailed {
                                request: restore.id,
                                error: "lost put-back",
                            }),
                        }
                    }
                }
                let error = failure(&h);
                let position = if side == OrderSide::Buy {
                    "short"
                } else {
                    "long"
                };
                let label = if kind == OrderType::TakeProfit {
                    "take profit"
                } else {
                    "stop"
                };
                let prefix = match result {
                    "restored" => {
                        format!("Your BTCUSDT {position} position's {label} is unchanged")
                    }
                    "refused" => format!("Your BTCUSDT {position} position has no {label}"),
                    "unknown_cancel" => format!(
                        "Your BTCUSDT {position} position's {label} edit outcome is unknown"
                    ),
                    "unknown_proof" => format!(
                        "Your BTCUSDT {position} position's {label} cancel was sent but could not be confirmed"
                    ),
                    "unknown_place" => {
                        format!(
                            "Your BTCUSDT {position} position's {label} replacement outcome is unknown"
                        )
                    }
                    _ => format!(
                        "Your BTCUSDT {position} position's {label} put-back outcome is unknown"
                    ),
                };
                assert!(
                    error.message.starts_with(&prefix),
                    "{result}: {}",
                    error.message
                );
                assert_eq!(
                    error.message.matches("Your BTCUSDT").count(),
                    1,
                    "{}",
                    error.message
                );
                assert_eq!(
                    error.protection,
                    match result {
                        "restored" => Some(Protection::Restored),
                        "refused" => Some(Protection::Unprotected),
                        _ => None,
                    }
                );
                assert!(writes(&h).is_empty());
            }
        }
    }
}

// Dispatched writes: transport ambiguity and native rate limits never prove refusal.
#[test]
fn dispatched_write_ambiguity_matrix() {
    for (status, body) in [
        (429, "{}"),
        (429, REFUSED),
        (429, RATE_LIMITED),
        (429, r#"{"code":"400005","msg":"signature refused"}"#),
        (200, RATE_LIMITED),
        (408, REFUSED),
        (500, REFUSED),
        (503, RATE_LIMITED),
        (408, r#"{"code":"400005","msg":"signature refused"}"#),
        (500, r#"{"code":"400005","msg":"signature refused"}"#),
        (503, r#"{"code":"400005","msg":"signature refused"}"#),
        (599, r#"{"code":"400005","msg":"signature refused"}"#),
        (400, r#"{"code":"429000","msg":"Too Many Requests"}"#),
        (404, r#"{"code":"429000","msg":"Too Many Requests"}"#),
        (200, BUSINESS_RATE_LIMITED),
        (404, BUSINESS_RATE_LIMITED),
    ] {
        for operation in [
            "create",
            "stop_create",
            "amend_cancel",
            "amend_replace",
            "stop_cancel",
            "stop_replace",
            "stop_restore",
            "cancel",
            "cancel_stop",
            "setting",
        ] {
            let mut h = Harness::new();
            let request = match operation {
                "create" | "stop_create" => {
                    let mut o = order();
                    if operation == "stop_create" {
                        o.kind = OrderType::StopLoss;
                        o.client_order_id = None;
                        o.price = Some(90.0);
                    }
                    place(&mut h, vec![o]);
                    h.metadata(false, "CROSS");
                    h.take("/api/v1/")
                }
                "amend_cancel" | "amend_replace" | "stop_cancel" | "stop_replace"
                | "stop_restore" => {
                    if operation.starts_with("stop") {
                        resting_stop(&mut h);
                    } else {
                        h.set_order("old", OrderType::Limit, 6.0);
                    }
                    amend_stop(&mut h, "old", 120.0);
                    if operation.ends_with("replace") || operation == "stop_restore" {
                        let request = to_replacement(
                            &mut h,
                            "old",
                            if operation == "amend_replace" {
                                4.0
                            } else {
                                0.0
                            },
                        );
                        if operation == "stop_restore" {
                            h.fresh_ticker(95.0);
                            h.response(&request, 200, REFUSED);
                            h.pace();
                            h.take("/api/v1/orders")
                        } else {
                            request
                        }
                    } else {
                        h.metadata(false, "CROSS");
                        h.take("/api/v1/orders/old")
                    }
                }
                "cancel" | "cancel_stop" => {
                    h.set_order(
                        "old",
                        if operation == "cancel_stop" {
                            OrderType::StopLoss
                        } else {
                            OrderType::Limit
                        },
                        6.0,
                    );
                    h.command(Command::CancelOrders {
                        command: 1,
                        ids: vec!["old".into()],
                    });
                    h.take("/api/v1/")
                }
                _ => {
                    h.command(Command::SetLeverage {
                        command: 1,
                        symbol: "BTCUSDT".into(),
                        leverage: 10.0,
                        side: None,
                    });
                    h.metadata(false, "CROSS");
                    h.take("changeCrossUserLeverage")
                }
            };
            assert_ne!(request.method, Method::Get, "{operation}");
            h.response(&request, status, body);
            assert!(
                writes(&h).is_empty(),
                "immediate resend/put-back: {operation} {status} {body}: {:?}",
                writes(&h)
            );
            h.pace();
            let error = failure(&h);
            let case = format!("{operation} {status} {body}: {error:?}");
            assert_eq!(error.outcome, Some(Outcome::Unknown), "{case}");
            let text = error.legacy_text().to_lowercase();
            assert!(
                text.contains("unknown") && text.contains("check open orders"),
                "{case}"
            );
            assert!(
                !text.contains("refused") && !text.contains("no stop"),
                "{case}"
            );
            assert_eq!(error.protection, None, "{case}");
            assert!(writes(&h).is_empty(), "resend/put-back: {case}");
        }
    }
}

const BUSINESS_RATE_LIMITED: &str =
    r#"{"code":"200002","msg":"Too many requests in a short period of time"}"#;

#[test]
fn review_3259_futures_business_rate_create_is_unknown() {
    let mut h = Harness::new();
    place(&mut h, vec![order()]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    h.response(&request, 200, BUSINESS_RATE_LIMITED);
    h.pace();
    let error = failure(&h);
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::RateLimited));
    assert_eq!(error.venue_code.as_deref(), Some("200002"));
    assert!(error.message.contains("unknown") && error.message.contains("Check Open Orders"));
    assert!(writes(&h).is_empty());
}

#[test]
fn review_3259_futures_business_rate_cancel_never_falls_back() {
    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["a".into()],
    });
    let request = h.take("/api/v1/orders/a");
    h.response(&request, 404, BUSINESS_RATE_LIMITED);
    assert!(
        writes(&h).is_empty(),
        "rate response released another cancel: {:?}",
        writes(&h)
    );
    h.pace();
    let error = failure(&h);
    assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::RateLimited));
    assert!(error.message.contains("unknown") && error.message.contains("Check Open Orders"));
    assert!(writes(&h).is_empty());
}

#[test]
fn review_3259_cancel_job_keeps_the_unknown_legs_text() {
    for unknown_first in [false, true] {
        let mut h = Harness::new();
        h.command(Command::CancelOrders {
            command: 1,
            ids: vec!["a".into(), "b".into()],
        });
        for (index, id) in ["a", "b"].into_iter().enumerate() {
            let request = h.take(&format!("/api/v1/orders/{id}"));
            if (index == 0) == unknown_first {
                h.response(&request, 200, RATE_LIMITED);
            } else {
                h.response(
                    &request,
                    400,
                    r#"{"code":"400100","msg":"cancellation refused"}"#,
                );
            }
            h.pace();
        }
        let error = failure(&h);
        assert_eq!(error.outcome, Some(Outcome::Unknown), "{error:?}");
        assert_eq!(error.venue_code.as_deref(), Some("429000"));
        assert!(
            error.message.contains("unknown") && error.message.contains("Check Open Orders"),
            "{error:?}"
        );
        assert!(!error.message.contains("refused"), "{error:?}");
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn review_3259_futures_warning_is_normalized_before_the_terminal() {
    for operation in ["create", "amend", "cancel", "setting"] {
        let mut h = Harness::new();
        let request = match operation {
            "create" => {
                place(&mut h, vec![order()]);
                h.metadata(false, "CROSS");
                h.take("/api/v1/orders")
            }
            "amend" => {
                h.set_order("old", OrderType::Limit, 6.0);
                amend_stop(&mut h, "old", 120.0);
                to_replacement(&mut h, "old", 4.0)
            }
            "cancel" => {
                h.command(Command::CancelOrders {
                    command: 1,
                    ids: vec!["old".into()],
                });
                h.take("/api/v1/orders/old")
            }
            _ => {
                h.command(Command::SetLeverage {
                    command: 1,
                    symbol: "BTCUSDT".into(),
                    leverage: 10.0,
                    side: None,
                });
                h.metadata(false, "CROSS");
                h.take("changeCrossUserLeverage")
            }
        };
        h.actions.clear();
        h.response(
            &request,
            429,
            r#"{"code":"400005","msg":"signature refused"}"#,
        );
        h.pace();
        let warnings: Vec<_> = h
            .actions
            .iter()
            .enumerate()
            .filter_map(|(index, action)| match action {
                Action::Notice(Notice::CommandError {
                    command: 1,
                    message,
                }) => Some((index, message)),
                _ => None,
            })
            .collect();
        assert_eq!(warnings.len(), 1, "{operation}: {:?}", h.actions);
        let (warning_at, message) = warnings[0];
        assert!(
            message.contains("unknown") && message.contains("Check Open Orders"),
            "{operation}: {message}"
        );
        assert!(!message.contains("refused"), "{operation}: {message}");
        let terminal_at = h
            .actions
            .iter()
            .position(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
            .unwrap();
        assert!(warning_at < terminal_at);
        assert_eq!(message, &failure(&h).message);
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn postmerge_internal_error_does_not_grant_cancel_fallback() {
    for body in [
        r#"{"code":"500000","msg":"system error"}"#,
        r#"{"code":"future-error","msg":"unlisted error"}"#,
        r#"{"code":"200000"}"#,
        "invalid JSON",
    ] {
        let mut h = Harness::new();
        h.command(Command::CancelOrders {
            command: 1,
            ids: vec!["a".into()],
        });
        let request = h.take("/api/v1/orders/a");
        h.response(&request, 404, body);
        let extra = writes(&h);
        if !extra.is_empty() {
            let fallback = h.take("/api/v1/stopOrders/a");
            h.response(
                &fallback,
                200,
                r#"{"code":"200000","data":{"cancelledOrderIds":["a"]}}"#,
            );
        }
        h.pace();
        let terminal = h.done(1);
        assert!(
            extra.is_empty(),
            "extra physical writes: {extra:?}; terminal after successful fallback: {terminal:?}"
        );
        assert_eq!(failure(&h).outcome, Some(Outcome::Unknown));
    }
}

#[test]
fn postmerge_two_cancel_unknown_native_message_stays_actionable() {
    let mut h = Harness::new();
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec!["a".into(), "b".into()],
    });
    for (id, body) in [
        ("a", r#"{"code":"500000","msg":"system error"}"#),
        ("b", r#"{"code":"429000","msg":"rate limit"}"#),
    ] {
        let request = h.take(&format!("/api/v1/orders/{id}"));
        h.response(&request, 200, body);
        h.pace();
    }
    assert!(writes(&h).is_empty());
    let error = failure(&h);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.venue_code.as_deref(), Some("500000"));
    assert_eq!(error.venue_message.as_deref(), Some("system error"));
    assert!(
        error.message.contains("unknown") && error.message.contains("Check Open Orders"),
        "{error:?}"
    );
}
#[test]
fn postmerge_accepted_cancel_proof_warning_explains_uncertainty() {
    let mut h = Harness::new();
    h.set_order("old", OrderType::Limit, 6.0);
    amend_stop(&mut h, "old", 120.0);
    h.metadata(false, "CROSS");
    let cancel = h.take("/api/v1/orders/old");
    h.response(
        &cancel,
        200,
        r#"{"code":"200000","data":{"cancelledOrderIds":["old"]}}"#,
    );
    h.pace();
    h.actions
        .retain(|a| !matches!(a, Action::Notice(Notice::CommandError { .. })));
    for attempt in 0..3 {
        let proof = h.take("/api/v1/orders/old");
        assert_eq!(proof.method, Method::Get);
        h.response(&proof, 400, r#"{"code":"400100","msg":"proof refused"}"#);
        if attempt < 2 {
            h.advance(100.);
        }
    }
    h.pace();
    let error = failure(&h);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(writes(&h).is_empty());
    let warnings: Vec<_> = h
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandError { message, .. }) => Some(message.as_str()),
            _ => None,
        })
        .collect();
    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|text| text.contains("unknown") && text.contains("Check Open Orders")),
        "{warnings:?}"
    );
}

//! Exercise the trading state machine independently of polling and sockets.
#[allow(dead_code)]
#[path = "../src/paths.rs"]
mod paths;
#[allow(dead_code)]
#[path = "../src/sign.rs"]
mod sign;
#[path = "../src/trading.rs"]
mod trading;

use safe_cex_engine::book::Book;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
};
use sign::Credentials;
use trading::Trading;

pub enum Pending {
    Trade(trading::Request),
}
impl Pending {
    fn into_trade(self) -> Option<trading::Request> {
        let Self::Trade(request) = self;
        Some(request)
    }
}
struct TestVenue {
    trading: Trading,
    initial: Option<(Order, String, String)>,
    credentials: Credentials,
    acknowledgements: Vec<(bool, bool)>,
    admitted_entries: usize,
}
impl Venue for TestVenue {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        cx.store.set_markets(vec![market()]);
        cx.store.set_tickers(vec![Ticker {
            id: "BTC-USDT".into(),
            symbol: "BTCUSDT".into(),
            last: 100.0,
            mark: 100.0,
            ..Ticker::default()
        }]);
        if let Some((order, raw, status)) = self.initial.take() {
            self.trading
                .observe_order(&order, &raw, "BOTH", false, &status);
            cx.store.set_orders(vec![order]);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let Pending::Trade(request) = pending;
        let _ = (request.places_order(), request.changes_settings());
        self.acknowledgements.push((
            request.accepts_placement(response),
            request.reconciles_orders(),
        ));
        self.trading
            .on_response(cx, Some(&self.credentials), request, response, |_, _| {});
    }
    fn unproven_http_evidence(
        &self,
        _: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        trading::evidence(response.body, evidence);
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        let Pending::Trade(request) = pending;
        request.reconciliation()
    }
    fn on_http_owner_retired(
        &mut self,
        owner: &safe_cex_engine::request_context::HttpOwnerRetirement,
    ) {
        self.trading.retired(owner);
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        let Pending::Trade(request) = pending;
        self.trading
            .on_sign_failed(cx, Some(&self.credentials), request, error);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        let Pending::Trade(request) = pending;
        self.trading
            .on_failed(cx, Some(&self.credentials), request, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        let Pending::Trade(request) = pending;
        self.trading
            .on_sign_failed(cx, Some(&self.credentials), request, error);
    }
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if matches!(command, Command::FetchLeverage { .. }) {
            use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy, Scope};
            for _ in 0..256 {
                cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            }
            return;
        }
        let entries = &mut self.admitted_entries;
        self.trading
            .command(cx, Some(&self.credentials), command, || *entries += 1);
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
struct Harness {
    engine: Engine<TestVenue>,
    requests: Vec<HttpRequest>,
    done: Vec<Result<String, String>>,
    failures: Vec<safe_cex_engine::VenueError>,
    now: f64,
}
impl Harness {
    fn new(initial: Option<(Order, &str)>) -> Self {
        Self::with_status(initial, "NEW")
    }
    fn with_status(initial: Option<(Order, &str)>, status: &str) -> Self {
        let (local, identities) =
            safe_cex_crypto::signer::LocalSigner::from_setup(safe_cex_crypto::signer::LocalSetup {
                primary: Some(safe_cex_crypto::signer::LocalIdentity {
                    key: safe_cex_crypto::signer::Secret::new("key".into()),
                    secret: safe_cex_crypto::signer::Secret::new("secret".into()),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        let mut engine = safe_cex_engine::test_support::seeded_engine(TestVenue {
            trading: Trading::default(),
            initial: initial.map(|(o, k)| (o, k.into(), status.into())),
            acknowledgements: Vec::new(),
            admitted_entries: 0,
            credentials: Credentials {
                read: None,
                primary: identities.primary.unwrap(),
            },
        });
        engine.set_signer(Some(std::sync::Arc::new(local))).unwrap();
        engine.start(1_000.0);
        let mut h = Self {
            engine,
            requests: Vec::new(),
            done: Vec::new(),
            failures: Vec::new(),
            now: 1_000.,
        };
        h.collect();
        h
    }
    fn collect(&mut self) {
        let mut receipts = Vec::new();
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Notice(Notice::CommandProgress {
                    receipts: actual, ..
                }) => receipts.extend(actual),
                Action::Http(r) => {
                    let body = r
                        .body
                        .as_deref()
                        .and_then(|body| serde_json::from_str::<serde_json::Value>(body).ok());
                    let expected: Vec<_> = body
                        .as_ref()
                        .and_then(|body| body["clientOrderId"].as_str())
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
                        "receipt must precede the exact outgoing native identity: {}",
                        r.url
                    );
                    self.requests.push(r);
                }
                Action::Notice(Notice::CommandDone { result, .. }) => self.done.push(Ok(result)),
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    self.failures.push(error.clone());
                    self.done.push(Err(error.legacy_text()))
                }
                _ => {}
            }
        }
    }
    fn command(&mut self, command: Command) {
        self.engine.handle(self.now, Event::Command(command));
        self.collect();
    }
    fn reply(&mut self, body: &str) {
        let id = self.requests.remove(0).id;
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    fn failed(&mut self) {
        let id = self.requests.remove(0).id;
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::HttpFailed {
                request: id,
                error: "connection lost",
            },
        );
        self.collect();
    }
    fn place(&mut self, orders: Vec<PlaceOrderOpts>) {
        self.command(Command::PlaceOrders { command: 1, orders });
    }
    fn body(&self) -> &str {
        self.requests[0].body.as_deref().unwrap()
    }
}
fn market() -> Market {
    Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 10.0,
            },
            leverage: MinMax {
                min: 1.0,
                max: 100.0,
            },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn opts() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":95}"#,
    )
    .unwrap()
}
fn order() -> Order {
    Order {
        id: "9223372036854775806".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 95.0,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: false,
        close: Some(false),
        client_order_id: Some("original".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 1_000.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn position() -> Position {
    Position {
        id: "BTCUSDT:long".into(),
        symbol: "BTCUSDT".into(),
        side: PositionSide::Long,
        entry_price: 100.0,
        notional: 200.0,
        leverage: 10.0,
        margin_type: Some(MarginType::Cross),
        position_mode: Some(PositionMode::Oneway),
        unrealized_pnl: 0.0,
        realized_pnl: None,
        contracts: 2.0,
        liquidation_price: 0.0,
        margin_size: None,
        last_update: None,
    }
}

#[test]
fn bybit_execution_controls_reject_entire_batch_before_writes() {
    for bbo in [true, false] {
        let mut h = Harness::new(None);
        let mut second = opts();
        if bbo {
            second.bybit_bbo = Some(BybitBbo {
                side_type: "Queue".into(),
                level: 1.0,
            });
        } else {
            second.bybit_slippage_tolerance_ticks = Some(2.0);
        }
        h.place(vec![opts(), second]);
        assert!(h.requests.is_empty());
        assert!(h.done[0].as_ref().unwrap_err().contains("Bybit"));
    }
}

#[test]
fn every_order_is_preflighted_before_the_first_write() {
    for invalid in [
        PlaceOrderOpts {
            client_order_id: Some("UPPER".into()),
            ..opts()
        },
        PlaceOrderOpts {
            client_order_id: Some("bracket".into()),
            stop_loss: Some(90.0),
            ..opts()
        },
        PlaceOrderOpts {
            amount: f64::NAN,
            ..opts()
        },
        PlaceOrderOpts {
            client_order_id: Some("oversized".into()),
            amount: 11.0,
            ..opts()
        },
    ] {
        let mut h = Harness::new(None);
        h.place(vec![opts(), invalid]);
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
    let mut h = Harness::new(None);
    let o = PlaceOrderOpts {
        client_order_id: Some("same".into()),
        ..opts()
    };
    h.place(vec![o.clone(), o]);
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}
#[test]
fn caller_id_is_exact_and_large_ack_id_is_lossless() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        client_order_id: Some("caller-ä_001".into()),
        ..opts()
    }]);
    assert!(h.body().contains("\"clientOrderId\":\"caller-ä_001\""));
    h.reply(r#"{"code":0,"data":{"order":{"orderId":9223372036854775806,"orderID":"9223372036854775806"}}}"#);
    assert_eq!(h.done[0].as_ref().unwrap(), "[\"9223372036854775806\"]");
}
#[test]
fn placements_are_sequential_and_an_ambiguous_write_is_never_retried() {
    let mut h = Harness::new(None);
    h.place(vec![opts(), opts()]);
    assert_eq!(h.requests.len(), 1);
    h.reply(r#"{"code":0,"data":{"orderId":1}}"#);
    assert_eq!(h.requests.len(), 1);
    h.failed();
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}
#[test]
fn brackets_have_no_client_id_and_use_opposite_side() {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_hedged(true);
    h.place(vec![PlaceOrderOpts {
        stop_loss: Some(90.0),
        take_profit: Some(110.0),
        ..opts()
    }]);
    assert!(h.body().contains("\"positionSide\":\"LONG\""));
    h.reply(r#"{"code":0,"data":{"orderId":1}}"#);
    for id in [2, 3] {
        assert!(h.body().contains("\"type\":\"TRIGGER_MARKET\""));
        assert!(h.body().contains("\"side\":\"SELL\""));
        assert!(h.body().contains("\"positionSide\":\"LONG\""));
        assert!(!h.body().contains("clientOrderId"));
        assert!(!h.body().contains("reduceOnly"));
        h.reply(&format!(r#"{{"code":0,"data":{{"orderId":{id}}}}}"#));
    }
    assert_eq!(h.done[0].as_ref().unwrap(), "[\"1\",\"2\",\"3\"]");
}
#[test]
fn full_position_stop_has_reconciled_quantity_and_close_flag_without_reduce_only() {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_positions(vec![position()]);
    h.place(vec![PlaceOrderOpts {
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        amount: 0.0,
        price: Some(90.0),
        close: Some(true),
        ..opts()
    }]);
    assert!(h.body().contains("\"closePosition\":\"true\""));
    assert!(h.body().contains("\"type\":\"STOP_MARKET\""));
    assert!(h.body().contains("\"quantity\":2.0"));
    assert!(!h.body().contains("reduceOnly"));
}
#[test]
fn trailing_uses_position_size_and_fractional_callback() {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_positions(vec![position()]);
    h.place(vec![PlaceOrderOpts {
        kind: OrderType::TrailingStopLoss,
        side: OrderSide::Sell,
        price: Some(95.0),
        ..opts()
    }]);
    assert!(h.body().contains("\"symbol\":\"BTC-USDT\""));
    assert!(h.body().contains("\"priceRate\":0.05"));
    assert!(h.body().contains("\"quantity\":2.0"));
    assert!(!h.body().contains("clientOrderId"));
}
#[test]
fn quantity_amend_keeps_id_and_requires_matching_acknowledgement() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: None,
        amount: Some(2.0),
    });
    assert!(h.requests[0].url.ends_with("/trade/amend"));
    h.reply(
        r#"{"code":0,"data":{"orderId":"9223372036854775806","symbol":"BTC-USDT","quantity":"2"}}"#,
    );
    assert!(h.done[0].is_ok());
    assert_eq!(h.engine.store().orders()[0].amount, 1.0);
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: None,
        amount: Some(2.0),
    });
    h.reply(r#"{"code":0,"data":{"orderId":"wrong","symbol":"BTC-USDT","quantity":"2"}}"#);
    assert!(h.done[0].is_err());
}
#[test]
fn partial_fills_and_combined_changes_fail_before_writes() {
    let mut partial = order();
    partial.filled = 0.1;
    partial.remaining = 0.9;
    for (o, price, amount) in [
        (partial, None, Some(2.0)),
        (order(), Some(96.0), Some(2.0)),
        (order(), None, Some(1.0001)),
    ] {
        let mut h = Harness::new(Some((o, "LIMIT")));
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price,
            amount,
        });
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
}
#[test]
fn price_replace_is_atomic_and_uses_successful_new_id() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.body().contains("\"cancelClientOrderId\":\"original\""));
    assert!(
        h.body()
            .contains("\"cancelReplaceMode\":\"STOP_ON_FAILURE\"")
    );
    assert!(h.body().contains("\"cancelRestrictions\":\"ONLY_NEW\""));
    h.reply(r#"{"code":0,"data":{"cancelResult":"true","replaceResult":"true","newOrderResponse":{"orderId":9223372036854775807}}}"#);
    assert_eq!(h.done[0].as_ref().unwrap(), "[\"9223372036854775807\"]");
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Canceled
    );
    assert!(h.engine.store().order("9223372036854775807").is_none());
}
#[test]
fn cancel_continues_after_failure_and_marks_only_successes() {
    let mut second = order();
    second.id = "2".into();
    let mut h = Harness::new(None);
    h.engine.store_mut().set_orders(vec![order(), second]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec![order().id, "2".into()],
    });
    h.reply(r#"{"code":109500,"msg":"busy"}"#);
    assert_eq!(h.requests.len(), 1);
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Open
    );
    h.reply(r#"{"code":80016,"msg":"order absent"}"#);
    assert_eq!(
        h.engine.store().order("2").unwrap().status,
        OrderStatus::Open
    );
    assert!(h.done[0].is_err());
}
#[test]
fn trigger_replacement_stops_when_cancel_fails() {
    let mut trigger = order();
    trigger.kind = OrderType::StopLoss;
    trigger.trigger_type = Some(StopTriggerType::MarkPrice);
    trigger.side = OrderSide::Sell;
    trigger.price = 90.0;
    trigger.client_order_id = None;
    trigger.time_in_force = None;
    let mut h = Harness::new(Some((trigger, "STOP_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    assert_eq!(h.requests.len(), 1);
    h.failed();
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}
#[test]
fn hedged_leverage_requires_side_without_sending_a_write() {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_hedged(true);
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 20.0,
        side: None,
    });
    assert!(h.requests.is_empty());
    assert!(h.done[0].as_ref().unwrap_err().contains("Side is required"));
}

#[test]
fn side_leverage_updates_only_the_acknowledged_leg_and_oneway_uses_both() {
    for (hedged, side, native) in [
        (true, PositionSide::Long, "LONG"),
        (true, PositionSide::Short, "SHORT"),
        (false, PositionSide::Short, "BOTH"),
    ] {
        let mut h = Harness::new(None);
        h.engine.store_mut().set_hedged(hedged);
        h.engine.store_mut().update_setting("BTCUSDT", |s| {
            s.leverage = 5.0;
            s.buy_leverage = Some(5.0);
            s.sell_leverage = Some(7.0);
        });
        let mut long = position();
        long.leverage = 5.0;
        long.side = PositionSide::Long;
        let mut short = long.clone();
        short.leverage = 7.0;
        short.side = PositionSide::Short;
        h.engine.store_mut().set_positions(vec![long, short]);
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.0,
            side: Some(side),
        });
        assert!(h.body().contains(&format!("\"side\":\"{native}\"")));
        assert!(h.engine.store().metadata_write_pending("BTCUSDT"));
        h.reply(r#"{"code":0,"data":{}}"#);
        assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
        assert!(h.requests.is_empty());
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        assert_eq!(
            setting.buy_leverage,
            Some(if !hedged || side == PositionSide::Long {
                20.0
            } else {
                5.0
            })
        );
        assert_eq!(
            setting.sell_leverage,
            Some(if !hedged || side == PositionSide::Short {
                20.0
            } else {
                7.0
            })
        );
        for p in h.engine.store().positions() {
            assert_eq!(
                p.leverage,
                if !hedged || p.side == side {
                    20.0
                } else if p.side == PositionSide::Long {
                    5.0
                } else {
                    7.0
                }
            );
        }
    }
}

#[test]
fn rejected_side_leverage_preserves_both_rails() {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_hedged(true);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 5.0;
        s.buy_leverage = Some(5.0);
        s.sell_leverage = Some(7.0);
    });
    let before = h.engine.store().setting("BTCUSDT");
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 20.0,
        side: Some(PositionSide::Short),
    });
    h.failed();
    assert_eq!(h.engine.store().setting("BTCUSDT"), before);
    assert!(h.done[0].is_err());
}

#[test]
fn an_absent_trigger_is_not_confirmation_to_place_a_replacement() {
    let mut trigger = order();
    trigger.kind = OrderType::StopLoss;
    trigger.trigger_type = Some(StopTriggerType::MarkPrice);
    trigger.side = OrderSide::Sell;
    trigger.price = 90.0;
    trigger.client_order_id = None;
    trigger.time_in_force = None;
    let mut h = Harness::new(Some((trigger, "STOP_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    h.reply(r#"{"code":80016,"msg":"order absent"}"#);
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}

#[test]
fn successful_cancel_with_failed_replacement_marks_old_order_canceled() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    h.reply(r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"FAILED"}}"#);
    assert!(h.done[0].is_err());
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn cancel_replace_preserves_numeric_int64_without_a_client_id() {
    let mut o = order();
    o.client_order_id = None;
    let mut h = Harness::new(Some((o, "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.body().contains("\"cancelOrderId\":9223372036854775806"));
    assert!(!h.body().contains("cancelClientOrderId"));
}

#[test]
fn raw_trigger_kind_is_authoritative_even_if_normalized_kind_was_reclassified() {
    let mut o = order();
    o.kind = OrderType::TakeProfit;
    o.trigger_type = Some(StopTriggerType::MarkPrice);
    o.side = OrderSide::Sell;
    o.price = 90.0;
    o.time_in_force = None;
    let mut h = Harness::new(Some((o, "STOP_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    h.reply(r#"{"code":0,"data":{}}"#);
    assert!(h.body().contains("\"type\":\"STOP_MARKET\""));
    assert!(h.body().contains("\"workingType\":\"MARK_PRICE\""));
}

#[test]
fn split_quantities_conserve_the_original_amount() {
    #[derive(serde::Deserialize)]
    struct Quantity {
        quantity: f64,
    }
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 25.001,
        ..opts()
    }]);
    let mut total = 0.0;
    for id in 1..=3 {
        let q: Quantity = serde_json::from_str(h.body()).unwrap();
        assert!(q.quantity > 0.0 && q.quantity <= 10.0);
        total += q.quantity;
        h.reply(&format!(r#"{{"code":0,"data":{{"orderId":{id}}}}}"#));
    }
    assert!((total - 25.001).abs() < 1e-10);
    assert!(h.done[0].is_ok());
}
#[test]
fn every_split_entry_is_receipted_but_trigger_children_still_omit_identity() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 25.,
        stop_loss: Some(90.),
        ..opts()
    }]);
    let mut actual = std::collections::HashSet::new();
    for index in 0..6 {
        let payload: serde_json::Value = serde_json::from_str(h.body()).unwrap();
        if index < 3 {
            assert!(actual.insert(payload["clientOrderId"].as_str().unwrap().to_owned()));
        } else {
            assert!(payload.get("clientOrderId").is_none());
            assert_eq!(payload["type"], "TRIGGER_MARKET");
        }
        // collect() checks the progress-before-HTTP ordering and exact native
        // IDs for every step, including the absence of IDs on trigger wires.
        h.reply(&format!(
            r#"{{"code":0,"data":{{"orderId":"accepted-{index}"}}}}"#
        ));
    }
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_ok());
}

#[test]
fn overflowing_normalization_is_rejected_before_any_write() {
    for o in [
        PlaceOrderOpts {
            price: Some(f64::MAX),
            ..opts()
        },
        PlaceOrderOpts {
            amount: f64::MAX,
            ..opts()
        },
        PlaceOrderOpts {
            stop_loss: Some(f64::MAX),
            ..opts()
        },
    ] {
        let mut h = Harness::new(None);
        h.place(vec![opts(), o]);
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(f64::MAX),
        amount: None,
    });
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}

#[test]
fn empty_native_client_id_uses_exact_numeric_cancel_id() {
    let mut o = order();
    o.client_order_id = Some(String::new());
    let mut h = Harness::new(Some((o, "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.body().contains("\"cancelOrderId\":9223372036854775806"));
    assert!(!h.body().contains("cancelClientOrderId"));
}

#[test]
fn full_stop_quantity_comes_from_the_position_for_both_stop_families() {
    for kind in [OrderType::StopLoss, OrderType::TakeProfit] {
        let mut h = Harness::new(None);
        h.engine.store_mut().set_positions(vec![position()]);
        h.place(vec![PlaceOrderOpts {
            kind,
            side: OrderSide::Sell,
            amount: 7.0,
            close: Some(true),
            reduce_only: Some(true),
            price: Some(90.0),
            ..opts()
        }]);
        assert!(h.body().contains("\"quantity\":2.0"));
        assert!(h.body().contains("\"stopPrice\":90.0"));
        assert!(h.body().contains("\"closePosition\":\"true\""));
        assert!(!h.body().contains("reduceOnly"));
    }
}

#[test]
fn malformed_or_rejected_entry_acknowledgements_never_send_brackets() {
    for body in [
        r#"{"code":0,"data":{"order":{"orderId":"1","orderID":"2","status":"NEW"}}}"#,
        r#"{"code":0,"data":{"orderId":"1","order":{"orderId":"2","status":"NEW"}}}"#,
        r#"{"code":0,"data":{"order":{"orderId":"1","status":"REJECTED"}}}"#,
        r#"{"code":0,"data":{"order":{"orderId":"1","status":"FAILED"}}}"#,
        r#"{"code":0,"data":{"order":{"orderId":" ","status":"NEW"}}}"#,
    ] {
        let mut h = Harness::new(None);
        h.place(vec![PlaceOrderOpts {
            stop_loss: Some(90.0),
            ..opts()
        }]);
        h.reply(body);
        assert!(
            h.requests.is_empty(),
            "invalid acknowledgement sent another write: {body}"
        );
        assert!(h.done[0].is_err());
    }
}

#[test]
fn replacement_acknowledgement_preserves_streamed_partial_and_terminal_rows() {
    for status in [
        OrderStatus::Open,
        OrderStatus::Closed,
        OrderStatus::Canceled,
    ] {
        let mut h = Harness::new(Some((order(), "LIMIT")));
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(96.0),
            amount: None,
        });
        let mut streamed = order();
        streamed.id = "replacement".into();
        streamed.status = status;
        streamed.price = 96.0;
        streamed.filled = if status == OrderStatus::Closed {
            1.0
        } else {
            0.2
        };
        streamed.remaining = 1.0 - streamed.filled;
        streamed.last_update = 1_000.5;
        h.engine.store_mut().upsert_order(streamed.clone());
        h.reply(r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"replacement"}}"#);
        assert!(h.done[0].is_ok());
        assert_eq!(h.engine.store().order("replacement"), Some(&streamed));
    }
}

#[test]
fn replacement_acknowledgements_require_consistent_ids_and_accepted_status() {
    for body in [
        r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"1","newOrderResponse":{"orderId":"2","status":"NEW"}}}"#,
        r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderResponse":{"orderId":"2","orderID":"3"}}}"#,
        r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderResponse":{"orderId":"2","status":"REJECTED"}}}"#,
    ] {
        let mut h = Harness::new(Some((order(), "LIMIT")));
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(96.0),
            amount: None,
        });
        h.reply(body);
        assert!(h.done[0].is_err());
        assert_eq!(h.engine.store().orders().len(), 1);
        assert_eq!(h.engine.store().orders()[0].status, OrderStatus::Canceled);
    }
}

#[test]
fn pending_native_orders_use_the_pending_cancel_restriction() {
    let mut h = Harness::with_status(Some((order(), "LIMIT")), "PENDING");
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.body().contains("\"cancelRestrictions\":\"ONLY_PENDING\""));
    let mut h = Harness::with_status(Some((order(), "LIMIT")), "");
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}

#[test]
fn entry_and_bracket_lots_respect_the_maximum_and_conserve_protected_quantity() {
    #[derive(serde::Deserialize)]
    struct Quantity {
        quantity: f64,
    }
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 25.001,
        stop_loss: Some(90.0),
        take_profit: Some(110.0),
        ..opts()
    }]);
    for family in 0..3 {
        let mut total = 0.0;
        for lot in 0..3 {
            let q: Quantity = serde_json::from_str(h.body()).unwrap();
            assert!(q.quantity > 0.0 && q.quantity <= 10.0);
            total += q.quantity;
            if family > 0 {
                assert!(h.body().contains("\"type\":\"TRIGGER_MARKET\""));
            }
            h.reply(&format!(
                r#"{{"code":0,"data":{{"orderId":{}}}}}"#,
                family * 3 + lot + 1
            ));
        }
        assert!((total - 25.001).abs() < 1e-10);
    }
    assert!(h.done[0].is_ok());
}

#[test]
fn trigger_replacement_requires_a_consistent_cancellation_acknowledgement() {
    for data in [
        r#"{"orderId":"another-order","status":"CANCELED"}"#,
        r#"{"orderId":"9223372036854775806","status":"REJECTED"}"#,
    ] {
        let mut o = order();
        o.kind = OrderType::StopLoss;
        o.trigger_type = Some(StopTriggerType::MarkPrice);
        o.side = OrderSide::Sell;
        o.price = 90.0;
        let mut h = Harness::new(Some((o, "STOP_MARKET")));
        h.engine.store_mut().set_positions(vec![position()]);
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(91.0),
            amount: None,
        });
        h.reply(&format!(r#"{{"code":0,"data":{data}}}"#));
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
}

#[test]
fn replacement_ack_never_recreates_a_terminal_row_removed_by_a_snapshot() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    let mut terminal = order();
    terminal.id = "replacement".into();
    terminal.status = OrderStatus::Closed;
    terminal.filled = terminal.amount;
    terminal.remaining = 0.0;
    h.engine.store_mut().upsert_order(terminal);
    h.engine.store_mut().set_orders(vec![order()]);
    h.reply(r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"replacement"}}"#);
    assert!(h.done[0].is_ok());
    assert!(h.engine.store().order("replacement").is_none());
    assert_eq!(h.engine.venue().acknowledgements, [(false, true)]);
}

#[test]
fn placement_guard_rejects_contradictory_outer_identity_before_account_hook() {
    for (body, accepted) in [
        (
            r#"{"code":0,"data":{"orderId":"wrong","order":{"orderId":"accepted","symbol":"BTC-USDT","side":"BUY","type":"LIMIT","origQty":"1","price":"95","status":"NEW"}}}"#,
            false,
        ),
        (
            r#"{"code":0,"data":{"orderId":"accepted","order":{"orderId":"accepted","orderID":"accepted","status":"NEW"}}}"#,
            true,
        ),
        (
            r#"{"code":0,"data":{"order":{"orderId":"accepted","status":"REJECTED"}}}"#,
            false,
        ),
        (
            r#"{"code":109400,"data":{"order":{"orderId":"accepted","status":"NEW"}}}"#,
            false,
        ),
    ] {
        let mut h = Harness::new(None);
        h.place(vec![opts()]);
        h.reply(body);
        assert_eq!(h.engine.venue().acknowledgements, [(accepted, false)]);
        assert_eq!(h.done[0].is_ok(), accepted);
    }
}

#[test]
fn last_price_trigger_amendment_rejects_a_crossed_last_even_when_mark_is_safe() {
    let mut trigger = order();
    trigger.kind = OrderType::StopLoss;
    trigger.side = OrderSide::Sell;
    trigger.price = 90.0;
    trigger.trigger_type = Some(StopTriggerType::LastPrice);
    let mut h = Harness::new(Some((trigger, "STOP_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.engine.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 100.0;
        t.mark = 120.0;
    });
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(110.0),
        amount: None,
    });
    assert!(
        h.requests.is_empty(),
        "must not cancel the existing protective stop"
    );
    assert!(h.done[0].as_ref().unwrap_err().contains("immediately"));
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn last_price_trigger_amendment_uses_last_and_preserves_the_native_source() {
    let mut trigger = order();
    trigger.kind = OrderType::StopLoss;
    trigger.side = OrderSide::Sell;
    trigger.price = 90.0;
    trigger.trigger_type = Some(StopTriggerType::LastPrice);
    let mut h = Harness::new(Some((trigger, "STOP_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.engine.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 120.0;
        t.mark = 100.0;
    });
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(110.0),
        amount: None,
    });
    assert_eq!(h.requests[0].method, safe_cex_engine::Method::Delete);
    h.reply(r#"{"code":0,"data":{}}"#);
    assert!(h.body().contains("\"workingType\":\"CONTRACT_PRICE\""));
}

#[test]
fn trigger_amendment_requires_its_reconciled_supported_and_valid_source_before_cancellation() {
    for (source, last, mark, missing_ticker) in [
        (None, 100.0, 100.0, false),
        (Some(StopTriggerType::IndexPrice), 100.0, 100.0, false),
        (Some(StopTriggerType::LastPrice), 0.0, 100.0, false),
        (Some(StopTriggerType::LastPrice), f64::NAN, 100.0, false),
        (
            Some(StopTriggerType::MarkPrice),
            100.0,
            f64::INFINITY,
            false,
        ),
        (Some(StopTriggerType::MarkPrice), 100.0, 0.0, false),
        (Some(StopTriggerType::MarkPrice), 100.0, 100.0, true),
    ] {
        let mut trigger = order();
        trigger.kind = OrderType::StopLoss;
        trigger.side = OrderSide::Sell;
        trigger.price = 80.0;
        trigger.trigger_type = source;
        let mut h = Harness::new(Some((trigger, "STOP_MARKET")));
        h.engine.store_mut().set_positions(vec![position()]);
        h.engine.store_mut().update_ticker("BTCUSDT", |t| {
            t.last = last;
            t.mark = mark;
        });
        if missing_ticker {
            h.engine.store_mut().set_tickers(Vec::new());
        }
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(90.0),
            amount: None,
        });
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
}

#[test]
fn attached_brackets_preserve_each_explicit_source_and_the_venue_default() {
    for (sl, tp, generic, expected_sl, expected_tp) in [
        (
            Some(StopTriggerType::LastPrice),
            Some(StopTriggerType::MarkPrice),
            None,
            "CONTRACT_PRICE",
            "MARK_PRICE",
        ),
        (
            Some(StopTriggerType::MarkPrice),
            Some(StopTriggerType::LastPrice),
            None,
            "MARK_PRICE",
            "CONTRACT_PRICE",
        ),
        (
            None,
            None,
            Some(StopTriggerType::LastPrice),
            "CONTRACT_PRICE",
            "CONTRACT_PRICE",
        ),
        (None, None, None, "MARK_PRICE", "MARK_PRICE"),
    ] {
        let mut h = Harness::new(None);
        h.place(vec![PlaceOrderOpts {
            stop_loss: Some(90.0),
            take_profit: Some(110.0),
            sl_trigger_type: sl,
            tp_trigger_type: tp,
            trigger_type: generic,
            ..opts()
        }]);
        h.reply(r#"{"code":0,"data":{"orderId":1}}"#);
        assert!(
            h.body()
                .contains(&format!("\"workingType\":\"{expected_sl}\""))
        );
        h.reply(r#"{"code":0,"data":{"orderId":2}}"#);
        assert!(
            h.body()
                .contains(&format!("\"workingType\":\"{expected_tp}\""))
        );
        h.reply(r#"{"code":0,"data":{"orderId":3}}"#);
        assert!(h.done[0].is_ok());
    }
}

#[test]
fn unsupported_attached_trigger_sources_reject_the_entire_batch_before_writes() {
    for (sl, tp, generic) in [
        (Some(StopTriggerType::IndexPrice), None, None),
        (None, Some(StopTriggerType::IndexPrice), None),
        (None, None, Some(StopTriggerType::IndexPrice)),
    ] {
        let mut h = Harness::new(None);
        h.place(vec![
            opts(),
            PlaceOrderOpts {
                stop_loss: Some(90.0),
                take_profit: Some(110.0),
                sl_trigger_type: sl,
                tp_trigger_type: tp,
                trigger_type: generic,
                ..opts()
            },
        ]);
        assert!(h.requests.is_empty());
        assert!(h.done[0].is_err());
    }
}

#[test]
fn attached_trigger_market_brackets_move_by_cancel_and_replacement() {
    // A stop bracket below mark, entry not yet filled: stays TRIGGER_MARKET.
    let mut stop = order();
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = 90.0;
    stop.reduce_only = true;
    stop.client_order_id = None;
    stop.trigger_type = Some(StopTriggerType::MarkPrice);
    let mut h = Harness::new(Some((stop, "TRIGGER_MARKET")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    assert_eq!(h.requests[0].method, safe_cex_engine::Method::Delete);
    h.reply(r#"{"code":0,"data":{}}"#);
    for field in [
        "\"type\":\"TRIGGER_MARKET\"",
        "\"stopPrice\":91",
        "\"quantity\":1",
        "\"reduceOnly\":true",
        "\"side\":\"SELL\"",
        "\"workingType\":\"MARK_PRICE\"",
    ] {
        assert!(h.body().contains(field), "{field} in {}", h.body());
    }

    // A take-profit bracket above mark, with the entry filled: like TS, the
    // replacement takes the type placement picks for an open position.
    let mut take = order();
    take.kind = OrderType::TakeProfit;
    take.side = OrderSide::Sell;
    take.price = 110.0;
    take.reduce_only = true;
    take.client_order_id = None;
    take.trigger_type = Some(StopTriggerType::MarkPrice);
    let mut h = Harness::new(Some((take, "TRIGGER_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(112.0),
        amount: None,
    });
    h.reply(r#"{"code":0,"data":{}}"#);
    assert!(h.body().contains("\"type\":\"TAKE_PROFIT_MARKET\""));
    assert!(h.body().contains("\"stopPrice\":112"));
    assert!(h.body().contains("\"reduceOnly\":true"), "{}", h.body());
    h.reply(r#"{"code":0,"data":{"order":{"orderId":"5","status":"NEW"}}}"#);
    assert_eq!(h.done[0].as_deref(), Ok(r#"["5"]"#));
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    for ids in [vec!["404".to_string()], vec![order().id, "404".into()]] {
        let mut h = Harness::new(None);
        h.engine.store_mut().set_orders(vec![order()]);
        h.command(Command::CancelOrders { command: 1, ids });
        assert!(h.requests.is_empty());
        assert!(h.done[0].as_ref().unwrap_err().contains("404"));
        assert_eq!(
            h.engine.store().order(&order().id).unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn a_failed_bracket_replacement_says_the_original_was_cancelled() {
    let mut stop = order();
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = 90.0;
    stop.reduce_only = true;
    stop.client_order_id = None;
    stop.trigger_type = Some(StopTriggerType::MarkPrice);
    let mut h = Harness::new(Some((stop, "TRIGGER_MARKET")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    h.reply(r#"{"code":0,"data":{}}"#);
    h.reply(r#"{"code":101204,"msg":"insufficient margin"}"#);
    let error = h.done[0].as_ref().unwrap_err();
    assert!(
        error.starts_with("original bracket cancelled; replacement failed: BingX 101204"),
        "{error}"
    );
    assert_eq!(
        h.failures[0].outcome, None,
        "the cancel already took effect"
    );

    // A failed cancellation is not reported as a cancelled bracket.
    let mut stop = order();
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = 90.0;
    stop.client_order_id = None;
    stop.trigger_type = Some(StopTriggerType::MarkPrice);
    let mut h = Harness::new(Some((stop, "TRIGGER_MARKET")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    h.reply(r#"{"code":109500,"msg":"busy"}"#);
    assert!(!h.done[0].as_ref().unwrap_err().contains("original bracket"));
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for (body, kind, outcome) in [
        (
            r#"{"code":101204,"msg":"Insufficient margin"}"#,
            Some(ErrorKind::Rejected),
            Some(Outcome::Rejected),
        ),
        (
            r#"{"code":100410,"msg":"rate limited"}"#,
            Some(ErrorKind::RateLimited),
            Some(Outcome::Rejected),
        ),
        (
            r#"{"code":80012,"msg":"service unavailable"}"#,
            Some(ErrorKind::VenueUnavailable),
            Some(Outcome::Unknown),
        ),
        (
            r#"{"code":0,"msg":"","data":{"order":{}}}"#,
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
        (
            "not json",
            Some(ErrorKind::Internal),
            Some(Outcome::Unknown),
        ),
        (
            r#"{"code":109500,"msg":"system busy"}"#,
            None,
            Some(Outcome::Unknown),
        ),
    ] {
        let mut h = Harness::new(None);
        h.place(vec![opts()]);
        h.reply(body);
        let [error] = h.failures.as_slice() else {
            panic!("{body}: {:?}", h.failures);
        };
        assert_eq!((error.kind, error.outcome), (kind, outcome), "{body}");
        assert_eq!(h.done, [Err(error.message.clone())], "text is unchanged");
    }
    let mut h = Harness::new(None);
    h.place(vec![opts()]);
    h.failed();
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
    assert_eq!(h.failures[0].kind, Some(ErrorKind::Network));
}

fn assert_prior_placements_survive_failure(failure: u8) {
    use safe_cex_engine::{ErrorKind, Outcome};
    for acknowledged in 0..=2 {
        let mut h = Harness::new(None);
        h.place(vec![opts(), opts(), opts()]);
        let ids: Vec<_> = (0..acknowledged).map(|i| format!("accepted-{i}")).collect();
        for id in &ids {
            h.reply(&format!(r#"{{"code":0,"data":{{"orderId":"{id}"}}}}"#));
        }
        let failed_request = h.requests[0].id;
        match failure {
            0 => h.reply(r#"{"code":100400,"msg":"bad quantity"}"#),
            1 => h.failed(),
            _ => h.reply(r#"{"code":0,"data":{}}"#),
        }
        assert_eq!(h.done.len(), 1);
        assert_eq!(h.failures.len(), 1);
        assert!(
            h.requests.is_empty(),
            "failure must stop the remaining writes"
        );
        let error = &h.failures[0];
        let expected_outcome = match failure {
            0 if acknowledged == 0 => Some(Outcome::Rejected),
            0 => None,
            _ => Some(Outcome::Unknown),
        };
        assert_eq!(error.outcome, expected_outcome);
        if failure == 0 {
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
            assert_eq!(error.venue_code.as_deref(), Some("100400"));
            assert_eq!(error.venue_message.as_deref(), Some("bad quantity"));
        } else if failure == 1 {
            assert_eq!(error.kind, Some(ErrorKind::Network));
        }
        if ids.is_empty() {
            assert!(error.partial.is_none(), "no accepted ID may be invented");
        } else {
            let partial = error
                .partial
                .as_ref()
                .expect("prior native IDs must survive");
            assert_eq!(partial.order_ids, ids);
            let legacy: serde_json::Value =
                serde_json::from_str(h.done[0].as_ref().unwrap_err()).unwrap();
            assert_eq!(legacy["partialOrderIds"], serde_json::json!(ids));
        }
        // A duplicate late acknowledgement cannot turn this failure into success.
        h.engine.handle(
            1_002.0,
            Event::HttpResponse {
                request: failed_request,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":0,"data":{"orderId":"late"}}"#,
                },
            },
        );
        h.collect();
        assert_eq!(h.done.len(), 1);
        assert!(h.requests.is_empty());
    }
}

#[test]
fn partial_placement_rejection_keeps_prior_native_ids() {
    assert_prior_placements_survive_failure(0);
}

#[test]
fn partial_placement_transport_loss_keeps_prior_native_ids() {
    assert_prior_placements_survive_failure(1);
}

#[test]
fn partial_placement_malformed_ack_keeps_prior_native_ids() {
    assert_prior_placements_survive_failure(2);
}

#[test]
fn app_protective_payload_with_matching_trigger_price_places_like_price_only() {
    let app = |kind: &str, price: f64, trigger: Option<f64>| {
        let mut o: PlaceOrderOpts = serde_json::from_value(serde_json::json!({"symbol":"BTCUSDT","side":"sell","type":kind,"notional":180,"timeInForce":"GoodTillCancel","reduceOnly":true,"price":price,"amount":1})).unwrap();
        o.trigger_price = trigger;
        o
    };
    for (kind, price) in [("stop_market", 90.0), ("take_profit_market", 110.0)] {
        let mut sent = vec![];
        for trigger in [Some(price), None] {
            let mut h = Harness::new(None);
            h.engine.store_mut().set_positions(vec![position()]);
            h.place(vec![app(kind, price, trigger)]);
            assert_eq!(h.requests.len(), 1, "{kind}");
            let mut body: serde_json::Value = serde_json::from_str(h.body()).unwrap();
            body.as_object_mut().unwrap().remove("clientOrderId");
            sent.push((h.requests[0].url.clone(), body));
        }
        assert_eq!(sent[0].1["stopPrice"], price, "{kind}");
        assert_eq!(sent[0], sent[1], "{kind}");
    }
    let mut h = Harness::new(None);
    h.engine.store_mut().set_positions(vec![position()]);
    h.place(vec![app("stop_market", 90.0, Some(89.0))]);
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
}

#[test]
fn replacement_ids_prevent_a_signature_resend_even_without_cancellation_proof() {
    for data in [
        r#"{"newOrderId":"replacement"}"#,
        r#"{"newOrderResponse":{"orderId":"replacement"}}"#,
    ] {
        let mut h = Harness::new(Some((order(), "LIMIT")));
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(96.),
            amount: None,
        });
        h.reply(&format!(
            r#"{{"code":100001,"msg":"Signature verification failed","data":{data}}}"#
        ));
        assert!(
            h.requests.is_empty(),
            "replacement evidence must prevent a resend"
        );
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(
            h.failures[0].partial.as_ref().unwrap().order_ids,
            ["replacement"]
        );
        assert_eq!(
            h.engine.store().order(&order().id).unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn duplicate_entry_cannot_run_native_freshness_effects_or_replace_the_live_job() {
    let mut h = Harness::new(None);
    h.place(vec![opts()]);
    assert_eq!(h.engine.venue().admitted_entries, 1);
    let request = h.requests[0].id;
    h.place(vec![opts()]);
    assert_eq!(h.engine.venue().admitted_entries, 1);
    assert_eq!(h.requests.len(), 1);
    assert_eq!(h.requests[0].id, request);
    h.reply(r#"{"code":0,"data":{"orderId":"original"}}"#);
    assert_eq!(h.done.iter().filter(|r| r.is_ok()).count(), 1);
}

#[test]
fn rejected_entry_cannot_run_native_freshness_effects_or_start_a_write() {
    let mut h = Harness::new(None);
    h.command(Command::FetchLeverage {
        command: 99,
        symbol: "BTCUSDT".into(),
    });
    h.place(vec![opts()]);
    assert_eq!(h.engine.venue().admitted_entries, 0);
    assert!(h.requests.is_empty());
    assert_eq!(h.failures.len(), 1);
    assert!(h.failures[0].message.contains("capacity"));
}

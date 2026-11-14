//! Exercise the trading state machine independently of polling and sockets.
#[allow(dead_code)]
#[path = "../src/paths.rs"]
mod paths;
#[allow(dead_code)]
#[path = "../src/sign.rs"]
mod sign;
#[path = "../src/trading.rs"]
mod trading;
#[allow(dead_code)]
#[path = "../src/wire.rs"]
mod wire;

use safe_cex_engine::book::Book;
use safe_cex_engine::model::*;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, HttpRequest, Notice, Response, SocketId, TimerId, Venue,
};
use sign::Credentials;
use trading::Trading;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

#[expect(
    clippy::large_enum_variant,
    reason = "Match the production Trade layout; unrelated native read variants are omitted"
)]
pub enum Pending {
    Trade(trading::Request),
    ProtectiveReference(Box<trading::ReferenceRead>),
}
impl Pending {
    fn into_trade(self) -> Option<trading::Request> {
        match self {
            Self::Trade(request) => Some(request),
            _ => None,
        }
    }
}
struct TestVenue {
    trading: Trading,
    initial: Option<(Order, String, String)>,
    credentials: Credentials,
    acknowledgements: Vec<(bool, bool)>,
    admitted_entries: usize,
    original: Option<trading::protection::Original>,
}
impl Venue for TestVenue {
    type Pending = Pending;
    // As `Bingx`: its native trigger kind decides and it checks the trigger itself.
    fn judges_protective_amend(
        &self,
        _store: &safe_cex_engine::store::Store,
        _order: &Order,
    ) -> bool {
        true
    }
    fn http_release_veto(
        &self,
        store: &safe_cex_engine::Store,
        pending: &Pending,
        release: &safe_cex_engine::request_context::HttpRelease,
    ) -> Result<(), safe_cex_engine::VenueError> {
        match pending {
            Pending::Trade(request) => self.trading.protective_release(store, request, release),
            _ => Ok(()),
        }
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.trading.set_min_order_value("BTC-USDT", 2.0);
        cx.store.set_markets(vec![market()]);
        cx.store.set_tickers(vec![Ticker {
            id: "BTC-USDT".into(),
            symbol: "BTCUSDT".into(),
            last: 100.0,
            mark: 100.0,
            ..Ticker::default()
        }]);
        for source in [StopTriggerType::LastPrice, StopTriggerType::MarkPrice] {
            self.trading
                .observe_reference("BTCUSDT", source, 100.0, cx.now);
        }
        if let Some((order, raw, status)) = self.initial.take() {
            self.trading
                .observe_order(&order, &raw, "BOTH", false, &status);
            self.trading
                .observe_original(&order.id, self.original.take());
            cx.store.set_orders(vec![order]);
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, _: SocketId, text: &str) {
        let [last, mark]: [Option<f64>; 2] = serde_json::from_str(text).unwrap();
        for (source, value) in [
            (StopTriggerType::LastPrice, last),
            (StopTriggerType::MarkPrice, mark),
        ] {
            if let Some(value) = value {
                self.trading
                    .observe_reference("BTCUSDT", source, value, cx.now);
                cx.store.update_ticker("BTCUSDT", |t| match source {
                    StopTriggerType::LastPrice => t.last = value,
                    StopTriggerType::MarkPrice => t.mark = value,
                    StopTriggerType::IndexPrice => unreachable!(),
                });
            }
        }
    }
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        if let Pending::ProtectiveReference(read) = pending {
            read.item(cx, item);
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        let request = match pending {
            Pending::Trade(request) => request,
            Pending::ProtectiveReference(read) => {
                let accepted = serde_json::from_str::<serde_json::Value>(response.body)
                    .ok()
                    .filter(|v| v["code"] == 0 && response.status == 200)
                    .map(|_| ())
                    .ok_or_else(|| "refused public reference".to_string());
                self.trading
                    .reference_response(cx, Some(&self.credentials), *read, accepted);
                return;
            }
        };
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
        pending: &Pending,
        response: Response<'_>,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        let Pending::Trade(request) = pending else {
            return;
        };
        self.trading
            .unproven_evidence(request, response.body, evidence);
    }
    fn capture_http_reconciliation(
        &self,
        pending: &Pending,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        match pending {
            Pending::Trade(request) => request.reconciliation(),
            _ => Err(safe_cex_engine::request_context::Error::Invalid),
        }
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
        match pending {
            Pending::Trade(request) => {
                self.trading
                    .on_sign_failed(cx, Some(&self.credentials), request, error)
            }
            Pending::ProtectiveReference(read) => read.reject(cx, &error.message),
        }
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        match pending {
            Pending::Trade(request) => {
                self.trading
                    .on_failed(cx, Some(&self.credentials), request, error)
            }
            Pending::ProtectiveReference(read) => read.reject(cx, error),
        }
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        error: safe_cex_engine::VenueError,
    ) {
        match pending {
            Pending::Trade(request) => {
                self.trading
                    .on_sign_failed(cx, Some(&self.credentials), request, error)
            }
            Pending::ProtectiveReference(read) => read.reject(cx, &error.message),
        }
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
            .command(cx, Some(&self.credentials), command, |_| *entries += 1);
    }
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[path = "trading/after_failure.rs"]
mod after_failure;
#[path = "trading/allocation.rs"]
mod allocation;

struct Harness {
    engine: Engine<TestVenue>,
    requests: Vec<HttpRequest>,
    done: Vec<Result<String, String>>,
    failures: Vec<safe_cex_engine::VenueError>,
    receipted: std::collections::HashSet<String>,
    sent: Vec<(safe_cex_engine::Method, String, Option<String>)>,
    now: f64,
}
impl Harness {
    fn new(initial: Option<(Order, &str)>) -> Self {
        Self::with_status(initial, "NEW")
    }
    fn with_status(initial: Option<(Order, &str)>, status: &str) -> Self {
        Self::with_original(initial, status, None)
    }
    fn with_original(
        initial: Option<(Order, &str)>,
        status: &str,
        original: Option<trading::protection::Original>,
    ) -> Self {
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
            original,
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
            receipted: Default::default(),
            sent: Vec::new(),
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
                    // A signature resend repeats an identity already receipted.
                    let resent = receipts.is_empty()
                        && expected
                            .iter()
                            .all(|r| self.receipted.contains(&r.identity));
                    self.receipted
                        .extend(expected.iter().map(|r| r.identity.clone()));
                    assert!(
                        resent || receipts == expected,
                        "receipt must precede the exact outgoing native identity: {}",
                        r.url
                    );
                    receipts.clear();
                    self.sent.push((r.method, r.url.clone(), r.body.clone()));
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
    fn quote(&mut self, last: Option<f64>, mark: Option<f64>) {
        self.engine.handle(
            self.now,
            Event::WsMessage {
                socket: SocketId(1),
                text: &serde_json::to_string(&[last, mark]).unwrap(),
            },
        );
    }
    fn reply(&mut self, body: &str) {
        self.reply_status(200, body);
    }
    fn reply_status(&mut self, status: u16, body: &str) {
        let id = self.requests.remove(0).id;
        self.now += 1.;
        self.engine.handle(
            self.now,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status,
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
    assert!(h.body().contains("\"quantity\":2"));
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
    assert!(h.body().contains("\"quantity\":2"));
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

fn accept_parity_payload(h: &mut Harness, id: &str) -> serde_json::Value {
    assert_eq!(h.requests.len(), 1);
    let body = serde_json::from_str(h.body()).expect("native payload");
    h.reply(&serde_json::json!({"code":0,"data":{"orderId":id}}).to_string());
    body
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564;1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | perp-max-qty-splitting trading.rs:1425
fn legacy_parity_below_notional_tail_can_follow_accepted_batch_writes() {
    let mut h = Harness::new(None);
    h.place(vec![
        opts(),
        PlaceOrderOpts {
            amount: 11.,
            price: Some(1.),
            ..opts()
        },
    ]);
    accept_parity_payload(&mut h, "prefix");
    assert_eq!(accept_parity_payload(&mut h, "first_lot")["quantity"], 5.);
    assert_eq!(accept_parity_payload(&mut h, "second_lot")["quantity"], 5.);
    let tail: serde_json::Value = serde_json::from_str(h.body()).expect("tail payload");
    assert_eq!(tail["quantity"], 1.);
    assert_eq!(tail["price"], 1.);
    h.reply(r#"{"code":109400,"msg":"minimum notional"}"#);
    assert!(h.requests.is_empty());
    assert_eq!(h.failures[0].outcome, None);
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("accepted prefix")
            .order_ids,
        ["prefix", "first_lot", "second_lot"]
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1485-1542;1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | missing/nonpositive notional reference trading.rs:1387
fn legacy_parity_market_batch_does_not_require_a_positive_last_reference() {
    for last in [None, Some(0.), Some(-1.)] {
        let mut h = Harness::new(None);
        if let Some(last) = last {
            h.quote(Some(last), None);
        } else {
            h.engine.store_mut().set_tickers(vec![]);
        }
        h.place(vec![
            opts(),
            PlaceOrderOpts {
                amount: 11.,
                kind: OrderType::Market,
                price: Some(100.),
                ..opts()
            },
        ]);
        accept_parity_payload(&mut h, "prefix");
        for (id, quantity) in [("first_lot", 5.), ("second_lot", 5.), ("tail", 1.)] {
            let body = accept_parity_payload(&mut h, id);
            assert_eq!(body["type"], "MARKET");
            assert_eq!(body["quantity"], quantity);
            assert!(body.get("price").is_none());
        }
        assert!(h.requests.is_empty());
        assert!(h.failures.is_empty());
        assert!(h.done[0].is_ok());
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1485-1487;1544-1564
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | exact-quantity-conservation and explicit-dust-close minimum policy trading.rs:1387/1425
fn legacy_parity_quantity_normalization_does_not_refuse_raw_off_step_intent() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 1.0004,
        ..opts()
    }]);
    assert_eq!(accept_parity_payload(&mut h, "normalized")["quantity"], 1.);
    assert!(h.requests.is_empty());
    assert!(h.failures.is_empty());
    assert!(h.done[0].is_ok());
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1125-1157
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | ordinary-quantity-amend trading.rs:1540
fn legacy_parity_quantity_amend_has_quantity_bounds_without_notional_gate() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: None,
        amount: Some(0.01),
    });
    assert_eq!(h.requests.len(), 1);
    assert!(h.requests[0].url.ends_with("/trade/amend"));
    let body: serde_json::Value = serde_json::from_str(h.body()).expect("amend payload");
    assert_eq!(body["quantity"], 0.01);
    h.reply(
        &serde_json::json!({"code":0,"data":{
            "orderId":order().id,"symbol":"BTC-USDT","quantity":"0.01"
        }})
        .to_string(),
    );
    assert!(h.requests.is_empty());
    assert!(h.failures.is_empty());
    assert!(h.done[0].is_ok());
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1331-1343;1360-1440
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | ordinary-atomic-cancel-replace trading.rs:1540
fn legacy_parity_atomic_replace_does_not_add_new_quantity_or_notional_checks() {
    for maximum in [10., 0.5] {
        let mut h = Harness::new(Some((order(), "LIMIT")));
        let mut native = market();
        native.limits.amount.max = maximum;
        h.engine.store_mut().set_markets(vec![native]);
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(1.04),
            amount: None,
        });
        assert_eq!(h.requests.len(), 1);
        assert!(h.requests[0].url.ends_with("/trade/cancelReplace"));
        let body: serde_json::Value = serde_json::from_str(h.body()).expect("replacement payload");
        assert_eq!(body["quantity"], 1.);
        assert_eq!(body["price"], 1.);
        assert_eq!(body["cancelReplaceMode"], "STOP_ON_FAILURE");
        h.reply(r#"{"code":0,"data":{"cancelResult":"true","replaceResult":"true","newOrderResponse":{"orderId":9223372036854775807}}}"#);
        assert!(h.requests.is_empty());
        assert!(h.failures.is_empty());
        assert!(h.done[0].is_ok());
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1567-1595;1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | attached-TP-SL children trading.rs:1450
fn legacy_parity_bracket_notional_is_left_to_the_venue_after_entry_placement() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        stop_loss: Some(0.1),
        ..opts()
    }]);
    accept_parity_payload(&mut h, "entry");
    let body: serde_json::Value = serde_json::from_str(h.body()).expect("stop payload");
    assert_eq!(body["quantity"], 1.);
    assert_eq!(body["stopPrice"], 0.1);
    assert_eq!(body["reduceOnly"], true);
    h.reply(r#"{"code":109400,"msg":"minimum notional"}"#);
    assert!(h.requests.is_empty());
    assert!(h.done[0].is_err());
    assert_eq!(
        h.failures[0]
            .partial
            .as_ref()
            .expect("entry placed")
            .order_ids,
        ["entry"]
    );
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1447-1452;1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | perp-serial-batch trading.rs:1233
fn legacy_parity_price_normalization_does_not_add_a_positive_tick_gate() {
    for tick in [0., f64::NAN] {
        let mut h = Harness::new(None);
        let mut native = market();
        native.precision.price = tick;
        h.engine.store_mut().set_markets(vec![native]);
        h.place(vec![opts(), opts()]);
        for id in ["first", "second"] {
            assert_eq!(accept_parity_payload(&mut h, id)["price"], 95.);
        }
        assert!(h.requests.is_empty());
        assert!(h.failures.is_empty());
        assert!(h.done[0].is_ok());
    }
}

#[test]
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1451-1452;1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | combined-native-write-count trading.rs:1233
fn legacy_parity_serial_batch_has_no_new_combined_native_write_limit() {
    let mut h = Harness::new(None);
    h.place(vec![
        PlaceOrderOpts {
            amount: 50_000.,
            ..opts()
        },
        PlaceOrderOpts {
            amount: 50_010.,
            ..opts()
        },
    ]);
    assert_eq!(h.requests.len(), 1);
    assert!(h.failures.is_empty());
    h.reply(r#"{"code":109400,"msg":"scripted venue refusal"}"#);
    assert_eq!(h.requests.len(), 1, "legacy continues after refusal");
    h.failed();
    assert!(h.requests.is_empty());
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
    assert_eq!(h.failures[0].elements.as_ref().unwrap().len(), 10_001);
}

fn writeplan_split() -> Harness {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 21.,
        price: Some(2.),
        ..opts()
    }]);
    accept_parity_payload(&mut h, "accepted-first-lot");
    h
}

fn assert_writeplan_middle_failure(h: &Harness, unknown: bool) {
    use safe_cex_engine::write_plan::ElementResult;
    assert!(
        h.requests.is_empty(),
        "failed lot replayed or suffix dispatched"
    );
    assert_eq!(h.done.len(), 1);
    let elements = h.failures[0].elements.as_ref().expect("per-lot outcomes");
    assert_eq!(elements.len(), 4);
    for (index, element) in elements.iter().enumerate() {
        assert_eq!(element.element.index, index);
        assert!(element.element.client_order_id.is_some());
    }
    assert!(matches!(&elements[0].result,
        ElementResult::Placed { order_ids } if order_ids == &["accepted-first-lot"]));
    if unknown {
        assert!(h.failures[0].message.contains(ENGINE_UNKNOWN_TEXT));
        assert!(matches!(&elements[1].result,
            ElementResult::Unknown { error } if error.outcome == Some(safe_cex_engine::Outcome::Unknown)));
    } else {
        assert!(matches!(&elements[1].result,
            ElementResult::NotSent { error: Some(error) }
                if error.outcome == Some(safe_cex_engine::Outcome::Rejected)));
    }
    for element in &elements[2..] {
        assert!(matches!(
            &element.result,
            ElementResult::NotSent { error: None }
        ));
    }
}

#[test]
fn writeplan_rejected_middle_lot_reports_every_element() {
    let mut h = writeplan_split();
    h.reply(r#"{"code":100400,"msg":"bad quantity"}"#);
    assert_eq!(
        accept_parity_payload(&mut h, "later-lot")["quantity"],
        6.667
    );
    assert_eq!(accept_parity_payload(&mut h, "last-lot")["quantity"], 1.);
    let rows = h.failures[0].elements.as_ref().unwrap();
    assert!(
        matches!(&rows[0].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["accepted-first-lot"])
    );
    assert!(
        matches!(&rows[1].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(e) } if e.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert!(
        matches!(&rows[2].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["later-lot"])
    );
    assert!(
        matches!(&rows[3].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["last-lot"])
    );
}

#[test]
fn writeplan_lost_middle_lot_reports_unknown_without_a_replay() {
    let mut h = writeplan_split();
    h.failed();
    assert_writeplan_middle_failure(&h, true);
}

#[test]
fn writeplan_gateway_middle_lot_keeps_prefix_and_unsent_suffix() {
    let mut h = writeplan_split();
    h.reply_status(
        500,
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
    );
    assert_writeplan_middle_failure(&h, true);
}

#[test]
fn writeplan_signature_refusal_after_accepted_prefix_never_resends() {
    let mut h = writeplan_split();
    h.reply(r#"{"code":100001,"msg":"Signature verification failed"}"#);
    assert_eq!(
        accept_parity_payload(&mut h, "later-lot")["quantity"],
        6.667
    );
    assert_eq!(accept_parity_payload(&mut h, "last-lot")["quantity"], 1.);
    let rows = h.failures[0].elements.as_ref().unwrap();
    assert!(
        matches!(&rows[1].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(e) } if e.venue_code.as_deref() == Some("100001"))
    );
    assert!(
        matches!(&rows[2].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["later-lot"])
    );
    assert!(
        matches!(&rows[3].result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &["last-lot"])
    );
}

#[test]
fn writeplan_all_valid_control_places_every_lot_once() {
    let mut h = writeplan_split();
    assert_eq!(
        accept_parity_payload(&mut h, "accepted-second-lot")["quantity"],
        6.667
    );
    assert_eq!(
        accept_parity_payload(&mut h, "accepted-third-lot")["quantity"],
        6.667
    );
    assert_eq!(
        accept_parity_payload(&mut h, "accepted-last-lot")["quantity"],
        1.
    );
    assert!(h.requests.is_empty());
    assert!(h.failures.is_empty());
    let ids: Vec<String> =
        serde_json::from_str(h.done[0].as_ref().expect("all lots placed")).expect("native IDs");
    assert_eq!(
        ids,
        [
            "accepted-first-lot",
            "accepted-second-lot",
            "accepted-third-lot",
            "accepted-last-lot"
        ]
    );
}

#[test]
fn writeplan_invalid_final_row_retains_legacy_zero_write_refusal() {
    let mut h = Harness::new(None);
    h.place(vec![
        opts(),
        PlaceOrderOpts {
            price: Some(0.),
            ..opts()
        },
    ]);
    for _ in 0..3 {
        h.collect();
    }
    assert!(h.requests.is_empty());
    assert_eq!(
        h.failures[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
}

#[test]
fn writeplan_independent_cancels_keep_native_continuation_and_applied_targets() {
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    let mut second = order();
    second.id = "second".into();
    let mut h = Harness::new(None);
    h.engine.store_mut().set_orders(vec![order(), second]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec![order().id, "second".into()],
    });
    h.reply(r#"{"code":100400,"msg":"scripted refusal"}"#);
    assert_eq!(h.requests.len(), 1);
    h.reply(r#"{"code":0,"data":{"orderId":"second"}}"#);
    assert!(h.requests.is_empty());
    let elements = h.failures[0]
        .elements
        .as_ref()
        .expect("both cancel outcomes");
    assert_eq!(elements.len(), 2);
    assert_eq!(elements[1].element.index, 1);
    assert!(
        matches!(&elements[0].result, ElementResult::NotSent { error: Some(error) }
        if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert!(matches!(&elements[1].result, ElementResult::Applied {
        effect: AppliedEffect::Cancel, target_order_id: Some(id), ..
    } if id == "second"));
    assert!(
        h.failures[0]
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
}

#[test]
fn writeplan_independent_cancel_keeps_its_initial_proven_refusal_resend() {
    let mut second = order();
    second.id = "second".into();
    let mut h = Harness::new(None);
    h.engine.store_mut().set_orders(vec![order(), second]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec![order().id, "second".into()],
    });
    h.reply(r#"{"code":100400,"msg":"first cancel refused"}"#);
    h.reply(r#"{"code":100001,"msg":"Signature verification failed"}"#);
    assert_eq!(
        h.requests.len(),
        1,
        "independent cancel retains its native attempt budget"
    );
    assert_eq!(h.requests[0].method, safe_cex_engine::Method::Delete);
    h.reply(r#"{"code":0,"data":{"orderId":"second"}}"#);
    assert!(h.requests.is_empty());
    let elements = h.failures[0]
        .elements
        .as_ref()
        .expect("both independent cancels");
    assert_eq!(elements.len(), 2);
    assert!(matches!(&elements[1].result,
        safe_cex_engine::write_plan::ElementResult::Applied { target_order_id: Some(id), .. }
        if id == "second"));
}

#[test]
fn writeplan_settings_prefix_is_applied_without_a_fake_created_order_id() {
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    let mut h = shared_hedged_leverage();
    h.reply(r#"{"code":0,"data":{}}"#);
    h.reply(r#"{"code":100400,"msg":"scripted short-side refusal"}"#);
    assert!(h.requests.is_empty());
    let elements = h.failures[0]
        .elements
        .as_ref()
        .expect("both setting outcomes");
    assert_eq!(elements.len(), 2);
    assert!(matches!(&elements[0].result, ElementResult::Applied {
        effect: AppliedEffect::Settings, target_order_id: None, position_key: Some(key)
    } if key.starts_with("BTCUSDT:")));
    assert!(
        matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) }
        if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert!(
        h.failures[0]
            .partial
            .as_ref()
            .is_none_or(|partial| partial.order_ids.is_empty())
    );
}

#[test]
fn writeplan_local_release_veto_settles_pending_cancel_replace_not_sent() {
    use safe_cex_engine::write_plan::ElementResult;
    let mut h = guarded_stop(true);
    h.engine.handle(
        h.now,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(91.),
            amount: None,
        }),
    );
    h.now += 6_000.;
    h.collect();
    assert!(h.requests.is_empty());
    let elements = h.failures[0]
        .elements
        .as_ref()
        .expect("both native effects were locally vetoed");
    assert_eq!(elements.len(), 2);
    for element in elements.iter() {
        assert!(
            matches!(&element.result, ElementResult::NotSent { error: Some(error) }
            if error.outcome == Some(safe_cex_engine::Outcome::NotSent))
        );
    }
}

#[test]
fn writeplan_held_restore_journals_the_cancel_refusal_and_restore_attempt() {
    use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
    for gateway in [false, true] {
        let mut h = guarded_stop(true);
        edit_stop(&mut h);
        half_refusal(&mut h);
        assert_eq!(h.requests.len(), 1, "class-A restore is held unchanged");
        if gateway {
            h.reply_status(503, r#"{"msg":"upstream unavailable"}"#);
        } else {
            h.reply(r#"{"code":0,"data":{"orderId":"restored","status":"NEW"}}"#);
        }
        assert!(h.requests.is_empty());
        let elements = h.failures[0].elements.as_ref().unwrap_or_else(|| {
            panic!(
                "all held-restore effects, gateway={gateway}: {:?}",
                h.failures
            )
        });
        assert_eq!(elements.len(), 3);
        assert_eq!(
            elements
                .iter()
                .map(|row| row.element.index)
                .collect::<Vec<_>>(),
            [0, 1, 2]
        );
        assert!(matches!(&elements[0].result, ElementResult::Applied {
            effect: AppliedEffect::Cancel, target_order_id: Some(id), ..
        } if id == &order().id));
        assert!(
            matches!(&elements[1].result, ElementResult::NotSent { error: Some(error) }
            if error.outcome == Some(safe_cex_engine::Outcome::Rejected))
        );
        if gateway {
            assert!(h.failures[0].message.contains(ENGINE_UNKNOWN_TEXT));
            assert!(
                matches!(&elements[2].result, ElementResult::Unknown { error }
                if error.outcome == Some(safe_cex_engine::Outcome::Unknown))
            );
        } else {
            assert!(
                matches!(&elements[2].result, ElementResult::Placed { order_ids }
                if order_ids == &["restored"])
            );
            assert_eq!(
                h.failures[0].protection,
                Some(safe_cex_engine::error::Protection::Restored)
            );
        }
    }
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
    h.reply(r#"{"code":100400,"msg":"definite refusal"}"#);
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
fn shared_hedged_leverage() -> Harness {
    let mut h = Harness::new(None);
    h.engine.store_mut().set_hedged(true);
    h.engine.store_mut().update_setting("BTCUSDT", |s| {
        s.leverage = 5.0;
        s.buy_leverage = Some(5.0);
        s.sell_leverage = Some(7.0);
    });
    h.command(Command::SetLeverage {
        command: 1,
        symbol: "BTCUSDT".into(),
        leverage: 20.0,
        side: None,
    });
    h
}

#[test]
fn shared_hedged_leverage_updates_long_then_short_without_an_open_position() {
    // The shared slider intentionally supplies no side, unlike the legacy refusal.
    let mut h = shared_hedged_leverage();
    assert!(h.engine.store().positions().is_empty());
    assert_eq!(h.requests.len(), 1);
    assert!(
        h.requests[0]
            .url
            .contains("/openApi/swap/v2/trade/leverage")
    );
    assert!(h.body().contains(r#""side":"LONG""#));
    assert!(h.body().contains(r#""leverage":20"#));
    assert!(h.done.is_empty());
    assert_eq!(
        h.engine.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(5.0)
    );
    h.reply(r#"{"code":0,"data":{}}"#);
    assert_eq!(h.requests.len(), 1);
    assert!(h.body().contains(r#""side":"SHORT""#));
    assert!(h.body().contains(r#""leverage":20"#));
    assert!(
        h.done.is_empty(),
        "LONG success alone cannot complete the command"
    );
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(20.0));
    assert_eq!(setting.sell_leverage, Some(7.0));
    assert!(h.engine.store().metadata_write_pending("BTCUSDT"));
    h.reply(r#"{"code":0,"data":{}}"#);
    assert!(h.requests.is_empty());
    assert_eq!(h.done.len(), 1);
    assert!(h.done[0].is_ok());
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(20.0));
    assert_eq!(setting.sell_leverage, Some(20.0));
    assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
}

#[test]
fn shared_hedged_leverage_long_refusal_never_sends_short() {
    let mut h = shared_hedged_leverage();
    assert_eq!(h.requests.len(), 1);
    h.reply(r#"{"code":80014,"msg":"leverage refused"}"#);
    assert!(h.requests.is_empty());
    assert_eq!(h.done.len(), 1);
    assert!(h.done[0].is_err());
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(5.0));
    assert_eq!(setting.sell_leverage, Some(7.0));
    assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
}

#[test]
fn shared_hedged_leverage_short_failure_reports_long_success_without_rollback() {
    for failure in [
        Some(r#"{"code":80014,"msg":"SHORT refused"}"#),
        Some("malformed"),
        None,
    ] {
        let mut h = shared_hedged_leverage();
        assert_eq!(h.requests.len(), 1);
        h.reply(r#"{"code":0,"data":{}}"#);
        assert_eq!(h.requests.len(), 1);
        match failure {
            Some(body) => h.reply(body),
            None => h.failed(),
        }
        assert!(h.requests.is_empty(), "no rollback or extra settings write");
        assert_eq!(h.done.len(), 1);
        let message = h.done[0].as_ref().unwrap_err();
        assert!(message.contains("LONG leverage was updated"), "{message}");
        assert!(
            message.contains("SHORT leverage update was not confirmed"),
            "{message}"
        );
        assert_eq!(h.failures.len(), 1);
        let error = &h.failures[0];
        assert!(error.message.contains("LONG leverage was updated"));
        assert!(
            error
                .message
                .contains("SHORT leverage update was not confirmed")
        );
        match failure {
            Some(body) if body != "malformed" => {
                assert_eq!(error.venue_message.as_deref(), Some("SHORT refused"))
            }
            Some(_) => assert!(message.contains("outcome is unknown")),
            None => {
                assert!(message.contains("outcome is unknown"));
                assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Network));
            }
        }
        let setting = h.engine.store().setting("BTCUSDT").unwrap();
        assert_eq!(setting.buy_leverage, Some(20.0));
        assert_eq!(setting.sell_leverage, Some(7.0));
        assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
    }
}

#[test]
fn shared_hedged_leverage_rejects_invalid_input_before_either_write() {
    for leverage in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut h = Harness::new(None);
        h.engine.store_mut().set_hedged(true);
        h.command(Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage,
            side: None,
        });
        assert!(h.requests.is_empty());
        assert_eq!(h.done.len(), 1);
        assert!(h.done[0].is_err());
        assert!(!h.engine.store().metadata_write_pending("BTCUSDT"));
    }
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
    cancel_stop(&mut h);
    assert!(h.body().contains("\"type\":\"STOP_MARKET\""));
    assert!(h.body().contains("\"workingType\":\"MARK_PRICE\""));
}

#[test]
fn split_quantities_match_the_independently_rounded_ts_lots() {
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
    for (id, expected) in [6.667, 6.667, 6.667, 5.001].into_iter().enumerate() {
        let q: Quantity = serde_json::from_str(h.body()).unwrap();
        assert_eq!(q.quantity, expected);
        total += q.quantity;
        h.reply(&format!(r#"{{"code":0,"data":{{"orderId":{id}}}}}"#));
    }
    assert!((total - 25.002).abs() < 1e-10);
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
    for (index, quantity) in [6.667, 6.667, 6.667, 5., 25.].into_iter().enumerate() {
        let payload: serde_json::Value = serde_json::from_str(h.body()).unwrap();
        assert_eq!(payload["quantity"], quantity);
        if index < 4 {
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
    assert_eq!(actual.len(), 4);
    assert_eq!(h.sent.len(), 5);
    assert!(h.requests.is_empty());
    let ids: Vec<String> = serde_json::from_str(h.done[0].as_ref().unwrap()).unwrap();
    assert_eq!(
        ids,
        (0..5)
            .map(|index| format!("accepted-{index}"))
            .collect::<Vec<_>>()
    );
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
        assert!(h.body().contains("\"quantity\":2"));
        assert!(h.body().contains("\"stopPrice\":90"));
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
fn entry_and_bracket_lots_share_the_exact_ts_allocation() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        amount: 25.001,
        stop_loss: Some(90.0),
        take_profit: Some(110.0),
        ..opts()
    }]);
    // TS formatCreateOrder splits only the entry; each attached child uses amount.
    let mut total = 0.;
    for (lot, expected) in [6.667, 6.667, 6.667, 5.001].into_iter().enumerate() {
        let body = accept_parity_payload(&mut h, &format!("entry-{lot}"));
        assert_eq!(body["type"], "LIMIT");
        assert_eq!(body["quantity"], expected);
        assert!(body["clientOrderId"].is_string());
        total += body["quantity"].as_f64().unwrap();
    }
    assert!((total - 25.002_f64).abs() < 1e-10);
    for (name, trigger) in [("stop", 90.), ("take-profit", 110.)] {
        let body = accept_parity_payload(&mut h, name);
        assert_eq!(body["type"], "TRIGGER_MARKET");
        assert_eq!(body["quantity"], 25.001);
        assert_eq!(body["stopPrice"], trigger);
        assert_eq!(body["side"], "SELL");
        assert_eq!(body["reduceOnly"], true);
        assert_eq!(body["timeInForce"], "GTC");
        assert!(body.get("clientOrderId").is_none());
    }
    assert_eq!(h.sent.len(), 6);
    assert!(h.requests.is_empty());
    assert!(h.failures.is_empty());
    assert_eq!(h.done.len(), 1);
    let ids: Vec<String> = serde_json::from_str(h.done[0].as_ref().unwrap()).unwrap();
    assert_eq!(
        ids,
        [
            "entry-0",
            "entry-1",
            "entry-2",
            "entry-3",
            "stop",
            "take-profit"
        ]
    );
}

#[test]
fn bracket_failure_text_distinguishes_refused_partial_and_accepted_entry() {
    use safe_cex_engine::write_plan::ElementResult;
    for (amount, accepted_entries, entry_count) in [(1., 0, 1), (1., 1, 1), (11., 1, 3)] {
        let mut h = Harness::new(None);
        h.place(vec![PlaceOrderOpts {
            amount,
            stop_loss: Some(90.),
            take_profit: Some(110.),
            ..opts()
        }]);
        for index in 0..entry_count {
            if index < accepted_entries {
                accept_parity_payload(&mut h, &format!("entry-{index}"));
            } else {
                h.reply(r#"{"code":100400,"msg":"entry refused"}"#);
            }
        }
        accept_parity_payload(&mut h, "stop-only");
        h.failed();
        assert!(h.requests.is_empty());
        assert_eq!(h.failures.len(), 1);
        let error = &h.failures[0];
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let rows = error.elements.as_ref().unwrap();
        assert_eq!(rows.len(), entry_count + 2);
        for (index, row) in rows.iter().take(entry_count).enumerate() {
            if index < accepted_entries {
                assert!(matches!(&row.result, ElementResult::Placed { order_ids }
                    if order_ids == &[format!("entry-{index}")]));
            } else {
                assert!(
                    matches!(&row.result, ElementResult::NotSent { error: Some(e) }
                    if e.outcome == Some(safe_cex_engine::Outcome::Rejected))
                );
            }
        }
        assert!(
            matches!(&rows[entry_count].result, ElementResult::Placed { order_ids }
            if order_ids == &["stop-only"])
        );
        assert!(matches!(
            &rows[entry_count + 1].result,
            ElementResult::Unknown { .. }
        ));
        assert!(error.message.contains("unknown"), "{}", error.message);
        if accepted_entries == 0 {
            assert!(!error.message.contains("entry was"), "{}", error.message);
        } else if accepted_entries < entry_count {
            assert!(
                error.message.contains("entry was partially placed"),
                "{}",
                error.message
            );
        } else {
            assert!(
                error.message.contains("entry was placed"),
                "{}",
                error.message
            );
        }
    }
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
    h.quote(Some(100.0), Some(120.0));
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
    assert!(
        h.done[0].as_ref().unwrap_err().contains("immediately"),
        "{:?}",
        h.done
    );
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
    h.quote(Some(120.0), Some(100.0));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(110.0),
        amount: None,
    });
    assert_eq!(h.requests[0].method, safe_cex_engine::Method::Delete);
    cancel_stop(&mut h);
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
        if matches!(
            source,
            Some(StopTriggerType::LastPrice | StopTriggerType::MarkPrice)
        ) {
            assert_eq!(h.requests.len(), 1);
            assert_eq!(h.requests[0].method, safe_cex_engine::Method::Get);
            h.reply(r#"{"code":0,"data":[]}"#);
        }
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
    assert!(h.requests.is_empty());
    assert!(
        h.done[0]
            .as_ref()
            .unwrap_err()
            .contains("protected position")
    );

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
    cancel_stop(&mut h);
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
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    cancel_stop(&mut h);
    h.reply(r#"{"code":101204,"msg":"insufficient margin"}"#);
    let error = h.done[0].as_ref().unwrap_err();
    assert!(
        error.contains("position is unprotected") && error.contains("BingX 101204"),
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
    h.engine.store_mut().set_positions(vec![position()]);
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
            Some(Outcome::Unknown),
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
        let mut ids: Vec<_> = (0..acknowledged).map(|i| format!("accepted-{i}")).collect();
        for id in &ids {
            h.reply(&format!(r#"{{"code":0,"data":{{"orderId":"{id}"}}}}"#));
        }
        let failed_request = h.requests[0].id;
        match failure {
            0 => h.reply(r#"{"code":100400,"msg":"bad quantity"}"#),
            1 => h.failed(),
            _ => h.reply(r#"{"code":0,"data":{}}"#),
        }
        if failure == 0 {
            for index in acknowledged + 1..3 {
                let id = format!("later-{index}");
                accept_parity_payload(&mut h, &id);
                ids.push(id);
            }
        }
        assert_eq!(h.done.len(), 1);
        assert_eq!(h.failures.len(), 1);
        assert!(h.requests.is_empty());
        let error = &h.failures[0];
        let expected_outcome = match failure {
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

fn split_order(amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSDT", "type": "market", "side": "buy", "amount": amount
    }))
    .unwrap()
}

#[test]
fn native_maximum_split_streams_every_lot_and_returns_every_accepted_id() {
    // 10,000 lots of the market's 10.0 maximum: native `split()`'s exact bound.
    let mut h = Harness::new(None);
    h.place(vec![split_order(100_000.)]);
    for lot in 0..10_000 {
        assert_eq!(
            h.requests.len(),
            1,
            "lot {lot}: one native write at a time: {:?}",
            h.failures
        );
        let body: serde_json::Value = serde_json::from_str(h.body()).unwrap();
        assert_eq!(body["quantity"].as_f64(), Some(10.0), "lot {lot}");
        assert!(
            h.done.is_empty(),
            "lot {lot}: the command is still streaming"
        );
        h.reply(&format!(r#"{{"code":0,"data":{{"orderId":"{lot}"}}}}"#));
    }
    assert!(h.requests.is_empty());
    let ids: Vec<String> = serde_json::from_str(h.done[0].as_ref().unwrap()).unwrap();
    assert_eq!(
        ids,
        (0..10_000).map(|id| id.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn mid_split_signature_refusal_keeps_accepted_ids_without_resending() {
    use safe_cex_engine::ErrorKind;
    let mut h = Harness::new(None);
    h.place(vec![split_order(30.)]);
    h.reply(r#"{"code":0,"data":{"orderId":"lot-0"}}"#);
    h.reply(r#"{"code":100001,"msg":"Signature verification failed"}"#);
    assert_eq!(accept_parity_payload(&mut h, "lot-2")["quantity"], 10.);
    assert!(h.requests.is_empty());
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures);
    };
    assert_eq!(
        (error.kind, error.outcome),
        (Some(ErrorKind::AuthInvalid), None)
    );
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["lot-0", "lot-2"]
    );
}

/// cancelReplace runs STOP_ON_FAILURE, so a refusal may follow the cancel: native
/// leaves its outcome unset, and no refusal on this path grants a resend.
fn assert_refused_replace_has_no_outcome(body: &str, kind: Option<safe_cex_engine::ErrorKind>) {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.),
        amount: None,
    });
    h.reply(body);
    assert!(
        h.requests.is_empty(),
        "{body}: no resend and no further call"
    );
    let [error] = h.failures.as_slice() else {
        panic!("{body}: {:?}", h.failures);
    };
    assert_eq!((error.kind, error.outcome), (kind, None), "{body}");
    assert!(
        error
            .partial
            .as_ref()
            .is_none_or(|p| p.order_ids.is_empty()),
        "{body}: no accepted ID may be invented"
    );
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Open,
        "{body}: the original stays until a read proves otherwise"
    );
}

#[test]
fn refused_cancel_replace_has_no_outcome() {
    assert_refused_replace_has_no_outcome(
        r#"{"code":100400,"msg":"invalid parameter"}"#,
        Some(safe_cex_engine::ErrorKind::InvalidInput),
    );
}

#[test]
fn signature_refused_cancel_replace_has_no_outcome_and_no_resend() {
    assert_refused_replace_has_no_outcome(
        r#"{"code":100001,"msg":"Signature verification failed"}"#,
        Some(safe_cex_engine::ErrorKind::AuthInvalid),
    );
}

fn cheap_limit(reduce_only: bool) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({
        "symbol": "BTCUSDT", "type": "limit", "side": "sell", "amount": 1.0, "price": 1.0, "reduceOnly": reduce_only
    }))
    .unwrap()
}

#[test]
fn an_order_below_the_venue_minimum_value_is_refused_here_but_a_close_is_not() {
    let mut h = Harness::new(None);
    h.place(vec![cheap_limit(false)]);
    assert!(h.requests.is_empty());
    assert!(
        format!("{:?}", (&h.failures, &h.done)).contains("below the BingX minimum of 2 USDT"),
        "{:?}",
        (&h.failures, &h.done)
    );

    let mut h = Harness::new(None);
    h.place(vec![cheap_limit(true)]);
    assert_eq!(h.requests.len(), 1, "{:?}", h.failures);
}

#[test]
fn a_limit_replace_that_cancels_and_refuses_says_the_order_is_gone() {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    h.reply(r#"{"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"FAILED"}}"#);
    let error = h.done[0].as_ref().unwrap_err();
    assert!(
        error.contains("outcome is unknown") && error.contains("Check Open Orders"),
        "{error}"
    );
}

fn moved_stop() -> Harness {
    let mut stop = order();
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = 90.0;
    stop.reduce_only = true;
    stop.client_order_id = None;
    stop.trigger_type = Some(StopTriggerType::MarkPrice);
    let mut h = Harness::new(Some((stop, "TRIGGER_MARKET")));
    h.engine.store_mut().set_positions(vec![position()]);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    cancel_stop(&mut h);
    h
}

#[test]
fn a_moved_stop_whose_replacement_is_lost_says_the_position_may_have_no_stop() {
    let mut h = moved_stop();
    h.failed();
    let error = h.done[0].as_ref().unwrap_err();
    assert!(
        error.contains("unknown") && error.contains("Open Orders"),
        "{error}"
    );
}

#[test]
fn a_moved_stop_whose_replacement_answer_is_unclear_does_not_claim_no_stop() {
    for body in [r#"{"code":110500,"msg":"busy"}"#, "not json"] {
        let mut h = moved_stop();
        h.reply(body);
        let error = h.done[0].as_ref().unwrap_err();
        assert!(
            error.contains("unknown") && error.contains("Open Orders"),
            "{body}: {error}"
        );
    }
}

#[test]
fn a_refused_bracket_says_the_entry_was_placed() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        stop_loss: Some(90.0),
        ..opts()
    }]);
    h.reply(r#"{"code":0,"data":{"order":{"orderId":9223372036854775806}}}"#);
    h.reply(r#"{"code":110411,"msg":"stop price on the wrong side"}"#);
    let error = format!("{:?}", (&h.failures, &h.done));
    assert!(error.contains("entry was placed"), "{error}");
    assert!(error.contains("BingX 110411"), "{error}");
}

#[test]
fn a_close_below_the_market_minimum_is_sent_but_an_open_is_not() {
    let dust = |reduce_only: bool| -> PlaceOrderOpts {
        serde_json::from_value(serde_json::json!({
            "symbol": "BTCUSDT", "type": "limit", "side": "sell", "amount": 0.005,
            "price": 100000.0, "reduceOnly": reduce_only,
        }))
        .unwrap()
    };
    let harness = || {
        let mut h = Harness::new(None);
        let mut market = market();
        market.limits.amount.min = 0.01;
        h.engine.store_mut().set_markets(vec![market]);
        h
    };
    let mut h = harness();
    h.place(vec![dust(false)]);
    assert!(h.requests.is_empty(), "{:?}", h.requests.len());
    let mut h = harness();
    h.place(vec![dust(true)]);
    assert_eq!(h.requests.len(), 1, "{:?}", (&h.failures, &h.done));
}

#[test]
fn a_price_move_on_a_working_order_sends_no_status_restriction() {
    for status in ["WORKING", "OPEN"] {
        let mut h = Harness::with_status(Some((order(), "LIMIT")), status);
        h.command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(96.0),
            amount: None,
        });
        assert!(
            !h.body().contains("cancelRestrictions"),
            "{status}: {}",
            h.body()
        );
    }
}

#[test]
fn a_duplicate_client_order_id_is_unknown_not_refused() {
    let mut h = Harness::new(None);
    h.place(vec![PlaceOrderOpts {
        client_order_id: Some("again".into()),
        ..opts()
    }]);
    h.reply(r#"{"code":101400,"msg":"duplicate clientOrderID"}"#);
    assert_ne!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::Rejected),
        "{:?}",
        h.failures
    );
    let mut h = Harness::new(None);
    h.place(vec![opts()]);
    h.reply(r#"{"code":101400,"msg":"quantity is invalid"}"#);
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::Rejected),
        "{:?}",
        h.failures
    );
}

fn protected_limit() -> Order {
    let mut o = order();
    o.stop_loss = Some(90.0);
    o.take_profit = Some(110.0);
    o
}

/// As legacy: a size change on a limit with TP/SL is a native amend, which keeps them.
#[test]
fn a_size_change_on_a_limit_with_tp_sl_is_amended() {
    let mut h = Harness::new(Some((protected_limit(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: None,
        amount: Some(order().amount * 2.0),
    });
    assert_eq!(h.requests.len(), 1, "{:?}", (&h.failures, &h.done));
    assert!(
        h.requests[0].url.contains("/trade/amend"),
        "{}",
        h.requests[0].url
    );
}

/// As legacy: a price move would drop the TP/SL, so it is refused with what to do instead.
#[test]
fn a_price_move_on_a_limit_with_tp_sl_says_what_to_do() {
    let mut h = Harness::new(Some((protected_limit(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(96.0),
        amount: None,
    });
    assert!(h.requests.is_empty());
    let error = h.done[0].as_ref().unwrap_err();
    assert!(
        error.contains("without dropping its TP/SL") && error.contains("Cancel it"),
        "{error}"
    );
}

#[test]
fn an_edited_standalone_stop_preserves_its_original_closing_authority_on_the_wire() {
    for (kind, native, price) in [
        (OrderType::StopLoss, "STOP_MARKET", 91.0),
        (OrderType::TakeProfit, "TAKE_PROFIT_MARKET", 111.0),
    ] {
        for full in [false, true] {
            let mut original = order();
            original.kind = kind;
            original.side = OrderSide::Sell;
            original.price = if kind == OrderType::StopLoss {
                90.0
            } else {
                110.0
            };
            original.reduce_only = !full;
            original.close = Some(full);
            original.client_order_id = None;
            original.trigger_type = Some(StopTriggerType::MarkPrice);
            let mut h = Harness::new(Some((original.clone(), native)));
            h.engine.store_mut().set_positions(vec![position()]);
            h.command(Command::AmendOrder {
                command: 1,
                id: original.id.clone(),
                price: Some(price),
                amount: None,
            });
            assert_eq!(h.requests.len(), 1, "{native}: {:?}", h.done);
            h.reply(
                &serde_json::json!({"code":0,"data":{"orderId":original.id,"status":"CANCELED","executedQty":"0"}})
                    .to_string(),
            );
            let wire: serde_json::Value = serde_json::from_str(h.body()).unwrap();
            assert_eq!(wire["type"], native);
            if full {
                assert_eq!(wire["closePosition"], "true");
                assert!(wire.get("reduceOnly").is_none());
            } else {
                assert_eq!(
                    wire["reduceOnly"], true,
                    "{native} closing flag was lost: {wire}"
                );
            }
        }
    }
}

fn guarded_stop(native: bool) -> Harness {
    let mut stop = order();
    stop.kind = OrderType::StopLoss;
    stop.side = OrderSide::Sell;
    stop.price = 90.0;
    stop.reduce_only = true;
    stop.close = Some(false);
    stop.client_order_id = None;
    stop.trigger_type = Some(StopTriggerType::MarkPrice);
    let original = native.then(|| {
        serde_json::from_value::<trading::protection::Original>(serde_json::json!({
            "symbol":"BTC-USDT", "side":"SELL", "positionSide":"BOTH", "type":"STOP_MARKET",
            "quantity":stop.amount, "stopPrice":90, "workingType":"MARK_PRICE", "reduceOnly":true,
            "stopGuaranteed":false, "priceProtect":true,
        }))
        .unwrap()
    });
    let mut h = Harness::with_original(Some((stop, "STOP_MARKET")), "NEW", original);
    h.engine.store_mut().set_positions(vec![position()]);
    h
}
fn edit_stop(h: &mut Harness) {
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
}
fn cancel_stop(h: &mut Harness) {
    h.reply(&serde_json::json!({"code":0,"data":{"orderId":order().id,"status":"CANCELED","executedQty":"0"}}).to_string());
}
fn half_refusal(h: &mut Harness) {
    h.reply(&serde_json::json!({"code":0,"data":{"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"FAILED"}}).to_string());
}

#[test]
fn proven_standalone_stops_use_native_cancel_replace_and_preserve_the_closing_flags() {
    let mut h = guarded_stop(true);
    edit_stop(&mut h);
    assert!(h.requests[0].url.ends_with("/trade/cancelReplace"));
    let wire: serde_json::Value = serde_json::from_str(h.body()).unwrap();
    assert_eq!(wire["reduceOnly"], true);
    assert_eq!(wire["stopPrice"], 91.0);
    assert_eq!(
        wire["cancelOrderId"].as_u64(),
        Some(order().id.parse::<u64>().unwrap())
    );
    assert_eq!(wire["cancelRestrictions"], "ONLY_NEW");
    assert_eq!(wire["workingType"], "MARK_PRICE");
    h.reply(&serde_json::json!({"code":0,"data":{"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"SUCCESS","newOrderId":"555"}}).to_string());
    assert!(h.done[0].is_ok(), "{:?}", h.done);
}

#[test]
fn native_partial_refusal_puts_back_the_exact_original_once_with_typed_protection() {
    use safe_cex_engine::error::Protection;
    let mut h = guarded_stop(true);
    edit_stop(&mut h);
    half_refusal(&mut h);
    assert_eq!(h.requests.len(), 1, "{:?}", h.done);
    assert!(h.requests[0].url.ends_with("/trade/order"));
    let wire: serde_json::Value = serde_json::from_str(h.body()).unwrap();
    for (key, value) in [
        ("type", serde_json::json!("STOP_MARKET")),
        ("stopPrice", serde_json::json!(90)),
        ("side", serde_json::json!("SELL")),
        ("workingType", serde_json::json!("MARK_PRICE")),
        ("reduceOnly", serde_json::json!(true)),
        ("priceProtect", serde_json::json!(true)),
        ("stopGuaranteed", serde_json::json!(false)),
    ] {
        assert_eq!(wire[key], value, "{key} in {wire}");
    }
    assert_eq!(wire["quantity"].as_f64(), Some(order().amount));
    assert!(wire.get("cancelOrderId").is_none());
    h.reply(r#"{"code":0,"data":{"orderId":"restored","status":"NEW"}}"#);
    assert_eq!(
        h.failures[0].protection,
        Some(Protection::Restored),
        "{:?}",
        h.failures
    );
    assert!(h.requests.is_empty());
}

#[test]
fn refused_or_unproven_put_back_never_loops_and_only_definite_refusal_claims_unprotected() {
    use safe_cex_engine::{Outcome, error::Protection};
    for (status, body, protection) in [
        (
            200,
            r#"{"code":110411,"msg":"would immediately trigger"}"#,
            Some(Protection::Unprotected),
        ),
        (
            200,
            r#"{"code":100001,"msg":"bad signature"}"#,
            Some(Protection::Unprotected),
        ),
        (200, r#"{"code":100410,"msg":"rate limited"}"#, None),
        (429, r#"{"code":110411,"msg":"rate limited"}"#, None),
        (200, r#"{"code":0,"data":{}}"#, None),
        (500, r#"{"code":110411,"msg":"old refusal in 500"}"#, None),
        (408, "", None),
    ] {
        let mut h = guarded_stop(true);
        edit_stop(&mut h);
        half_refusal(&mut h);
        h.reply_status(status, body);
        assert!(h.requests.is_empty(), "{status}/{body} must never retry");
        let error = &h.failures[0];
        assert_eq!(error.protection, protection, "{status}/{body}: {error:?}");
        if protection.is_none() {
            assert_eq!(
                error.outcome,
                Some(Outcome::Unknown),
                "{status}/{body}: {error:?}"
            );
            assert!(
                error.message.contains("unknown") && error.message.contains("Open Orders"),
                "{error:?}"
            );
            assert!(!error.message.contains("is unprotected"));
            assert!(
                error.message.contains("original stop put-back"),
                "{status}/{body}: {error:?}"
            );
        }
    }
}

#[test]
fn guarded_rate_limits_are_unknown_without_changing_unrelated_gateway_classification() {
    for native in [false, true] {
        for (status, body) in [
            (200, r#"{"code":100410,"msg":"rate limit"}"#),
            (429, r#"{"code":101204,"msg":"refusal-shaped"}"#),
        ] {
            let mut h = guarded_stop(native);
            edit_stop(&mut h);
            if !native {
                cancel_stop(&mut h);
            }
            h.reply_status(status, body);
            assert!(h.requests.is_empty());
            assert_eq!(
                h.failures[0].outcome,
                Some(safe_cex_engine::Outcome::Unknown),
                "{:?}",
                h.failures
            );
            assert!(h.failures[0].protection.is_none());
            assert!(
                h.failures[0].message.contains("Open Orders"),
                "{:?}",
                h.failures
            );
        }
    }
}

#[test]
fn contradictory_execution_and_replacement_results_never_authorize_put_back() {
    for (native, data) in [
        (
            true,
            serde_json::json!({"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"FAILED","executedQty":"0.25"}),
        ),
        (
            true,
            serde_json::json!({"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"FAILED","replaceResult":"SUCCESS"}),
        ),
        (
            true,
            serde_json::json!({"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"FAILED","failed":[{}]}),
        ),
        (
            false,
            serde_json::json!({"orderId":order().id,"status":"CANCELED","executedQty":"0.25","order":{"orderId":order().id,"status":"CANCELED","executedQty":"0"}}),
        ),
    ] {
        let mut h = guarded_stop(native);
        edit_stop(&mut h);
        h.reply(&serde_json::json!({"code":0,"data":data}).to_string());
        assert!(
            h.requests.is_empty(),
            "contradictory evidence caused a write"
        );
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert!(h.failures[0].protection.is_none());
    }
}

#[test]
fn ambiguous_native_halves_and_empty_or_filled_cancel_evidence_never_create() {
    for body in [r#"{"code":0,"data":{}}"#.to_string(),
        serde_json::json!({"code":0,"data":{"cancelResult":"SUCCESS","newOrderResult":"FAILED"}}).to_string(),
        serde_json::json!({"code":0,"data":{"cancelOrderId":"other","cancelResult":"SUCCESS","newOrderResult":"FAILED"}}).to_string(),
        serde_json::json!({"code":0,"data":{"cancelOrderId":order().id,"cancelResult":"SUCCESS"}}).to_string()] {
        let mut h=guarded_stop(true); edit_stop(&mut h); h.reply(&body);
        assert!(h.requests.is_empty());
        assert_eq!(h.failures[0].outcome,Some(safe_cex_engine::Outcome::Unknown));
    }
    for (status, fill) in [
        ("CANCELED", None),
        ("CANCELED", Some("0.5")),
        ("FILLED", Some("1")),
    ] {
        let mut h = guarded_stop(false);
        edit_stop(&mut h);
        h.reply(&serde_json::json!({"code":0,"data":{"orderId":order().id,"status":status,"executedQty":fill}}).to_string());
        assert!(h.requests.is_empty());
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
    }
}

#[test]
fn final_claim_refuses_stale_crossed_changed_or_unprotected_stops_and_unwinds() {
    for case in ["stale", "crossed", "filled", "gone-position"] {
        let mut h = guarded_stop(true);
        h.engine.handle(
            h.now,
            Event::Command(Command::AmendOrder {
                command: 1,
                id: order().id,
                price: Some(91.0),
                amount: None,
            }),
        );
        match case {
            "stale" => h.now += 5001.0,
            "crossed" => h.quote(None, Some(90.0)),
            "filled" => {
                let mut o = h.engine.store().order(&order().id).unwrap().clone();
                o.filled = 0.1;
                o.remaining -= 0.1;
                h.engine.store_mut().upsert_order(o);
            }
            _ => h.engine.store_mut().set_positions(Vec::new()),
        }
        h.collect();
        assert!(h.requests.is_empty(), "{case}");
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::NotSent),
            "{case}: {:?}",
            h.failures
        );
        assert_eq!(
            h.engine.store().order(&order().id).unwrap().status,
            OrderStatus::Open
        );
    }
    let mut h = guarded_stop(true);
    h.engine.handle(
        h.now,
        Event::Command(Command::AmendOrder {
            command: 1,
            id: order().id,
            price: Some(91.0),
            amount: None,
        }),
    );
    h.quote(None, Some(90.0));
    h.collect();
    h.quote(None, Some(100.0));
    h.command(Command::AmendOrder {
        command: 2,
        id: order().id,
        price: Some(91.0),
        amount: None,
    });
    assert_eq!(
        h.requests.len(),
        1,
        "veto stranded the claim: {:?}",
        h.failures
    );
}

#[test]
fn genuine_reference_boundary_is_inclusive_and_signature_resend_rechecks_it() {
    for (age, allowed) in [(4999., true), (5000., true), (5001., false)] {
        let mut h = guarded_stop(true);
        h.now = 1_000.0 + age;
        edit_stop(&mut h);
        assert_eq!(
            h.requests
                .iter()
                .any(|r| r.method != safe_cex_engine::Method::Get),
            allowed,
            "{age}: {:?}",
            h.done
        );
        if !allowed {
            assert_eq!(h.requests.len(), 1);
            assert_eq!(h.requests[0].method, safe_cex_engine::Method::Get);
            h.reply(r#"{"code":0,"data":[]}"#);
            assert!(h.requests.is_empty());
            assert!(h.failures[0].message.contains("reference stale"));
        }
    }
    let mut h = guarded_stop(false);
    edit_stop(&mut h);
    let request = h.requests.remove(0);
    h.now += 1.;
    h.engine.handle(
        h.now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":100001,"msg":"Signature verification failed"}"#,
            },
        },
    );
    h.now += 5001.;
    h.collect();
    assert!(
        h.requests.is_empty(),
        "signature retry skipped the final veto"
    );
    assert!(!h.failures.is_empty());
}

#[test]
fn native_404_does_not_fall_back_to_cancel() {
    let mut h = guarded_stop(true);
    edit_stop(&mut h);
    h.reply_status(404, r#"{"code":100400,"msg":"not found"}"#);
    assert!(h.requests.is_empty());
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn native_sl_tp_both_sides_sources_and_full_close_keep_the_original_size() {
    for (kind, native, side, old, new) in [
        (
            OrderType::StopLoss,
            "STOP_MARKET",
            OrderSide::Sell,
            90.,
            91.,
        ),
        (
            OrderType::TakeProfit,
            "TAKE_PROFIT_MARKET",
            OrderSide::Sell,
            110.,
            111.,
        ),
        (
            OrderType::StopLoss,
            "STOP_MARKET",
            OrderSide::Buy,
            110.,
            109.,
        ),
        (
            OrderType::TakeProfit,
            "TAKE_PROFIT_MARKET",
            OrderSide::Buy,
            90.,
            89.,
        ),
    ] {
        for (source, working) in [
            (StopTriggerType::MarkPrice, "MARK_PRICE"),
            (StopTriggerType::LastPrice, "CONTRACT_PRICE"),
        ] {
            for full in [false, true] {
                let mut o = order();
                o.kind = kind;
                o.side = side;
                o.price = old;
                o.amount = 2.;
                o.remaining = 2.;
                o.reduce_only = !full;
                o.close = Some(full);
                o.client_order_id = None;
                o.trigger_type = Some(source);
                let raw = serde_json::json!({"symbol":"BTC-USDT","type":native,"side":if side==OrderSide::Sell {"SELL"} else {"BUY"},"positionSide":"BOTH","quantity":2.,"stopPrice":old,"workingType":working,"closePosition":full,"reduceOnly":(!full).then_some(true)});
                let original = serde_json::from_value(raw).unwrap();
                let mut h = Harness::with_original(Some((o, native)), "PENDING", Some(original));
                let mut p = position();
                p.side = if side == OrderSide::Sell {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                };
                p.contracts = 3.;
                h.engine.store_mut().set_positions(vec![p]);
                h.command(Command::AmendOrder {
                    command: 1,
                    id: order().id,
                    price: Some(new),
                    amount: None,
                });
                assert!(h.requests[0].url.ends_with("/trade/cancelReplace"));
                let edit: serde_json::Value = serde_json::from_str(h.body()).unwrap();
                assert_eq!(edit["cancelRestrictions"], "ONLY_PENDING");
                assert_eq!(edit["quantity"].as_f64(), Some(2.));
                assert_eq!(edit["workingType"], working);
                if full {
                    assert_eq!(edit["closePosition"], "true");
                    assert!(edit.get("reduceOnly").is_none());
                } else {
                    assert_eq!(edit["reduceOnly"], true);
                }
                half_refusal(&mut h);
                let put_back: serde_json::Value = serde_json::from_str(h.body()).unwrap();
                assert_eq!(
                    put_back["quantity"].as_f64(),
                    Some(2.),
                    "put-back must not substitute the 3-contract position"
                );
                assert_eq!(put_back["stopPrice"].as_f64(), Some(old));
                assert_eq!(put_back["type"], native);
                assert_eq!(put_back["side"], edit["side"]);
                assert_eq!(put_back["workingType"], working);
            }
        }
    }
}

#[test]
fn contradictory_native_halves_or_symbols_are_unknown_without_put_back() {
    for extra in [
        serde_json::json!({"newOrderId":"possibly-live"}),
        serde_json::json!({"symbol":"ETH-USDT"}),
    ] {
        let mut h = guarded_stop(true);
        edit_stop(&mut h);
        let mut data = serde_json::json!({"cancelOrderId":order().id,"cancelResult":"SUCCESS","newOrderResult":"FAILED"});
        data.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        h.reply(&serde_json::json!({"code":0,"data":data}).to_string());
        assert!(h.requests.is_empty());
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert!(h.failures[0].protection.is_none());
    }
}

#[test]
fn unsupported_stop_size_edit_never_cancels_the_original() {
    let mut h = guarded_stop(true);
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        price: None,
        amount: Some(3.),
    });
    assert!(h.requests.is_empty());
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    assert_eq!(
        h.engine.store().order(&order().id).unwrap().status,
        OrderStatus::Open
    );
}

#[test]
fn a_streamed_fill_wins_over_an_older_cancel_reply_and_never_creates() {
    for native in [false, true] {
        let mut h = guarded_stop(native);
        edit_stop(&mut h);
        let mut o = h.engine.store().order(&order().id).unwrap().clone();
        o.filled = 0.25;
        o.remaining -= 0.25;
        h.engine.store_mut().upsert_order(o);
        if native {
            half_refusal(&mut h);
        } else {
            cancel_stop(&mut h);
        }
        assert!(h.requests.is_empty());
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(h.engine.store().order(&order().id).unwrap().filled, 0.25);
    }
}

#[test]
fn another_fresh_ticker_field_never_refreshes_a_stale_or_missing_trigger_reference() {
    for missing in [false, true] {
        let mut h = guarded_stop(true);
        h.now += 6000.;
        h.engine
            .handle(h.now, Event::PollScaleChanged { scale: 1.0 });
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.last = 101.0);
        if missing {
            h.engine.store_mut().set_tickers(Vec::new());
        }
        edit_stop(&mut h);
        assert_eq!(h.requests.len(), 1);
        assert_eq!(h.requests[0].method, safe_cex_engine::Method::Get);
        h.reply(r#"{"code":0,"data":[]}"#);
        assert!(h.requests.is_empty());
        assert!(
            h.failures[0].message.contains("reference stale"),
            "{:?}",
            h.failures
        );
        assert_eq!(
            h.engine.store().order(&order().id).unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn missing_order_edits_never_replace_or_put_back_without_cancel_proof() {
    for (code, message) in [
        (109400, "order not exist"),
        (109421, "order does not exist"),
    ] {
        for shape in 0..4 {
            let mut h = if shape < 2 {
                Harness::new(Some((order(), "LIMIT")))
            } else {
                guarded_stop(shape == 3)
            };
            let original = h.engine.store().order(&order().id).unwrap().clone();
            h.command(Command::AmendOrder {
                command: 1,
                id: original.id.clone(),
                price: (shape != 0).then_some(if shape < 2 { 96. } else { 91. }),
                amount: (shape == 0).then_some(2.),
            });
            assert_eq!(h.requests.len(), 1, "shape {shape}: {:?}", h.done);
            h.reply(&serde_json::json!({"code":code,"msg":message}).to_string());
            assert!(
                h.requests.is_empty(),
                "no follow-up writes for shape {shape}"
            );
            let error = &h.failures[0];
            assert_eq!(
                error.message,
                "BingX order was already filled or cancelled; check Open Orders"
            );
            assert_ne!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
            assert_eq!(error.protection, None);
            assert_eq!(h.engine.store().order(&original.id), Some(&original));
        }
    }
}

#[test]
fn missing_order_text_on_placement_keeps_the_generic_parameter_classification() {
    let mut h = Harness::new(None);
    h.place(vec![opts()]);
    h.reply(r#"{"code":109400,"msg":"order not exist"}"#);
    assert_eq!(
        h.failures[0].kind,
        Some(safe_cex_engine::ErrorKind::InvalidInput)
    );
    assert_eq!(
        h.failures[0].outcome,
        Some(safe_cex_engine::Outcome::Rejected)
    );
    assert!(h.requests.is_empty());
}

#[test]
fn a_rate_limited_or_server_failure_keeps_unknown_words_even_with_missing_order_text() {
    for status in [429, 500] {
        let mut h = guarded_stop(true);
        edit_stop(&mut h);
        h.reply_status(status, r#"{"code":109400,"msg":"order not exist"}"#);
        assert!(h.requests.is_empty());
        assert!(
            h.failures[0].message.contains("outcome is unknown"),
            "{:?}",
            h.failures
        );
        assert!(!h.failures[0].message.contains("already filled"));
        assert_eq!(h.failures[0].protection, None);
    }
}

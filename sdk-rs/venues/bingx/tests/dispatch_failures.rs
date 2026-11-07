//! Actual pending owners receive closed provider failures, never fake HTTP failures.
use safe_cex_engine::model::*;
use safe_cex_engine::signing::{Provider, SignError, SignFuture, SignRequest, SignResult, Signer};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Outcome, Query, Response, VenueError,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared};
use std::collections::{HashMap, VecDeque};

struct Parked(Provider);
impl Signer for Parked {
    fn immediate(&self) -> bool {
        false
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        self.0.sign(work)
    }
    fn sign_ready(&self, _: &SignRequest) -> Option<Result<SignResult, SignError>> {
        None
    }
}
struct Harness {
    engine: Engine<Bingx>,
    local: Provider,
    routes: HashMap<safe_cex_engine::RequestId, Preparation>,
    preparing: HashMap<safe_cex_engine::RequestId, HttpRequest>,
    signs: Vec<(safe_cex_engine::RequestId, SignRequest)>,
    http: Vec<HttpRequest>,
    errors: Vec<VenueError>,
    done: Vec<String>,
    timers: Vec<(safe_cex_engine::TimerId, f64)>,
    now: f64,
    progress: Vec<safe_cex_engine::model::OrderIdentityReceipt>,
}
impl Harness {
    fn new() -> Self {
        let (venue, local) = support::Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Default::default()
        }
        .build();
        let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
        engine
            .set_signer(Some(std::sync::Arc::new(Parked(local.clone()))))
            .unwrap();
        engine.store_mut().set_markets(vec![market()]);
        engine.store_mut().set_tickers(vec![Ticker {
            id: "BTC-USDT".into(),
            symbol: "BTCUSDT".into(),
            last: 100.,
            mark: 100.,
            ..Default::default()
        }]);
        Self {
            engine,
            local,
            routes: HashMap::new(),
            preparing: HashMap::new(),
            signs: vec![],
            http: vec![],
            errors: vec![],
            done: vec![],
            timers: vec![],
            now: 1_000.,
            progress: vec![],
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        let mut queue = VecDeque::from(self.engine.drain_actions().collect::<Vec<_>>());
        while let Some(action) = queue.pop_front() {
            match action {
                Action::ManagedHttp(action) => match *action {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } => {
                        assert!(self.routes.insert(request.id, preparation).is_none());
                        assert!(self.preparing.insert(request.id, request.clone()).is_none());
                        self.engine.http_prepared(
                            self.now,
                            preparation,
                            Prepared {
                                id: request.id.0,
                                expires_at: None,
                            },
                        );
                    }
                    HttpAction::Bind {
                        preparation,
                        permit,
                        request,
                    } => {
                        assert_eq!(self.routes.remove(&request.id), Some(preparation));
                        self.preparing.remove(&request.id);
                        if matches!(
                            self.engine.claim_http(self.now, permit),
                            Ok(Claim::Released(_))
                        ) {
                            self.http.push(request);
                        }
                    }
                    HttpAction::BindGroup { .. } => {
                        panic!("BingX fixture declares no Mutation barriers")
                    }
                    HttpAction::Cancel { preparation, .. } => {
                        let request = preparation.attempt().request;
                        if self.routes.get(&request) == Some(&preparation) {
                            self.routes.remove(&request);
                            self.preparing.remove(&request);
                        }
                    }
                },
                Action::Sign { attempt, work, .. } => self.signs.push((attempt, work)),
                Action::Http(r) => self.http.push(r),
                Action::Notice(Notice::CommandFailed { error, .. }) => self.errors.push(error),
                Action::Notice(Notice::CommandDone { result, .. }) => self.done.push(result),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                Action::Notice(Notice::CommandProgress { receipts, .. }) => {
                    self.progress.extend(receipts)
                }
                Action::Notice(Notice::Deactivate) => {
                    panic!("provider refusal cannot deactivate the venue")
                }
                Action::Notice(Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::AUTH_REJECTED =>
                {
                    panic!("provider refusal cannot reject account authentication")
                }
                Action::Notice(Notice::AccountAuth { .. }) => {
                    panic!("provider refusal is not venue auth evidence")
                }
                _ => (),
            }
            for action in self
                .engine
                .drain_actions()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                queue.push_front(action);
            }
        }
    }
    fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    fn refuse(&mut self, error: SignError) {
        assert_eq!(self.signs.len(), 1);
        let (attempt, _) = self.signs.remove(0);
        self.event(Event::Signed {
            attempt,
            result: Err(error),
        });
    }
    fn release(&mut self) {
        assert_eq!(self.signs.len(), 1);
        let (attempt, work) = self.signs.remove(0);
        let result = self.local.sign_ready(&work).unwrap().unwrap();
        self.event(Event::Signed {
            attempt,
            result: Ok(result),
        });
    }
    fn reply(&mut self, body: &str) {
        self.reply_status(200, body);
    }
    fn reply_status(&mut self, status: u16, body: &str) {
        assert_eq!(self.http.len(), 1);
        let request = self.http.remove(0).id;
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    fn place(&mut self, count: usize) {
        self.command(Command::PlaceOrders {
            command: 42,
            orders: (0..count)
                .map(|_| {
                    serde_json::from_str(
                        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":95}"#,
                    )
                    .unwrap()
                })
                .collect(),
        });
    }
    fn cancel(&mut self) {
        let orders = [order("one"), order("two")];
        self.engine.store_mut().set_orders(orders.to_vec());
        self.command(Command::CancelOrders {
            command: 42,
            ids: vec!["one".into(), "two".into()],
        });
    }
    fn error(&mut self) -> &VenueError {
        if !self.signs.is_empty() {
            assert_eq!(self.signs.len(), 1);
            assert_eq!(self.preparing.len(), 1);
            let read = self.preparing.values().next().unwrap();
            assert_eq!(read.method, safe_cex_engine::Method::Get);
            assert!(
                read.url.contains("/trade/openOrders"),
                "only the required fresh reconciliation may follow uncertainty"
            );
            self.refuse(SignError::Refused);
        }
        assert_eq!(self.errors.len(), 1);
        assert!(self.done.is_empty());
        assert!(self.signs.is_empty() && self.http.is_empty());
        &self.errors[0]
    }
}
fn read_owner_keeps_typed_provider_failure(kind: u8) {
    for error in [
        SignError::Refused,
        SignError::Unsupported,
        SignError::TimedOut,
    ] {
        {
            let mut h = Harness::new();
            let command = match kind {
                0 => Command::FetchAccount { command: 42 },
                1 => Command::Query {
                    command: 42,
                    query: Query::Fills {
                        symbol: Some("BTCUSDT".into()),
                        order_id: None,
                        from_ms: Some(0.),
                        to_ms: Some(1_000.),
                        limit: Some(2),
                    },
                },
                _ => Command::FetchLeverage {
                    command: 42,
                    symbol: "BTCUSDT".into(),
                },
            };
            h.command(command);
            let auth_failures = h.engine.health().auth_failures;
            h.refuse(error);
            assert_eq!(h.error(), &error.venue_error(), "owner {kind}");
            assert_eq!(h.engine.health().auth_failures, auth_failures);
        }
    }
}
#[test]
fn identity_keeps_typed_provider_failure() {
    read_owner_keeps_typed_provider_failure(0);
}
#[test]
fn history_keeps_typed_provider_failure() {
    read_owner_keeps_typed_provider_failure(1);
}
#[test]
fn metadata_keeps_typed_provider_failure() {
    read_owner_keeps_typed_provider_failure(2);
}

#[test]
fn typed_margin_failure_preserves_newer_store_settings() {
    let mut h = Harness::new();
    h.command(Command::FetchLeverage {
        command: 42,
        symbol: "BTCUSDT".into(),
    });
    h.release();
    h.reply(r#"{"code":0,"data":{"longLeverage":5,"shortLeverage":6}}"#);
    assert_eq!(
        h.signs.len(),
        1,
        "the real margin Plan slot awaits its provider"
    );
    h.engine.store_mut().update_setting("BTCUSDT", |setting| {
        setting.leverage = 25.;
        setting.buy_leverage = Some(25.);
        setting.sell_leverage = Some(25.);
        setting.margin_type = Some(MarginType::Cross);
    });
    let auth_failures = h.engine.health().auth_failures;
    h.refuse(SignError::Refused);
    assert_eq!(h.error(), &SignError::Refused.venue_error());
    assert_eq!(h.engine.health().auth_failures, auth_failures);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.leverage, 25.);
    assert_eq!(
        (setting.buy_leverage, setting.sell_leverage),
        (Some(25.), Some(25.))
    );
}
#[test]
fn first_write_provider_refusal_is_typed_not_sent() {
    let mut h = Harness::new();
    h.place(1);
    h.refuse(SignError::Refused);
    assert_eq!(h.error(), &SignError::Refused.venue_error());
}
#[test]
fn signature_retry_provider_refusal_keeps_prior_sent_unknown() {
    let mut h = Harness::new();
    h.place(1);
    let original_work = serde_json::to_value(&h.signs[0].1).unwrap();
    let original_progress = h.progress.clone();
    h.release();
    let original = h.http[0].body.clone();
    h.reply(r#"{"code":100001,"msg":"Signature verification failed"}"#);
    assert_eq!(h.signs.len(), 1);
    assert_eq!(serde_json::to_value(&h.signs[0].1).unwrap(), original_work);
    assert_eq!(
        h.progress, original_progress,
        "retry must not allocate another client identity"
    );
    h.refuse(SignError::Refused);
    let error = h.error();
    assert_eq!(error.kind, SignError::Refused.venue_error().kind);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(original.unwrap().contains("clientOrderId"));
    assert_eq!(h.timers.len(), 1, "only the engine reconciliation re-ask");
    let (timer, at) = h.timers.remove(0);
    assert_eq!(at, h.now + 1_000.);
    h.now = at;
    h.event(Event::TimerFired { timer });
    assert_eq!(h.preparing.len(), 1);
    let read = h.preparing.values().next().unwrap();
    assert_eq!(read.method, safe_cex_engine::Method::Get);
    assert!(read.url.contains("/trade/openOrders"));
    assert_eq!(h.signs.len(), 1, "only the reconciliation GET is signed");
    h.refuse(SignError::Refused);
    assert!(h.signs.is_empty() && h.http.is_empty());
    assert!(h.preparing.is_empty() && h.routes.is_empty());
    assert_eq!(h.errors.len(), 1, "no duplicate caller terminal");
    assert_eq!(h.progress, original_progress, "no new placement identity");
    assert_eq!(h.timers.len(), 1, "the bounded re-ask remains owed");
    assert_eq!(h.timers[0].1, h.now + 2_000.);
}
#[test]
fn accepted_leg_then_unsent_leg_keeps_accepted_id_and_never_not_sent() {
    let mut h = Harness::new();
    h.place(2);
    h.release();
    h.reply(r#"{"code":0,"data":{"order":{"orderId":"accepted"}}}"#);
    h.refuse(SignError::Refused);
    let error = h.error();
    assert_ne!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
}
#[test]
fn cancellation_sent_evidence_survives_refusal_in_either_order() {
    for provider_first in [true, false] {
        let mut h = Harness::new();
        h.cancel();
        if provider_first {
            h.refuse(SignError::Refused);
        }
        h.release();
        h.reply(r#"{"code":80014,"msg":"invalid cancellation"}"#);
        if !provider_first {
            h.refuse(SignError::Refused);
        }
        assert_ne!(h.error().outcome, Some(Outcome::NotSent));
    }
}
#[test]
fn successful_cancel_without_ids_is_effective_before_later_refusal() {
    let mut h = Harness::new();
    h.cancel();
    h.release();
    h.reply(r#"{"code":0,"data":{"orderId":"one"}}"#);
    h.refuse(SignError::Refused);
    assert_ne!(h.error().outcome, Some(Outcome::NotSent));
    assert_eq!(
        h.engine.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
}

#[test]
fn unclassified_native_response_after_or_before_unsent_cancel_is_unknown() {
    for status in [200, 400, 429] {
        for provider_first in [true, false] {
            let mut h = Harness::new();
            h.cancel();
            if provider_first {
                h.refuse(SignError::Refused);
            }
            h.release();
            h.reply_status(
                status,
                if status == 200 {
                    r#"{"code":999999,"msg":"unclassified"}"#
                } else {
                    "upstream unavailable"
                },
            );
            if !provider_first {
                h.refuse(SignError::Refused);
            }
            assert_eq!(
                h.error().outcome,
                Some(Outcome::Unknown),
                "status {status}, provider first {provider_first}"
            );
        }
    }
}

#[test]
fn later_history_page_signer_refusal_discards_buffered_rows_and_releases_owner() {
    let mut h = Harness::new();
    let query = || Command::Query {
        command: 42,
        query: Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms: Some(0.),
            to_ms: Some(1_000.),
            limit: Some(1),
        },
    };
    h.command(query());
    h.release();
    h.reply(r#"{"code":0,"data":{"fill_orders":[{"tradeId":"fill-one","orderId":"order-one","symbol":"BTC-USDT","time":500,"price":"95","qty":"1","side":"BUY","commission":"0"}]}}"#);
    assert!(h.done.is_empty() && h.errors.is_empty());
    assert_eq!(h.timers.iter().filter(|(_, at)| *at == 1_200.).count(), 1);
    let index = h.timers.iter().position(|(_, at)| *at == 1_200.).unwrap();
    let (timer, at) = h.timers.remove(index);
    h.now = at;
    h.event(Event::TimerFired { timer });
    h.refuse(SignError::Refused);
    assert_eq!(h.error(), &SignError::Refused.venue_error());
    assert!(h.timers.is_empty());
    h.errors.clear();
    h.command(query());
    assert_eq!(
        h.signs.len(),
        1,
        "failed read must release its command owner"
    );
}

#[test]
fn margin_stage_keeps_typed_provider_failure() {
    let mut h = Harness::new();
    h.command(Command::FetchLeverage {
        command: 42,
        symbol: "BTCUSDT".into(),
    });
    h.release();
    h.reply(r#"{"code":0,"data":{"longLeverage":5,"shortLeverage":6}}"#);
    h.refuse(SignError::Unsupported);
    assert_eq!(h.error(), &SignError::Unsupported.venue_error());
}

#[test]
fn refused_setting_releases_publication_fence_for_the_next_metadata_read() {
    let mut h = Harness::new();
    h.command(Command::SetLeverage {
        command: 41,
        symbol: "BTCUSDT".into(),
        leverage: 9.,
        side: None,
    });
    h.refuse(SignError::Refused);
    assert_eq!(h.error(), &SignError::Refused.venue_error());
    h.errors.clear();
    h.command(Command::FetchLeverage {
        command: 42,
        symbol: "BTCUSDT".into(),
    });
    h.release();
    h.reply(r#"{"code":0,"data":{"longLeverage":13,"shortLeverage":14}}"#);
    h.release();
    h.reply(r#"{"code":0,"data":{"marginType":"ISOLATED"}}"#);
    assert!(h.errors.is_empty());
    assert_eq!(h.done.len(), 1);
    let setting = h.engine.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(13.));
    assert_eq!(setting.sell_leverage, Some(14.));
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
fn order(id: &str) -> Order {
    Order {
        id: id.into(),
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

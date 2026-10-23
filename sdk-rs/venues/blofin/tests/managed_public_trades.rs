use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Operation, Permit, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent, Scope,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Query, RequestId, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options, Pending};
use serde_json::{Value, json};

const MARKETS: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"},{"instId":"ETH-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"ETH","quoteCurrency":"USDT","state":"live","contractValue":"0.1","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
const TRADE: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","side":"sell","price":"60000","size":"2","ts":"1700000000000"}]}"#;

fn response<V: Venue>(e: &mut Engine<V>, now: f64, request: RequestId, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}

fn seeded<V: Venue>(venue: V) -> Engine<V> {
    let mut e = safe_cex_engine::test_support::seeded_engine(venue);
    e.start(0.);
    for (path, body) in [
        ("instruments", MARKETS),
        ("tickers", r#"{"code":"0","data":[]}"#),
    ] {
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains(path) => Some(r.id),
                _ => None,
            })
            .expect("native public startup request");
        response(&mut e, 0., request, 200, body);
    }
    e.drain_actions().for_each(drop);
    e
}

fn venue(keyed: bool) -> Blofin {
    Blofin::new(Options {
        public_data_only: !keyed,
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        passphrase: keyed.then(|| "fixture-pass".into()),
        ..Options::default()
    })
}

fn query<V: Venue>(e: &mut Engine<V>, now: f64, command: u32, symbol: &str, limit: Option<u32>) {
    e.handle(
        now,
        Event::Command(Command::Query {
            command,
            query: Query::PublicTrades {
                symbol: symbol.into(),
                limit,
            },
        }),
    );
}

fn terminals(actions: &[Action], command: u32) -> Vec<Result<Value, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => {
                Some(Ok(serde_json::from_str(result).expect("native JSON result")))
            }
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}

fn no_marker(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::Event(event))
            if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE
    )));
}

fn failed(actions: &[Action], command: u32) -> VenueError {
    no_marker(actions);
    let results = terminals(actions, command);
    assert_eq!(results.len(), 1, "one terminal failure: {results:?}");
    results
        .into_iter()
        .next()
        .unwrap()
        .expect_err("failed read")
}

fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "trade HTTP must wait for prepared release"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } if request.url.contains("/market/trades?") => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("native trade managed preparation")
}

fn bind(e: &mut Engine<Blofin>, now: f64, token: Preparation) -> Permit {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared bind")
}

fn released<V: Venue>(e: &mut Engine<V>, now: f64) -> (HttpRequest, Vec<Action>) {
    let actions = safe_cex_engine::test_support::prepared_actions(e, now);
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/market/trades?") => Some(r.clone()),
            _ => None,
        })
        .expect("native public trade request");
    (request, actions)
}

#[test]
fn public_trades_wait_for_managed_preparation() {
    let mut e = seeded(venue(false));
    query(&mut e, 100., 71, "BTCUSDT", Some(2));
    let actions = e.drain_actions().collect::<Vec<_>>();
    let (_, request) = preparation(&actions);
    assert_eq!(request.method, Method::Get);
    assert!(
        request
            .url
            .ends_with("/api/v1/market/trades?instId=BTC-USDT&limit=2")
    );
    assert!(request.headers.is_empty() && request.signer_headers.is_empty());
    assert_eq!(request.body, None);
    assert_eq!(request.timeout_ms, Some(20_000));
    assert!(terminals(&actions, 71).is_empty());
}

#[test]
fn unreleased_and_unclaimed_replies_cannot_complete_the_owner() {
    for prepared in [false, true] {
        let mut e = seeded(venue(false));
        query(&mut e, 100., 71, "BTCUSDT", None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let token = actions.iter().find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { preparation, .. } => Some(*preparation),
                _ => None,
            },
            _ => None,
        });
        let request = token
            .map(|t| t.attempt().request)
            .or_else(|| {
                actions.iter().find_map(|a| match a {
                    Action::Http(r) => Some(r.id),
                    _ => None,
                })
            })
            .expect("physical request identity");
        let permit = prepared
            .then(|| token.map(|t| bind(&mut e, 101., t)))
            .flatten();
        response(&mut e, 102., request, 200, TRADE);
        assert!(
            terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty(),
            "an unreleased response cannot publish a command result"
        );
        let token = token.expect("managed original read");
        let permit = permit.unwrap_or_else(|| bind(&mut e, 103., token));
        assert!(matches!(e.claim_http(104., permit), Ok(Claim::Released(_))));
        e.drain_actions().for_each(drop);
        response(&mut e, 105., request, 200, TRADE);
        assert_eq!(
            terminals(&e.drain_actions().collect::<Vec<_>>(), 71).len(),
            1
        );
    }
}

#[test]
fn response_at_original_deadline_uses_typed_physical_expiry() {
    let mut e = seeded(venue(false));
    query(&mut e, 100., 71, "BTCUSDT", None);
    let (request, actions) = released(&mut e, 100.);
    let timer = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
            _ => None,
        })
        .next_back();
    response(&mut e, 20_100., request.id, 200, TRADE);
    let actions = e.drain_actions().collect::<Vec<_>>();
    // The shared engine expires this original bound when either its timer or
    // an at-bound reply arrives first. Neither path may publish the body.
    let error = failed(&actions, 71);
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    if let Some(timer) = timer {
        e.handle(20_100., Event::TimerFired { timer });
        assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    }
    response(&mut e, 20_101., request.id, 200, TRADE);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    query(&mut e, 20_102., 71, "BTCUSDT", None);
    let (next, _) = released(&mut e, 20_102.);
    assert_ne!(next.id, request.id);
    response(&mut e, 20_103., request.id, 200, TRADE);
    assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    response(&mut e, 20_104., next.id, 200, r#"{"code":"0","data":[]}"#);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 71),
        vec![Ok(json!([]))]
    );
}

#[test]
fn parked_preparation_expiry_and_refusal_retain_typed_failures() {
    for timeout in [false, true] {
        let mut e = seeded(venue(false));
        query(&mut e, 100., 71, "BTCUSDT", None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let (token, _) = preparation(&actions);
        if timeout {
            let timer = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms } if *at_ms == 20_100. => Some(*timer),
                    _ => None,
                })
                .expect("preparation-inclusive deadline");
            e.handle(20_100., Event::TimerFired { timer });
        } else {
            e.http_preparation_failed(
                101.,
                token,
                Error::Invalid,
                VenueError::new("fixture transport unsupported")
                    .kind(ErrorKind::Unsupported)
                    .outcome(Outcome::NotSent),
            );
        }
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions, 71);
        assert_eq!(
            error.kind,
            Some(if timeout {
                ErrorKind::Timeout
            } else {
                ErrorKind::Unsupported
            })
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(!e.http_preparation_pending(token));
        response(&mut e, 20_101., token.attempt().request, 200, TRADE);
        assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
    }
}

#[test]
fn released_transport_failure_keeps_the_shared_network_kind() {
    let mut e = seeded(venue(false));
    query(&mut e, 100., 71, "BTCUSDT", None);
    let (request, _) = released(&mut e, 100.);
    e.handle(
        101.,
        Event::HttpFailed {
            request: request.id,
            error: "fixture network failure",
        },
    );
    let error = failed(&e.drain_actions().collect::<Vec<_>>(), 71);
    assert_eq!(error.kind, Some(ErrorKind::Network));
    assert_eq!(error.message, "fixture network failure");
    assert_eq!(error.outcome, None);
}

#[test]
fn duplicate_command_cannot_allocate_another_trade_request() {
    let mut e = seeded(venue(false));
    query(&mut e, 100., 71, "BTCUSDT", None);
    let (first, _) = released(&mut e, 100.);
    query(&mut e, 101., 71, "BTCUSDT", None);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
        )),
        "the original command owns its only transport/deadline"
    );
    assert!(terminals(&actions, 71).is_empty());
    response(&mut e, 102., first.id, 200, TRADE);
    assert_eq!(
        terminals(&e.drain_actions().collect::<Vec<_>>(), 71).len(),
        1
    );
}

#[test]
fn wire_limits_and_keyed_unsigned_requests_remain_native() {
    for keyed in [false, true] {
        for (limit, expected) in [(None, 100), (Some(0), 1), (Some(2), 2), (Some(999), 100)] {
            let mut e = seeded(venue(keyed));
            query(&mut e, 100., 71, "BTCUSDT", limit);
            let (request, _) = released(&mut e, 100.);
            assert_eq!(request.method, Method::Get);
            assert!(request.url.ends_with(&format!(
                "/api/v1/market/trades?instId=BTC-USDT&limit={expected}"
            )));
            assert!(request.headers.is_empty() && request.signer_headers.is_empty());
            assert_eq!(request.body, None);
            response(&mut e, 101., request.id, 200, r#"{"code":"0","data":[]}"#);
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert_eq!(terminals(&actions, 71), vec![Ok(json!([]))]);
            no_marker(&actions);
        }
    }
}

#[test]
fn native_cross_market_identity_defaults_order_and_wire_only_limit_are_preserved() {
    let mut e = seeded(venue(false));
    query(&mut e, 100., 71, "BTCUSDT", Some(1));
    let (request, _) = released(&mut e, 100.);
    response(
        &mut e,
        101.,
        request.id,
        200,
        r#"{"code":"0","data":[
        {"instId":"UNKNOWN","tradeId":"same","price":"1","size":"1","ts":"5000"},
        {"instId":"BTC-USDT","tradeId":"same","price":"1","size":"1","ts":"5000"},
        {"instId":"ETH-USDT","instType":"SPOT","tradeId":"cross","side":"sell","price":"20","size":"2","ts":"3000","createTime":"9999"},
        {"instId":"BTC-USDT","tradeId":"non-numeric","side":"SELL","price":"60000","size":"2","createTime":"1000"},
        {"instId":"BTC-USDT","tradeId":"defaults","ts":"2000"},
        {"instId":"BTC-USDT","tradeId":"negative","side":"sell","price":"-5","size":"-2","ts":"4000"}
    ]}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(
        terminals(&actions, 71),
        vec![Ok(json!([
            {"id":"cross","symbol":"ETHUSDT","price":20.0,"qty":0.2,"quoteQty":4.0,"timestamp":3000.0,"side":"sell"},
            {"id":"non-numeric","symbol":"BTCUSDT","price":60000.0,"qty":0.002,"quoteQty":120.0,"timestamp":1000.0,"side":"buy"},
            {"id":"defaults","symbol":"BTCUSDT","price":0.0,"qty":0.0,"quoteQty":0.0,"timestamp":2000.0,"side":"buy"},
            {"id":"negative","symbol":"BTCUSDT","price":-5.0,"qty":-0.002,"quoteQty":0.01,"timestamp":4000.0,"side":"sell"}
        ]))]
    );
    no_marker(&actions);
}

#[test]
fn rejected_or_malformed_first_pages_never_publish_the_streamed_prefix() {
    for body in [
        r#"{"code":"400","msg":"denied","data":[{"instId":"BTC-USDT","tradeId":"1","ts":"1000"}]}"#,
        r#"{"code":"0","data":[{"instId":"BTC-USDT","tradeId":"1","ts":"1000"},{"instId":]}"#,
        r#"{"code":"0","data":[7]}"#,
        r#"{"code":"0"}"#,
        r#"{"code":"0","data":null}"#,
        r#"{"code":"0","data":{}}"#,
    ] {
        let mut e = seeded(venue(false));
        query(&mut e, 100., 71, "BTCUSDT", None);
        let (request, _) = released(&mut e, 100.);
        response(&mut e, 101., request.id, 200, body);
        assert!(
            failed(&e.drain_actions().collect::<Vec<_>>(), 71)
                .kind
                .is_some()
        );
    }
}

#[test]
fn preflight_and_success_retire_the_original_owner_at_fixed_clock() {
    let mut e = seeded(venue(false));
    for _ in 0..270 {
        query(&mut e, 100., 71, "MISSING", None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        let error = failed(&actions, 71);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert_eq!(error.message, "Unknown market MISSING");
    }
    let mut previous = None;
    for _ in 0..270 {
        query(&mut e, 100., 71, "BTCUSDT", None);
        let (request, _) = released(&mut e, 100.);
        if let Some(old) = previous {
            response(&mut e, 100., old, 200, TRADE);
            assert!(terminals(&e.drain_actions().collect::<Vec<_>>(), 71).is_empty());
        }
        response(&mut e, 100., request.id, 200, r#"{"code":"0","data":[]}"#);
        assert_eq!(
            terminals(&e.drain_actions().collect::<Vec<_>>(), 71),
            vec![Ok(json!([]))]
        );
        previous = Some(request.id);
    }
}

struct FullQueue {
    venue: Blofin,
    filled: bool,
}

impl Venue for FullQueue {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error);
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.venue
            .on_stream_malformed(cx, pending, status, headers, error);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.venue.on_stream_error(cx, pending, response);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if !self.filled && matches!(command, Command::SetTickerSymbols { .. }) {
            let context = cx
                .http_context(
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
            for _ in 0..256 {
                assert!(
                    cx.request_managed(
                        context.clone(),
                        Method::Get,
                        "https://fixture.invalid/parked".into(),
                        None,
                        Pending::Tickers {
                            rows: vec![],
                            failed: false
                        },
                    )
                    .is_ok()
                );
            }
            self.filled = true;
        }
        self.venue.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
}

#[test]
fn immediate_managed_queue_refusal_is_typed_and_retires_each_read_owner() {
    let mut e = seeded(FullQueue {
        venue: venue(false),
        filled: false,
    });
    // Park unrelated work in its own event. The subsequent genuine Query must
    // enter with an untouched resource snapshot before it admits its Read root.
    e.handle(
        100.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    for attempt in 0..270 {
        query(&mut e, 100., 71, "BTCUSDT", None);
        let actions = e.drain_actions().collect::<Vec<_>>();
        let error = failed(&actions, 71);
        assert_eq!(error.kind, Some(ErrorKind::Internal));
        assert_eq!(
            error.outcome,
            Some(Outcome::NotSent),
            "entry must succeed before queue refusal"
        );
        assert_eq!(error.message, Error::Capacity.to_string());
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let prepares = actions
            .iter()
            .filter_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare { request, .. } => Some(request),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            prepares
                .iter()
                .all(|r| r.url == "https://fixture.invalid/parked")
        );
        assert_eq!(prepares.len(), if attempt == 0 { 256 } else { 0 });
    }
}

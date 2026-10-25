mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Owner, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Notice, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex, Pending};

const NOW: f64 = 1_700_000_000_000.;
const SYMBOL: &str = "BTC-USD-PERP";
const KLINES: &str =
    r#"{"results":[[120000,"1","3","0.5","2","4",99],[60000,"2","4","1","3","5"]]}"#;
const TV: &str = r#"{"s":"ok","t":[120,60],"o":["1","2"],"h":["3","4"],"l":["0.5","1"],"c":["2","3"],"v":["4","5"]}"#;

struct Probe {
    inner: Paradex,
    peers: Vec<Context>,
    socket: Option<SocketId>,
    preparation_failures: Vec<VenueError>,
    items: usize,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.socket = Some(socket);
        self.inner.on_open(cx, socket)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text)
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, data: &[u8]) {
        self.inner.on_binary(cx, socket, data)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.items += 1;
        self.inner.on_item(cx, pending, item)
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response)
    }
    fn on_stream_malformed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        status: u16,
        headers: &str,
        error: &str,
    ) {
        self.inner
            .on_stream_malformed(cx, pending, status, headers, error)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error)
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.preparation_failures.push(error.clone());
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn read_root_retired(&mut self, owner: Owner, pending: Vec<Pending>) {
        self.inner.read_root_retired(owner, pending)
    }
    fn on_public_feed(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.on_public_feed(cx)
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__hold" => {
                self.peers.push(
                    cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 120_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .expect("genuine peer command Read"),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["__retire"] => {
                let original = self.peers.remove(0);
                cx.fail_context(&original, "fixture peer retired").unwrap();
                for peer in &self.peers {
                    cx.validate_read_root_context(peer).unwrap();
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["__validate"] => {
                for peer in &self.peers {
                    cx.validate_read_root_context(peer).unwrap();
                }
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
type TestEngine = Engine<Probe>;
fn reply(
    e: &mut TestEngine,
    now: f64,
    request: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn boot(testnet: bool) -> TestEngine {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Paradex::new(Options {
            public_data_only: true,
            testnet,
            ..Default::default()
        }),
        peers: vec![],
        socket: None,
        preparation_failures: vec![],
        items: 0,
    });
    e.start(NOW);
    for _ in 0..8 {
        let actions = prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            if let Action::WsConnect { socket, .. } = action {
                e.handle(NOW, Event::WsOpened { socket });
            } else if let Action::Http(request) = action {
                let body = if request.url.ends_with("/markets") {
                    support::MARKETS
                } else if request.url.contains("/markets/summary") {
                    support::TICKERS
                } else {
                    panic!("unexpected startup request {}", request.url)
                };
                e.handle(
                    NOW,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body,
                        },
                    },
                );
            }
        }
    }
    assert!(e.store().market(SYMBOL).is_some());
    assert!(e.store().loaded().tickers);
    e.take_frame(NOW);
    e
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "poll must Prepare before HTTP"
    );
    actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("one managed unsigned poll preparation")
}
fn bind(
    e: &mut TestEngine,
    now: f64,
    token: Preparation,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
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
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared poll binds")
}
fn release(e: &mut TestEngine, now: f64, token: Preparation) -> HttpRequest {
    let (permit, request) = bind(e, now, token);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn native_packet(e: &mut TestEngine, now: f64, actions: Vec<Action>) -> HttpRequest {
    let raw: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r.clone())
            } else {
                None
            }
        })
        .collect();
    if !raw.is_empty() {
        assert_eq!(raw.len(), 1);
        raw[0].clone()
    } else {
        let (token, _) = preparation(&actions);
        release(e, now, token)
    }
}
fn subscribe(e: &mut TestEngine, now: f64, interval: Interval) -> Vec<Action> {
    event(
        e,
        now,
        Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval,
        },
    )
}
fn event(e: &mut TestEngine, now: f64, command: Command) -> Vec<Action> {
    e.handle(now, Event::Command(command));
    e.drain_actions().collect()
}
fn unsubscribe(e: &mut TestEngine, now: f64, interval: Interval) {
    event(
        e,
        now,
        Command::UnsubscribeCandles {
            symbol: SYMBOL.into(),
            interval,
        },
    );
}
fn no_http(actions: &[Action]) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))
            || matches!(a,Action::ManagedHttp(a) if matches!(&**a,HttpAction::Prepare{..}))),
        "no HTTP or next preparation: {actions:?}"
    );
}
fn no_bind(actions: &[Action]) {
    assert!(!actions.iter().any(|a|matches!(a,Action::ManagedHttp(a) if matches!(&**a,HttpAction::Bind{..}|HttpAction::BindGroup{..}))), "no late Bind: {actions:?}");
}
fn no_cycle_effects(actions: &[Action]) {
    no_http(actions);
    no_bind(actions);
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            Action::SetTimer { .. } | Action::WsSend { .. } | Action::Notice(_)
        )),
        "stale/invalid cycle has no timer, recovery or notice: {actions:?}"
    );
}
fn timer(actions: &[Action], at: f64) -> TimerId {
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "one timer at {at}: {actions:?}");
    timers[0]
}
fn fire(e: &mut TestEngine, now: f64, timer: TimerId) -> Vec<Action> {
    e.handle(now, Event::TimerFired { timer });
    e.drain_actions().collect()
}
fn candles(e: &mut TestEngine, now: f64) -> Vec<[f64; 6]> {
    let Some(frame) = e.take_frame(now) else {
        return vec![];
    };
    safe_cex_engine::wire::sections(frame)
        .filter(|(kind, _)| *kind == safe_cex_engine::wire::section::CANDLES)
        .map(|(_, body)| {
            assert_eq!(u32::from_le_bytes(body[..4].try_into().unwrap()), 1);
            std::array::from_fn(|i| {
                f64::from_le_bytes(body[16 + i * 8..24 + i * 8].try_into().unwrap())
            })
        })
        .collect()
}
fn no_candles(e: &mut TestEngine, now: f64) {
    assert!(candles(e, now).is_empty(), "no candle publication");
}
fn expect_last(e: &mut TestEngine, now: f64) {
    assert_eq!(candles(e, now), vec![[60000., 2., 4., 1., 3., 5.]]);
}
fn failed(e: &mut TestEngine, now: f64, r: &HttpRequest) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpFailed {
            request: r.id,
            error: "fixture network",
        },
    );
    e.drain_actions().collect()
}
fn trade(e: &mut TestEngine, now: f64) {
    e.handle(now,Event::WsMessage{socket:e.venue().socket.unwrap(),text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"trades.BTC-USD-PERP","data":{"price":"111","size":"2","created_at":1700000000000}}}"#});
    e.drain_actions().for_each(drop);
}
fn validate_peers(e: &mut TestEngine, now: f64, expected: usize) {
    assert_eq!(e.venue().peers.len(), expected);
    no_http(&event(
        e,
        now,
        Command::SetTickerSymbols {
            symbols: vec!["__validate".into()],
        },
    ));
}

#[test]
fn native_documented_both_hosts_wire_and_last_row() {
    for testnet in [false, true] {
        let mut e = boot(testnet);
        let a = subscribe(&mut e, NOW, Interval::M1);
        let r = native_packet(&mut e, NOW, a);
        let host = if testnet {
            "api.testnet.paradex.trade"
        } else {
            "api.prod.paradex.trade"
        };
        assert_eq!(
            r.url,
            format!(
                "https://{host}/v1/markets/klines?symbol=BTC-USD-PERP&resolution=1&start_at=1699999820000&end_at=1700000000000&price_kind=last"
            )
        );
        assert!(r.headers.is_empty() && r.body.is_none());
        assert_eq!(r.method, safe_cex_engine::Method::Get);
        let out = reply(&mut e, NOW + 1., &r, 200, KLINES);
        expect_last(&mut e, NOW + 1.);
        timer(&out, NOW + 3001.);
        no_http(&out);
    }
}
#[test]
fn native_wide_route_floors_seconds_and_uses_last_parallel_row() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW + 0.75, Interval::H4);
    let r = native_packet(&mut e, NOW + 0.75, a);
    assert_eq!(
        r.url,
        "https://api.prod.paradex.trade/v1/tradingview/history?symbol=BTC-USD-PERP&resolution=240&from=1699956800&to=1700000000&countback=2000&price_kind=last"
    );
    let out = reply(&mut e, NOW + 1., &r, 200, TV);
    expect_last(&mut e, NOW + 1.);
    timer(&out, NOW + 3001.);
}
#[test]
fn native_status_envelope_and_row_failures_fallback_once_without_prefix() {
    for (status, body) in [
        (503, "{}"),
        (200, r#"{"message":"absent"}"#),
        (200, r#"{"results":[[1,2,3,4,5,6],[1]]}"#),
        (
            200,
            r#"{"results":[[1,2,3,4,5,6]],"error":{},"message":"bad"}"#,
        ),
    ] {
        let mut e = boot(false);
        let a = subscribe(&mut e, NOW, Interval::M1);
        let first = native_packet(&mut e, NOW, a);
        let a = reply(&mut e, NOW + 1., &first, status, body);
        no_candles(&mut e, NOW + 1.);
        let second = native_packet(&mut e, NOW + 1., a);
        assert!(
            second
                .url
                .ends_with("&from=1699999820&to=1700000000&countback=2000&price_kind=last")
        );
        let out = reply(&mut e, NOW + 2., &second, 200, TV);
        expect_last(&mut e, NOW + 2.);
        timer(&out, NOW + 3002.);
        no_http(&out);
    }
}
#[test]
fn native_transport_fallback_final_warning_and_no_third_call() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = native_packet(&mut e, NOW, a);
    let a = failed(&mut e, NOW + 1., &r);
    let r = native_packet(&mut e, NOW + 1., a);
    let out = reply(&mut e, NOW + 2., &r, 503, "{}");
    assert!(out.iter().any(
        |a| matches!(a,Action::Notice(Notice::Event(event)) if event.message=="Paradex HTTP 503")
    ));
    timer(&out, NOW + 3002.);
    no_http(&out);
    no_candles(&mut e, NOW + 2.);
}
#[test]
fn native_empty_and_malformed_tradingview_schedule_completion_plus_three_seconds() {
    for body in [
        r#"{"s":"no_data"}"#,
        r#"{"s":"ok","t":[1],"o":[],"h":[],"l":[],"c":[],"v":[]}"#,
    ] {
        let mut e = boot(false);
        let a = subscribe(&mut e, NOW, Interval::H4);
        let r = native_packet(&mut e, NOW, a);
        let out = reply(&mut e, NOW + 7., &r, 200, body);
        timer(&out, NOW + 3007.);
        no_http(&out);
        no_candles(&mut e, NOW + 7.);
    }
}
#[test]
fn native_reference_count_and_removed_generation_success_are_preserved() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let old = native_packet(&mut e, NOW, a);
    no_http(&subscribe(&mut e, NOW, Interval::M1));
    unsubscribe(&mut e, NOW, Interval::M1);
    let a = reply(&mut e, NOW + 1., &old, 200, KLINES);
    expect_last(&mut e, NOW + 1.);
    let t = timer(&a, NOW + 3001.);
    let a = fire(&mut e, NOW + 3001., t);
    let old = native_packet(&mut e, NOW + 3001., a);
    unsubscribe(&mut e, NOW + 3001., Interval::M1);
    let a = subscribe(&mut e, NOW + 3001., Interval::M1);
    let current = native_packet(&mut e, NOW + 3001., a);
    no_cycle_effects(&reply(&mut e, NOW + 3002., &old, 200, KLINES));
    no_candles(&mut e, NOW + 3002.);
    timer(
        &reply(&mut e, NOW + 3003., &current, 200, r#"{"results":[]}"#),
        NOW + 6003.,
    );
}
#[test]
fn revision_change_suppresses_publication_but_not_completion_cleanup() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = native_packet(&mut e, NOW, a);
    trade(&mut e, NOW + 1.);
    assert_eq!(
        candles(&mut e, NOW + 1.),
        vec![[1699999980000., 111., 111., 111., 111., 2.]]
    );
    let out = reply(&mut e, NOW + 2., &r, 200, KLINES);
    no_candles(&mut e, NOW + 2.);
    let t = timer(&out, NOW + 3002.);
    let a = fire(&mut e, NOW + 3002., t);
    let r = native_packet(&mut e, NOW + 3002., a);
    assert!(r.url.contains("start_at=1699999823002"));
}
#[test]
fn initial_documented_poll_prepares_before_http() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, r) = preparation(&a);
    assert_eq!(r.timeout_ms, Some(20000));
    let r = release(&mut e, NOW, token);
    expect_last_after(&mut e, NOW + 1., &r, KLINES);
}
fn expect_last_after(e: &mut TestEngine, now: f64, r: &HttpRequest, body: &str) {
    let out = reply(e, now, r, 200, body);
    expect_last(e, now);
    timer(&out, now + 3000.);
}
#[test]
fn initial_tradingview_poll_prepares_before_http() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::H4);
    let (token, r) = preparation(&a);
    assert_eq!(r.timeout_ms, Some(20000));
    let r = release(&mut e, NOW, token);
    expect_last_after(&mut e, NOW + 1., &r, TV);
}
#[test]
fn fallback_gets_fresh_context_and_preparation() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let first = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &first, 503, "{}");
    let (token, r) = preparation(&a);
    assert_ne!(r.id, first.id);
    assert_eq!(r.timeout_ms, Some(20000));
    let second = release(&mut e, NOW + 1., token);
    expect_last_after(&mut e, NOW + 2., &second, TV);
}
#[test]
fn held_prepare_expires_strictly_and_late_prepared_cannot_bind() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, _) = preparation(&a);
    let t = timer(&a, NOW + 20000.);
    let out = fire(&mut e, NOW + 20000., t);
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW + 20000.);
    timer(&out, NOW + 23000.);
    e.http_prepared(
        NOW + 20001.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    no_cycle_effects(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn unclaimed_bind_expires_without_fallback_or_publication() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, _) = preparation(&a);
    let (permit, _) = bind(&mut e, NOW + 1., token);
    let out = fire(&mut e, NOW + 20000., timer(&a, NOW + 20000.));
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW + 20000.);
    timer(&out, NOW + 23000.);
    assert!(e.claim_http(NOW + 20001., permit).is_err());
}
#[test]
fn unclaimed_data_cannot_publish_or_schedule() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = a
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            Action::ManagedHttp(a) => match &**a {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let out = reply(&mut e, NOW + 1., &r, 200, KLINES);
    no_cycle_effects(&out);
    no_candles(&mut e, NOW + 1.);
}
#[test]
fn released_first_timeout_uses_a_fresh_fallback() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, _) = preparation(&a);
    let first = release(&mut e, NOW, token);
    let out = fire(&mut e, NOW + 20000., timer(&a, NOW + 20000.));
    let (token, r) = preparation(&out);
    assert!(r.url.contains("tradingview"));
    let second = release(&mut e, NOW + 20000., token);
    no_cycle_effects(&failed(&mut e, NOW + 20000., &first));
    expect_last_after(&mut e, NOW + 20001., &second, TV);
}
#[test]
fn released_second_timeout_cleans_up_without_third_request() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let first = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &first, 503, "{}");
    let (token, _) = preparation(&a);
    let second = release(&mut e, NOW + 1., token);
    let out = fire(&mut e, NOW + 20001., timer(&a, NOW + 20001.));
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW + 20001.);
    timer(&out, NOW + 23001.);
    no_cycle_effects(&reply(&mut e, NOW + 20002., &second, 200, TV));
}
#[test]
fn typed_preparation_refusal_is_strict_and_releases_its_credit() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, _) = preparation(&a);
    e.http_preparation_failed(
        NOW + 1.,
        token,
        Error::Invalid,
        VenueError::new("fixture route refused")
            .kind(ErrorKind::Unsupported)
            .venue_code("fixture.route"),
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    let error = e
        .venue()
        .preparation_failures
        .last()
        .expect("real typed preparation callback");
    assert_eq!(error.message, "fixture route refused");
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code.as_deref(), Some("fixture.route"));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW + 1.);
    timer(&out, NOW + 3001.);
    e.http_prepared(
        NOW + 2.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    no_cycle_effects(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn one_background_credit_recovers_under_255_genuine_command_roots() {
    let mut e = boot(false);
    for id in 1000..1256 {
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command: id,
                symbol: "__hold".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(120000),
        )
        .unwrap();
        assert!(e.drain_actions().next().is_none());
    }
    validate_peers(&mut e, NOW, 256);
    let out = subscribe(&mut e, NOW, Interval::M1);
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW);
    let t = timer(&out, NOW + 3000.);
    event(
        &mut e,
        NOW,
        Command::SetTickerSymbols {
            symbols: vec!["__retire".into()],
        },
    );
    validate_peers(&mut e, NOW, 255);
    let a = fire(&mut e, NOW + 3000., t);
    let (token, _) = preparation(&a);
    let r = release(&mut e, NOW + 3000., token);
    expect_last_after(&mut e, NOW + 3001., &r, KLINES);
    validate_peers(&mut e, NOW + 3001., 255);
}
#[test]
fn nan_admission_emits_no_remote_or_nonfinite_cleanup() {
    nonfinite_entry(f64::NAN);
}
#[test]
fn infinity_admission_emits_no_remote_or_nonfinite_cleanup() {
    nonfinite_entry(f64::INFINITY);
}
fn nonfinite_entry(now: f64) {
    let mut e = boot(false);
    let out = subscribe(&mut e, now, Interval::M1);
    no_http(&out);
    no_bind(&out);
    assert!(
        !out.iter()
            .any(|a| matches!(a, Action::SetTimer { .. } | Action::WsSend { .. })),
        "invalid clock cannot schedule poll/recovery: {out:?}"
    );
    no_candles(&mut e, NOW);
}
#[test]
fn removed_generation_failure_cannot_start_fallback_or_recovery() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let old = native_packet(&mut e, NOW, a);
    unsubscribe(&mut e, NOW + 1., Interval::M1);
    no_cycle_effects(&failed(&mut e, NOW + 2., &old));
    no_candles(&mut e, NOW + 2.);
}
#[test]
fn retired_generation_preparation_cannot_bind() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let (token, _) = preparation(&a);
    unsubscribe(&mut e, NOW + 1., Interval::M1);
    let current = subscribe(&mut e, NOW + 1., Interval::M1);
    e.http_prepared(
        NOW + 20000.,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    no_cycle_effects(&out);
    let (token, _) = preparation(&current);
    let r = release(&mut e, NOW + 20000., token);
    expect_last_after(&mut e, NOW + 20000., &r, KLINES);
}
#[test]
fn staged_rows_cannot_cross_the_physical_deadline() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = native_packet(&mut e, NOW, a);
    let before = e.venue().items;
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 2.,
        Event::HttpChunk {
            request: r.id,
            data: KLINES.as_bytes(),
        },
    );
    assert_eq!(
        e.venue().items - before,
        2,
        "two actual streamed items before expiry"
    );
    no_candles(&mut e, NOW + 2.);
    e.handle(NOW + 20000., Event::HttpEnd { request: r.id });
    let out = e.drain_actions().collect::<Vec<_>>();
    no_candles(&mut e, NOW + 20000.);
    let (token, _) = preparation(&out);
    let r = release(&mut e, NOW + 20000., token);
    expect_last_after(&mut e, NOW + 20001., &r, TV);
}
#[test]
fn broken_and_non_utf8_first_streams_fallback_once() {
    for bytes in [
        b"{\"results\":[[120000,1,3,0.5,2,4]],".as_slice(),
        b"\xff".as_slice(),
    ] {
        let mut e = boot(false);
        let a = subscribe(&mut e, NOW, Interval::M1);
        let r = native_packet(&mut e, NOW, a);
        e.handle(
            NOW + 1.,
            Event::HttpStart {
                request: r.id,
                status: 200,
                headers: "",
            },
        );
        e.handle(
            NOW + 1.,
            Event::HttpChunk {
                request: r.id,
                data: bytes,
            },
        );
        e.handle(NOW + 1., Event::HttpEnd { request: r.id });
        let a = e.drain_actions().collect();
        no_candles(&mut e, NOW + 1.);
        let r = native_packet(&mut e, NOW + 1., a);
        let out = reply(&mut e, NOW + 2., &r, 200, TV);
        expect_last(&mut e, NOW + 2.);
        timer(&out, NOW + 3002.);
    }
}

fn recovery(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a,Action::WsSend{text,..} if text.contains("trades.BTC-USD-PERP")))
}
#[test]
fn native_trade_silence_recovery_is_strictly_after_twenty_seconds() {
    for (delta, expected) in [(20000., false), (20000.5, true)] {
        let mut e = boot(false);
        let a = subscribe(&mut e, NOW, Interval::M1);
        let r = native_packet(&mut e, NOW, a);
        trade(&mut e, NOW);
        candles(&mut e, NOW);
        let a = reply(&mut e, NOW + 1., &r, 200, r#"{"results":[]}"#);
        assert!(!recovery(&a));
        let a = fire(&mut e, NOW + 3001., timer(&a, NOW + 3001.));
        let r = native_packet(&mut e, NOW + 3001., a);
        let out = reply(&mut e, NOW + delta, &r, 200, r#"{"results":[]}"#);
        assert_eq!(recovery(&out), expected);
        timer(&out, NOW + delta + 3000.);
    }
}
#[test]
fn native_recovery_refresh_uses_inclusive_thirty_second_spacing() {
    for (delta, expected) in [(30000., false), (30001., true)] {
        let mut e = boot(false);
        let a = subscribe(&mut e, NOW, Interval::M1);
        let r = native_packet(&mut e, NOW, a);
        let a = reply(&mut e, NOW + 1., &r, 200, r#"{"results":[]}"#);
        assert!(recovery(&a));
        let a = fire(&mut e, NOW + 3001., timer(&a, NOW + 3001.));
        let r = native_packet(&mut e, NOW + 3001., a);
        let a = reply(&mut e, NOW + 15001., &r, 200, r#"{"results":[]}"#);
        assert!(!recovery(&a));
        let a = fire(&mut e, NOW + 18001., timer(&a, NOW + 18001.));
        let r = native_packet(&mut e, NOW + 18001., a);
        let out = reply(&mut e, NOW + delta, &r, 200, r#"{"results":[]}"#);
        assert_eq!(recovery(&out), expected);
        timer(&out, NOW + delta + 3000.);
    }
}
#[test]
fn native_pre_market_subscription_starts_when_discovery_finishes() {
    let mut h = support::Harness::empty(Options {
        public_data_only: true,
        ..Default::default()
    });
    h.command(Command::SubscribeCandles {
        symbol: SYMBOL.into(),
        interval: Interval::M1,
    });
    assert!(!h.pending.iter().any(|r| r.url.contains("klines")));
    h.reply("/markets", support::MARKETS);
    let r = h.take("klines");
    assert!(
        r.url
            .ends_with("resolution=1&start_at=1699999820000&end_at=1700000000000&price_kind=last")
    );
    h.respond(r, 200, KLINES);
    h.timer_at(NOW + 3000.);
}
#[test]
fn removed_generation_broken_stream_does_not_fallback_or_restart() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = native_packet(&mut e, NOW, a);
    let before = e.venue().items;
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 1.,
        Event::HttpChunk {
            request: r.id,
            data: br#"{"results":[[120000,1,3,0.5,2,4]],"#,
        },
    );
    assert_eq!(e.venue().items - before, 1);
    unsubscribe(&mut e, NOW + 2., Interval::M1);
    e.handle(NOW + 3., Event::HttpEnd { request: r.id });
    no_cycle_effects(&e.drain_actions().collect::<Vec<_>>());
    no_candles(&mut e, NOW + 3.);
}
#[test]
fn later_tradingview_preparation_refusal_is_strict() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let r = native_packet(&mut e, NOW, a);
    let a = reply(&mut e, NOW + 1., &r, 503, "{}");
    let (token, _) = preparation(&a);
    e.http_preparation_failed(
        NOW + 2.,
        token,
        Error::Invalid,
        VenueError::new("fixture second route denied"),
    );
    let out = e.drain_actions().collect::<Vec<_>>();
    no_http(&out);
    no_bind(&out);
    no_candles(&mut e, NOW + 2.);
    timer(&out, NOW + 3002.);
}
#[test]
fn current_generation_revision_change_still_runs_recovery_cleanup() {
    let mut e = boot(false);
    let a = subscribe(&mut e, NOW, Interval::M1);
    let first = native_packet(&mut e, NOW, a);
    trade(&mut e, NOW + 1.);
    candles(&mut e, NOW + 1.);
    let a = reply(&mut e, NOW + 19999., &first, 503, "{}");
    let second = native_packet(&mut e, NOW + 19999., a);
    let out = reply(&mut e, NOW + 20002., &second, 200, TV);
    assert!(recovery(&out));
    no_candles(&mut e, NOW + 20002.);
    timer(&out, NOW + 23002.);
}
#[test]
fn native_all_declared_nonsecond_intervals_keep_route_selection() {
    for label in [
        "1m", "3m", "5m", "15m", "30m", "1h", "2h", "3h", "4h", "5h", "6h", "7h", "8h", "9h",
        "10h", "11h", "12h", "24h", "1d", "2d", "3d", "1w", "1M",
    ] {
        let mut e = boot(false);
        let interval = Interval::parse(label).unwrap();
        let a = subscribe(&mut e, NOW, interval);
        let r = native_packet(&mut e, NOW, a);
        let documented = matches!(label, "1m" | "3m" | "5m" | "15m" | "30m" | "1h");
        assert_eq!(r.url.contains("/markets/klines"), documented, "{label}");
        let body = if documented {
            r#"{"results":[]}"#
        } else {
            r#"{"s":"no_data"}"#
        };
        let out = reply(&mut e, NOW + 1., &r, 200, body);
        timer(&out, NOW + 3001.);
        no_http(&out);
    }
}

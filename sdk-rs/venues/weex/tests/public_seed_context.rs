//! Public seed custody originals; the wrapper only holds genuine peer contexts.
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent, Scope,
};
use safe_cex_engine::{
    Action, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice, RequestId,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_weex::{Pending, WeexFutures};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use support::setup::Options;

const NOW: f64 = 1_000.;
const LIMIT: f64 = 20_000.;
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"2","volume":"3","quoteVolume":"300"}]"#;
const BOOK: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"99","askPrice":"101"}]"#;
const BLANK_BOOK: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"","askPrice":""}]"#;
const PATHS: [&str; 3] = ["exchangeInfo", "24hr", "bookTicker"];

struct Probe {
    inner: WeexFutures,
    peers: Vec<Context>,
    initial_peers: usize,
    start_native: bool,
    failures: Arc<Mutex<Vec<VenueError>>>,
}
impl Probe {
    fn fill(&mut self, cx: &mut Cx<'_, Pending>, count: usize) {
        for _ in 0..count {
            self.peers.push(
                cx.http_context(
                    Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 100_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .expect("genuine finite Background peer admission"),
            );
        }
    }
    fn release(&mut self, cx: &mut Cx<'_, Pending>) {
        for peer in self.peers.drain(..) {
            cx.finish_http_context(&peer).unwrap();
        }
    }
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = Pending;
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.fill(cx, self.initial_peers);
        if self.start_native {
            self.inner.start(cx);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("fixture:boot") => self.start(cx),
                Some("fixture:start") => self.inner.start(cx),
                Some("fixture:release") => self.release(cx),
                Some("fixture:validate") => {
                    for peer in &self.peers {
                        cx.validate_http_context(peer).unwrap();
                    }
                }
                Some("fixture:credits") => {
                    assert!(self.peers.is_empty());
                    self.fill(cx, 256);
                    assert_eq!(self.peers.len(), 256, "all physical credits returned");
                    self.release(cx);
                }
                _ => self.inner.on_command(cx, command),
            }
        } else {
            self.inner.on_command(cx, command);
        }
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
        self.inner.on_item(cx, p, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.inner.on_response(cx, p, r);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
        self.inner.on_request_failed(cx, p, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
        self.failures.lock().unwrap().push(error.clone());
        self.inner.on_http_failed(cx, p, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        p: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.failures.lock().unwrap().push(error.clone());
        self.inner.on_http_preparation_failed(cx, p, reason, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}
type Driver = Engine<Probe>;
fn engine(peers: usize, start_native: bool) -> (Driver, Arc<Mutex<Vec<VenueError>>>) {
    let failures = Arc::new(Mutex::new(Vec::new()));
    let errors = failures.clone();
    let e = support::setup::with_venue(
        Options {
            public_data_only: true,
            ..Default::default()
        },
        |inner| Probe {
            inner,
            peers: Vec::new(),
            initial_peers: peers,
            start_native,
            failures: errors,
        },
    );
    (e, failures)
}
fn control(e: &mut Driver, now: f64, name: &str) {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![format!("fixture:{name}")],
        }),
    );
}
#[derive(Default)]
struct Output {
    requests: Vec<HttpRequest>,
    prepares: Vec<(Preparation, HttpRequest, RoutingIntent)>,
    actions: Vec<Action>,
    resources: Vec<Resource>,
    cleanup: Vec<Action>,
    legacy: usize,
    claimed: usize,
}
#[derive(Clone, Copy, Debug)]
struct Resource {
    request: RequestId,
    preparation: Preparation,
    timer: TimerId,
}
fn resources(actions: &[Action]) -> Vec<Resource> {
    let mut timer = None;
    let mut found = Vec::new();
    for action in actions {
        match action {
            Action::SetTimer { timer: current, .. } => timer = Some(*current),
            Action::ManagedHttp(action) => {
                if let HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } = action.as_ref()
                {
                    // Managed issuance emits its physical timer before Prepare.
                    // Both values come from this actual action trace, never next IDs.
                    found.push(Resource {
                        request: request.id,
                        preparation: *preparation,
                        timer: timer
                            .take()
                            .expect("observed physical timer before Prepare"),
                    });
                }
            }
            _ => {}
        }
    }
    found
}
fn resource(out: &Output, id: RequestId) -> Option<Resource> {
    out.resources.iter().find(|r| r.request == id).copied()
}
fn drain(e: &mut Driver, now: f64) -> Output {
    let mut out = Output::default();
    let initial: Vec<_> = e.drain_actions().collect();
    out.resources = resources(&initial);
    let mut queue: VecDeque<_> = initial.into();
    while let Some(a) = queue.pop_front() {
        if matches!(&a, Action::ClearTimer { .. })
            || matches!(&a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { .. }))
        {
            out.cleanup.push(a.clone());
        }
        match a {
            Action::Http(r) => {
                out.legacy += 1;
                out.requests.push(r);
            }
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    intent,
                    request,
                } => {
                    out.prepares.push((preparation, request.clone(), intent));
                    e.http_prepared(
                        now,
                        preparation,
                        Prepared {
                            id: request.id.0,
                            expires_at: None,
                        },
                    );
                }
                HttpAction::Bind {
                    permit, request, ..
                } => {
                    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                    out.claimed += 1;
                    out.requests.push(request);
                }
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => panic!("no seed mutation barriers"),
            },
            a => out.actions.push(a),
        }
        queue.extend(e.drain_actions());
    }
    assert!(
        !out.actions.iter().any(|a| matches!(
            a,
            Action::Sign { .. }
                | Action::SignPayload { .. }
                | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )),
        "public seed has no signing or command authority"
    );
    out
}
fn respond(e: &mut Driver, r: &HttpRequest, now: f64, status: u16, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn request(out: &Output, kind: usize) -> HttpRequest {
    let mut requests = out.requests.iter().filter(|r| r.url.ends_with(PATHS[kind]));
    let r = requests.next().expect("one native seed request").clone();
    assert!(requests.next().is_none());
    r
}
fn started(kind: usize) -> (Driver, HttpRequest, Output) {
    started_with_watch(kind, true)
}
fn started_with_watch(kind: usize, watch: bool) -> (Driver, HttpRequest, Output) {
    started_with_peers(kind, watch, 0)
}
fn started_with_peers(kind: usize, watch: bool, peers: usize) -> (Driver, HttpRequest, Output) {
    let (mut e, _) = engine(peers, true);
    if watch {
        e.start(NOW);
    } else {
        control(&mut e, NOW, "boot");
    }
    let out = drain(&mut e, NOW);
    let r = request(&out, 0);
    if kind == 0 {
        return (e, r, out);
    }
    respond(&mut e, &r, NOW + 1., 200, INFO);
    let out = drain(&mut e, NOW + 1.);
    let r = request(&out, kind);
    (e, r, out)
}
fn no_new_work(out: &[Action]) {
    assert!(
        out.is_empty(),
        "unissued work owns no cleanup or other action: {out:?}"
    );
}
#[derive(Clone, Debug, Default)]
struct Cleanup {
    timers: Vec<TimerId>,
    preparations: Vec<Preparation>,
}
impl Cleanup {
    fn issued(resource: Option<Resource>) -> Self {
        resource.map_or_else(Self::default, |r| Self {
            timers: vec![r.timer],
            preparations: vec![r.preparation],
        })
    }
    fn remaining(&self, seen: &Self) -> Self {
        Self {
            timers: self
                .timers
                .iter()
                .copied()
                .filter(|t| !seen.timers.contains(t))
                .collect(),
            preparations: self
                .preparations
                .iter()
                .copied()
                .filter(|p| !seen.preparations.contains(p))
                .collect(),
        }
    }
    fn complete(&self, seen: &Self) {
        assert!(
            self.remaining(seen).timers.is_empty(),
            "missing exact timer cleanup"
        );
        assert!(
            self.remaining(seen).preparations.is_empty(),
            "missing exact preparation cleanup"
        );
    }
}
fn only_cleanup(out: &[Action], allowed: &Cleanup) -> Cleanup {
    let mut seen = Cleanup::default();
    for action in out {
        match action {
            Action::ClearTimer { timer }
                if allowed.timers.contains(timer) && !seen.timers.contains(timer) =>
            {
                seen.timers.push(*timer)
            }
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Cancel {
                    preparation,
                    signing: None,
                } if allowed.preparations.contains(preparation)
                    && !seen.preparations.contains(preparation) =>
                {
                    seen.preparations.push(*preparation)
                }
                _ => panic!("unexpected or unrelated cleanup/work: {out:?}"),
            },
            _ => panic!("unexpected or unrelated cleanup/work: {out:?}"),
        }
    }
    seen
}
fn store_snapshot(e: &Driver) -> Vec<u8> {
    let store = e.store();
    let mut frame = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 0.);
    store.write(&store.everything(), &mut frame);
    frame.finish()
}
struct Independent {
    command: u32,
    request: HttpRequest,
    resource: Resource,
    cleanup: Cleanup,
}
fn independent(e: &mut Driver) -> Independent {
    independent_with_command(e, 991)
}
fn independent_with_command(e: &mut Driver, command: u32) -> Independent {
    e.handle_command(
        NOW + 1.5,
        Command::FetchOrderBook {
            command,
            symbol: "INDEPENDENT".into(),
            limit: Some(1),
        },
        Some(100_000),
    )
    .unwrap();
    let out = drain(e, NOW + 1.5);
    assert_eq!(out.requests.len(), 1);
    let request = out.requests[0].clone();
    let resource = resource(&out, request.id).expect("actual independent managed request");
    assert!(e.http_release(request.id).is_some());
    let mut cleanup = Cleanup::issued(Some(resource));
    for action in out.actions {
        match action {
            Action::SetTimer { timer, .. } if !cleanup.timers.contains(&timer) => {
                cleanup.timers.push(timer)
            }
            Action::ClearTimer { timer } => cleanup.timers.retain(|id| *id != timer),
            _ => {}
        }
    }
    Independent {
        command,
        request,
        resource,
        cleanup,
    }
}
fn complete_independent(e: &mut Driver, read: Independent) {
    assert!(
        e.http_release(read.request.id).is_some(),
        "unrelated released sibling was canceled"
    );
    let before = store_snapshot(e);
    respond(
        e,
        &read.request,
        NOW + 6.,
        200,
        r#"{"lastUpdateId":"77","bids":[["1","2"]],"asks":[]}"#,
    );
    let mut cleanup = Vec::new();
    let mut done = 0;
    for action in e.drain_actions() {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) if command == read.command => {
                done += 1;
                assert_eq!(
                    result,
                    r#"{"bids":[{"price":1.0,"amount":2.0,"total":2.0}],"asks":[],"lastUpdateId":"77"}"#
                );
            }
            action => cleanup.push(action),
        }
    }
    assert_eq!(done, 1, "unrelated managed read stays usable");
    let seen = only_cleanup(&cleanup, &read.cleanup);
    read.cleanup.complete(&seen);
    assert!(seen.preparations.contains(&read.resource.preparation));
    assert!(e.http_release(read.request.id).is_none());
    assert_eq!(
        store_snapshot(e),
        before,
        "book query cannot mutate any Store/readiness section"
    );
}
fn retry(out: &[Action], now: f64) -> TimerId {
    out.iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if (750. ..=1000.).contains(&(*at_ms - now)) => {
                Some(*timer)
            }
            _ => None,
        })
        .expect("one native first retry, without inline HTTP")
}

#[test]
fn catalogue_requires_prepare_before_any_http() {
    let (mut e, _) = engine(0, true);
    e.start(NOW);
    let out: Vec<_> = e.drain_actions().collect();
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "catalogue escaped before Claim"
    );
    assert_eq!(out.iter().filter(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. }))).count(), 1);
}
#[test]
fn ticker_pair_requires_two_prepares_before_any_http() {
    let (mut e, r, _) = started(0);
    respond(&mut e, &r, NOW + 1., 200, INFO);
    let out: Vec<_> = e.drain_actions().collect();
    assert!(e.store().loaded().markets);
    assert!(
        !out.iter().any(|a| matches!(a, Action::Http(_))),
        "ticker pair escaped before Claim"
    );
    assert_eq!(out.iter().filter(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Prepare { .. }))).count(), 2);
}
#[test]
fn released_wire_is_unsigned_primary_with_twenty_second_bound() {
    for kind in 0..3 {
        let (_, r, out) = started(kind);
        assert_eq!(r.method, Method::Get);
        assert_eq!(
            r.url,
            format!(
                "https://api-contract.weex.com/capi/v3/market/{}",
                ["exchangeInfo", "ticker/24hr", "ticker/bookTicker"][kind]
            )
        );
        assert!(r.route.is_none());
        assert!(r.headers.is_empty());
        assert!(r.signer_headers.is_empty());
        assert!(r.body.is_none());
        assert_eq!(r.timeout_ms, Some(20_000));
        assert_eq!(out.legacy, 0);
        assert_eq!(out.prepares.len(), out.requests.len());
        assert_eq!(out.claimed, out.requests.len());
        assert!(
            out.prepares
                .iter()
                .all(|(_, _, intent)| *intent == RoutingIntent::Primary)
        );
    }
}
#[test]
fn native_seed_preserves_both_response_orders_and_blank_book_fallback() {
    for book_first in [false, true] {
        for blank in [false, true] {
            let (mut e, _, out) = started(1);
            let daily = request(&out, 1);
            let book = request(&out, 2);
            let book_body = if blank { BLANK_BOOK } else { BOOK };
            let order = if book_first {
                [(&book, book_body), (&daily, DAILY)]
            } else {
                [(&daily, DAILY), (&book, book_body)]
            };
            respond(&mut e, order[0].0, NOW + 2., 200, order[0].1);
            let first = drain(&mut e, NOW + 2.);
            assert!(!e.store().loaded().tickers);
            assert!(
                !first
                    .actions
                    .iter()
                    .any(|a| matches!(a, Action::WsConnect { .. }))
            );
            respond(&mut e, order[1].0, NOW + 3., 200, order[1].1);
            let last = drain(&mut e, NOW + 3.);
            let t = e.store().ticker("BTCUSDT").unwrap();
            assert_eq!(t.last, 100.);
            assert_eq!(t.bid, if blank { 100. } else { 99. });
            assert_eq!(t.ask, if blank { 100. } else { 101. });
            assert!(e.store().loaded().tickers);
            assert_eq!(
                last.actions
                    .iter()
                    .filter(|a| matches!(a, Action::WsConnect { .. }))
                    .count(),
                1
            );
        }
    }
}
#[test]
fn native_book_only_rows_do_not_publish_a_ticker_seed() {
    let (mut e, _, out) = started(1);
    respond(&mut e, &request(&out, 1), NOW + 2., 200, "[]");
    drain(&mut e, NOW + 2.);
    respond(&mut e, &request(&out, 2), NOW + 3., 200, BOOK);
    let out = drain(&mut e, NOW + 3.);
    assert!(!e.store().loaded().tickers);
    assert!(e.store().tickers().is_empty());
    assert!(
        !out.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    retry(&out.actions, NOW + 3.);
}
#[test]
fn finite_fixture_entry_matches_real_start_without_the_engine_watchdog() {
    // Invalid-clock controls isolate native custody from the engine's independent
    // startup watchdog; the same native start runs under an engine-issued Cx.
    for kind in 0..3 {
        let (_, actual, _) = started(kind);
        let (_, isolated, _) = started_with_watch(kind, false);
        assert_eq!(actual.method, isolated.method);
        assert_eq!(actual.url, isolated.url);
        assert_eq!(actual.headers, isolated.headers);
        assert_eq!(actual.body, isolated.body);
        assert_eq!(actual.route, isolated.route);
        assert_eq!(actual.timeout_ms, isolated.timeout_ms);
    }
}
#[test]
fn fixture_peers_are_finite_and_all_256_credits_are_reusable() {
    let (mut e, _) = engine(256, false);
    e.start(NOW);
    let out = drain(&mut e, NOW);
    assert!(out.requests.is_empty());
    assert!(out.prepares.is_empty());
    control(&mut e, NOW + 1., "validate");
    control(&mut e, NOW + 1., "release");
    control(&mut e, NOW + 1., "credits");
    drain(&mut e, NOW + 1.);
}
#[test]
fn independent_managed_read_fixture_completes_with_exact_cleanup() {
    let (mut e, _) = engine(1, false);
    control(&mut e, NOW, "boot");
    drain(&mut e, NOW);
    let read = independent(&mut e);
    complete_independent(&mut e, read);
    control(&mut e, NOW + 7., "validate");
    control(&mut e, NOW + 7., "release");
    control(&mut e, NOW + 7., "credits");
    drain(&mut e, NOW + 7.);
}
#[test]
fn full_store_snapshot_detects_silent_private_readiness_changes() {
    for section in 0..3 {
        let (mut e, _) = engine(0, false);
        let before = store_snapshot(&e);
        match section {
            0 => e.store_mut().set_balance(Default::default()),
            1 => e.store_mut().set_positions(Vec::new()),
            _ => e.store_mut().set_orders(Vec::new()),
        }
        assert_ne!(
            store_snapshot(&e),
            before,
            "readiness-only mutation escaped the full snapshot"
        );
    }
}
#[test]
fn cleanup_matcher_rejects_unrelated_duplicate_and_new_actions() {
    let (mut e, _) = engine(0, false);
    let first = independent(&mut e);
    let other = independent_with_command(&mut e, 992);
    let allowed = Cleanup::issued(Some(first.resource));
    let matching = vec![
        Action::ClearTimer {
            timer: first.resource.timer,
        },
        Action::ManagedHttp(Box::new(HttpAction::Cancel {
            preparation: first.resource.preparation,
            signing: None,
        })),
    ];
    allowed.complete(&only_cleanup(&matching, &allowed));
    let mut duplicate = matching.clone();
    duplicate.extend(matching.clone());
    let cases = [
        vec![Action::ClearTimer {
            timer: other.resource.timer,
        }],
        vec![Action::ManagedHttp(Box::new(HttpAction::Cancel {
            preparation: other.resource.preparation,
            signing: None,
        }))],
        vec![Action::SetTimer {
            timer: first.resource.timer,
            at_ms: NOW + LIMIT,
        }],
        duplicate,
    ];
    for actions in cases {
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| only_cleanup(
                &actions, &allowed
            )))
            .is_err()
        );
    }
    complete_independent(&mut e, first);
    complete_independent(&mut e, other);
}
#[test]
fn partial_catalogue_retains_valid_peers_and_native_skip_diagnostics() {
    let (mut e, r, _) = started(0);
    let body = INFO.replace("\"symbols\":[", "\"symbols\":[{\"broken\":true},");
    respond(&mut e, &r, NOW + 1., 200, &body);
    let out = drain(&mut e, NOW + 1.);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert!(e.store().market("BTCUSDT").is_some());
    assert_eq!(out.requests.len(), 2);
    assert!(out.actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::Event(event)) if event.message.contains("WEEX skipped 1 malformed market row(s): unknown")
    )));
}
#[test]
fn broken_catalogue_stream_does_not_publish_or_start_tickers() {
    let (mut e, r, _) = started(0);
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
            data: br#"{"symbols":[{"symbol":"BTCUSDT"}"#,
        },
    );
    e.handle(NOW + 2., Event::HttpEnd { request: r.id });
    let out = drain(&mut e, NOW + 2.);
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
    assert!(out.requests.is_empty());
    retry(&out.actions, NOW + 2.);
}
#[test]
fn current_429_defers_admission_until_the_native_pause_ends() {
    let (mut e, r, _) = started(0);
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 429,
                headers: "Retry-After: 3",
                body: "native rate limit",
            },
        },
    );
    let out = drain(&mut e, NOW + 1.);
    let first = retry(&out.actions, NOW + 1.);
    e.handle(NOW + 1001., Event::TimerFired { timer: first });
    let waiting = drain(&mut e, NOW + 1001.);
    assert!(waiting.requests.is_empty());
    assert!(
        waiting.prepares.is_empty(),
        "paused work cannot mint or prepare transport"
    );
    let deferred = waiting
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 3001. => Some(*timer),
            _ => None,
        })
        .expect("native shared pause deadline");
    e.handle(NOW + 3001., Event::TimerFired { timer: deferred });
    let out = drain(&mut e, NOW + 3001.);
    assert!(request(&out, 0).url.ends_with(PATHS[0]));
}
#[test]
fn failed_seed_sibling_is_inert_before_and_after_generation_reset() {
    for sibling_first in [false, true] {
        let (mut e, _, initial) = started(1);
        let old_book = request(&initial, 2);
        let old_cleanup = Cleanup::issued(resource(&initial, old_book.id));
        e.handle(
            NOW + 2.,
            Event::HttpFailed {
                request: request(&initial, 1).id,
                error: "finite ticker failure",
            },
        );
        let failed = drain(&mut e, NOW + 2.);
        let timer = retry(&failed.actions, NOW + 2.);
        if sibling_first {
            respond(&mut e, &old_book, NOW + 3., 200, BOOK);
            let out = drain(&mut e, NOW + 3.);
            assert!(!e.store().loaded().tickers);
            assert!(
                !out.actions
                    .iter()
                    .any(|a| matches!(a, Action::WsConnect { .. }))
            );
        }
        e.handle(NOW + 1002., Event::TimerFired { timer });
        let fresh = drain(&mut e, NOW + 1002.);
        assert_eq!(fresh.requests.len(), 2);
        if !sibling_first {
            let at_rollover = only_cleanup(&fresh.cleanup, &old_cleanup);
            if !at_rollover.timers.is_empty() || !at_rollover.preparations.is_empty() {
                old_cleanup.complete(&at_rollover);
            }
            if let Some(old) = resource(&initial, old_book.id) {
                assert_eq!(
                    e.http_release(old.request).is_some(),
                    at_rollover.preparations.is_empty()
                );
            }
            for current in &fresh.resources {
                assert!(
                    e.http_release(current.request).is_some(),
                    "fresh generation must stay live"
                );
            }
            let before = store_snapshot(&e);
            respond(&mut e, &old_book, NOW + 1003., 200, BOOK);
            let returned: Vec<_> = e.drain_actions().collect();
            let remaining = old_cleanup.remaining(&at_rollover);
            remaining.complete(&only_cleanup(&returned, &remaining));
            assert!(e.http_release(old_book.id).is_none());
            assert_eq!(store_snapshot(&e), before);
            assert!(!e.store().loaded().tickers);
            for current in &fresh.resources {
                assert!(
                    e.http_release(current.request).is_some(),
                    "stale delivery canceled fresh work"
                );
            }
        }
        respond(&mut e, &request(&fresh, 1), NOW + 1004., 200, DAILY);
        drain(&mut e, NOW + 1004.);
        respond(&mut e, &request(&fresh, 2), NOW + 1005., 200, BOOK);
        drain(&mut e, NOW + 1005.);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 99.);
        control(&mut e, NOW + 1006., "credits");
        drain(&mut e, NOW + 1006.);
    }
}
#[test]
fn catalogue_final_at_physical_equality_cannot_publish_or_start_tickers() {
    let (mut e, r, _) = started(0);
    respond(&mut e, &r, NOW + LIMIT, 200, INFO);
    let out = drain(&mut e, NOW + LIMIT);
    assert!(
        e.store().markets().is_empty(),
        "expired catalogue published"
    );
    assert!(!e.store().loaded().markets);
    assert!(out.requests.is_empty());
    retry(&out.actions, NOW + LIMIT);
}
#[test]
fn late_ticker_final_cannot_publish_or_open_a_socket() {
    let (mut e, _, out) = started(1);
    respond(&mut e, &request(&out, 1), NOW + 2., 200, DAILY);
    drain(&mut e, NOW + 2.);
    respond(&mut e, &request(&out, 2), NOW + 1. + LIMIT, 200, BOOK);
    let out = drain(&mut e, NOW + 1. + LIMIT);
    assert!(!e.store().loaded().tickers, "expired ticker published");
    assert!(e.store().tickers().is_empty());
    assert!(
        !out.actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    retry(&out.actions, NOW + 1. + LIMIT);
}
#[test]
fn late_429_cannot_pause_an_unrelated_current_book_query() {
    let (mut e, r, _) = started(0);
    e.handle(
        NOW + LIMIT,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 429,
                headers: "Retry-After: 30",
                body: "untrusted expired payload",
            },
        },
    );
    drain(&mut e, NOW + LIMIT);
    e.handle_command(
        NOW + LIMIT + 1.,
        Command::FetchOrderBook {
            command: 91,
            symbol: "BTCUSDT".into(),
            limit: Some(1),
        },
        Some(100_000),
    )
    .unwrap();
    let out = drain(&mut e, NOW + LIMIT + 1.);
    assert_eq!(
        out.requests.len(),
        1,
        "expired seed rate signal paused a current independent read"
    );
    assert!(out.requests[0].url.contains("/depth?"));
}
#[test]
fn genuine_full_registry_refuses_catalogue_and_recovers_all_credits() {
    let (mut e, _) = engine(256, true);
    e.start(NOW);
    let out = drain(&mut e, NOW);
    assert!(
        out.requests.is_empty(),
        "catalogue bypassed genuine shared capacity"
    );
    assert!(out.prepares.is_empty());
    let timer = retry(&out.actions, NOW);
    control(&mut e, NOW + 1., "validate");
    control(&mut e, NOW + 1., "release");
    drain(&mut e, NOW + 1.);
    e.handle(NOW + 1000., Event::TimerFired { timer });
    let out = drain(&mut e, NOW + 1000.);
    let r = request(&out, 0);
    respond(&mut e, &r, NOW + 1001., 500, "fixture refusal");
    drain(&mut e, NOW + 1001.);
    control(&mut e, NOW + 1002., "credits");
    drain(&mut e, NOW + 1002.);
}
#[test]
fn catalogue_retires_before_both_ticker_successors_under_pressure() {
    let (mut e, _) = engine(254, true);
    e.start(NOW);
    let out = drain(&mut e, NOW);
    respond(&mut e, &request(&out, 0), NOW + 1., 200, INFO);
    let out = drain(&mut e, NOW + 1.);
    assert_eq!(
        out.prepares.len(),
        2,
        "both successors need the retired catalogue credit"
    );
    assert_eq!(out.requests.len(), 2);
    for kind in [1, 2] {
        e.handle(
            NOW + 2.,
            Event::HttpFailed {
                request: request(&out, kind).id,
                error: "finite failure",
            },
        );
        drain(&mut e, NOW + 2.);
    }
    control(&mut e, NOW + 3., "validate");
    control(&mut e, NOW + 3., "release");
    control(&mut e, NOW + 3., "credits");
    drain(&mut e, NOW + 3.);
}
#[test]
fn finite_physical_timeout_recovers_through_native_backoff_once() {
    for kind in 0..3 {
        let (mut e, r, initial) = started(kind);
        let issued = if kind == 0 { NOW } else { NOW + 1. };
        let timer = resource(&initial, r.id).and_then(|owned| {
            initial.actions.iter().find_map(|a| match a {
                Action::SetTimer { timer, at_ms }
                    if *timer == owned.timer && *at_ms == issued + LIMIT =>
                {
                    Some(*timer)
                }
                _ => None,
            })
        });
        assert!(timer.is_some(), "physical bound starts before host work");
        e.handle(
            issued + LIMIT,
            Event::TimerFired {
                timer: timer.unwrap(),
            },
        );
        let out = drain(&mut e, issued + LIMIT);
        assert!(out.requests.is_empty());
        let again = retry(&out.actions, issued + LIMIT);
        e.handle(
            issued + LIMIT,
            Event::HttpFailed {
                request: r.id,
                error: "late duplicate",
            },
        );
        assert!(e.drain_actions().next().is_none());
        e.handle(issued + LIMIT + 1000., Event::TimerFired { timer: again });
        let next = drain(&mut e, issued + LIMIT + 1000.);
        assert_eq!(next.requests.len(), if kind == 0 { 1 } else { 2 });
        assert!(!next.requests.iter().any(|new| new.id == r.id));
    }
}
#[test]
fn held_catalogue_preparation_expires_and_late_completion_is_inert() {
    let (mut e, _) = engine(0, true);
    e.start(NOW);
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = actions.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms == NOW + LIMIT => Some(*timer),
        _ => None,
    });
    assert!(
        timer.is_some(),
        "admission must bound an unprepared catalogue"
    );
    let (token, id) = actions
        .iter()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => Some((*preparation, request.id)),
                _ => None,
            },
            _ => None,
        })
        .expect("genuine held preparation");
    e.handle(
        NOW + LIMIT,
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    let expired: Vec<_> = e.drain_actions().collect();
    assert!(!e.http_preparation_pending(token));
    assert!(expired.iter().any(|a| matches!(a, Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { preparation, .. } if *preparation == token))));
    assert!(!expired.iter().any(|a| matches!(a, Action::Http(_))));
    let backoff = retry(&expired, NOW + LIMIT);
    e.http_prepared(
        NOW + LIMIT + 1.,
        token,
        Prepared {
            id: id.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
    e.handle(NOW + LIMIT + 1000., Event::TimerFired { timer: backoff });
    let fresh = drain(&mut e, NOW + LIMIT + 1000.);
    assert_ne!(request(&fresh, 0).id, id);
    assert_eq!(fresh.prepares.len(), 1);
}
fn invalid_admission(now: f64) {
    let (mut e, _) = engine(1, false);
    control(&mut e, NOW, "boot");
    drain(&mut e, NOW);
    let unrelated = independent(&mut e);
    let before = store_snapshot(&e);
    control(&mut e, now, "start");
    no_new_work(&e.drain_actions().collect::<Vec<_>>());
    assert!(e.store().markets().is_empty());
    assert_eq!(store_snapshot(&e), before);
    control(&mut e, NOW + 2., "validate");
    complete_independent(&mut e, unrelated);
    control(&mut e, NOW + 7., "release");
    control(&mut e, NOW + 7., "credits");
    drain(&mut e, NOW + 7.);
}
fn invalid_released_failure(kind: usize, now: f64) {
    let (mut e, r, initial) = started_with_peers(kind, false, 1);
    let unrelated = independent(&mut e);
    let markets = e.store().markets().to_vec();
    let before = store_snapshot(&e);
    let owned = Cleanup::issued(resource(&initial, r.id));
    e.handle(
        now,
        Event::HttpFailed {
            request: r.id,
            error: "returned at invalid clock",
        },
    );
    let out: Vec<_> = e.drain_actions().collect();
    let seen = only_cleanup(&out, &owned);
    owned.complete(&seen);
    assert_eq!(e.store().markets(), markets);
    assert!(!e.store().loaded().tickers);
    assert_eq!(
        store_snapshot(&e),
        before,
        "invalid failure changed full Store/readiness"
    );
    assert!(e.http_release(r.id).is_none());
    control(&mut e, NOW + 2., "validate");
    if kind != 0 {
        let sibling = request(&initial, if kind == 1 { 2 } else { 1 });
        let sibling_resource =
            resource(&initial, sibling.id).expect("observed managed seed sibling");
        assert!(
            e.http_release(sibling.id).is_some(),
            "seed sibling was wrongly canceled"
        );
        respond(
            &mut e,
            &sibling,
            NOW + 3.,
            200,
            if kind == 1 { BOOK } else { DAILY },
        );
        let out: Vec<_> = e.drain_actions().collect();
        let expected = Cleanup::issued(Some(sibling_resource));
        expected.complete(&only_cleanup(&out, &expected));
        assert!(e.http_release(sibling.id).is_none());
        assert_eq!(store_snapshot(&e), before);
    }
    complete_independent(&mut e, unrelated);
    control(&mut e, NOW + 7., "release");
    control(&mut e, NOW + 7., "credits");
    drain(&mut e, NOW + 7.);
}
fn invalid_preparation_failure(kind: usize, now: f64) {
    let (mut e, _) = engine(1, true);
    e.start(NOW);
    if kind != 0 {
        let initial = drain(&mut e, NOW);
        respond(&mut e, &request(&initial, 0), NOW + 1., 200, INFO);
    }
    let initial: Vec<_> = e.drain_actions().collect();
    let found = initial.iter().find_map(|a| match a {
        Action::ManagedHttp(a) => match a.as_ref() {
            HttpAction::Prepare {
                preparation,
                request,
                ..
            } if request.url.ends_with(PATHS[kind]) => Some(*preparation),
            _ => None,
        },
        _ => None,
    });
    assert!(
        found.is_some(),
        "adopted native request must expose genuine preparation custody"
    );
    let p = found.unwrap();
    let captured = resources(&initial);
    let current = captured
        .iter()
        .find(|r| r.preparation == p)
        .copied()
        .unwrap();
    let unrelated = independent(&mut e);
    let before = store_snapshot(&e);
    e.http_preparation_failed(
        now,
        p,
        Error::Invalid,
        VenueError::new("invalid host observation").kind(ErrorKind::Internal),
    );
    let out: Vec<_> = e.drain_actions().collect();
    let owned = Cleanup::issued(Some(current));
    owned.complete(&only_cleanup(&out, &owned));
    assert_eq!(
        store_snapshot(&e),
        before,
        "invalid preparation failure changed full Store/readiness"
    );
    assert!(!e.http_preparation_pending(p));
    control(&mut e, NOW + 2., "validate");
    for sibling in captured.into_iter().filter(|r| r.preparation != p) {
        assert!(
            e.http_preparation_pending(sibling.preparation),
            "unrelated preparation was canceled"
        );
        e.http_prepared(
            NOW + 3.,
            sibling.preparation,
            Prepared {
                id: sibling.request.0,
                expires_at: None,
            },
        );
        let bound = drain(&mut e, NOW + 3.);
        assert_eq!(bound.requests.len(), 1);
        assert_eq!(bound.requests[0].id, sibling.request);
        assert!(e.http_release(sibling.request).is_some());
        respond(
            &mut e,
            &bound.requests[0],
            NOW + 4.,
            200,
            if kind == 1 { BOOK } else { DAILY },
        );
        let out: Vec<_> = e.drain_actions().collect();
        let expected = Cleanup::issued(Some(sibling));
        expected.complete(&only_cleanup(&out, &expected));
        assert_eq!(store_snapshot(&e), before);
    }
    complete_independent(&mut e, unrelated);
    control(&mut e, NOW + 7., "release");
    control(&mut e, NOW + 7., "credits");
    drain(&mut e, NOW + 7.);
}
fn invalid_ticker_retry_admission(now: f64) {
    let (mut e, r, initial) = started_with_peers(1, false, 1);
    let unrelated = independent(&mut e);
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: r.id,
            error: "finite ticker failure",
        },
    );
    let out = drain(&mut e, NOW + 2.);
    let timer = retry(&out.actions, NOW + 2.);
    let before = store_snapshot(&e);
    e.handle(now, Event::TimerFired { timer });
    no_new_work(&e.drain_actions().collect::<Vec<_>>());
    assert!(!e.store().loaded().tickers);
    assert_eq!(store_snapshot(&e), before);
    control(&mut e, NOW + 3., "validate");
    let sibling = request(&initial, 2);
    let sibling_resource = resource(&initial, sibling.id).expect("actual managed seed sibling");
    assert!(e.http_release(sibling.id).is_some());
    respond(&mut e, &sibling, NOW + 4., 200, BOOK);
    let out: Vec<_> = e.drain_actions().collect();
    let expected = Cleanup::issued(Some(sibling_resource));
    expected.complete(&only_cleanup(&out, &expected));
    assert_eq!(store_snapshot(&e), before);
    complete_independent(&mut e, unrelated);
    control(&mut e, NOW + 7., "release");
    control(&mut e, NOW + 7., "credits");
    drain(&mut e, NOW + 7.);
}
macro_rules! invalid_clock_tests {
    ($admission:ident, $retry:ident, $markets:ident, $daily:ident, $book:ident,
     $prepare_markets:ident, $prepare_daily:ident, $prepare_book:ident, $clock:expr) => {
        #[test]
        fn $admission() {
            invalid_admission($clock);
        }
        #[test]
        fn $retry() {
            invalid_ticker_retry_admission($clock);
        }
        #[test]
        fn $markets() {
            invalid_released_failure(0, $clock);
        }
        #[test]
        fn $daily() {
            invalid_released_failure(1, $clock);
        }
        #[test]
        fn $book() {
            invalid_released_failure(2, $clock);
        }
        #[test]
        fn $prepare_markets() {
            invalid_preparation_failure(0, $clock);
        }
        #[test]
        fn $prepare_daily() {
            invalid_preparation_failure(1, $clock);
        }
        #[test]
        fn $prepare_book() {
            invalid_preparation_failure(2, $clock);
        }
    };
}
invalid_clock_tests!(
    nan_admission,
    nan_ticker_retry,
    nan_markets_failure,
    nan_daily_failure,
    nan_book_failure,
    nan_markets_preparation,
    nan_daily_preparation,
    nan_book_preparation,
    f64::NAN
);
invalid_clock_tests!(
    infinity_admission,
    infinity_ticker_retry,
    infinity_markets_failure,
    infinity_daily_failure,
    infinity_book_failure,
    infinity_markets_preparation,
    infinity_daily_preparation,
    infinity_book_preparation,
    f64::INFINITY
);
invalid_clock_tests!(
    negative_infinity_admission,
    negative_infinity_ticker_retry,
    negative_infinity_markets_failure,
    negative_infinity_daily_failure,
    negative_infinity_book_failure,
    negative_infinity_markets_preparation,
    negative_infinity_daily_preparation,
    negative_infinity_book_preparation,
    f64::NEG_INFINITY
);

fn private_pressure_engine(peers: usize) -> Driver {
    support::setup::with_venue(
        Options {
            key: Some("seed-pressure-key".into()),
            secret: Some("seed-pressure-secret".into()),
            passphrase: Some("seed-pressure-passphrase".into()),
            ..Options::default()
        },
        |inner| Probe {
            inner,
            peers: Vec::new(),
            initial_peers: peers,
            start_native: true,
            failures: Default::default(),
        },
    )
}
fn private_seed_pair(peers: usize) -> (Driver, Output) {
    let mut e = private_pressure_engine(peers);
    e.start(NOW);
    let initial = drain(&mut e, NOW);
    let catalogue = request(&initial, 0);
    respond(&mut e, &catalogue, NOW + 1., 200, INFO);
    let pair = drain(&mut e, NOW + 1.);
    assert_eq!(pair.requests.len(), 2);
    (e, pair)
}
fn six_private_prepares_after_seed_cleanup(e: &mut Driver, now: f64, old: Resource, socket: bool) {
    let actions: Vec<_> = e.drain_actions().collect();
    let cancel = actions.iter().position(|a| matches!(a,
        Action::ManagedHttp(a) if matches!(a.as_ref(), HttpAction::Cancel { preparation, .. } if *preparation == old.preparation)
    )).expect("completing seed's exact physical cleanup");
    let prepares: Vec<_> = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::ManagedHttp(a) => match a.as_ref() {
                HttpAction::Prepare { request, .. } => Some((i, request)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        prepares.len(),
        6,
        "all six private reads need retired seed credit"
    );
    assert!(
        prepares.iter().all(|(i, r)| *i > cancel
            && !r.url.ends_with("24hr")
            && !r.url.ends_with("bookTicker"))
    );
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::WsConnect { .. }))
            .count(),
        usize::from(socket)
    );
    e.prepare_dispatch(now);
    let mut queue: VecDeque<_> = actions.into();
    queue.extend(e.drain_actions());
    let mut routes = std::collections::HashMap::new();
    let mut released = 0;
    while let Some(action) = queue.pop_front() {
        if let Action::ManagedHttp(action) = action {
            match *action {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } => {
                    assert!(routes.insert(request.id, preparation).is_none());
                    e.http_prepared(
                        now,
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
                    assert_eq!(routes.remove(&request.id), Some(preparation));
                    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
                    released += 1;
                }
                HttpAction::Cancel { .. } => {}
                HttpAction::BindGroup { .. } => panic!("private GETs have no mutation group"),
            }
        }
        queue.extend(e.drain_actions());
    }
    assert!(routes.is_empty());
    assert_eq!(released, 6);
}
#[test]
fn each_final_seed_retires_before_six_private_reads_under_full_pressure() {
    for final_kind in [1, 2] {
        let (mut e, pair) = private_seed_pair(250);
        let first = if final_kind == 1 { 2 } else { 1 };
        respond(
            &mut e,
            &request(&pair, first),
            NOW + 2.,
            200,
            if first == 1 { DAILY } else { BOOK },
        );
        drain(&mut e, NOW + 2.);
        let final_request = request(&pair, final_kind);
        let old = resource(&pair, final_request.id).unwrap();
        respond(
            &mut e,
            &final_request,
            NOW + 3.,
            200,
            if final_kind == 1 { DAILY } else { BOOK },
        );
        assert!(e.store().loaded().tickers);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 99.);
        six_private_prepares_after_seed_cleanup(&mut e, NOW + 3., old, true);
    }
}
#[test]
fn finite_failed_or_timed_out_seed_retires_before_private_start_once() {
    for timeout in [false, true] {
        let (mut e, pair) = private_seed_pair(249);
        let daily = request(&pair, 1);
        let old = resource(&pair, daily.id).unwrap();
        let now = if timeout { NOW + 1. + LIMIT } else { NOW + 2. };
        let event = if timeout {
            Event::TimerFired { timer: old.timer }
        } else {
            Event::HttpFailed {
                request: daily.id,
                error: "pressure failure",
            }
        };
        e.handle(now, event);
        assert!(!e.store().loaded().tickers);
        six_private_prepares_after_seed_cleanup(&mut e, now, old, false);
        e.handle(
            now,
            Event::HttpFailed {
                request: daily.id,
                error: "duplicate",
            },
        );
        assert!(e.drain_actions().next().is_none());
    }
}

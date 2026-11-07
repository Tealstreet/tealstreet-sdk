//! Original unsigned public-trade history controls on unchanged native source.
use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Owner, Policy, Preparation, Prepared,
    RetryPolicy, RoutingIntent,
};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Notice, Outcome,
    Query, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_kraken::{Kraken, Options, Pending};
use serde_json::{Value, json};

const NOW: f64 = 1_700_000_000_000.;
const COMMAND: u32 = 71;
const SYMBOL: &str = "PF_XBTUSD";
const META: &str = r#"{"result":"success","instruments":[{"symbol":"PF_XBTUSD","base":"BTC","quote":"USD","tradeable":true,"tickSize":0.5,"contractSize":1,"contractValueTradePrecision":3,"marginLevels":[{"initialMargin":0.02}]}]}"#;
const TICKERS: &str = r#"{"result":"success","tickers":[{"symbol":"PF_XBTUSD","last":100,"markPrice":101,"indexPrice":102,"bid":99,"ask":101,"vol24h":2}]}"#;
struct Probe {
    inner: Kraken,
    peers: Vec<Context>,
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
        self.inner.on_open(cx, socket)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item)
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
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error)
    }
    fn read_root_retired(&mut self, owner: Owner, pending: Vec<Pending>) {
        self.inner.read_root_retired(owner, pending)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::Query {
                query: Query::PublicTrades { symbol, .. },
                ..
            } if symbol == "__hold" => {
                self.peers.push(
                    cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 120_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .expect("genuine peer command admission"),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["__retire"] => {
                let original = self.peers.remove(0);
                cx.fail_context(&original, "original peer retired").unwrap();
                for peer in &self.peers {
                    cx.validate_read_root_context(peer).unwrap();
                }
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}
type TestEngine = Engine<Probe>;
fn response(e: &mut TestEngine, now: f64, request: &HttpRequest, status: u16, body: &str) {
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
}
fn boot() -> (TestEngine, TimerId) {
    boot_with(false)
}
fn boot_with(testnet: bool) -> (TestEngine, TimerId) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Kraken::new(Options {
            public_data_only: true,
            testnet,
            ..Default::default()
        }),
        peers: vec![],
    });
    e.start(NOW);
    let mut poll = None;
    for _ in 0..8 {
        let actions = prepared_actions(&mut e, NOW);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            match action {
                Action::SetTimer { timer, at_ms } if at_ms == NOW + 5000. => {
                    poll.get_or_insert(timer);
                }
                Action::Http(request) => {
                    let body = if request.url.ends_with("/instruments") {
                        META
                    } else if request.url.ends_with("/tickers") {
                        TICKERS
                    } else {
                        panic!("unexpected fixture request {}", request.url)
                    };
                    response(&mut e, NOW, &request, 200, body);
                }
                _ => {}
            }
        }
    }
    assert!(e.store().market(SYMBOL).is_some());
    assert!(e.store().ticker(SYMBOL).is_some());
    e.take_frame(NOW);
    (e, poll.expect("native five-second polling timer"))
}
fn command(id: u32) -> Command {
    query(id, SYMBOL, Some(200))
}
fn query(id: u32, symbol: &str, limit: Option<u32>) -> Command {
    Command::Query {
        command: id,
        query: Query::PublicTrades {
            symbol: symbol.into(),
            limit,
        },
    }
}
fn fetch(e: &mut TestEngine, now: f64, wait: u32) -> Vec<Action> {
    e.handle_command(now, command(COMMAND), Some(wait)).unwrap();
    e.drain_actions().collect()
}
fn packet(actions: &[Action]) -> HttpRequest {
    let packets: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) => Some(request.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(packets.len(), 1, "exactly one physical page: {actions:?}");
    packets[0].clone()
}
fn preparation(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "physical HTTP must wait for Prepare/Bind/Claim"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match &**action {
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
        .expect("one genuine unsigned page preparation")
}
fn release(e: &mut TestEngine, now: f64, token: Preparation) -> HttpRequest {
    e.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("exact prepared page binds");
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn native_packet(e: &mut TestEngine, actions: Vec<Action>, now: f64) -> HttpRequest {
    if actions.iter().any(|a| matches!(a, Action::Http(_))) {
        packet(&actions)
    } else {
        let (token, _) = preparation(&actions);
        release(e, now, token)
    }
}
fn terminal(actions: &[Action]) -> Result<Value, VenueError> {
    let results: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: COMMAND,
                result,
            }) => Some(Ok(serde_json::from_str(result).unwrap())),
            Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                error,
            }) => Some(Err(error.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        results.len(),
        1,
        "one original command terminal: {actions:?}"
    );
    results.into_iter().next().unwrap()
}
fn row(id: &str, time: &str) -> Value {
    json!({"trade_id":id,"time":time,"price":"100","size":"0.5","side":"sell"})
}
fn body(rows: Vec<Value>) -> String {
    json!({"result":"success","history":rows}).to_string()
}
fn full_page(page: usize) -> String {
    body(
        (0..100)
            .map(|i| {
                row(
                    &format!("p{page}-{i}"),
                    &format!("2023-11-14T22:13:{:02}.999Z", 59 - page % 50),
                )
            })
            .collect(),
    )
}
fn empty() -> &'static str {
    r#"{"result":"success","history":[]}"#
}
fn outcome(e: &mut TestEngine, now: f64, request: &HttpRequest, body: &str) -> Vec<Action> {
    response(e, now, request, 200, body);
    e.drain_actions().collect()
}
fn no_http(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Sign { .. } | Action::SignedOutbound { .. }
    )));
}
fn saturate(e: &mut TestEngine) {
    for id in 1000..1256 {
        e.handle_command(NOW, query(id, "__hold", None), Some(120_000))
            .unwrap();
        assert!(e.drain_actions().next().is_none());
    }
    assert_eq!(e.venue().peers.len(), 256);
}
fn retire(e: &mut TestEngine) {
    let before = e.venue().peers.len();
    let id = 1000 + (256 - before) as u32;
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["__retire".into()],
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(e.venue().peers.len(), before - 1);
    assert_eq!(
        actions
            .iter()
            .filter(
                |a| matches!(a, Action::Notice(Notice::CommandFailed { command, error })
        if *command == id && error.message == "original peer retired")
            )
            .count(),
        1
    );
}
fn no_next(actions: &[Action]) {
    no_http(actions);
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(a)
        if !matches!(&**a, HttpAction::Cancel { .. }))));
}
fn partial(actions: &[Action], expected: usize) {
    let markers = actions
        .iter()
        .enumerate()
        .filter_map(|(i, a)| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE =>
            {
                assert!(event.fields.iter().any(|(key,value)| *key == "command"
                && matches!(value, safe_cex_engine::Value::Num(n) if *n == f64::from(COMMAND))));
                Some(i)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(markers.len(), 1);
    let done = actions
        .iter()
        .position(|a| {
            matches!(
                a,
                Action::Notice(Notice::CommandDone {
                    command: COMMAND,
                    ..
                })
            )
        })
        .unwrap();
    assert!(markers[0] < done);
    assert_eq!(
        terminal(actions).unwrap().as_array().unwrap().len(),
        expected
    );
    no_next(actions);
}
fn continuation(e: &mut TestEngine) -> HttpRequest {
    let a = fetch(e, NOW, 60_000);
    let first = native_packet(e, a, NOW);
    let a = outcome(e, NOW + 1., &first, &full_page(0));
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    native_packet(e, a, NOW + 1.)
}

#[test]
fn command_requires_public_preparation_before_http() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let (p, r) = preparation(&a);
    assert!(r.headers.is_empty() && r.body.is_none());
    assert_eq!(r.method, safe_cex_engine::Method::Get);
    let r = release(&mut e, NOW, p);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &r, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn original_fractional_receipt_caps_first_child() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW + 0.25, 1000);
    assert!(
        a.iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..} if *at_ms==NOW+1000.25))
    );
    let (p, _) = preparation(&a);
    let r = release(&mut e, NOW + 1.25, p);
    assert_eq!(r.timeout_ms, Some(999));
}
#[test]
fn root_expiry_refuses_late_first_page_without_marker() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 1000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1000., &r, empty());
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_next(&a);
}
#[test]
fn root_expiry_after_accepted_page_never_returns_partial() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 1000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &r, &full_page(0));
    let r = native_packet(&mut e, a, NOW + 1.);
    let a = outcome(&mut e, NOW + 1000., &r, empty());
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert!(!a.iter().any(|a| matches!(a,Action::Notice(Notice::Event(ev)) if ev.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    no_next(&a);
}
#[test]
fn second_page_has_its_own_prepare_and_original_deadline() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 1000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 100., &r, &full_page(0));
    let (p, _) = preparation(&a);
    let r = release(&mut e, NOW + 100., p);
    assert!(r.timeout_ms.is_some_and(|ms| ms <= 900));
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 101., &r, empty()))
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        100
    );
}
#[test]
fn typed_preparation_refusal_is_strict_and_late_prepare_is_inert() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let (p, _) = preparation(&a);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::Invalid,
        VenueError::new("route refused")
            .kind(ErrorKind::Unsupported)
            .venue_code("route-fixture"),
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.message, "route refused");
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.venue_code.as_deref(), Some("route-fixture"));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_next(&a);
    e.http_prepared(
        NOW + 2.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn later_preparation_refusal_cannot_recover_accepted_prefix() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &r, &full_page(0));
    let (p, _) = preparation(&a);
    e.http_preparation_failed(
        NOW + 2.,
        p,
        Error::Invalid,
        VenueError::new("later route refused").kind(ErrorKind::Unsupported),
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(!a.iter().any(|a| matches!(a,Action::Notice(Notice::Event(ev)) if ev.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    no_next(&a);
}
#[test]
fn new_twenty_second_prepare_deadline_precedes_live_caller_root() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let (p, _) = preparation(&a);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("new child admission deadline");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    no_next(&a);
    e.http_prepared(
        NOW + 20_001.,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
}
#[test]
fn held_bind_expires_at_new_child_bound_under_live_root() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let (p, _) = preparation(&a);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        NOW,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let a = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal(&a).unwrap_err().outcome, Some(Outcome::NotSent));
    no_next(&a);
    assert!(e.claim_http(NOW + 20_001., permit).is_err());
    no_next(&e.drain_actions().collect::<Vec<_>>());
}
#[test]
fn released_physical_timeout_is_typed_under_a_live_root() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("new physical child bound");
    let r = native_packet(&mut e, a, NOW);
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let a = e.drain_actions().collect::<Vec<_>>();
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Timeout));
    assert_eq!(error.outcome, None);
    no_next(&a);
    no_next(&outcome(&mut e, NOW + 20_001., &r, empty()));
}
#[test]
fn later_physical_timeout_recovers_only_accepted_rows_while_root_live() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &r, &full_page(0));
    let timer = a
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_001. => Some(*timer),
            _ => None,
        })
        .expect("second page admission bound");
    let r = native_packet(&mut e, a, NOW + 1.);
    e.handle(NOW + 20_001., Event::TimerFired { timer });
    partial(&e.drain_actions().collect::<Vec<_>>(), 100);
    assert!(outcome(&mut e, NOW + 20_002., &r, empty()).is_empty());
}
#[test]
fn genuine_capacity_then_exact_two_slot_recovery_preserves_peers() {
    let (mut e, _) = boot();
    saturate(&mut e);
    let a = fetch(&mut e, NOW, 60_000);
    no_next(&a);
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    retire(&mut e);
    let a = fetch(&mut e, NOW, 60_000);
    no_next(&a);
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(error.message, "Engine request capacity reached");
    assert_eq!(e.venue().peers.len(), 255);
    retire(&mut e);
    assert_eq!(e.venue().peers.len(), 254);
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &r, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn duplicate_id_keeps_original_held_read_and_never_reissues() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    let a = fetch(&mut e, NOW + 1., 60_000);
    no_next(&a);
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { .. } | Notice::CommandDone { .. })
    )));
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 2., &r, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn invalid_external_clock_refuses_without_public_http() {
    let (mut e, _) = boot();
    e.handle(f64::NAN, Event::Command(command(COMMAND)));
    let a = e.drain_actions().collect::<Vec<_>>();
    no_next(&a);
    let error = terminal(&a).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
}
#[test]
fn unclaimed_data_cannot_stage_rows_or_complete_owner() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let (p, r) = preparation(&a);
    let a = outcome(&mut e, NOW + 1., &r, empty());
    assert!(a.is_empty());
    let r = release(&mut e, NOW + 2., p);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 3., &r, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn native_unsigned_wire_uid_exact_identity_milliseconds_and_side() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    assert_eq!(
        r.url,
        "https://futures.kraken.com/derivatives/api/v3/history?symbol=pf_xbtusd"
    );
    assert!(r.headers.is_empty() && r.body.is_none());
    let mut first = row("18446744073709551615", "2023-11-14T22:13:19.999Z");
    first["uid"] = json!("native-uid");
    let a = outcome(
        &mut e,
        NOW + 1.,
        &r,
        &body(vec![
            first,
            row("18446744073709551615", "2023-11-14T22:13:19.998Z"),
        ]),
    );
    let rows = terminal(&a).unwrap();
    assert_eq!(rows[0]["id"], "native-uid");
    assert_eq!(rows[1]["id"], "18446744073709551615");
    assert_eq!(rows[0]["timestamp"], NOW - 1.);
    assert_eq!(rows[0]["quoteQty"], 50.);
    assert_eq!(rows[0]["side"], "sell");
}
#[test]
fn native_empty_page_succeeds_without_coverage_marker() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &r, empty())).unwrap(),
        json!([])
    );
}
#[test]
fn native_cursor_uses_oldest_floor_plus_one_and_final_sort_dedup() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &r, &full_page(0));
    let r = native_packet(&mut e, a, NOW + 1.);
    assert!(
        r.url
            .ends_with("?lastTime=2023-11-14T22%3A14%3A00.000Z&symbol=pf_xbtusd")
    );
    let rows = terminal(&outcome(
        &mut e,
        NOW + 2.,
        &r,
        &body(vec![
            row("p0-0", "2023-11-14T22:13:59.999Z"),
            row("new", "2023-11-14T22:13:18.000Z"),
        ]),
    ))
    .unwrap();
    assert_eq!(rows.as_array().unwrap().len(), 101);
    assert_eq!(rows[100]["id"], "new");
}
#[test]
fn native_default_and_zero_limit_are_one_hundred_without_wire_limit() {
    for limit in [None, Some(0), Some(1)] {
        let (mut e, _) = boot();
        e.handle_command(NOW, query(COMMAND, SYMBOL, limit), Some(60_000))
            .unwrap();
        let a = e.drain_actions().collect();
        let r = native_packet(&mut e, a, NOW);
        assert!(!r.url.contains("limit"));
        let rows = terminal(&outcome(&mut e, NOW + 1., &r, &full_page(0))).unwrap();
        assert_eq!(
            rows.as_array().unwrap().len(),
            if limit == Some(1) { 1 } else { 100 }
        );
    }
}
#[test]
fn native_symbol_is_not_catalogue_validated_and_output_keeps_caller_case() {
    let (mut e, _) = boot();
    e.handle_command(NOW, query(COMMAND, "MiXeD/Unknown", Some(1)), Some(60_000))
        .unwrap();
    let a = e.drain_actions().collect();
    let r = native_packet(&mut e, a, NOW);
    assert!(r.url.ends_with("symbol=mixed%2Funknown"));
    let rows = terminal(&outcome(
        &mut e,
        NOW + 1.,
        &r,
        &body(vec![row("a", "2023-11-14T22:13:19.999Z")]),
    ))
    .unwrap();
    assert_eq!(rows[0]["symbol"], "MiXeD/Unknown");
}
#[test]
fn native_later_transport_native_and_codec_failures_recover_accepted_only() {
    for mode in 0..3 {
        let (mut e, _) = boot();
        let r = continuation(&mut e);
        if mode == 0 {
            e.handle(
                NOW + 2.,
                Event::HttpFailed {
                    request: r.id,
                    error: "later reset",
                },
            );
        } else if mode == 1 {
            response(&mut e, NOW + 2., &r, 503, "{}");
        } else {
            response(
                &mut e,
                NOW + 2.,
                &r,
                200,
                &body(vec![
                    row("failed-prefix", "2023-11-14T22:13:18.000Z"),
                    json!({}),
                ]),
            );
        }
        let a = e.drain_actions().collect::<Vec<_>>();
        partial(&a, 100);
        assert!(!terminal(&a).unwrap().to_string().contains("failed-prefix"));
    }
}
#[test]
fn native_first_failure_and_staged_prefix_never_mint_partial_success() {
    for mode in 0..3 {
        let (mut e, _) = boot();
        let a = fetch(&mut e, NOW, 60_000);
        let r = native_packet(&mut e, a, NOW);
        if mode == 0 {
            e.handle(
                NOW + 1.,
                Event::HttpFailed {
                    request: r.id,
                    error: "first reset",
                },
            );
        } else if mode == 1 {
            response(&mut e, NOW + 1., &r, 503, "{}");
        } else {
            response(
                &mut e,
                NOW + 1.,
                &r,
                200,
                &body(vec![row("staged", "2023-11-14T22:13:19.999Z"), json!({})]),
            );
        }
        let a = e.drain_actions().collect::<Vec<_>>();
        assert!(terminal(&a).is_err());
        no_next(&a);
        assert!(!a.iter().any(|a|matches!(a,Action::Notice(Notice::Event(ev))if ev.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE)));
    }
}
#[test]
fn native_repeated_full_boundary_fails_without_false_complete_prefix() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &r, &full_page(0));
    let r = native_packet(&mut e, a, NOW + 1.);
    let a = outcome(&mut e, NOW + 2., &r, &full_page(0));
    assert!(terminal(&a).unwrap_err().message.contains("INCOMPLETE"));
    no_next(&a);
}
#[test]
fn native_fifty_page_budget_refuses_without_fifty_first_request() {
    let (mut e, _) = boot();
    e.handle_command(NOW, query(COMMAND, SYMBOL, Some(10_000)), Some(60_000))
        .unwrap();
    let mut a = e.drain_actions().collect();
    for i in 0..50 {
        let r = native_packet(&mut e, a, NOW + i as f64);
        a = outcome(&mut e, NOW + i as f64, &r, &full_page(i));
    }
    assert!(terminal(&a).unwrap_err().message.contains("page budget"));
    no_next(&a);
}
#[test]
fn zero_wait_rejects_before_native_entry() {
    let (mut e, _) = boot();
    assert_eq!(
        e.handle_command(NOW, command(COMMAND), Some(0)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
}

#[test]
fn native_testnet_wire_stays_unsigned_and_exact() {
    let (mut e, _) = boot_with(true);
    let a = fetch(&mut e, NOW, 60_000);
    let r = native_packet(&mut e, a, NOW);
    assert_eq!(
        r.url,
        "https://demo-futures.kraken.com/derivatives/api/v3/history?symbol=pf_xbtusd"
    );
    assert!(r.headers.is_empty() && r.body.is_none());
    assert_eq!(
        terminal(&outcome(&mut e, NOW + 1., &r, empty())).unwrap(),
        json!([])
    );
}

#[test]
fn stale_completed_page_failure_cannot_settle_current_page() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 60_000);
    let first = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &first, &full_page(0));
    let second = native_packet(&mut e, a, NOW + 1.);
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: first.id,
            error: "stale first page failure",
        },
    );
    assert!(e.drain_actions().next().is_none());
    let a = outcome(&mut e, NOW + 3., &second, empty());
    assert_eq!(terminal(&a).unwrap().as_array().unwrap().len(), 100);
}
#[test]
fn original_root_expiry_precedes_later_failure_recovery() {
    let (mut e, _) = boot();
    let a = fetch(&mut e, NOW, 1000);
    let first = native_packet(&mut e, a, NOW);
    let a = outcome(&mut e, NOW + 1., &first, &full_page(0));
    let second = native_packet(&mut e, a, NOW + 1.);
    e.handle(
        NOW + 1000.,
        Event::HttpFailed {
            request: second.id,
            error: "failure after root expiry",
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    assert_eq!(terminal(&a).unwrap_err().kind, Some(ErrorKind::Timeout));
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Notice(Notice::Event(event))
        if event.code==safe_cex_engine::log::code::HISTORY_INCOMPLETE))
    );
    no_next(&a);
}
#[test]
fn native_transport_failure_discards_unaccepted_streamed_second_page_rows() {
    let (mut e, _) = boot();
    let r = continuation(&mut e);
    e.handle(
        NOW + 2.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(NOW+3.,Event::HttpChunk{request:r.id,data:br#"{"result":"success","history":[{"trade_id":"unaccepted-streamed","time":"2023-11-14T22:13:18.999Z","price":100,"size":0.5,"side":"buy"},"#});
    assert!(e.drain_actions().next().is_none());
    e.handle(
        NOW + 4.,
        Event::HttpFailed {
            request: r.id,
            error: "second page interrupted",
        },
    );
    let a = e.drain_actions().collect::<Vec<_>>();
    partial(&a, 100);
    assert!(
        !terminal(&a)
            .unwrap()
            .to_string()
            .contains("unaccepted-streamed")
    );
}

#[test]
fn invalid_host_receipt_is_refused_before_dispatch() {
    let (mut e, _) = boot();
    assert_eq!(
        e.handle_command(f64::NAN, command(COMMAND), Some(60_000)),
        Err(Error::Invalid)
    );
    assert!(e.drain_actions().next().is_none());
}

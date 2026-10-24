//! Wire/reply ingress probes accept legacy HTTP; managed custody probes require Prepare.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Context, Error, HttpAction, Operation, Policy, Preparation, Prepared, RetryPolicy,
    RoutingIntent,
};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Event, HttpRequest, Method, Notice, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options, Pending};

const NOW: f64 = 1_700_000_000_000.;
const GOOD: &str = r#"{"r":[{"i":"BTC_USDT_Perp","ih":"0x030501","b":"BTC","q":"USDT","k":"PERPETUAL","ts":"0.1","ms":"0.001","mp":"1000","bd":9,"qd":6}]}"#;
struct Probe {
    inner: Grvt,
    defer_start: bool,
    forwarded: usize,
    items: usize,
    peers: Vec<Context>,
    validated: usize,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if !self.defer_start {
            self.inner.start(cx);
        }
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
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.items += 1;
        self.inner.on_item(cx, pending, item);
    }
    fn on_stream_error(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        response: Response<'_>,
    ) {
        self.inner.on_stream_error(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.inner.on_sign_error(cx, pending, error);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__peer" => {
                self.peers.push(
                    cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .unwrap(),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["start_catalog"] => {
                self.inner.start(cx)
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire_peer"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for peer in &self.peers {
                    cx.validate_http_context(peer).unwrap();
                }
                self.validated = self.peers.len();
            }
            _ => {
                self.forwarded += 1;
                self.inner.on_command(cx, command);
            }
        }
    }
}
fn boot(testnet: bool, defer_start: bool) -> (Engine<Probe>, Vec<Action>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Grvt::new(Options {
            testnet,
            public_data_only: true,
            ..Options::default()
        }),
        defer_start,
        forwarded: 0,
        items: 0,
        peers: vec![],
        validated: 0,
    });
    e.start(NOW);
    let actions = e.drain_actions().collect();
    (e, actions)
}
fn drain(e: &mut Engine<Probe>) -> Vec<Action> {
    e.drain_actions().collect()
}
fn no_commands(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Notice(
            Notice::CommandDone { .. } | Notice::CommandFailed { .. } | Notice::CommandError { .. }
        )
    )));
}
fn no_outbound(actions: &[Action]) {
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::Sign { .. }
            | Action::SignPayload { .. }
            | Action::SignedOutbound { .. }
            | Action::WsSend { .. }
            | Action::WsConnect { .. }
    )));
    assert!(!actions.iter().any(|a| matches!(a, Action::ManagedHttp(m) if matches!(&**m, HttpAction::Prepare { .. } | HttpAction::Bind { .. }))));
}
fn prep(actions: &[Action]) -> (Preparation, HttpRequest) {
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "catalog must prepare before HTTP"
    );
    let found: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(m) => match &**m {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } => {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one genuine public catalog Prepare");
    found[0].clone()
}
fn bind(
    e: &mut Engine<Probe>,
    p: Preparation,
    now: f64,
) -> (safe_cex_engine::request_context::Permit, HttpRequest) {
    assert!(e.http_preparation_pending(p));
    e.http_prepared(
        now,
        p,
        Prepared {
            id: p.attempt().request.0,
            expires_at: None,
        },
    );
    let actions = drain(e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let found: Vec<_> = actions
        .into_iter()
        .filter_map(|a| match a {
            Action::ManagedHttp(m) => match *m {
                HttpAction::Bind {
                    preparation,
                    permit,
                    request,
                } => {
                    assert_eq!(preparation, p);
                    Some((permit, request))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one real catalog Bind");
    found[0].clone()
}
fn release_managed(e: &mut Engine<Probe>, actions: &[Action], now: f64) -> HttpRequest {
    let (p, _) = prep(actions);
    let (permit, request) = bind(e, p, now);
    assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
    request
}
fn native_release(e: &mut Engine<Probe>, actions: &[Action], now: f64) -> HttpRequest {
    let raw: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    if !raw.is_empty() {
        assert_eq!(raw.len(), 1);
        raw[0].clone()
    } else {
        release_managed(e, actions, now)
    }
}
fn reply(
    e: &mut Engine<Probe>,
    request: &HttpRequest,
    now: f64,
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
    drain(e)
}
fn backoff(actions: &[Action], now: f64, nominal: f64) -> TimerId {
    no_outbound(actions);
    no_commands(actions);
    let found: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if (0.75 * nominal..=nominal).contains(&(*at_ms - now)) =>
            {
                Some(*timer)
            }
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one native jittered startup retry");
    found[0]
}
fn refresh_timer(actions: &[Action], now: f64) -> TimerId {
    let found: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == now + 300_000. => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1);
    found[0]
}
fn wire(request: &HttpRequest, testnet: bool) {
    assert_eq!(request.method, Method::Post);
    assert_eq!(
        request.url,
        format!(
            "https://market-data{}.grvt.io/lite/v1/all_instruments",
            if testnet { ".testnet" } else { "" }
        )
    );
    assert_eq!(
        request.headers,
        vec![("Content-Type", "application/json".into())]
    );
    assert!(request.signer_headers.is_empty());
    assert_eq!(request.body.as_deref(), Some("{}"));
    assert_eq!(request.route, None);
}
#[test]
fn startup_requires_real_public_preparation() {
    let (_, a) = boot(false, false);
    prep(&a);
}
#[test]
fn managed_catalog_has_new_twenty_second_wire_bound() {
    let (mut e, a) = boot(false, false);
    let r = release_managed(&mut e, &a, NOW);
    assert_eq!(r.timeout_ms, Some(20_000));
}
#[test]
fn native_mainnet_wire_and_catalog_publication() {
    native_wire(false);
}
#[test]
fn native_testnet_wire_and_catalog_publication() {
    native_wire(true);
}
fn native_wire(testnet: bool) {
    let (mut e, a) = boot(testnet, false);
    let r = native_release(&mut e, &a, NOW);
    wire(&r, testnet);
    let a = reply(&mut e, &r, NOW + 1., 200, GOOD);
    no_commands(&a);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].symbol, "BTC_USDT_PERP");
    refresh_timer(&a, NOW + 1.);
    assert!(a.iter().any(|a| matches!(a, Action::WsConnect { .. })));
}
#[test]
fn unclaimed_catalog_data_cannot_publish_or_connect() {
    let (mut e, a) = boot(false, false);
    let r = a
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.clone()),
            Action::ManagedHttp(m) => match &**m {
                HttpAction::Prepare { request, .. } => Some(request.clone()),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let a = reply(&mut e, &r, NOW + 1., 200, GOOD);
    assert!(!e.store().loaded().markets);
    no_outbound(&a);
    no_commands(&a);
}
#[test]
fn late_data_at_the_new_bound_cannot_publish() {
    let (mut e, a) = boot(false, false);
    let r = native_release(&mut e, &a, NOW);
    let a = reply(&mut e, &r, NOW + 20_000., 200, GOOD);
    assert!(!e.store().loaded().markets);
    no_outbound(&a);
    no_commands(&a);
}
#[test]
fn preparation_failure_cancels_once_then_uses_native_backoff() {
    let (mut e, a) = boot(false, false);
    let (p, _) = prep(&a);
    e.http_preparation_failed(
        NOW + 1.,
        p,
        Error::State,
        VenueError::new("fixture route refused"),
    );
    let a = drain(&mut e);
    let timer = backoff(&a, NOW + 1., 1000.);
    assert_eq!(a.iter().filter(|a| matches!(a, Action::ManagedHttp(m) if matches!(&**m, HttpAction::Cancel { preparation, signing: None } if *preparation==p))).count(),1);
    e.handle(NOW + 1001., Event::TimerFired { timer });
    let a = drain(&mut e);
    let (next, _) = prep(&a);
    assert_ne!(p, next);
    let r = release_managed(&mut e, &a, NOW + 1001.);
    reply(&mut e, &r, NOW + 1002., 200, GOOD);
    assert!(e.store().loaded().markets);
    let a = reply(
        &mut e,
        &HttpRequest {
            id: p.attempt().request,
            ..r
        },
        NOW + 1003.,
        200,
        GOOD,
    );
    assert!(a.is_empty());
}
#[test]
fn held_prepare_expires_without_bind_or_publication() {
    let (mut e, a) = boot(false, false);
    let (p, _) = prep(&a);
    let timer = a.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
        _ => None,
    });
    assert!(timer.is_some(), "bound starts at admission");
    e.handle(
        NOW + 20_000.,
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    let a = drain(&mut e);
    backoff(&a, NOW + 20_000., 1000.);
    assert!(!e.http_preparation_pending(p));
    e.http_prepared(
        NOW + 20_001.,
        p,
        Prepared {
            id: 1,
            expires_at: None,
        },
    );
    let a = drain(&mut e);
    no_outbound(&a);
    assert!(!e.store().loaded().markets);
}
#[test]
fn held_bind_cannot_release_at_the_new_bound() {
    let (mut e, a) = boot(false, false);
    let (p, _) = prep(&a);
    let (permit, _) = bind(&mut e, p, NOW);
    assert!(e.claim_http(NOW + 20_000., permit).is_err());
    let a = drain(&mut e);
    backoff(&a, NOW + 20_000., 1000.);
    assert!(!e.store().loaded().markets);
}
#[test]
fn physical_timeout_yields_to_backoff_then_a_fresh_root() {
    let (mut e, a) = boot(false, false);
    let r = release_managed(&mut e, &a, NOW);
    let timer = a.iter().find_map(|a| match a {
        Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
        _ => None,
    });
    assert!(timer.is_some());
    e.handle(
        NOW + 20_000.,
        Event::TimerFired {
            timer: timer.unwrap(),
        },
    );
    let a = drain(&mut e);
    let timer = backoff(&a, NOW + 20_000., 1000.);
    e.handle(NOW + 21_000., Event::TimerFired { timer });
    let a = drain(&mut e);
    let (next, _) = prep(&a);
    assert_ne!(r.id, next.attempt().request);
    let a = reply(&mut e, &r, NOW + 21_001., 200, GOOD);
    assert!(a.is_empty());
    assert!(!e.store().loaded().markets);
}
#[test]
fn successful_catalog_refresh_has_a_new_root_and_keeps_old_failures_inert() {
    let (mut e, a) = boot(false, false);
    let r = release_managed(&mut e, &a, NOW);
    let a = reply(&mut e, &r, NOW + 1., 200, GOOD);
    let timer = refresh_timer(&a, NOW + 1.);
    e.handle(NOW + 300_001., Event::TimerFired { timer });
    let a = drain(&mut e);
    let (next, _) = prep(&a);
    assert_ne!(r.id, next.attempt().request);
    e.handle(
        NOW + 300_002.,
        Event::HttpFailed {
            request: r.id,
            error: "late old failure",
        },
    );
    assert!(drain(&mut e).is_empty());
    let r = release_managed(&mut e, &a, NOW + 300_002.);
    wire(&r, false);
    let a = reply(&mut e, &r, NOW + 300_003., 200, GOOD);
    refresh_timer(&a, NOW + 300_003.);
}
#[test]
fn native_transport_failures_keep_exponential_startup_backoff() {
    let (mut e, a) = boot(false, false);
    let mut r = native_release(&mut e, &a, NOW);
    let mut now = NOW;
    for nominal in [1000., 2000.] {
        e.handle(
            now,
            Event::HttpFailed {
                request: r.id,
                error: "fixture network failure",
            },
        );
        let a = drain(&mut e);
        let timer = backoff(&a, now, nominal);
        now += nominal;
        e.handle(now, Event::TimerFired { timer });
        let a = drain(&mut e);
        r = native_release(&mut e, &a, now);
    }
    reply(&mut e, &r, now + 1., 200, GOOD);
    assert!(e.store().loaded().markets);
}
fn strict_native(status: u16, body: &str) {
    let (mut e, a) = boot(false, false);
    let r = native_release(&mut e, &a, NOW);
    let a = reply(&mut e, &r, NOW + 1., status, body);
    backoff(&a, NOW + 1., 1000.);
    assert!(!e.store().loaded().markets);
}
#[test]
fn native_http_status_failure_never_authenticates() {
    strict_native(500, r#"{"c":500,"m":"failure"}"#);
}
#[test]
fn native_public_auth_code_failure_never_refreshes_login() {
    strict_native(200, r#"{"c":1000,"m":"Session expired"}"#);
}
#[test]
fn native_empty_catalog_uses_startup_failure() {
    strict_native(200, r#"{"r":[]}"#);
}
#[test]
fn live_missing_envelope_stream_error_uses_native_backoff() {
    strict_native(200, r#"{"unexpected":[]}"#);
}
#[test]
fn live_non_utf8_integrity_failure_uses_native_backoff() {
    let (mut e, a) = boot(false, false);
    let r = native_release(&mut e, &a, NOW);
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
            data: &[0xff],
        },
    );
    e.handle(NOW + 1., Event::HttpEnd { request: r.id });
    let a = drain(&mut e);
    backoff(&a, NOW + 1., 1000.);
    assert!(!e.store().loaded().markets);
}
#[test]
fn stale_stream_error_cannot_replace_the_live_catalog() {
    let (mut e, a) = boot(false, false);
    let r = release_managed(&mut e, &a, NOW);
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: r.id,
            error: "network",
        },
    );
    let a = drain(&mut e);
    let timer = backoff(&a, NOW + 1., 1000.);
    e.handle(NOW + 1001., Event::TimerFired { timer });
    let a = drain(&mut e);
    let next = release_managed(&mut e, &a, NOW + 1001.);
    e.handle(
        NOW + 1002.,
        Event::HttpStart {
            request: r.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 1002.,
        Event::HttpChunk {
            request: r.id,
            data: &[0xff],
        },
    );
    e.handle(NOW + 1002., Event::HttpEnd { request: r.id });
    assert!(drain(&mut e).is_empty());
    reply(&mut e, &next, NOW + 1003., 200, GOOD);
    assert!(e.store().loaded().markets);
}
#[test]
fn full_context_pool_refuses_before_transport_and_recovers_with_all_peers() {
    let (mut e, a) = boot(false, true);
    assert!(a.is_empty());
    for command in 1000..1256 {
        e.handle_command(
            NOW,
            Command::FetchCandles {
                command,
                symbol: "__peer".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60_000),
        )
        .unwrap();
        let a = drain(&mut e);
        no_outbound(&a);
        no_commands(&a);
    }
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["start_catalog".into()],
        }),
    );
    let a = drain(&mut e);
    let timer = backoff(&a, NOW, 1000.);
    assert!(!e.store().loaded().markets);
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["retire_peer".into()],
        }),
    );
    let a = drain(&mut e);
    assert_eq!(
        a.iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 1000, .. })
            ))
            .count(),
        1
    );
    e.handle(NOW + 1000., Event::TimerFired { timer });
    let a = drain(&mut e);
    let r = release_managed(&mut e, &a, NOW + 1000.);
    reply(&mut e, &r, NOW + 1001., 200, GOOD);
    assert!(e.store().loaded().markets);
    e.handle(
        NOW + 1002.,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert!(drain(&mut e).is_empty());
    assert_eq!(e.venue().validated, 255);
}
fn option_refused(command: Command) {
    let (mut e, a) = boot(false, true);
    assert!(a.is_empty());
    assert!(!e.venue().capabilities().options);
    e.handle_command(NOW, command, Some(60_000)).unwrap();
    let a = drain(&mut e);
    no_outbound(&a);
    let errors: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 700,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "Options are not enabled on this account");
    assert_eq!(errors[0].kind, None);
    assert_eq!(errors[0].outcome, None);
    assert_eq!(e.venue().forwarded, 0);
    assert!(!e.store().loaded().markets);
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    assert!(drain(&mut e).is_empty());
    assert_eq!(e.venue().forwarded, 1);
}
#[test]
fn option_chain_is_refused_before_venue_entry() {
    option_refused(Command::FetchOptionChain {
        command: 700,
        query: serde_json::from_str(r#"{"underlying":"BTC","right":"call"}"#).unwrap(),
    });
}
#[test]
fn option_expiries_are_refused_before_venue_entry() {
    option_refused(Command::FetchOptionExpiries {
        command: 700,
        underlying: "BTC".into(),
    });
}
#[test]
fn account_greeks_are_refused_before_venue_entry() {
    option_refused(Command::FetchAccountGreeks { command: 700 });
}

#[test]
fn validated_items_cannot_publish_after_mid_stream_expiry() {
    let (mut e, actions) = boot(false, false);
    let request = native_release(&mut e, &actions, NOW);
    e.handle(
        NOW + 1.,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        NOW + 19_999.,
        Event::HttpChunk {
            request: request.id,
            data: GOOD.strip_suffix('}').unwrap().as_bytes(),
        },
    );
    assert_eq!(
        e.venue().items,
        1,
        "real streamed row reached the item callback"
    );
    assert!(!e.store().loaded().markets);
    drain(&mut e);
    e.handle(
        NOW + 20_000.,
        Event::HttpChunk {
            request: request.id,
            data: b"}",
        },
    );
    e.handle(
        NOW + 20_000.,
        Event::HttpEnd {
            request: request.id,
        },
    );
    let actions = drain(&mut e);
    assert!(!e.store().loaded().markets);
    no_outbound(&actions);
    no_commands(&actions);
}
#[test]
fn successful_publication_ignores_a_late_failure_without_backoff() {
    let (mut e, actions) = boot(false, false);
    let request = native_release(&mut e, &actions, NOW);
    reply(&mut e, &request, NOW + 1., 200, GOOD);
    assert!(e.store().loaded().markets);
    e.handle(
        NOW + 2.,
        Event::HttpFailed {
            request: request.id,
            error: "late after success",
        },
    );
    assert!(drain(&mut e).is_empty());
    assert!(e.store().loaded().markets);
}
fn fill_all_peer_contexts(e: &mut Engine<Probe>, now: f64) {
    for command in 1000..1256 {
        e.handle_command(
            now,
            Command::FetchCandles {
                command,
                symbol: "__peer".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60_000),
        )
        .unwrap();
        let actions = drain(e);
        no_outbound(&actions);
        no_commands(&actions);
    }
    assert_eq!(e.venue().peers.len(), 256);
}
#[test]
fn success_releases_the_catalog_context_capacity() {
    let (mut e, actions) = boot(false, false);
    let request = release_managed(&mut e, &actions, NOW);
    reply(&mut e, &request, NOW + 1., 200, GOOD);
    assert!(e.store().loaded().markets);
    fill_all_peer_contexts(&mut e, NOW + 2.);
}
#[test]
fn failure_releases_the_catalog_context_capacity_before_backoff() {
    let (mut e, actions) = boot(false, false);
    let request = release_managed(&mut e, &actions, NOW);
    e.handle(
        NOW + 1.,
        Event::HttpFailed {
            request: request.id,
            error: "network",
        },
    );
    let actions = drain(&mut e);
    backoff(&actions, NOW + 1., 1000.);
    fill_all_peer_contexts(&mut e, NOW + 2.);
}

#[test]
fn suppressed_feed_keeps_the_unanswered_catalog_deadline_and_backoff() {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Grvt::new(Options {
            public_data_only: true,
            ..Options::default()
        }),
        defer_start: false,
        forwarded: 0,
        items: 0,
        peers: vec![],
        validated: 0,
    });
    e.set_public_feed_policy(safe_cex_engine::PublicFeedPolicy::Shared)
        .unwrap();
    e.start(NOW);
    let actions = drain(&mut e);
    let (first, _) = prep(&actions);
    release_managed(&mut e, &actions, NOW);
    let timer = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20_000. => Some(*timer),
            _ => None,
        })
        .expect("unanswered shared-feed catalog remains bounded");
    e.handle(NOW + 20_000., Event::TimerFired { timer });
    let actions = drain(&mut e);
    let retry = backoff(&actions, NOW + 20_000., 1000.);
    assert!(!e.store().loaded().markets);
    e.handle(NOW + 21_000., Event::TimerFired { timer: retry });
    let actions = drain(&mut e);
    let (next, _) = prep(&actions);
    assert_ne!(first, next);
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::WsConnect { .. } | Action::WsSend { .. }))
    );
}

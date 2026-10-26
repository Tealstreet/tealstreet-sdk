//! Local originals for unsigned seed custody; production is unchanged.
mod support;

use safe_cex_engine::log::code;
use safe_cex_engine::request_context::{Claim, HttpAction, Preparation, Prepared, RoutingIntent};
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Method, Notice, Response, TimerId, Venue,
};
use safe_cex_venue_woo::{Options, Pending, Woo};
use support::{MARKETS, TICKERS};

type Driver = Engine<Woo>;
const FUNDING: &str = r#"{"success":true,"rows":[{"symbol":"PERP_BTC_USDT","est_funding_rate":0.002,"next_funding_time":1700057600000}]}"#;
const EMPTY: &str = r#"{"success":true,"rows":[]}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seed {
    Markets,
    Tickers,
    Funding,
}
impl Seed {
    fn path(self) -> &'static str {
        match self {
            Self::Markets => "/v1/public/info",
            Self::Tickers => "/v1/public/futures",
            Self::Funding => "/v1/public/funding_rates",
        }
    }
    fn body(self) -> &'static str {
        match self {
            Self::Markets => MARKETS,
            Self::Tickers => TICKERS,
            Self::Funding => FUNDING,
        }
    }
}
struct Packet {
    request: HttpRequest,
    preparation: Option<Preparation>,
    intent: Option<RoutingIntent>,
}
fn packet(actions: &[Action], seed: Seed) -> Packet {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with(seed.path()) => Some(Packet {
                request: request.clone(),
                preparation: None,
                intent: None,
            }),
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.ends_with(seed.path()) => Some(Packet {
                    request: request.clone(),
                    preparation: Some(*preparation),
                    intent: Some(*intent),
                }),
                _ => None,
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("actual {seed:?} request: {actions:?}"))
}
fn release<V: Venue<Pending = Pending>>(e: &mut Engine<V>, p: &Packet, now: f64) {
    if let Some(preparation) = p.preparation {
        e.http_prepared(
            now,
            preparation,
            Prepared {
                id: p.request.id.0,
                expires_at: None,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let permit = actions
            .iter()
            .find_map(|action| match action {
                Action::ManagedHttp(action) => match action.as_ref() {
                    HttpAction::Bind {
                        request, permit, ..
                    } if request.id == p.request.id => Some(*permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_else(|| panic!("real Bind for exact request: {actions:?}"));
        assert!(matches!(e.claim_http(now, permit), Ok(Claim::Released(_))));
        assert!(e.http_release(p.request.id).is_some());
    }
}
fn response<V: Venue<Pending = Pending>>(
    e: &mut Engine<V>,
    p: &Packet,
    now: f64,
    status: u16,
    body: &str,
) {
    e.handle(
        now,
        Event::HttpResponse {
            request: p.request.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
}
fn issued(seed: Seed, released: bool) -> (Driver, Packet, f64, Vec<Action>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.);
    let actions: Vec<_> = e.drain_actions().collect();
    let markets = packet(&actions, Seed::Markets);
    if seed == Seed::Markets {
        if released {
            release(&mut e, &markets, 0.);
        }
        return (e, markets, 0., actions);
    }
    release(&mut e, &markets, 0.);
    response(&mut e, &markets, 10., 200, MARKETS);
    let actions: Vec<_> = e.drain_actions().collect();
    let tickers = packet(&actions, Seed::Tickers);
    if seed == Seed::Tickers {
        if released {
            release(&mut e, &tickers, 10.);
        }
        return (e, tickers, 10., actions);
    }
    release(&mut e, &tickers, 10.);
    response(&mut e, &tickers, 20., 200, TICKERS);
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = timer_at(&actions, 60_020.);
    e.handle(60_020., Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let funding = packet(&actions, Seed::Funding);
    if released {
        release(&mut e, &funding, 60_020.);
    }
    (e, funding, 60_020., actions)
}
fn timer_at(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap_or_else(|| panic!("actual timer advertised at {at}: {actions:?}"))
}
fn assert_unpublished(e: &Driver, seed: Seed) {
    match seed {
        Seed::Markets => {
            assert!(!e.store().loaded().markets);
            assert!(e.store().markets().is_empty());
        }
        Seed::Tickers => {
            assert!(!e.store().loaded().tickers);
            assert!(e.store().tickers().is_empty());
        }
        Seed::Funding => {
            let ticker = e.store().ticker("BTCUSDT").unwrap();
            assert_eq!(
                (ticker.funding_rate, ticker.funding_time),
                (0.0001, 1700028800000.)
            );
        }
    }
}
fn warns(actions: &[Action], expected: &'static str) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::Notice(Notice::Event(n)) if n.code == expected))
        .count()
}
fn managed(seed: Seed) {
    let (mut e, p, now, actions) = issued(seed, false);
    assert!(
        p.preparation.is_some(),
        "{seed:?} requires a genuine managed Prepare"
    );
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(_))),
        "no dispatch before Claim"
    );
    assert_eq!(p.intent, Some(RoutingIntent::Primary));
    assert_eq!(p.request.timeout_ms, Some(20_000));
    assert!(p.request.route.is_none());
    release(&mut e, &p, now);
    response(&mut e, &p, now + 1., 200, seed.body());
    match seed {
        Seed::Markets => assert!(e.store().loaded().markets),
        Seed::Tickers => assert!(e.store().loaded().tickers),
        Seed::Funding => assert_eq!(e.store().ticker("BTCUSDT").unwrap().funding_rate, 0.002),
    }
}
#[test]
fn markets_require_managed_prepare_and_primary_egress() {
    managed(Seed::Markets);
}
#[test]
fn tickers_require_managed_prepare_and_primary_egress() {
    managed(Seed::Tickers);
}
#[test]
fn funding_requires_managed_prepare_and_primary_egress() {
    managed(Seed::Funding);
}

fn held(seed: Seed) {
    let (mut e, p, now, _) = issued(seed, false);
    response(&mut e, &p, now + 1., 200, seed.body());
    assert_unpublished(&e, seed);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
        "unreleased response cannot start successors: {actions:?}"
    );
}
#[test]
fn held_markets_cannot_publish_or_advance() {
    held(Seed::Markets);
}
#[test]
fn held_tickers_cannot_publish_or_advance() {
    held(Seed::Tickers);
}
#[test]
fn held_funding_cannot_publish() {
    held(Seed::Funding);
}

fn equality(seed: Seed, offset: f64) {
    let (mut e, p, now, _) = issued(seed, true);
    response(&mut e, &p, now + offset, 200, seed.body());
    assert_unpublished(&e, seed);
}
// NEW managed observation bound; the raw native requests have no such deadline.
#[test]
fn markets_new_deadline_equality_rejects_data() {
    equality(Seed::Markets, 20_000.);
}
#[test]
fn tickers_new_deadline_equality_rejects_data() {
    equality(Seed::Tickers, 20_000.);
}
#[test]
fn funding_new_deadline_equality_rejects_data() {
    equality(Seed::Funding, 20_000.);
}
#[test]
fn markets_new_deadline_late_rejects_data() {
    equality(Seed::Markets, 20_001.);
}
#[test]
fn tickers_new_deadline_late_rejects_data() {
    equality(Seed::Tickers, 20_001.);
}
#[test]
fn funding_new_deadline_late_rejects_data() {
    equality(Seed::Funding, 20_001.);
}

fn held_streamed_bad_row(seed: Seed) {
    let (mut e, p, now, _) = issued(seed, false);
    e.handle(
        now + 1.,
        Event::HttpStart {
            request: p.request.id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        now + 1.,
        Event::HttpChunk {
            request: p.request.id,
            data: br#"{"success":true,"rows":[{"symbol":"PERP_BTC_USDT","bad":true},"#,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(
        warns(&actions, code::ROW_SKIPPED),
        0,
        "held DATA cannot emit row diagnostics"
    );
    assert_unpublished(&e, seed);
}
#[test]
fn held_markets_stream_cannot_emit_row_skipped() {
    held_streamed_bad_row(Seed::Markets);
}
#[test]
fn held_tickers_stream_cannot_emit_row_skipped() {
    held_streamed_bad_row(Seed::Tickers);
}

#[test]
fn native_three_route_unsigned_wire_and_financial_values() {
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        let (mut e, p, now, _) = issued(seed, true);
        assert_eq!(p.request.method, Method::Get);
        assert_eq!(p.request.url, format!("https://api.woox.io{}", seed.path()));
        assert!(p.request.route.is_none());
        assert!(p.request.headers.is_empty() && p.request.signer_headers.is_empty());
        assert!(p.request.body.is_none());
        response(&mut e, &p, now + 1., 200, seed.body());
        let market = e.store().market("BTCUSDT").unwrap();
        assert_eq!(
            (
                market.precision.amount,
                market.precision.price,
                market.limits.amount.min,
                market.limits.leverage.max
            ),
            (0.001, 0.1, 0.001, 50.)
        );
        assert_eq!(e.venue().price_range("BTCUSDT"), Some(0.01));
        if seed != Seed::Markets {
            let ticker = e.store().ticker("BTCUSDT").unwrap();
            assert_eq!(
                (
                    ticker.last,
                    ticker.mark,
                    ticker.index,
                    ticker.percentage,
                    ticker.volume,
                    ticker.quote_volume
                ),
                (60000., 60001., 59999., 20., 4., 240000.)
            );
            let expected = if seed == Seed::Funding {
                (0.002, 1700057600000.)
            } else {
                (0.0001, 1700028800000.)
            };
            assert_eq!((ticker.funding_rate, ticker.funding_time), expected);
        }
    }
}
#[test]
fn native_empty_discovery_retries_as_a_distinct_observation() {
    for seed in [Seed::Markets, Seed::Tickers] {
        let (mut e, p, now, _) = issued(seed, true);
        response(&mut e, &p, now + 1., 200, EMPTY);
        assert_unpublished(&e, seed);
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(warns(&actions, code::REQUEST_FAILED), 2);
        let (timer, due) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms }
                    if (750. ..=1000.).contains(&(*at_ms - now - 1.)) =>
                {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("native keyed jittered backoff");
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
        );
        e.handle(due, Event::TimerFired { timer });
        let next = packet(&e.drain_actions().collect::<Vec<_>>(), seed);
        assert_ne!(next.request.id, p.request.id);
        release(&mut e, &next, due);
        response(&mut e, &next, due + 1., 200, seed.body());
        if seed == Seed::Markets {
            assert!(e.store().loaded().markets);
        } else {
            assert!(e.store().loaded().tickers);
        }
    }
}
#[test]
fn native_funding_cadence_is_dispatch_driven_and_failure_warns_only() {
    let (mut e, p, now, issued_actions) = issued(Seed::Funding, true);
    let next_timer = timer_at(&issued_actions, now + 60_000.);
    e.handle(
        now + 1.,
        Event::HttpFailed {
            request: p.request.id,
            error: "funding outage",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(warns(&actions, code::REQUEST_FAILED), 1);
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::SetTimer { .. } | Action::Http(_) | Action::ManagedHttp(_)
    )));
    assert_unpublished(&e, Seed::Funding);
    e.handle(now + 60_000., Event::TimerFired { timer: next_timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let next = packet(&actions, Seed::Funding);
    assert_ne!(next.request.id, p.request.id);
    timer_at(&actions, now + 120_000.);
}
#[test]
fn funding_new_deadline_excludes_the_previous_cadence_response() {
    let (mut e, first, now, actions) = issued(Seed::Funding, true);
    e.handle(
        now + 60_000.,
        Event::TimerFired {
            timer: timer_at(&actions, now + 60_000.),
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let second = packet(&actions, Seed::Funding);
    release(&mut e, &second, now + 60_000.);
    let newer = FUNDING.replace("0.002", "0.003");
    response(&mut e, &second, now + 60_001., 200, &newer);
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().funding_rate, 0.003);
    response(&mut e, &first, now + 60_002., 200, FUNDING);
    assert_eq!(
        e.store().ticker("BTCUSDT").unwrap().funding_rate,
        0.003,
        "NEW bound excludes the previous cadence response after its own expiry"
    );
}
#[test]
fn native_duplicate_success_cannot_republish_or_restart_successors() {
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        let (mut e, p, now, _) = issued(seed, true);
        response(&mut e, &p, now + 1., 200, seed.body());
        e.drain_actions().for_each(drop);
        let before = e.snapshot_frame(now + 3.).to_vec();
        response(&mut e, &p, now + 2., 200, seed.body());
        assert_eq!(e.snapshot_frame(now + 3.), before);
        assert!(e.drain_actions().next().is_none());
    }
}

#[test]
fn native_transport_and_status_failures_keep_diagnostics_and_recovery() {
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        for rejected_status in [false, true] {
            let (mut e, p, now, _) = issued(seed, true);
            if rejected_status {
                response(&mut e, &p, now + 1., 503, EMPTY);
            } else {
                e.handle(
                    now + 1.,
                    Event::HttpFailed {
                        request: p.request.id,
                        error: "native seed outage",
                    },
                );
            }
            let actions: Vec<_> = e.drain_actions().collect();
            let text = if rejected_status {
                "WOO read rejected"
            } else {
                "native seed outage"
            };
            assert!(actions.iter().any(|a|matches!(a,Action::Notice(Notice::Event(n)) if n.code==code::REQUEST_FAILED && n.message==text)));
            let retries=actions.iter().filter(|a|matches!(a,Action::SetTimer { at_ms,.. } if (750. ..=1000.).contains(&(*at_ms-now-1.)))).count();
            assert_eq!(retries, usize::from(seed != Seed::Funding));
            assert_unpublished(&e, seed);
        }
    }
}
#[test]
fn native_partial_catalogue_publishes_valid_rows_and_keeps_reread_and_tickers() {
    let (mut e, p, now, _) = issued(Seed::Markets, true);
    let body = format!(
        "{},{{\"symbol\":\"PERP_ETH_USDT\",\"bad\":true}}]}}",
        MARKETS.strip_suffix("]}").unwrap()
    );
    response(&mut e, &p, now + 1., 200, &body);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().market("BTCUSDT").unwrap().precision.amount, 0.001);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(warns(&actions, code::ROW_SKIPPED) > 0);
    packet(&actions, Seed::Tickers);
    assert!(actions.iter().any(
        |a| matches!(a,Action::SetTimer { at_ms,.. } if (750. ..=1000.).contains(&(*at_ms-now-1.)))
    ));
}

#[test]
fn nonfinite_start_creates_no_read_timer_or_diagnostic() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        e.start(now);
        assert!(e.drain_actions().next().is_none());
        assert!(!e.store().loaded().markets);
    }
}

fn nonfinite_delivery(failure: bool) {
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let (mut e, p, now, _) = issued(seed, true);
            let before = e.snapshot_frame(now + 3.).to_vec();
            if failure {
                e.handle(
                    invalid,
                    Event::HttpFailed {
                        request: p.request.id,
                        error: "nonfinite transport",
                    },
                );
            } else {
                response(&mut e, &p, invalid, 200, seed.body());
            }
            assert_eq!(
                e.snapshot_frame(now + 3.),
                before,
                "nonfinite DATA cannot change any Store section"
            );
            let actions: Vec<_> = e.drain_actions().collect();
            let mut cancelled = 0;
            for action in actions {
                match action {
                    Action::ManagedHttp(action) => match *action {
                        HttpAction::Cancel { preparation, .. } => {
                            assert_eq!(Some(preparation), p.preparation);
                            cancelled += 1;
                        }
                        _ => panic!("nonfinite delivery cannot prepare, bind or reissue"),
                    },
                    Action::ClearTimer { .. } => (),
                    other => panic!("nonfinite delivery permits exact cleanup only: {other:?}"),
                }
            }
            assert_eq!(
                cancelled, 1,
                "the exact released preparation is cancelled once"
            );
            assert!(e.http_release(p.request.id).is_none());
        }
    }
}
#[test]
fn nonfinite_released_response_is_exact_cleanup_only() {
    nonfinite_delivery(false);
}
#[test]
fn nonfinite_released_failure_is_exact_cleanup_only() {
    nonfinite_delivery(true);
}

#[test]
fn precommit_typed_matrix_retires_once_and_only_transient_rows_recover() {
    use safe_cex_engine::request_context::Error;
    use safe_cex_engine::{ErrorKind, VenueError};
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        for (reason, kind, recovery) in [
            (Error::Capacity, None, true),
            (Error::TimedOut, None, true),
            (Error::Invalid, Some(ErrorKind::Network), true),
            (Error::Invalid, Some(ErrorKind::Timeout), true),
            (Error::Invalid, Some(ErrorKind::Internal), true),
            (Error::Invalid, Some(ErrorKind::InvalidInput), false),
            (Error::Invalid, Some(ErrorKind::Unsupported), false),
            (Error::Invalid, Some(ErrorKind::AuthInvalid), false),
            (Error::Invalid, None, false),
            (Error::Invalid, Some(ErrorKind::Rejected), false),
            (Error::Invalid, Some(ErrorKind::AuthExpired), false),
            (Error::Invalid, Some(ErrorKind::AuthPermission), false),
            (Error::Invalid, Some(ErrorKind::RateLimited), false),
            (Error::Invalid, Some(ErrorKind::VenueUnavailable), false),
            (Error::Duplicate, None, false),
            (Error::State, Some(ErrorKind::Internal), false),
            (Error::Revoked, Some(ErrorKind::Network), false),
            (Error::Stale, None, false),
            (Error::RetryForbidden, None, false),
        ] {
            let (mut e, p, now, _) = issued(seed, false);
            let preparation = p
                .preparation
                .expect("genuine managed preparation for typed matrix");
            let mut error = VenueError::new("same message across typed failures");
            error.kind = kind;
            e.http_preparation_failed(now + 1., preparation, reason, error);
            assert_unpublished(&e, seed);
            let actions: Vec<_> = e.drain_actions().collect();
            assert!(
                !actions.iter().any(|a| matches!(a, Action::Http(_))),
                "precommit failure cannot dispatch"
            );
            let timers = actions
                .iter()
                .filter(|a| matches!(a, Action::SetTimer { .. }))
                .count();
            assert_eq!(
                timers,
                usize::from(recovery && seed != Seed::Funding),
                "{seed:?} {reason:?} {kind:?}"
            );
            if reason != Error::Stale {
                assert_eq!(
                    warns(&actions, code::REQUEST_FAILED),
                    1 + usize::from(recovery && seed != Seed::Funding),
                    "owned finite refusal warns even when its reason is Revoked/State/Duplicate"
                );
            }
            e.http_preparation_failed(now + 2., preparation, reason, VenueError::new("duplicate"));
            let duplicate: Vec<_> = e.drain_actions().collect();
            assert_eq!(
                warns(&duplicate, code::REQUEST_FAILED),
                0,
                "duplicate callback cannot warn twice"
            );
            assert!(
                !duplicate
                    .iter()
                    .any(|a| matches!(a, Action::SetTimer { .. } | Action::Http(_))),
                "consumed preparation cannot rearm recovery"
            );
        }
    }
}

mod pressure {
    use super::*;
    use safe_cex_engine::book::Book;
    use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy, Scope};
    use safe_cex_engine::socket_lifecycle::{
        Control, EncodedBatch, Epoch, FeedId, Interest, LifecycleEvent, SocketError,
    };
    use safe_cex_engine::{Command, Cx, SocketId, Store, VenueError};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct CreditAudit {
        native: Woo,
        peers: Vec<Context>,
        cleanup: Option<TimerId>,
        credits: Rc<RefCell<Vec<usize>>>,
        peer_count: usize,
        cleanup_ms: f64,
        reserve_on: Rc<RefCell<Option<TimerId>>>,
    }
    fn policy() -> Policy {
        Policy {
            operation: Operation::Read,
            credential: None,
            timeout_ms: 600_000,
            pacing: None,
            retry: RetryPolicy::Never,
        }
    }
    impl CreditAudit {
        fn probe(&self, cx: &mut Cx<'_, Pending>) {
            let mut probes = vec![];
            while let Ok(context) = cx.http_context(Scope::Background, policy()) {
                probes.push(context);
                assert!(probes.len() <= 256);
            }
            self.credits.borrow_mut().push(probes.len());
            for context in probes {
                cx.finish_http_context(&context).unwrap();
            }
        }
    }
    impl Venue for CreditAudit {
        type Pending = Pending;
        fn capabilities(&self) -> safe_cex_engine::Capabilities {
            self.native.capabilities()
        }
        fn guard_order_mutations(&self) -> bool {
            self.native.guard_order_mutations()
        }
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            for _ in 0..self.peer_count {
                self.peers
                    .push(cx.http_context(Scope::Background, policy()).unwrap());
            }
            self.cleanup = Some(cx.set_timer(self.cleanup_ms));
            self.native.start(cx);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
            self.native.on_command(cx, command)
        }
        fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
            self.native.on_open(cx, socket)
        }
        fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
            self.native.on_close(cx, socket, code)
        }
        fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
            self.native.on_message(cx, socket, text)
        }
        fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, item: &str) {
            self.native.on_item(cx, p, item)
        }
        fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
            self.probe(cx);
            self.native.on_response(cx, p, r);
            self.probe(cx);
        }
        fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: &str) {
            self.native.on_request_failed(cx, p, error)
        }
        fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, error: VenueError) {
            self.native.on_http_failed(cx, p, error)
        }
        fn on_http_preparation_failed(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            p: Pending,
            reason: safe_cex_engine::request_context::Error,
            error: VenueError,
        ) {
            self.native.on_http_preparation_failed(cx, p, reason, error)
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
            if self.cleanup == Some(timer) {
                self.cleanup = None;
                for context in self.peers.drain(..) {
                    cx.finish_http_context(&context).unwrap();
                }
                self.probe(cx);
            } else {
                if *self.reserve_on.borrow() == Some(timer) {
                    *self.reserve_on.borrow_mut() = None;
                    for _ in 0..256 {
                        self.peers
                            .push(cx.http_context(Scope::Background, policy()).unwrap());
                    }
                }
                self.native.on_timer(cx, timer);
            }
        }
        fn socket_control(
            &self,
            store: &Store,
            feed: FeedId,
            authenticated: bool,
            text: &str,
        ) -> Control {
            self.native.socket_control(store, feed, authenticated, text)
        }
        fn on_socket_event(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            event: LifecycleEvent,
        ) -> Result<(), SocketError> {
            self.native.on_socket_event(cx, event)
        }
        fn on_socket_interests_released(
            &mut self,
            cx: &mut Cx<'_, Pending>,
            epoch: Epoch,
            pending: Pending,
        ) -> Result<(), SocketError> {
            self.native.on_socket_interests_released(cx, epoch, pending)
        }
        fn encode_socket_interests(
            &self,
            store: &Store,
            feed: FeedId,
            subscribe: bool,
            items: &[Interest],
            emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError> {
            self.native
                .encode_socket_interests(store, feed, subscribe, items, emit)
        }
        fn book(&self, symbol: &str) -> Option<&Book> {
            self.native.book(symbol)
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            self.native.books()
        }
    }
    #[test]
    fn completing_catalogue_releases_the_only_credit_before_ticker_admission() {
        let credits = Rc::new(RefCell::new(vec![]));
        let mut e = safe_cex_engine::test_support::seeded_engine(CreditAudit {
            native: Woo::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
            peers: vec![],
            cleanup: None,
            credits: credits.clone(),
            peer_count: 255,
            cleanup_ms: 2.,
            reserve_on: Rc::new(RefCell::new(None)),
        });
        e.start(0.);
        let actions: Vec<_> = e.drain_actions().collect();
        let cleanup = timer_at(&actions, 2.);
        let markets = packet(&actions, Seed::Markets);
        release(&mut e, &markets, 0.);
        response(&mut e, &markets, 1., 200, MARKETS);
        assert_eq!(
            &*credits.borrow(),
            &[0, 0],
            "255 genuine peers plus predecessor consume every credit; successor inherits the exact retired credit"
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let tickers = packet(&actions, Seed::Tickers);
        assert!(
            tickers.preparation.is_some(),
            "the successor was admitted under sole-credit pressure"
        );
        release(&mut e, &tickers, 1.);
        response(&mut e, &tickers, 1.5, 200, TICKERS);
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
        assert_eq!(&*credits.borrow(), &[0, 0, 0, 1]);
        e.drain_actions().for_each(drop);
        e.handle(2., Event::TimerFired { timer: cleanup });
        assert_eq!(
            &*credits.borrow(),
            &[0, 0, 0, 1, 256],
            "all peers remained owned and all credits are reusable"
        );
    }
    #[test]
    fn full_admission_retires_nothing_and_recovery_reuses_all_credits() {
        let credits = Rc::new(RefCell::new(vec![]));
        let mut e = safe_cex_engine::test_support::seeded_engine(CreditAudit {
            native: Woo::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
            peers: vec![],
            cleanup: None,
            credits: credits.clone(),
            peer_count: 256,
            cleanup_ms: 2.,
            reserve_on: Rc::new(RefCell::new(None)),
        });
        e.start(0.);
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
            "full capacity cannot dispatch an unowned observation"
        );
        let cleanup = timer_at(&actions, 2.);
        let (retry, due) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if (750. ..=1000.).contains(at_ms) => {
                    Some((*timer, *at_ms))
                }
                _ => None,
            })
            .expect("one native keyed backoff after new admission refusal");
        e.handle(2., Event::TimerFired { timer: cleanup });
        assert_eq!(&*credits.borrow(), &[256]);
        e.drain_actions().for_each(drop);
        e.handle(due, Event::TimerFired { timer: retry });
        let p = packet(&e.drain_actions().collect::<Vec<_>>(), Seed::Markets);
        assert!(p.preparation.is_some());
        release(&mut e, &p, due);
        response(&mut e, &p, due + 1., 200, MARKETS);
        assert!(e.store().loaded().markets);
    }
    #[test]
    fn full_funding_dispatch_keeps_exactly_one_native_cadence_then_recovers() {
        let credits = Rc::new(RefCell::new(vec![]));
        let reserve_on = Rc::new(RefCell::new(None));
        let mut e = safe_cex_engine::test_support::seeded_engine(CreditAudit {
            native: Woo::new(Options {
                public_data_only: true,
                ..Default::default()
            }),
            peers: vec![],
            cleanup: None,
            credits: credits.clone(),
            peer_count: 0,
            cleanup_ms: 100_000.,
            reserve_on: reserve_on.clone(),
        });
        e.start(0.);
        let actions: Vec<_> = e.drain_actions().collect();
        let cleanup = timer_at(&actions, 100_000.);
        let markets = packet(&actions, Seed::Markets);
        release(&mut e, &markets, 0.);
        response(&mut e, &markets, 10., 200, MARKETS);
        let tickers = packet(&e.drain_actions().collect::<Vec<_>>(), Seed::Tickers);
        release(&mut e, &tickers, 10.);
        response(&mut e, &tickers, 20., 200, TICKERS);
        let actions: Vec<_> = e.drain_actions().collect();
        let funding = timer_at(&actions, 60_020.);
        *reserve_on.borrow_mut() = Some(funding);
        e.handle(60_020., Event::TimerFired { timer: funding });
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_))),
            "refused Funding has no physical request"
        );
        let next = timer_at(&actions, 120_020.);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::SetTimer { .. }))
                .count(),
            1,
            "admission refusal keeps native cadence, without an extra recovery timer"
        );
        e.handle(100_000., Event::TimerFired { timer: cleanup });
        assert_eq!(credits.borrow().last(), Some(&256));
        e.drain_actions().for_each(drop);
        e.handle(120_020., Event::TimerFired { timer: next });
        let p = packet(&e.drain_actions().collect::<Vec<_>>(), Seed::Funding);
        assert!(p.preparation.is_some());
        release(&mut e, &p, 120_020.);
        response(&mut e, &p, 120_021., 200, FUNDING);
        assert_eq!(e.store().ticker("BTCUSDT").unwrap().funding_rate, 0.002);
    }
}

#[test]
fn nonfinite_funding_timer_creates_no_work_and_preserves_its_live_peer() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let (mut e, p, now, actions) = issued(Seed::Funding, true);
        let timer = timer_at(&actions, now + 60_000.);
        let before = e.snapshot_frame(now + 3.).to_vec();
        e.handle(invalid, Event::TimerFired { timer });
        assert_eq!(e.snapshot_frame(now + 3.), before);
        assert!(
            e.drain_actions().next().is_none(),
            "nonfinite dispatch is inert, including diagnostics and timers"
        );
        response(&mut e, &p, now + 5., 200, FUNDING);
        assert_eq!(
            e.store().ticker("BTCUSDT").unwrap().funding_rate,
            0.002,
            "unrelated issued HTTP remains live"
        );
    }
}
#[test]
fn native_saved_primary_never_signs_public_seed_routes() {
    for public_data_only in [true, false] {
        let mut e = safe_cex_engine::test_support::seeded_engine(Woo::new(Options {
            key: Some("fixture-key".into()),
            secret: Some("fixture-secret".into()),
            public_data_only,
            ..Default::default()
        }));
        e.start(0.);
        let markets = packet(&e.drain_actions().collect::<Vec<_>>(), Seed::Markets);
        assert!(markets.request.headers.is_empty() && markets.request.signer_headers.is_empty());
        release(&mut e, &markets, 0.);
        response(&mut e, &markets, 10., 200, MARKETS);
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(
                    a,
                    Action::WsConnect {
                        channel: Some(safe_cex_engine::SocketChannel::Private),
                        ..
                    }
                ))
                .count(),
            usize::from(!public_data_only)
        );
        let tickers = packet(&actions, Seed::Tickers);
        assert!(tickers.request.headers.is_empty() && tickers.request.signer_headers.is_empty());
        release(&mut e, &tickers, 10.);
        response(&mut e, &tickers, 20., 200, TICKERS);
        assert!(e.store().loaded().markets && e.store().loaded().tickers);
    }
}

#[test]
fn owned_deadline_failure_keeps_native_recovery_once_without_data_authority() {
    for seed in [Seed::Markets, Seed::Tickers, Seed::Funding] {
        let (mut e, p, now, _) = issued(seed, true);
        assert!(
            p.preparation.is_some(),
            "managed owner needed to test genuine expiry custody"
        );
        e.handle(
            now + 20_000.,
            Event::HttpFailed {
                request: p.request.id,
                error: "released at exact managed deadline",
            },
        );
        assert_unpublished(&e, seed);
        assert!(e.http_release(p.request.id).is_none());
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::SetTimer { .. }))
                .count(),
            usize::from(seed != Seed::Funding)
        );
        assert_eq!(
            warns(&actions, code::REQUEST_FAILED),
            1 + usize::from(seed != Seed::Funding)
        );
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        for action in actions {
            if let Action::ManagedHttp(action) = action {
                match *action {
                    HttpAction::Cancel { preparation, .. } => {
                        assert_eq!(Some(preparation), p.preparation)
                    }
                    _ => panic!("expiry cannot prepare or bind another observation inline"),
                }
            }
        }
        e.handle(
            now + 20_001.,
            Event::HttpFailed {
                request: p.request.id,
                error: "duplicate expiry",
            },
        );
        assert!(
            e.drain_actions().next().is_none(),
            "only the genuinely returned Pending may recover once"
        );
    }
}

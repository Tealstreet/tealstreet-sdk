//! Real Background ownership around the private setting helper; native startup is separate.
use super::*;
use crate::{Options, Pending};
use safe_cex_engine::request_context::{Context, Error, HttpAction, Preparation};
use safe_cex_engine::{Action, Command, Engine, ErrorKind, Event, HttpRequest, Venue, VenueError};

struct Fixture {
    inner: TxFlow,
    pressure: bool,
    peers: Vec<Context>,
    failures: Vec<VenueError>,
    reclaimed: usize,
}
fn peer_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 120_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
impl Venue for Fixture {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.pressure {
            for _ in 0..256 {
                self.peers
                    .push(cx.http_context(Scope::Background, peer_policy()).unwrap());
            }
        }
        self.inner
            .account
            .setting_queue
            .extend(["1".into(), "2".into()]);
        self.inner.next_setting(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Command::SetTickerSymbols { symbols } = command else {
            panic!("fixture command")
        };
        match symbols[0].as_str() {
            "free-one" => {
                cx.finish_http_context(&self.peers.pop().unwrap()).unwrap();
            }
            "replace" => {
                self.inner.account_disconnected(cx);
                self.inner.account.setting_queue.push_back("2".into());
                self.inner.next_setting(cx);
            }
            "revision" => self.inner.account.settings_revision += 1,
            "retire" => {
                cx.finish_http_context(self.inner.account.setting_context.as_ref().unwrap())
                    .unwrap();
            }
            "credits" => {
                for peer in self.peers.drain(..) {
                    cx.finish_http_context(&peer).unwrap();
                }
                for _ in 0..256 {
                    self.peers
                        .push(cx.http_context(Scope::Background, peer_policy()).unwrap());
                }
                assert!(matches!(
                    cx.http_context(Scope::Background, peer_policy()),
                    Err(Error::Capacity)
                ));
                self.reclaimed = self.peers.len();
                for peer in self.peers.drain(..) {
                    cx.finish_http_context(&peer).unwrap();
                }
            }
            _ => panic!("fixture command"),
        }
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_http_preparation_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        pending: Pending,
        reason: Error,
        error: VenueError,
    ) {
        self.failures.push(error.clone());
        self.inner
            .on_http_preparation_failed(cx, pending, reason, error);
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.failures.push(error.clone());
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: safe_cex_engine::SocketId, _: &str) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
type Driver = Engine<Fixture>;
fn engine(pressure: bool, now: f64) -> Driver {
    let mut e = safe_cex_engine::test_support::seeded_engine(Fixture {
        inner: TxFlow::new(Options {
            testnet: true,
            application_id: Some("0x0000000000000000000000000000000000000001".into()),
            ..Default::default()
        }),
        pressure,
        peers: vec![],
        failures: vec![],
        reclaimed: 0,
    });
    e.start(now);
    e
}
fn command(e: &mut Driver, now: f64, name: &str) {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![name.into()],
        }),
    );
}
fn timer(a: &[Action], at: f64) -> TimerId {
    let ids: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 1);
    ids[0]
}
fn token(e: &mut Driver) -> Preparation {
    e.drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare { preparation, .. } => Some(preparation),
                _ => None,
            },
            _ => None,
        })
        .unwrap()
}
fn released(e: &mut Driver, now: f64) -> HttpRequest {
    safe_cex_engine::test_support::prepared_actions(e, now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap()
}
fn response(e: &mut Driver, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn no_send(a: &[Action]) {
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Http(_) | Action::ManagedHttp(_)))
    );
}
fn reclaimed(e: &mut Driver, now: f64) {
    command(e, now, "credits");
    assert_eq!(e.venue().reclaimed, 256);
}
#[test]
fn full_pressure_refuses_once_then_recovers_one_slot_and_reclaims_every_credit() {
    let mut e = engine(true, 1000.);
    let refused: Vec<_> = e.drain_actions().collect();
    no_send(&refused);
    let next = timer(&refused, 1100.);
    assert!(!e.venue().inner.account.inflight.contains("setting"));
    assert!(e.venue().inner.account.setting_context.is_none());
    assert_eq!(e.venue().peers.len(), 256);
    command(&mut e, 1001., "free-one");
    e.handle(1100., Event::TimerFired { timer: next });
    let r = released(&mut e, 1100.);
    assert!(r.body.as_deref().unwrap().contains(r#""coin":"2""#));
    response(&mut e, &r, 1101., "{");
    let done: Vec<_> = e.drain_actions().collect();
    timer(&done, 1201.);
    assert!(e.venue().inner.account.setting_context.is_none());
    reclaimed(&mut e, 1102.);
}
#[test]
fn nonfinite_admission_creates_no_send_or_recovery_timer() {
    for now in [f64::NAN, f64::INFINITY] {
        let mut e = engine(false, now);
        let a: Vec<_> = e.drain_actions().collect();
        no_send(&a);
        assert!(!a.iter().any(|a| matches!(a, Action::SetTimer { .. })));
        assert!(!e.venue().inner.account.inflight.contains("setting"));
        assert!(e.venue().inner.account.setting_context.is_none());
    }
}
#[test]
fn typed_precommit_failure_retires_once_and_keeps_no_data_authority() {
    let mut e = engine(false, 1000.);
    let p = token(&mut e);
    let error = Error::State.read_venue_error();
    e.http_preparation_failed(1001., p, Error::State, error.clone());
    let a: Vec<_> = e.drain_actions().collect();
    timer(&a, 1101.);
    assert!(!e.http_preparation_pending(p));
    assert_eq!(e.venue().failures.len(), 1);
    assert_eq!(e.venue().failures[0].kind, error.kind);
    assert_eq!(
        e.venue().failures[0].outcome,
        Some(safe_cex_engine::Outcome::NotSent)
    );
    assert!(e.venue().inner.account.setting_context.is_none());
    reclaimed(&mut e, 1002.);
}
#[test]
fn old_preparation_cannot_clear_a_new_generation_owner() {
    let mut e = engine(false, 1000.);
    let old = token(&mut e);
    command(&mut e, 1001., "replace");
    let current = token(&mut e);
    let marker = e.venue().inner.account.setting_context.clone();
    e.http_prepared(
        1002.,
        old,
        safe_cex_engine::request_context::Prepared {
            id: old.attempt().request.0,
            expires_at: None,
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert!(e.http_preparation_pending(current));
    assert_eq!(e.venue().inner.account.setting_context, marker);
    e.http_preparation_failed(
        1003.,
        current,
        Error::State,
        Error::State.read_venue_error(),
    );
    let done: Vec<_> = e.drain_actions().collect();
    timer(&done, 1103.);
    reclaimed(&mut e, 1004.);
}
#[test]
fn old_released_data_and_duplicate_failure_cannot_touch_replacement() {
    let mut e = engine(false, 1000.);
    let old = released(&mut e, 1000.);
    command(&mut e, 1001., "replace");
    let current = token(&mut e);
    let marker = e.venue().inner.account.setting_context.clone();
    response(&mut e, &old, 1002., "{");
    e.handle(
        1002.,
        Event::HttpFailed {
            request: old.id,
            error: "old failed",
        },
    );
    assert!(e.drain_actions().next().is_none());
    assert_eq!(e.venue().inner.account.setting_context, marker);
    assert!(e.http_preparation_pending(current));
    e.http_preparation_failed(
        1003.,
        current,
        Error::State,
        Error::State.read_venue_error(),
    );
    e.drain_actions().for_each(drop);
    reclaimed(&mut e, 1004.);
}
#[test]
fn revision_stale_current_reply_retires_before_decode_and_keeps_native_cadence() {
    let mut e = engine(false, 1000.);
    let r = released(&mut e, 1000.);
    command(&mut e, 1001., "revision");
    e.drain_actions().for_each(drop);
    response(&mut e, &r, 1002., "{");
    let a: Vec<_> = e.drain_actions().collect();
    timer(&a, 1102.);
    assert!(!a.iter().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Event(n)) if n.code==code::STREAM_MALFORMED)));
    assert!(e.venue().inner.account.setting_context.is_none());
    reclaimed(&mut e, 1003.);
}
#[test]
fn released_network_and_timeout_retire_without_a_mutation_outcome() {
    for timeout in [false, true] {
        let mut e = engine(false, 1000.);
        let r = released(&mut e, 1000.);
        let now = if timeout { 21_000. } else { 1001. };
        if timeout {
            response(&mut e, &r, now, "{");
        } else {
            e.handle(
                now,
                Event::HttpFailed {
                    request: r.id,
                    error: "setting network",
                },
            );
        }
        let a: Vec<_> = e.drain_actions().collect();
        timer(&a, now + 100.);
        assert_eq!(e.venue().failures.len(), 1);
        assert_eq!(
            e.venue().failures[0].kind,
            Some(if timeout {
                ErrorKind::Timeout
            } else {
                ErrorKind::Network
            })
        );
        assert!(e.venue().failures[0].outcome.is_none());
        reclaimed(&mut e, now + 1.);
    }
}
#[test]
fn engine_context_retirement_cancels_without_native_followup() {
    let mut e = engine(false, 1000.);
    let p = token(&mut e);
    command(&mut e, 1001., "retire");
    let a: Vec<_> = e.drain_actions().collect();
    assert!(!e.http_preparation_pending(p));
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::SetTimer { .. } | Action::Http(_)))
    );
    assert!(e.venue().failures.is_empty());
    reclaimed(&mut e, 1002.);
}

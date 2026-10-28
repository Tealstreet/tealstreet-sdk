//! Original timing controls; the explicit progress mapping changes at API adoption.
#![cfg(feature = "managed-sockets")]
mod support;
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, Epoch, FeedId,
    FeedPolicy, Interest, Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
use safe_cex_engine::{Action, Cx, Engine, Event, SocketChannel, SocketId, Store, TimerId, Venue};
use std::collections::HashMap;

fn progress_evidence() -> Control {
    Control::Auth(AuthEvidence::Progress)
}
struct Native {
    feed: Option<FeedId>,
    auth: AuthMode,
    snapshot: bool,
    waiting_challenge: bool,
    challenge: bool,
    acks: usize,
    accepted_challenges: usize,
    ready: usize,
    authenticated: usize,
    messages: usize,
    epochs: Vec<Epoch>,
    signed_auth: bool,
}
impl Venue for Native {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let policy = FeedPolicy {
            role: SocketChannel::Private,
            supervision: Config {
                connect_timeout_ms: 30_000.,
                auth_timeout_ms: (self.auth != AuthMode::None).then_some(15_000.),
                idle_timeout_ms: 60_000.,
                ping_interval_ms: None,
                backoff_base_ms: 1_000.,
                backoff_cap_ms: 16_000.,
                stable_ms: 30_000.,
            },
            auth: self.auth,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::AnyText,
            controls: ControlPolicy::Native,
            invalidation: Invalidation::Required,
            subscription_interval_ms: 0.,
            close_floors: &[],
        };
        let feed = if self.snapshot {
            cx.register_socket_feed_with_snapshot(policy)
        } else {
            cx.register_socket_feed(policy)
        }
        .unwrap();
        self.feed = Some(feed);
        cx.set_socket_interests(feed, vec![Interest::from_wire_args("ordinary-interest")])
            .unwrap();
        cx.connect_socket(feed, "wss://auth-progress.test").unwrap();
    }
    fn socket_control(&self, _: &Store, _: FeedId, authenticated: bool, text: &str) -> Control {
        if text == "direct-progress" {
            return progress_evidence();
        }
        if text
            .strip_prefix("challenge:")
            .is_some_and(|s| !s.is_empty())
            && self.waiting_challenge
        {
            return progress_evidence();
        }
        if text == "ack" && self.challenge && self.acks + 1 == 2 && !authenticated {
            return Control::Auth(AuthEvidence::Accepted);
        }
        if text == "pong" {
            return Control::Pong;
        }
        Control::Other
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Opened(epoch) => {
                self.epochs.push(epoch);
                self.waiting_challenge = self.auth == AuthMode::Required;
                self.challenge = false;
                self.acks = 0;
                if self.waiting_challenge {
                    cx.send(epoch.socket(), "key-challenge");
                }
            }
            LifecycleEvent::Ready { .. } => self.ready += 1,
            LifecycleEvent::Authenticated { .. } => self.authenticated += 1,
            LifecycleEvent::Retired { .. } => {
                self.waiting_challenge = false;
                self.challenge = false;
                self.acks = 0;
            }
            LifecycleEvent::Reconnect(feed) => {
                cx.connect_socket(feed, "wss://auth-progress.test")?;
            }
            _ => {}
        }
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &Store,
        _: FeedId,
        _: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for interest in interests {
            emit(EncodedBatch {
                text: interest.wire_args.to_string(),
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, text: &str) {
        self.messages += 1;
        if text
            .strip_prefix("challenge:")
            .is_some_and(|s| !s.is_empty())
            && self.waiting_challenge
        {
            self.waiting_challenge = false;
            self.challenge = true;
            self.accepted_challenges += 1;
            if self.signed_auth {
                cx.send_signed(socket, "auth-subscription".into(), Box::new(AuthRecipe));
            } else {
                cx.send(socket, "auth-subscription");
            }
        }
        if text == "ack" && self.challenge {
            self.acks += 1;
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: safe_cex_engine::Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::Command) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
struct Harness {
    engine: Engine<Native>,
    socket: SocketId,
    output: Vec<Action>,
    timers: HashMap<TimerId, f64>,
}
impl Harness {
    fn new() -> Self {
        Self::policy(AuthMode::Required, false)
    }
    fn policy(auth: AuthMode, snapshot: bool) -> Self {
        Self::configured(auth, snapshot, false)
    }
    fn configured(auth: AuthMode, snapshot: bool, signed_auth: bool) -> Self {
        let mut engine = support::seeded_engine(Native {
            feed: None,
            auth,
            snapshot,
            waiting_challenge: false,
            challenge: false,
            acks: 0,
            accepted_challenges: 0,
            ready: 0,
            authenticated: 0,
            messages: 0,
            epochs: vec![],
            signed_auth,
        });
        engine.start(0.);
        let mut h = Self {
            engine,
            socket: SocketId(0),
            output: vec![],
            timers: HashMap::new(),
        };
        h.collect();
        h.socket = h
            .output
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        h
    }
    fn collect(&mut self) {
        for a in self.engine.drain_actions() {
            match &a {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(*timer, *at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(timer);
                }
                _ => {}
            }
            self.output.push(a);
        }
    }
    fn event(&mut self, at: f64, event: Event<'_>) {
        self.engine.handle(at, event);
        self.collect();
    }
    fn open(&mut self, at: f64) {
        self.event(
            at,
            Event::WsOpened {
                socket: self.socket,
            },
        );
    }
    fn message(&mut self, at: f64, text: &str) {
        self.event(
            at,
            Event::WsMessage {
                socket: self.socket,
                text,
            },
        );
    }
    fn closed(&self, socket: SocketId) -> bool {
        self.output
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    }
    fn sends(&self, text: &str) -> usize {
        self.output
            .iter()
            .filter(|a| matches!(a, Action::WsSend { text: t, .. } if t == text))
            .count()
    }
}
#[test]
fn accepted_challenge_renews_open_auth_bound_without_readiness_or_interests() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(14_000., "challenge:nonce");
    assert_eq!(h.engine.venue().accepted_challenges, 1);
    assert_eq!(h.sends("auth-subscription"), 1);
    assert_eq!(h.sends("ordinary-interest"), 0);
    assert_eq!(h.engine.venue().ready, 0);
    assert_eq!(h.engine.venue().authenticated, 0);
    h.message(15_000., "ordinary-data");
    assert!(
        !h.closed(h.socket),
        "accepted progress renews the 15s auth clock"
    );
    assert_eq!(h.engine.venue().messages, 2);
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn final_ack_after_old_bound_releases_once_then_progress_is_inert() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(14_000., "challenge:nonce");
    h.message(18_000., "ack");
    h.message(18_001., "ack");
    assert_eq!(
        h.sends("ordinary-interest"),
        1,
        "all ACKs may finish inside the renewed bound"
    );
    assert_eq!(h.engine.venue().ready, 1);
    assert_eq!(h.engine.venue().authenticated, 1);
    h.message(18_002., "ack");
    h.message(28_999., "challenge:duplicate");
    h.message(29_000., "ordinary-data");
    assert!(!h.closed(h.socket));
    assert_eq!(h.sends("ordinary-interest"), 1);
}
#[test]
fn renewed_equality_expires_before_codec_without_timer_delivery() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(14_000., "challenge:nonce");
    h.message(28_999., "ordinary-data");
    assert!(!h.closed(h.socket), "live twin before the renewed boundary");
    let messages = h.engine.venue().messages;
    h.message(29_000., "ack");
    assert!(h.closed(h.socket));
    assert_eq!(h.engine.venue().messages, messages);
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn canceled_old_watch_cannot_expire_the_renewed_auth_phase() {
    let mut h = Harness::new();
    h.open(0.);
    let old = *h.timers.iter().min_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
    h.message(14_000., "challenge:nonce");
    h.event(15_000., Event::TimerFired { timer: old });
    assert!(
        !h.closed(h.socket),
        "progress replaces the old watch custody"
    );
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn delayed_open_gets_its_own_auth_window_and_connect_is_separate() {
    let mut h = Harness::new();
    h.message(16_000., "challenge:not-open");
    assert!(!h.closed(h.socket));
    assert_eq!(h.engine.venue().messages, 0);
    h.open(20_000.);
    h.message(34_999., "ordinary-data");
    assert!(!h.closed(h.socket));
    h.message(35_000., "challenge:too-late");
    assert!(h.closed(h.socket));
    assert_eq!(h.engine.venue().accepted_challenges, 0);
}
#[test]
fn original_equality_fences_challenge_before_native_callback() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(15_000., "challenge:late");
    assert!(h.closed(h.socket));
    assert_eq!(h.engine.venue().messages, 0);
    assert_eq!(h.sends("auth-subscription"), 0);
}
#[test]
fn empty_challenge_and_pongs_do_not_renew_auth() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(10_000., "challenge:");
    h.message(14_999., "pong");
    h.message(15_000., "ordinary-data");
    assert!(h.closed(h.socket));
    assert_eq!(h.engine.venue().accepted_challenges, 0);
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn complete_auth_before_original_bound_is_a_working_release_control() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(1_000., "challenge:nonce");
    h.message(2_000., "ack");
    assert_eq!(h.sends("ordinary-interest"), 0);
    h.message(2_001., "ack");
    assert_eq!(h.sends("ordinary-interest"), 1);
    h.message(16_000., "ordinary-data");
    assert!(!h.closed(h.socket));
}
#[test]
fn default_unauthenticated_policy_keeps_immediate_ready_behavior() {
    let mut h = Harness::policy(AuthMode::None, false);
    h.open(0.);
    assert_eq!(h.sends("ordinary-interest"), 1);
    assert_eq!(h.engine.venue().ready, 1);
    assert_eq!(h.sends("key-challenge"), 0);
}
#[test]
fn progress_cannot_extend_the_connect_origin_snapshot_barrier() {
    let mut h = Harness::policy(AuthMode::Required, true);
    h.open(0.);
    h.message(14_000., "challenge:nonce");
    h.message(15_000., "ordinary-data");
    assert!(h.closed(h.socket));
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn retired_challenge_does_not_enter_a_replacement_epoch() {
    let mut h = Harness::new();
    h.open(0.);
    let old = h.socket;
    h.event(
        1_000.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let (&retry, &at) = h.timers.iter().min_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    h.event(at, Event::TimerFired { timer: retry });
    h.socket = h
        .output
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } if *socket != old => Some(*socket),
            _ => None,
        })
        .expect("genuine shared retry connects a replacement");
    h.open(at + 1.);
    assert_ne!(h.engine.venue().epochs[0], h.engine.venue().epochs[1]);
    h.event(
        at + 2.,
        Event::WsMessage {
            socket: old,
            text: "challenge:stale",
        },
    );
    assert_eq!(h.engine.venue().messages, 0);
    assert_eq!(h.engine.venue().accepted_challenges, 0);
    h.message(at + 3., "challenge:fresh");
    assert_eq!(h.engine.venue().accepted_challenges, 1);
}

#[test]
fn duplicate_challenge_does_not_renew_the_second_phase_again() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(1_000., "challenge:nonce");
    h.message(14_000., "challenge:duplicate");
    assert_eq!(h.engine.venue().accepted_challenges, 1);
    assert_eq!(h.sends("auth-subscription"), 1);
    h.message(15_999., "ordinary-data");
    assert!(
        !h.closed(h.socket),
        "the first accepted challenge provides the live twin"
    );
    let messages = h.engine.venue().messages;
    h.message(16_000., "ack");
    assert!(
        h.closed(h.socket),
        "a duplicate cannot extend the 16s bound to 29s"
    );
    assert_eq!(h.engine.venue().messages, messages);
}

#[test]
fn nonfinite_challenge_clocks_cannot_grant_progress() {
    for invalid in [f64::NAN, f64::INFINITY] {
        let mut h = Harness::new();
        h.open(0.);
        h.message(10_000., "ordinary-data");
        let messages = h.engine.venue().messages;
        h.message(invalid, "challenge:bad-clock");
        assert_eq!(h.engine.venue().messages, messages);
        assert_eq!(h.engine.venue().accepted_challenges, 0);
        h.message(15_000., "ordinary-data");
        assert!(h.closed(h.socket));
        assert_eq!(h.sends("ordinary-interest"), 0);
    }
}

struct AuthRecipe;
impl safe_cex_engine::SignStep for AuthRecipe {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError> {
        use safe_cex_engine::signing::{
            Algorithm, Context, KeyRef, Operation, PayloadPart, SignRequest,
        };
        Ok(SignRequest {
            receipt: Default::default(),
            context: Context::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("fixture-auth".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"accepted-challenge".to_vec(),
                }],
            }],
        })
    }
    fn freshness_ms(&self) -> f64 {
        60_000.
    }
    fn apply(
        &mut self,
        _: &mut Action,
        _: &safe_cex_engine::signing::SignResult,
    ) -> Result<(), safe_cex_engine::signing::SignError> {
        Ok(())
    }
}
fn parked_auth() -> (Harness, safe_cex_engine::RequestId) {
    let mut h = Harness::configured(AuthMode::Required, false, true);
    h.open(0.);
    h.message(14_000., "challenge:nonce");
    let attempt = h
        .output
        .iter()
        .find_map(|a| match a {
            Action::Sign { attempt, .. } => Some(*attempt),
            _ => None,
        })
        .expect("real parked external signing attempt");
    assert_eq!(h.sends("auth-subscription"), 0);
    assert_eq!(h.sends("ordinary-interest"), 0);
    (h, attempt)
}
fn signed(attempt: safe_cex_engine::RequestId) -> Event<'static> {
    Event::Signed {
        attempt,
        result: Ok(safe_cex_engine::signing::SignResult::new(
            vec![vec![0; 32]],
            Default::default(),
        )),
    }
}
#[test]
fn direct_progress_is_inert_for_none_and_observe_policies() {
    for auth in [AuthMode::None, AuthMode::Observe] {
        let mut h = Harness::policy(auth, false);
        h.open(0.);
        let timers = h.timers.clone();
        let ready = h.engine.venue().ready;
        h.message(14_000., "direct-progress");
        assert_eq!(h.timers, timers, "ignored evidence cannot rearm a watch");
        assert_eq!(h.engine.venue().ready, ready);
        assert_eq!(h.engine.venue().authenticated, 0);
        assert_eq!(h.sends("ordinary-interest"), 1);
        h.message(15_000., "ordinary-data");
        assert_eq!(h.closed(h.socket), auth == AuthMode::Observe);
    }
}
#[test]
fn direct_progress_after_accepted_does_not_rearm_or_release() {
    let mut h = Harness::new();
    h.open(0.);
    h.message(1_000., "challenge:nonce");
    h.message(2_000., "ack");
    h.message(2_001., "ack");
    let timers = h.timers.clone();
    h.message(14_000., "direct-progress");
    assert_eq!(h.timers, timers);
    assert_eq!(h.sends("ordinary-interest"), 1);
    assert_eq!(h.engine.venue().authenticated, 1);
    assert_eq!(h.engine.venue().ready, 1);
}
#[test]
fn parked_provider_release_checks_the_renewed_equality_boundary() {
    for at in [28_999., 29_000.] {
        let (mut h, attempt) = parked_auth();
        h.event(at, signed(attempt));
        assert_eq!(h.sends("auth-subscription"), usize::from(at < 29_000.));
        assert_eq!(h.closed(h.socket), at == 29_000.);
        assert_eq!(h.sends("ordinary-interest"), 0);
        assert_eq!(h.engine.venue().ready, 0);
    }
}
#[test]
fn completed_provider_still_checks_auth_at_the_final_delayed_drain() {
    let (mut h, attempt) = parked_auth();
    h.engine.handle(28_999., signed(attempt));
    h.engine.prepare_dispatch(29_000.);
    h.collect();
    assert_eq!(h.sends("auth-subscription"), 0);
    assert!(h.closed(h.socket));
    assert_eq!(h.sends("ordinary-interest"), 0);
}
#[test]
fn parked_old_signer_cannot_release_after_real_reconnect() {
    let (mut h, attempt) = parked_auth();
    let old = h.socket;
    h.event(
        14_001.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let (&retry, &at) = h.timers.iter().min_by(|a, b| a.1.total_cmp(b.1)).unwrap();
    h.event(at, Event::TimerFired { timer: retry });
    h.socket = h
        .output
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } if *socket != old => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.open(at + 1.);
    let messages = h.engine.venue().messages;
    h.event(at + 2., signed(attempt));
    assert_eq!(h.engine.venue().messages, messages);
    assert_eq!(h.sends("auth-subscription"), 0);
    assert_eq!(h.sends("ordinary-interest"), 0);
    assert!(!h.closed(h.socket));
    assert_ne!(h.engine.venue().epochs[0], h.engine.venue().epochs[1]);
}

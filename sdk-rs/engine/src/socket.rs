//! Replayable socket liveness and retry policy; wire protocols remain venue-owned.
use crate::backoff::Backoff;
use crate::{Cx, TimerId};

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub connect_timeout_ms: f64,
    pub auth_timeout_ms: Option<f64>,
    pub idle_timeout_ms: f64,
    pub ping_interval_ms: Option<f64>,
    pub backoff_base_ms: f64,
    pub backoff_cap_ms: f64,
    pub stable_ms: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            connect_timeout_ms: 10_000.0,
            auth_timeout_ms: None,
            idle_timeout_ms: 60_000.0,
            ping_interval_ms: None,
            backoff_base_ms: 1_000.0,
            backoff_cap_ms: 30_000.0,
            stable_ms: 30_000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timeout {
    Connect,
    Auth,
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tick {
    Unhandled,
    Handled,
    Ping,
    Reconnect,
    TimedOut(Timeout),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Stopped,
    Connecting,
    Authenticating,
    Ready,
    Waiting,
}

pub struct Supervisor {
    config: Config,
    phase: Phase,
    #[cfg(feature = "managed-sockets")]
    snapshot_deadline: Option<f64>,
    since: f64,
    received: f64,
    idle_enabled: bool,
    next_ping: Option<f64>,
    watch: Option<TimerId>,
    watch_at: Option<f64>,
    retry: Option<TimerId>,
    failures: u32,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Supervisor {
    // Venue constants let validation fold away at construction.
    #[inline(always)]
    pub fn new(config: Config) -> Self {
        let positive = |n: f64| n.is_finite() && n > 0.0;
        assert!(positive(config.connect_timeout_ms));
        assert!(config.auth_timeout_ms.is_none_or(positive));
        assert!(positive(config.idle_timeout_ms));
        assert!(config.ping_interval_ms.is_none_or(positive));
        assert!(positive(config.backoff_base_ms));
        assert!(positive(config.backoff_cap_ms));
        assert!(config.backoff_cap_ms >= config.backoff_base_ms);
        assert!(config.stable_ms.is_finite() && config.stable_ms >= 0.0);
        Self {
            config,
            phase: Phase::Stopped,
            #[cfg(feature = "managed-sockets")]
            snapshot_deadline: None,
            since: 0.0,
            received: 0.0,
            idle_enabled: true,
            next_ping: None,
            watch: None,
            watch_at: None,
            retry: None,
            failures: 0,
        }
    }

    /// Call before connecting (or acquiring a connection token).
    pub fn connecting<P>(&mut self, cx: &mut Cx<'_, P>) {
        self.cancel(cx);
        #[cfg(feature = "managed-sockets")]
        {
            self.snapshot_deadline = None;
        }
        self.phase = Phase::Connecting;
        self.since = cx.now;
        self.received = cx.now;
        self.arm(cx);
    }

    pub fn opened<P>(&mut self, cx: &mut Cx<'_, P>) {
        if self.phase != Phase::Connecting {
            return;
        }
        self.phase = if self.config.auth_timeout_ms.is_some() {
            Phase::Authenticating
        } else {
            Phase::Ready
        };
        self.since = cx.now;
        self.received = cx.now;
        self.next_ping = self.config.ping_interval_ms.map(|ms| cx.now + ms);
        self.rearm(cx);
    }

    pub fn authenticated<P>(&mut self, cx: &mut Cx<'_, P>) {
        if self.phase == Phase::Authenticating {
            #[cfg(feature = "managed-sockets")]
            {
                self.snapshot_deadline = None;
            }
            self.phase = Phase::Ready;
            self.since = cx.now;
            self.received = cx.now;
            self.rearm(cx);
        }
    }

    /// Call only for traffic belonging to the current connection.
    pub fn received(&mut self, now: f64) {
        if matches!(self.phase, Phase::Authenticating | Phase::Ready) {
            self.received = now;
            if self.phase == Phase::Ready && now - self.since >= self.config.stable_ms {
                self.failures = 0;
            }
        }
    }

    #[cfg(feature = "managed-sockets")]
    pub(crate) fn idle_enabled(&self) -> bool {
        self.idle_enabled
    }

    /// Activate receive supervision only after the declared liveness proof.
    /// Existing callers start enabled. Repeated activation does not refresh
    /// the receive timestamp; traffic admission remains a separate operation.
    pub fn set_idle_enabled<P>(&mut self, cx: &mut Cx<'_, P>, enabled: bool) {
        if self.idle_enabled == enabled {
            return;
        }
        self.idle_enabled = enabled;
        if enabled {
            self.received = cx.now;
        }
        if enabled
            && self
                .watch_at
                .zip(self.next_watch_at(cx.now))
                .is_some_and(|(old, new)| old <= new)
        {
            return;
        }
        self.rearm(cx);
    }

    /// The same absolute expiry predicate used by timer delivery. Managed
    /// ingress checks it before admitting an open, ACK or liveness frame, so
    /// delayed timer delivery cannot revive an expired connection. This query
    /// does not consume the timer or change phase.
    pub fn expired(&self, now: f64) -> Option<Timeout> {
        #[cfg(feature = "managed-sockets")]
        if matches!(self.phase, Phase::Connecting | Phase::Authenticating)
            && self.snapshot_deadline.is_some_and(|at| now >= at)
        {
            return Some(Timeout::Auth);
        }
        match self.phase {
            Phase::Connecting if now >= self.since + self.config.connect_timeout_ms => {
                Some(Timeout::Connect)
            }
            Phase::Authenticating
                if self
                    .config
                    .auth_timeout_ms
                    .is_some_and(|ms| now >= self.since + ms) =>
            {
                Some(Timeout::Auth)
            }
            Phase::Authenticating | Phase::Ready
                if self.idle_enabled && now >= self.received + self.config.idle_timeout_ms =>
            {
                Some(Timeout::Idle)
            }
            _ => None,
        }
    }

    #[cfg(feature = "managed-sockets")]
    pub(crate) fn retain_snapshot_deadline<P>(&mut self, cx: &mut Cx<'_, P>, at: f64) {
        assert!(self.phase == Phase::Connecting && at.is_finite() && at > cx.now);
        self.snapshot_deadline = Some(at);
        if self
            .watch_at
            .zip(self.next_watch_at(cx.now))
            .is_none_or(|(old, new)| new < old)
        {
            self.rearm(cx);
        }
    }

    pub fn set_ping_interval<P>(&mut self, cx: &mut Cx<'_, P>, interval_ms: Option<f64>) {
        assert!(interval_ms.is_none_or(|ms| ms.is_finite() && ms > 0.0));
        self.config.ping_interval_ms = interval_ms;
        if matches!(self.phase, Phase::Authenticating | Phase::Ready) {
            self.next_ping = interval_ms.map(|ms| cx.now + ms);
            self.rearm(cx);
        }
    }

    /// Negotiate a protocol's welcome/auth bound without extending its start time.
    pub fn set_auth_timeout<P>(&mut self, cx: &mut Cx<'_, P>, timeout_ms: Option<f64>) {
        assert!(timeout_ms.is_none_or(|ms| ms.is_finite() && ms > 0.0));
        assert!(self.phase != Phase::Authenticating || timeout_ms.is_some());
        self.config.auth_timeout_ms = timeout_ms;
        if self.phase == Phase::Authenticating {
            self.rearm(cx);
        }
    }

    /// Retire supervision and schedule exactly one retry. The venue closes its socket.
    pub fn retry<P>(&mut self, cx: &mut Cx<'_, P>) {
        self.retry_after(cx, 0.0);
    }

    /// The minimum may enforce an upstream rate-limit/token-acquisition pause.
    pub fn retry_after<P>(&mut self, cx: &mut Cx<'_, P>, minimum_ms: f64) {
        if self.retry.is_some() {
            return;
        }
        self.cancel(cx);
        let nominal = Backoff::new(self.config.backoff_base_ms, 31, self.config.backoff_cap_ms)
            .delay(self.failures);
        self.failures = self.failures.saturating_add(1);
        let delay = cx.jittered(nominal).max(minimum_ms.max(0.0));
        self.phase = Phase::Waiting;
        self.retry = Some(cx.set_timer(delay));
    }

    pub fn cancel<P>(&mut self, cx: &mut Cx<'_, P>) {
        if let Some(timer) = self.watch.take() {
            cx.clear_timer(timer);
        }
        if let Some(timer) = self.retry.take() {
            cx.clear_timer(timer);
        }
        self.watch_at = None;
        #[cfg(feature = "managed-sockets")]
        {
            self.snapshot_deadline = None;
        }
        self.phase = Phase::Stopped;
        self.next_ping = None;
    }

    pub fn owns_timer(&self, timer: TimerId) -> bool {
        self.watch == Some(timer) || self.retry == Some(timer)
    }

    pub fn is_waiting(&self) -> bool {
        self.phase == Phase::Waiting
    }

    pub fn on_timer<P>(&mut self, cx: &mut Cx<'_, P>, timer: TimerId) -> Tick {
        if self.retry == Some(timer) {
            self.retry = None;
            self.phase = Phase::Stopped;
            return Tick::Reconnect;
        }
        if self.watch != Some(timer) {
            return Tick::Unhandled;
        }
        self.watch = None;
        self.watch_at = None;
        if let Some(timeout) = self.expired(cx.now) {
            // Retire first; late activity cannot rescue an expired connection.
            self.phase = Phase::Stopped;
            self.next_ping = None;
            return Tick::TimedOut(timeout);
        }
        let ping = self.next_ping.is_some_and(|at| cx.now >= at);
        if ping {
            self.next_ping = self.config.ping_interval_ms.map(|ms| cx.now + ms);
        }
        self.arm(cx);
        if ping { Tick::Ping } else { Tick::Handled }
    }

    fn rearm<P>(&mut self, cx: &mut Cx<'_, P>) {
        if let Some(timer) = self.watch.take() {
            cx.clear_timer(timer);
        }
        self.arm(cx);
    }

    fn arm<P>(&mut self, cx: &mut Cx<'_, P>) {
        if let Some(at) = self.next_watch_at(cx.now) {
            self.watch = Some(cx.set_timer(at - cx.now));
            self.watch_at = Some(at);
        } else {
            self.watch_at = None;
        }
    }
    fn next_watch_at(&self, now: f64) -> Option<f64> {
        let idle = self
            .idle_enabled
            .then_some(self.received + self.config.idle_timeout_ms);
        let deadline = match self.phase {
            Phase::Connecting => Some(self.since + self.config.connect_timeout_ms),
            Phase::Authenticating => {
                let auth = self.config.auth_timeout_ms.map(|ms| self.since + ms);
                match (auth, idle) {
                    (Some(a), Some(i)) => Some(a.min(i)),
                    (auth, idle) => auth.or(idle),
                }
            }
            Phase::Ready => idle,
            Phase::Stopped | Phase::Waiting => return None,
        };
        #[cfg(feature = "managed-sockets")]
        let deadline = match (deadline, self.snapshot_deadline) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let deadline = match (deadline, self.next_ping) {
            (Some(at), Some(ping)) => Some(at.min(ping)),
            (deadline, ping) => deadline.or(ping),
        };
        deadline.map(|deadline| now + (deadline - now).clamp(0.0, 5_000.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::book::Book;
    use crate::{Action, Command, Engine, Event, Response, SocketId, Venue};
    use std::collections::HashMap;

    struct Probe {
        supervisor: Supervisor,
        socket: Option<SocketId>,
        ticks: Vec<Tick>,
    }

    impl Probe {
        fn connect(&mut self, cx: &mut Cx<'_, ()>) {
            self.supervisor.connecting(cx);
            self.socket = Some(cx.connect("wss://socket.test"));
        }

        fn fail(&mut self, cx: &mut Cx<'_, ()>) {
            if let Some(socket) = self.socket.take() {
                cx.close(socket);
            }
            self.supervisor.retry(cx);
        }
    }

    impl Venue for Probe {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            self.connect(cx);
        }
        fn on_open(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId) {
            if self.socket == Some(socket) {
                self.supervisor.opened(cx);
            }
        }
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, text: &str) {
            if self.socket != Some(socket) {
                return;
            }
            match text {
                "auth" => self.supervisor.authenticated(cx),
                "fail" => self.fail(cx),
                "stop" => self.supervisor.cancel(cx),
                "short-ping" => self.supervisor.set_ping_interval(cx, Some(100.0)),
                "short-auth" => self.supervisor.set_auth_timeout(cx, Some(1_000.0)),
                "idle-disable" => self.supervisor.set_idle_enabled(cx, false),
                "idle-enable" => self.supervisor.set_idle_enabled(cx, true),
                "rate-limit" => {
                    if let Some(socket) = self.socket.take() {
                        cx.close(socket);
                    }
                    self.supervisor.retry_after(cx, 60_000.0);
                }
                _ => self.supervisor.received(cx.now),
            }
        }
        fn on_close(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, _code: u16) {
            if self.socket == Some(socket) {
                self.fail(cx);
            }
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
            let tick = self.supervisor.on_timer(cx, timer);
            self.ticks.push(tick);
            match tick {
                Tick::TimedOut(_) => self.fail(cx),
                Tick::Reconnect => self.connect(cx),
                Tick::Ping => {
                    if let Some(socket) = self.socket {
                        cx.send(socket, "native-ping");
                    }
                }
                _ => {}
            }
        }
        fn on_response(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _response: Response<'_>) {}
        fn on_request_failed(&mut self, _cx: &mut Cx<'_, ()>, _pending: (), _error: &str) {}
        fn on_command(&mut self, _cx: &mut Cx<'_, ()>, _command: Command) {}
        fn book(&self, _symbol: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    struct Harness {
        engine: Engine<Probe>,
        now: f64,
        timers: HashMap<TimerId, f64>,
        closed: Vec<SocketId>,
        pings: Vec<SocketId>,
    }

    impl Harness {
        fn new(config: Config, now: f64) -> Self {
            Self::with_seed(config, now, 17)
        }
        fn with_seed(config: Config, now: f64, seed: u64) -> Self {
            let mut h = Self {
                engine: Engine::new(
                    Probe {
                        supervisor: Supervisor::new(config),
                        socket: None,
                        ticks: Vec::new(),
                    },
                    Some(crate::test_support::seeded_entropy(seed)),
                )
                .unwrap(),
                now,
                timers: HashMap::new(),
                closed: Vec::new(),
                pings: Vec::new(),
            };
            h.engine.start(now);
            h.collect();
            h
        }
        fn collect(&mut self) {
            for action in self.engine.drain_actions() {
                match action {
                    Action::SetTimer { timer, at_ms } => {
                        self.timers.insert(timer, at_ms);
                    }
                    Action::ClearTimer { timer } => {
                        self.timers.remove(&timer);
                    }
                    Action::WsClose { socket } => self.closed.push(socket),
                    Action::WsSend { socket, text } => {
                        assert_eq!(text, "native-ping");
                        self.pings.push(socket);
                    }
                    _ => {}
                }
            }
        }
        fn event(&mut self, event: Event<'_>) {
            self.engine.handle(self.now, event);
            self.collect();
        }
        fn socket(&self) -> SocketId {
            self.engine.venue().socket.unwrap()
        }
        fn open(&mut self) {
            self.event(Event::WsOpened {
                socket: self.socket(),
            });
        }
        fn message(&mut self, text: &str) {
            self.event(Event::WsMessage {
                socket: self.socket(),
                text,
            });
        }
        fn advance(&mut self, until: f64) {
            let mut limit = 1_000;
            while let Some((&timer, &at_ms)) = self
                .timers
                .iter()
                .filter(|(_, at)| **at <= until)
                .min_by(|a, b| a.1.total_cmp(b.1))
            {
                limit -= 1;
                assert!(limit > 0, "timer failed to make progress");
                self.now = at_ms;
                self.timers.remove(&timer);
                self.event(Event::TimerFired { timer });
            }
            self.now = until;
        }
        fn retry_delay(&self) -> f64 {
            let timer = self.engine.venue().supervisor.retry.unwrap();
            self.timers[&timer] - self.now
        }
    }

    #[test]
    fn never_opened_socket_expires_closes_and_retries_once() {
        let mut h = Harness::new(Config::default(), 0.0);
        let old = h.socket();
        h.advance(10_000.0);
        assert_eq!(h.closed, [old]);
        assert!(
            h.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Connect))
        );
        let delay = h.retry_delay();
        assert!((750.0..=1_000.0).contains(&delay));
        h.event(Event::WsClosed {
            socket: old,
            code: 1006,
        });
        assert_eq!(h.retry_delay(), delay);
        assert_eq!(h.closed, [old]);
        h.advance(h.now + delay);
        assert_ne!(h.socket(), old);
    }

    #[test]
    fn pongs_never_extend_auth_deadline() {
        let mut h = Harness::new(
            Config {
                auth_timeout_ms: Some(10_000.0),
                ping_interval_ms: Some(1_000.0),
                ..Config::default()
            },
            0.0,
        );
        h.open();
        let old = h.socket();
        for now in 1..10 {
            h.advance(f64::from(now) * 1_000.0);
            h.message("pong");
        }
        h.advance(10_000.0);
        assert_eq!(h.closed, [old]);
        assert!(
            h.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Auth))
        );
        assert_eq!(h.pings.len(), 9);
    }

    #[test]
    fn successful_auth_cancels_auth_timer_but_not_receive_watchdog() {
        let mut h = Harness::new(
            Config {
                auth_timeout_ms: Some(10_000.0),
                idle_timeout_ms: 20_000.0,
                ..Config::default()
            },
            0.0,
        );
        h.open();
        let old_timer = h.engine.venue().supervisor.watch.unwrap();
        h.advance(4_000.0);
        h.message("auth");
        h.event(Event::TimerFired { timer: old_timer });
        h.advance(23_999.0);
        assert!(h.closed.is_empty());
        h.advance(24_000.0);
        assert_eq!(h.closed.len(), 1);
        assert!(
            h.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Idle))
        );
    }

    #[test]
    fn repeated_opens_and_auth_rejections_increase_delay_up_to_cap() {
        let mut h = Harness::new(
            Config {
                auth_timeout_ms: Some(1_000.0),
                ..Config::default()
            },
            0.0,
        );
        for attempt in 0..40 {
            h.open();
            h.message("fail");
            let nominal = (1_000.0 * 2f64.powi(attempt.min(31))).min(30_000.0);
            let delay = h.retry_delay();
            assert!((nominal * 0.75..=nominal).contains(&delay));
            h.advance(h.now + delay);
        }
        assert_eq!(h.engine.venue().supervisor.failures, 40);
    }

    #[test]
    fn stable_ready_traffic_resets_backoff_while_old_socket_traffic_does_not() {
        let mut h = Harness::new(Config::default(), 0.0);
        let old = h.socket();
        h.message("fail");
        h.advance(h.now + h.retry_delay());
        h.open();
        h.advance(h.now + 30_000.0);
        h.event(Event::WsMessage {
            socket: old,
            text: "pong",
        });
        assert_eq!(h.engine.venue().supervisor.failures, 1);
        h.message("valid");
        assert_eq!(h.engine.venue().supervisor.failures, 0);
        h.message("fail");
        assert!((750.0..=1_000.0).contains(&h.retry_delay()));
    }

    #[test]
    fn jitter_is_replayable_independently_of_clock() {
        let fail = |now| {
            let mut h = Harness::new(Config::default(), now);
            h.message("fail");
            h.retry_delay()
        };
        assert_eq!(fail(100.0), fail(100.0));
        assert_eq!(fail(100.0), fail(101.0));
        let mut other = Harness::with_seed(Config::default(), 100.0, 18);
        other.message("fail");
        assert_ne!(fail(100.0), other.retry_delay());
    }

    #[test]
    fn cancel_retires_all_timers_and_prevents_automatic_recovery() {
        let mut h = Harness::new(Config::default(), 0.0);
        h.open();
        let old = h.engine.venue().supervisor.watch.unwrap();
        h.message("stop");
        assert!(h.timers.is_empty());
        h.event(Event::TimerFired { timer: old });
        h.advance(1_000_000.0);
        assert!(h.closed.is_empty());
        assert!(h.pings.is_empty());
    }

    #[test]
    fn negotiated_ping_interval_replaces_old_timer_without_changing_auth_start() {
        let mut h = Harness::new(
            Config {
                auth_timeout_ms: Some(10_000.0),
                ..Config::default()
            },
            0.0,
        );
        h.open();
        h.advance(500.0);
        h.message("short-ping");
        h.advance(600.0);
        assert_eq!(h.pings.len(), 1);
        h.message("short-auth");
        h.advance(1_000.0);
        assert!(
            h.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Auth))
        );
    }

    #[test]
    fn rate_limit_floor_survives_small_jittered_delay() {
        let mut h = Harness::new(Config::default(), 0.0);
        h.message("rate-limit");
        assert_eq!(h.retry_delay(), 60_000.0);
        h.advance(59_999.0);
        assert!(h.engine.venue().socket.is_none());
        h.advance(60_000.0);
        assert!(h.engine.venue().socket.is_some());
    }

    #[test]
    fn fractional_retry_timestamps_expire_without_zero_delay_timer_loops() {
        for start in [813.297_568_491_074_2, 10_731.000_000_000_002, 0.1] {
            let mut connecting = Harness::new(Config::default(), start);
            connecting.advance(start + 10_000.0);
            assert_eq!(connecting.closed.len(), 1);

            let mut auth = Harness::new(
                Config {
                    auth_timeout_ms: Some(10_000.0),
                    ..Config::default()
                },
                start,
            );
            auth.open();
            auth.advance(start + 10_000.0);
            assert_eq!(auth.closed.len(), 1);

            let mut idle = Harness::new(Config::default(), start);
            idle.open();
            idle.advance(start + 60_000.0);
            assert_eq!(idle.closed.len(), 1);
        }
    }

    #[test]
    fn ingress_expiry_query_is_absolute_and_does_not_consume_watchdog() {
        let mut h = Harness::new(
            Config {
                auth_timeout_ms: Some(10_000.0),
                ..Config::default()
            },
            0.25,
        );
        let supervisor = &h.engine.venue().supervisor;
        assert_eq!(supervisor.expired(10_000.249), None);
        assert_eq!(supervisor.expired(10_000.25), Some(Timeout::Connect));
        let timer = supervisor.watch;
        assert_eq!(supervisor.expired(20_000.0), Some(Timeout::Connect));
        assert_eq!(supervisor.watch, timer);
        h.open();
        assert_eq!(h.engine.venue().supervisor.expired(10_000.249), None);
        assert_eq!(
            h.engine.venue().supervisor.expired(10_000.25),
            Some(Timeout::Auth)
        );
        // Merely querying leaves the ordinary timer path intact.
        h.advance(10_000.25);
        assert_eq!(h.closed.len(), 1);
        assert!(
            h.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Auth))
        );
    }

    #[test]
    fn idle_activation_starts_at_proof_and_repeated_activation_does_not_extend_it() {
        let mut h = Harness::new(
            Config {
                idle_timeout_ms: 45_000.0,
                ..Config::default()
            },
            0.0,
        );
        h.message("idle-disable");
        h.open();
        h.advance(500_000.0);
        assert!(h.closed.is_empty());
        assert!(h.timers.is_empty());
        assert_eq!(h.engine.venue().supervisor.expired(h.now), None);
        h.message("idle-enable");
        h.advance(530_000.0);
        h.message("idle-enable");
        assert_eq!(h.engine.venue().supervisor.expired(544_999.0), None);
        assert_eq!(
            h.engine.venue().supervisor.expired(545_000.0),
            Some(Timeout::Idle)
        );
        h.advance(545_000.0);
        assert_eq!(h.closed.len(), 1);
    }

    #[test]
    fn disabled_idle_preserves_connect_auth_bounds_and_ping_cadence() {
        let config = Config {
            connect_timeout_ms: 1_000.0,
            auth_timeout_ms: Some(2_000.0),
            idle_timeout_ms: 100.0,
            ping_interval_ms: Some(500.0),
            ..Config::default()
        };
        let mut unopened = Harness::new(config, 0.0);
        unopened.message("idle-disable");
        unopened.advance(1_000.0);
        assert!(
            unopened
                .engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Connect))
        );
        let mut auth = Harness::new(config, 0.0);
        auth.message("idle-disable");
        auth.open();
        auth.advance(1_999.0);
        assert_eq!(auth.pings.len(), 3);
        assert!(auth.closed.is_empty());
        auth.advance(2_000.0);
        assert!(
            auth.engine
                .venue()
                .ticks
                .contains(&Tick::TimedOut(Timeout::Auth))
        );
    }

    #[test]
    fn traffic_refresh_after_activation_and_disabled_idle_without_ping_are_independent() {
        let mut h = Harness::new(Config::default(), 0.0);
        h.open();
        h.advance(1_000.0);
        h.message("idle-disable");
        assert!(h.timers.is_empty());
        h.advance(100_000.0);
        h.message("idle-enable");
        h.advance(120_000.0);
        h.message("valid");
        assert_eq!(h.engine.venue().supervisor.expired(160_000.0), None);
        h.advance(180_000.0);
        assert_eq!(h.closed.len(), 1);
    }
}

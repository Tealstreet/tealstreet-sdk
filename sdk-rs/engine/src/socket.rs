//! Replayable socket liveness and retry policy; wire protocols remain venue-owned.
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
    since: f64,
    received: f64,
    next_ping: Option<f64>,
    watch: Option<TimerId>,
    retry: Option<TimerId>,
    failures: u32,
    jitter: u64,
}

impl Default for Supervisor {
    fn default() -> Self {
        Self::new(Config::default())
    }
}

impl Supervisor {
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
            since: 0.0,
            received: 0.0,
            next_ping: None,
            watch: None,
            retry: None,
            failures: 0,
            jitter: 0x9e37_79b9_7f4a_7c15,
        }
    }

    /// Call before connecting (or acquiring a connection token).
    pub fn connecting<P>(&mut self, cx: &mut Cx<'_, P>) {
        self.cancel(cx);
        self.phase = Phase::Connecting;
        self.since = cx.now;
        self.received = cx.now;
        self.arm(cx);
        if let Some(timer) = self.watch {
            self.mix(cx.now.to_bits() ^ u64::from(timer.0));
        }
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
        let nominal = (self.config.backoff_base_ms * 2f64.powi(self.failures.min(31) as i32))
            .min(self.config.backoff_cap_ms);
        self.failures = self.failures.saturating_add(1);
        let bits = self.mix(cx.now.to_bits() ^ u64::from(self.failures));
        let unit = (bits >> 11) as f64 / ((1u64 << 53) as f64);
        let delay = (nominal * (0.75 + unit * 0.25)).max(minimum_ms.max(0.0));
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
        let timeout = match self.phase {
            Phase::Connecting if cx.now - self.since >= self.config.connect_timeout_ms => {
                Some(Timeout::Connect)
            }
            Phase::Authenticating
                if self
                    .config
                    .auth_timeout_ms
                    .is_some_and(|ms| cx.now - self.since >= ms) =>
            {
                Some(Timeout::Auth)
            }
            Phase::Authenticating | Phase::Ready
                if cx.now - self.received >= self.config.idle_timeout_ms =>
            {
                Some(Timeout::Idle)
            }
            _ => None,
        };
        if let Some(timeout) = timeout {
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
        let deadline = match self.phase {
            Phase::Connecting => self.since + self.config.connect_timeout_ms,
            Phase::Authenticating => (self.since + self.config.auth_timeout_ms.unwrap())
                .min(self.received + self.config.idle_timeout_ms),
            Phase::Ready => self.received + self.config.idle_timeout_ms,
            Phase::Stopped | Phase::Waiting => return,
        };
        let deadline = self.next_ping.map_or(deadline, |ping| deadline.min(ping));
        self.watch = Some(cx.set_timer((deadline - cx.now).clamp(0.0, 5_000.0)));
    }

    fn mix(&mut self, salt: u64) -> u64 {
        self.jitter = self
            .jitter
            .wrapping_add(salt)
            .wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.jitter;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
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
            let mut h = Self {
                engine: Engine::new(Probe {
                    supervisor: Supervisor::new(config),
                    socket: None,
                    ticks: Vec::new(),
                }),
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
    fn jitter_is_replayable_and_varies_with_clock() {
        let fail = |now| {
            let mut h = Harness::new(Config::default(), now);
            h.message("fail");
            h.retry_delay()
        };
        assert_eq!(fail(100.0), fail(100.0));
        assert_ne!(fail(100.0), fail(101.0));
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
}

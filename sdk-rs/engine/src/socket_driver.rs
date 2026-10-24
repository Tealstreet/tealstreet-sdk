//! Supervised transitions for the engine's managed socket registry.
//!
//! Effects contain owned data only. Engine restores its registry before calling
//! a venue codec/effect handler, then acknowledges required invalidation here.
use crate::socket::{Supervisor, Tick};
use crate::socket_lifecycle::{
    Admission, AuthEvidence, AuthRecovery, AuthTransition, Control, Epoch, FeedId, FeedPolicy,
    Ingress, Lifecycle, RetireReason, Retirement, SocketError, TrafficPolicy,
};
use crate::{Cx, SocketId, TimerId};

#[derive(Clone, Debug)]
pub(crate) enum Effect {
    Opened(Epoch),
    Ready {
        epoch: Epoch,
        reopened: bool,
    },
    Authenticated {
        epoch: Epoch,
        first: bool,
    },
    AuthFailure {
        epoch: Epoch,
        evidence: AuthEvidence,
        terminal: bool,
    },
    Retired {
        retirement: Retirement,
        reason: RetireReason,
        retry: bool,
    },
    Ping(Epoch),
    RetryLogin(Epoch),
    Reconnect(FeedId),
}

pub(crate) struct IngressResult {
    pub admission: Admission,
    pub effects: Vec<Effect>,
}

/// Immediate continuation of a fresh message observation; never retained or
/// exposed as permission for another ingress.
pub(crate) enum ControlWork {
    Complete,
    Expired(Retirement, crate::socket::Timeout),
    Transition { refresh: bool, receive: bool },
}

/// Immutable arithmetic bounds, not a cached clock or admission decision.
/// Rounded addition is monotonic: if both extreme positive delays advance a
/// finite raw time without overflow, all delays in this fixed policy do too.
#[derive(Clone, Copy)]
struct TimeBounds {
    shortest: f64,
    longest: f64,
}
impl TimeBounds {
    fn new(policy: &FeedPolicy) -> Self {
        let c = policy.supervision;
        let mut bounds = Self {
            shortest: 5_000.,
            longest: 5_000.,
        };
        for delay in [
            Some(c.connect_timeout_ms),
            Some(c.idle_timeout_ms),
            Some(c.backoff_base_ms * 0.75),
            Some(c.backoff_cap_ms),
            c.auth_timeout_ms,
            c.ping_interval_ms,
            (policy.subscription_interval_ms != 0.).then_some(policy.subscription_interval_ms),
            match policy.recovery {
                AuthRecovery::Reconnect => None,
                AuthRecovery::RetrySameSocket { delay_ms } => Some(delay_ms),
            },
        ]
        .into_iter()
        .flatten()
        {
            debug_assert!(delay.is_finite() && delay > 0.);
            bounds.shortest = bounds.shortest.min(delay);
            bounds.longest = bounds.longest.max(delay);
        }
        bounds
    }
    fn valid(self, now: f64) -> bool {
        let advances = |delay: f64| (now + delay).is_finite() && now + delay > now;
        now.is_finite() && advances(self.shortest) && advances(self.longest)
    }
}

struct Runtime {
    feed: FeedId,
    supervisor: Supervisor,
    time: TimeBounds,
    login_retry: Option<TimerId>,
    batch_timer: Option<TimerId>,
    next_batch: Option<f64>,
    snapshot_timeout: Option<f64>,
    snapshot_pending: bool,
}

#[derive(Default)]
pub(crate) struct Driver {
    pub state: Lifecycle,
    runtimes: Vec<Runtime>,
}

impl Driver {
    pub(crate) fn frames_ready(&self, now: f64, barrier: impl Fn(SocketId) -> bool) -> bool {
        if !self.state.has_pending_frames() {
            return false;
        }
        self.runtimes.iter().any(|r| {
            self.state.epoch(r.feed).is_some_and(|e| {
                self.state.ready(e)
                    && r.batch_timer.is_none()
                    && self.state.has_batches(r.feed)
                    && r.next_batch.is_none_or(|at| now >= at)
                    && r.time.valid(now)
                    && !barrier(e.socket())
            })
        })
    }
    pub(crate) fn one_shot_ready(&self, epoch: Epoch, now: f64, barrier: bool) -> bool {
        self.state.ready(epoch)
            && !self.state.has_batches(epoch.feed())
            && !barrier
            && self.valid_time(epoch.feed(), now)
    }
    pub(crate) fn handoff_ready(&self, epoch: Epoch, now: f64, barrier: bool) -> bool {
        self.one_shot_ready(epoch, now, barrier)
            && self
                .runtimes
                .get(epoch.feed().index())
                .is_some_and(|runtime| runtime.supervisor.expired(now).is_none())
    }
    /// Fresh representability only. Supervisor still owns all deadlines/backoff.
    fn valid_time(&self, feed: FeedId, now: f64) -> bool {
        self.runtimes
            .get(feed.index())
            .is_some_and(|r| r.time.valid(now))
    }

    /// Called before allocating/queuing the physical connection. begin repeats
    /// this check so a caller cannot accidentally bypass it.
    pub(crate) fn preflight_begin(&self, feed: FeedId, now: f64) -> Result<(), SocketError> {
        self.state.can_begin(feed)?;
        if !self.valid_time(feed, now) {
            return Err(SocketError::InvalidTime);
        }
        Ok(())
    }

    pub(crate) fn register(&mut self, policy: FeedPolicy) -> Result<FeedId, SocketError> {
        self.register_with_snapshot(policy, false)
    }
    pub(crate) fn register_with_snapshot(
        &mut self,
        policy: FeedPolicy,
        snapshot: bool,
    ) -> Result<FeedId, SocketError> {
        if snapshot
            && (policy.role != crate::SocketChannel::Private
                || policy.auth != crate::socket_lifecycle::AuthMode::Required
                || policy.controls != crate::socket_lifecycle::ControlPolicy::Native
                || policy.recovery != AuthRecovery::Reconnect
                || policy.supervision.auth_timeout_ms.is_none())
        {
            return Err(SocketError::InvalidPolicy);
        }
        let snapshot_timeout = snapshot
            .then_some(policy.supervision.auth_timeout_ms)
            .flatten();
        let config = policy.supervision;
        let feed = self.state.register(policy)?;
        let time = TimeBounds::new(self.state.policy(feed).expect("registered feed"));
        let supervisor = Supervisor::new(config);
        debug_assert_eq!(feed.index(), self.runtimes.len());
        self.runtimes.push(Runtime {
            feed,
            supervisor,
            time,
            login_retry: None,
            batch_timer: None,
            next_batch: None,
            snapshot_timeout,
            snapshot_pending: false,
        });
        Ok(feed)
    }

    pub(crate) fn snapshot_connect_ids(&self, feed: FeedId) -> Option<u64> {
        let timeout = self.runtimes.get(feed.index())?.snapshot_timeout?;
        let initial = self
            .state
            .policy(feed)
            .ok()?
            .supervision
            .connect_timeout_ms
            .min(5_000.);
        Some(if timeout < initial { 3 } else { 2 })
    }
    pub(crate) fn validate_snapshot_completion(
        &self,
        epoch: Epoch,
        now: f64,
    ) -> Result<(), SocketError> {
        if !self.state.current(epoch) || !self.state.authenticated(epoch.feed()) {
            return Err(SocketError::Stale);
        }
        let runtime = &self.runtimes[epoch.feed().index()];
        if !runtime.snapshot_pending
            || !runtime.time.valid(now)
            || runtime.supervisor.expired(now).is_some()
        {
            return Err(SocketError::Stale);
        }
        Ok(())
    }
    pub(crate) fn complete_snapshot<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        epoch: Epoch,
    ) -> Result<(), SocketError> {
        self.validate_snapshot_completion(epoch, cx.now)?;
        let runtime = self.runtime_mut(epoch.feed());
        runtime.snapshot_pending = false;
        runtime.supervisor.authenticated(cx);
        Ok(())
    }
    fn runtime_mut(&mut self, feed: FeedId) -> &mut Runtime {
        let runtime = &mut self.runtimes[feed.index()];
        debug_assert_eq!(runtime.feed, feed);
        runtime
    }

    pub(crate) fn begin<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        feed: FeedId,
        socket: SocketId,
    ) -> Result<Epoch, SocketError> {
        self.preflight_begin(feed, cx.now)?;
        let epoch = self.state.begin(feed, socket)?;
        let idle = self.state.policy(feed)?.traffic != TrafficPolicy::TextAfterFirstPong;
        let runtime = self.runtime_mut(feed);
        runtime.next_batch = None;
        // The activation flag deliberately persists in Supervisor. Reset it on
        // every physical epoch, including replacement after a first pong.
        runtime.supervisor.set_idle_enabled(cx, idle);
        runtime.supervisor.connecting(cx);
        runtime.snapshot_pending = runtime.snapshot_timeout.is_some();
        if let Some(timeout) = runtime.snapshot_timeout {
            runtime
                .supervisor
                .retain_snapshot_deadline(cx, cx.now + timeout);
        }
        Ok(epoch)
    }

    fn cancel_runtime<P>(&mut self, cx: &mut Cx<'_, P>, feed: FeedId) {
        let runtime = self.runtime_mut(feed);
        runtime.supervisor.cancel(cx);
        runtime.snapshot_pending = false;
        if let Some(timer) = runtime.login_retry.take() {
            cx.clear_timer(timer);
        }
        if let Some(timer) = runtime.batch_timer.take() {
            cx.clear_timer(timer);
        }
        runtime.next_batch = None;
    }

    pub(crate) fn stop_feed<P>(&mut self, cx: &mut Cx<'_, P>, feed: FeedId) -> Option<Effect> {
        // Revocation applies even while waiting for retry, before any new epoch.
        self.state.policy(feed).ok()?;
        self.cancel_runtime(cx, feed);
        self.state
            .stop_feed(feed)
            .map(|retirement| Effect::Retired {
                retirement,
                reason: RetireReason::Stopped,
                retry: false,
            })
    }

    pub(crate) fn stop_all<P>(&mut self, cx: &mut Cx<'_, P>) -> Vec<Effect> {
        let retired = self.state.stop_all();
        for feed in self.state.feeds() {
            self.cancel_runtime(cx, feed);
        }
        retired
            .into_iter()
            .map(|r| self.finish_retirement(cx, r, RetireReason::Stopped, false))
            .collect()
    }

    pub(crate) fn release_frame<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        epoch: Epoch,
        barrier: bool,
    ) -> (Option<String>, Vec<Effect>) {
        if !self.state.has_batches(epoch.feed())
            || self.runtime_mut(epoch.feed()).batch_timer.is_some()
        {
            return (None, Vec::new());
        }
        let result = self.ingress(cx, epoch.socket(), Ingress::Send);
        if result.admission != Admission::Current(epoch) || barrier {
            return (None, result.effects);
        }
        if !self.state.has_batches(epoch.feed()) {
            return (None, Vec::new());
        }
        let cadence = self
            .state
            .policy(epoch.feed())
            .expect("current feed")
            .subscription_interval_ms;
        let runtime = self.runtime_mut(epoch.feed());
        if let Some(at) = runtime.next_batch
            && cx.now < at
        {
            if runtime.batch_timer.is_none() {
                runtime.batch_timer = Some(cx.set_timer(at - cx.now));
            }
            return (None, Vec::new());
        }
        let frame = self.state.release_batch(epoch, false);
        if frame.is_some() {
            self.runtime_mut(epoch.feed()).next_batch = Some(cx.now + cadence);
        }
        if frame.is_some() && cadence > 0. && self.state.has_batches(epoch.feed()) {
            let runtime = self.runtime_mut(epoch.feed());
            if runtime.batch_timer.is_none() {
                runtime.batch_timer = Some(cx.set_timer(cadence));
            }
        }
        (frame, Vec::new())
    }

    fn finish_retirement<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        retirement: Retirement,
        reason: RetireReason,
        retry: bool,
    ) -> Effect {
        self.cancel_runtime(cx, retirement.epoch.feed());
        // Engine consumes this effect by canceling signing/closing the physical
        // transport through its raw close primitive BEFORE invoking the native
        // invalidation callback. Public Cx::close enters managed retirement and
        // must not be called recursively from this component.
        Effect::Retired {
            retirement,
            reason,
            retry,
        }
    }

    pub(crate) fn retire<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        feed: FeedId,
        reason: RetireReason,
        retry: bool,
    ) -> Option<Effect> {
        let retirement = self.state.retire(feed)?;
        Some(self.finish_retirement(cx, retirement, reason, retry))
    }

    pub(crate) fn ingress<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        socket: SocketId,
        ingress: Ingress,
    ) -> IngressResult {
        let admission = self.admit_ingress(cx.now, socket, ingress);
        self.finish_ingress(cx, admission, ingress)
    }

    /// Fresh lookup and time/expiry check at each ingress boundary. Expiry
    /// revokes the epoch here; its owned retirement must be finished at once.
    #[inline(always)]
    pub(crate) fn admit_ingress(
        &mut self,
        now: f64,
        socket: SocketId,
        ingress: Ingress,
    ) -> Admission {
        let runtimes = &self.runtimes;
        self.state.admit_checked(socket, ingress, |epoch| {
            let runtime = runtimes
                .get(epoch.feed().index())
                .ok_or(SocketError::InvalidTime)?;
            if !runtime.time.valid(now) {
                return Err(SocketError::InvalidTime);
            }
            Ok(runtime.supervisor.expired(now))
        })
    }

    #[inline(never)]
    pub(crate) fn finish_ingress<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        admission: Admission,
        ingress: Ingress,
    ) -> IngressResult {
        let mut effects = Vec::new();
        match admission {
            Admission::Expired(retirement, timeout) => {
                effects.push(self.finish_retirement(
                    cx,
                    retirement,
                    RetireReason::Timeout(timeout),
                    true,
                ));
            }
            Admission::Current(epoch) if ingress == Ingress::Open => {
                let ready = self.state.opened(epoch).expect("admitted first open");
                self.runtime_mut(epoch.feed()).supervisor.opened(cx);
                effects.push(Effect::Opened(epoch));
                if let Some(reopened) = ready {
                    effects.push(Effect::Ready { epoch, reopened });
                }
            }
            Admission::Current(epoch) if ingress == Ingress::Close => {
                if let Some(retired) = self.retire(cx, epoch.feed(), RetireReason::Closed, true) {
                    effects.push(retired);
                }
            }
            _ => {}
        }
        IngressResult { admission, effects }
    }

    /// Control evidence comes from the pure native codec after ingress. Recheck
    /// expiry here too, before any auth or receive-time mutation.
    #[cfg(test)]
    pub(crate) fn control<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        epoch: Epoch,
        control: Control,
        startup_ready: bool,
    ) -> Vec<Effect> {
        let work = self.observe_control(cx.now, epoch, &control);
        self.finish_control(cx, epoch, control, startup_ready, work)
    }

    /// Ordinary current traffic stays in place. Scheduling/activation and auth
    /// still require the original context before receive/auth transitions.
    pub(crate) fn observe_control(
        &mut self,
        now: f64,
        epoch: Epoch,
        control: &Control,
    ) -> ControlWork {
        let admission = self.admit_ingress(now, epoch.socket(), Ingress::Message);
        if admission != Admission::Current(epoch) {
            return match admission {
                Admission::Expired(retirement, timeout) => {
                    ControlWork::Expired(retirement, timeout)
                }
                _ => ControlWork::Complete,
            };
        }
        let traffic = self
            .state
            .policy(epoch.feed())
            .expect("current feed")
            .traffic;
        let refresh = self
            .state
            .traffic(epoch, true, matches!(control, Control::Pong));
        let receive = refresh || traffic == TrafficPolicy::TextAfterFirstPong;
        let runtime = self.runtime_mut(epoch.feed());
        if matches!(control, Control::Auth(_)) || refresh && !runtime.supervisor.idle_enabled() {
            return ControlWork::Transition { refresh, receive };
        }
        if receive {
            runtime.supervisor.received(now);
        }
        ControlWork::Complete
    }

    #[inline(never)]
    pub(crate) fn finish_control<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        epoch: Epoch,
        control: Control,
        startup_ready: bool,
        work: ControlWork,
    ) -> Vec<Effect> {
        let (refresh, receive) = match work {
            ControlWork::Complete => return Vec::new(),
            ControlWork::Expired(retirement, timeout) => {
                return self
                    .finish_ingress(
                        cx,
                        Admission::Expired(retirement, timeout),
                        Ingress::Message,
                    )
                    .effects;
            }
            ControlWork::Transition { refresh, receive } => (refresh, receive),
        };
        let runtime = self.runtime_mut(epoch.feed());
        if refresh {
            runtime.supervisor.set_idle_enabled(cx, true);
        }
        if receive {
            runtime.supervisor.received(cx.now);
        }
        let Control::Auth(evidence) = control else {
            return Vec::new();
        };
        let first = !self.state.authenticated(epoch.feed());
        match self.state.auth(epoch, evidence, startup_ready) {
            AuthTransition::Ignored => Vec::new(),
            AuthTransition::Progress(epoch) => {
                self.runtime_mut(epoch.feed())
                    .supervisor
                    .authentication_progress(cx);
                Vec::new()
            }
            AuthTransition::Accepted {
                epoch,
                ready,
                reopened,
            } => {
                let runtime = self.runtime_mut(epoch.feed());
                if let Some(timer) = runtime.login_retry.take() {
                    cx.clear_timer(timer);
                }
                if !runtime.snapshot_pending {
                    runtime.supervisor.authenticated(cx);
                }
                let mut effects = vec![Effect::Authenticated { epoch, first }];
                if ready {
                    effects.push(Effect::Ready { epoch, reopened });
                }
                effects
            }
            AuthTransition::Failed {
                epoch,
                evidence,
                recovery,
            } => {
                let mut effects = vec![Effect::AuthFailure {
                    epoch,
                    evidence,
                    terminal: false,
                }];
                match recovery {
                    AuthRecovery::Reconnect => {
                        if let Some(retired) =
                            self.retire(cx, epoch.feed(), RetireReason::Authentication, true)
                        {
                            effects.push(retired);
                        }
                    }
                    AuthRecovery::RetrySameSocket { delay_ms } => {
                        let runtime = self.runtime_mut(epoch.feed());
                        if runtime.login_retry.is_none() {
                            runtime.login_retry = Some(cx.set_timer(delay_ms));
                        }
                    }
                }
                effects
            }
            AuthTransition::Permanent {
                epoch,
                group,
                error,
            } => {
                // Commit ALL active and inactive siblings before exposing any
                // callback. Inactive retries are canceled as well.
                let retirements = self.state.retire_group(group);
                let feeds: Vec<_> = self
                    .runtimes
                    .iter()
                    .filter_map(|r| {
                        (self.state.policy(r.feed).ok()?.permanent_refusal_group == Some(group))
                            .then_some(r.feed)
                    })
                    .collect();
                for feed in feeds {
                    self.cancel_runtime(cx, feed);
                }
                let mut effects = vec![Effect::AuthFailure {
                    epoch,
                    evidence: AuthEvidence::Rejected(error),
                    terminal: true,
                }];
                for retirement in retirements {
                    effects.push(self.finish_retirement(
                        cx,
                        retirement,
                        RetireReason::Authentication,
                        false,
                    ));
                }
                effects
            }
        }
    }

    pub(crate) fn acknowledge_retirement<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        retirement: Retirement,
        acknowledged: bool,
        retry: bool,
    ) -> Result<(), SocketError> {
        if acknowledged && retry {
            self.state.policy(retirement.epoch.feed())?;
        }
        if acknowledged && retry && !self.valid_time(retirement.epoch.feed(), cx.now) {
            return Err(SocketError::InvalidTime);
        }
        self.state.retired(retirement.epoch, acknowledged)?;
        if acknowledged && retry && self.state.awaiting_retry(retirement.epoch.feed()) {
            self.runtime_mut(retirement.epoch.feed())
                .supervisor
                .retry(cx);
        }
        Ok(())
    }

    /// Engine queries this BEFORE removing a managed timer from Io.
    pub(crate) fn timer_admissible(&self, timer: TimerId, now: f64) -> Option<bool> {
        let runtime = self.runtimes.iter().find(|r| {
            r.login_retry == Some(timer)
                || r.batch_timer == Some(timer)
                || r.supervisor.owns_timer(timer)
        })?;
        Some(runtime.time.valid(now))
    }

    /// None means an unowned timer; Some(empty) is owned but has no callback.
    pub(crate) fn timer<P>(&mut self, cx: &mut Cx<'_, P>, timer: TimerId) -> Option<Vec<Effect>> {
        let index = self.runtimes.iter().position(|r| {
            r.login_retry == Some(timer)
                || r.batch_timer == Some(timer)
                || r.supervisor.owns_timer(timer)
        })?;
        let feed = self.runtimes[index].feed;
        if self.timer_admissible(timer, cx.now) == Some(false) {
            // Engine must perform this admission before removing an owned timer
            // from Io; an invalid host timestamp does not consume a live timer.
            return Some(Vec::new());
        }
        if self.runtimes[index].batch_timer == Some(timer) {
            self.runtimes[index].batch_timer = None;
            return Some(Vec::new());
        }
        if self.runtimes[index].login_retry == Some(timer) {
            self.runtimes[index].login_retry = None;
            let Some(epoch) = self.state.epoch(feed) else {
                return Some(Vec::new());
            };
            let admitted = self.ingress(cx, epoch.socket(), Ingress::Message);
            return Some(if admitted.admission == Admission::Current(epoch) {
                vec![Effect::RetryLogin(epoch)]
            } else {
                admitted.effects
            });
        }
        let tick = self.runtimes[index].supervisor.on_timer(cx, timer);
        Some(match tick {
            Tick::Reconnect => self
                .state
                .grant_retry(feed)
                .map(|()| vec![Effect::Reconnect(feed)])
                .unwrap_or_default(),
            Tick::TimedOut(timeout) => self
                .retire(cx, feed, RetireReason::Timeout(timeout), true)
                .into_iter()
                .collect(),
            Tick::Ping
                if self.runtimes[index].snapshot_timeout.is_none()
                    || self.state.authenticated(feed) =>
            {
                self.state
                    .epoch(feed)
                    .map(Effect::Ping)
                    .into_iter()
                    .collect()
            }
            Tick::Ping => Vec::new(),
            Tick::Unhandled | Tick::Handled => Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket::Timeout;
    use crate::socket_lifecycle::{AuthMode, Invalidation};
    use crate::{
        Action, Command, Engine, ErrorKind, Event, Response, SocketChannel, Venue, VenueError,
        book::Book, socket::Config,
    };

    struct Probe {
        policies: Vec<FeedPolicy>,
        driver: Driver,
        feeds: Vec<FeedId>,
        effects: Vec<Effect>,
        acknowledge: bool,
        begin_errors: Vec<SocketError>,
    }
    impl Probe {
        fn connect(&mut self, cx: &mut Cx<'_, ()>, feed: FeedId) {
            if let Err(error) = self.driver.preflight_begin(feed, cx.now) {
                self.begin_errors.push(error);
                return;
            }
            let socket = cx.connect("wss://driver.test");
            self.driver.begin(cx, feed, socket).unwrap();
        }
        fn apply(&mut self, cx: &mut Cx<'_, ()>, effects: Vec<Effect>) {
            for effect in effects {
                match &effect {
                    Effect::Retired {
                        retirement,
                        reason,
                        retry,
                    } => {
                        assert!(!self.driver.state.current(retirement.epoch));
                        // The probe has no managed Io registry. Production
                        // Engine uses its raw transport-close primitive here.
                        cx.close(retirement.epoch.socket());
                        assert_eq!(retirement.invalidation, Invalidation::Required);
                        assert!(matches!(
                            reason,
                            RetireReason::Closed
                                | RetireReason::Timeout(_)
                                | RetireReason::Authentication
                                | RetireReason::Stopped
                        ));
                        self.driver
                            .acknowledge_retirement(cx, *retirement, self.acknowledge, *retry)
                            .unwrap();
                    }
                    Effect::Reconnect(feed) => self.connect(cx, *feed),
                    _ => {}
                }
                self.effects.push(effect);
            }
        }
    }
    impl Venue for Probe {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            for policy in self.policies.clone() {
                let feed = self.driver.register(policy).unwrap();
                self.feeds.push(feed);
                self.connect(cx, feed);
            }
        }
        fn on_open(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId) {
            let result = self.driver.ingress(cx, socket, Ingress::Open);
            self.apply(cx, result.effects);
        }
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, text: &str) {
            let result = self.driver.ingress(cx, socket, Ingress::Message);
            self.apply(cx, result.effects);
            let Admission::Current(epoch) = result.admission else {
                return;
            };
            let effects = if text == "stop" {
                self.driver
                    .retire(cx, epoch.feed(), RetireReason::Stopped, false)
                    .into_iter()
                    .collect()
            } else {
                let control = match text {
                    "accepted" => Control::Auth(AuthEvidence::Accepted),
                    "malformed" => Control::Auth(AuthEvidence::Malformed),
                    "rejected" => Control::Auth(AuthEvidence::Rejected(
                        VenueError::new("native refusal").kind(ErrorKind::AuthInvalid),
                    )),
                    "pong" => Control::Pong,
                    _ => Control::Other,
                };
                self.driver.control(cx, epoch, control, false)
            };
            self.apply(cx, effects);
        }
        fn on_close(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId, _code: u16) {
            let result = self.driver.ingress(cx, socket, Ingress::Close);
            self.apply(cx, result.effects);
        }
        fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
            if let Some(effects) = self.driver.timer(cx, timer) {
                self.apply(cx, effects);
            }
        }
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    struct Harness {
        engine: Engine<Probe>,
        now: f64,
        timers: crate::IdMap<TimerId, f64>,
        closes: Vec<SocketId>,
        connects: Vec<SocketId>,
    }
    impl Harness {
        fn new(policies: Vec<FeedPolicy>, acknowledge: bool) -> Self {
            Self::at(policies, acknowledge, 0.)
        }
        fn at(policies: Vec<FeedPolicy>, acknowledge: bool, now: f64) -> Self {
            let mut h = Self {
                engine: Engine::new(
                    Probe {
                        policies,
                        driver: Driver::default(),
                        feeds: Vec::new(),
                        effects: Vec::new(),
                        acknowledge,
                        begin_errors: Vec::new(),
                    },
                    Some(crate::entropy::Entropy::new(
                        Box::new(|bytes| {
                            bytes.fill(17);
                            Ok(())
                        }),
                        Box::new(|_| Err("socket policy probe does not sign".into())),
                    )),
                )
                .unwrap(),
                now,
                timers: Default::default(),
                closes: Vec::new(),
                connects: Vec::new(),
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
                    Action::WsClose { socket } => self.closes.push(socket),
                    Action::WsConnect { socket, .. } => self.connects.push(socket),
                    _ => {}
                }
            }
        }
        fn event(&mut self, event: Event<'_>) {
            self.engine.handle(self.now, event);
            self.collect();
        }
        fn epoch(&self, index: usize) -> Epoch {
            let venue = self.engine.venue();
            venue.driver.state.epoch(venue.feeds[index]).unwrap()
        }
        fn open(&mut self, index: usize) {
            self.event(Event::WsOpened {
                socket: self.epoch(index).socket(),
            });
        }
        fn text(&mut self, index: usize, text: &str) {
            self.event(Event::WsMessage {
                socket: self.epoch(index).socket(),
                text,
            });
        }
        fn advance(&mut self, until: f64) {
            let mut remaining = 2_000;
            while let Some((&timer, &at)) = self
                .timers
                .iter()
                .filter(|(_, at)| **at <= until)
                .min_by(|a, b| a.1.total_cmp(b.1))
            {
                remaining -= 1;
                assert!(remaining > 0, "driver timer loop did not progress");
                self.timers.remove(&timer);
                self.now = at;
                self.event(Event::TimerFired { timer });
            }
            self.now = until;
        }
        fn retry_at(&self, index: usize) -> f64 {
            // At this checkpoint each retired feed owns exactly its retry.
            let venue = self.engine.venue();
            assert!(venue.driver.state.awaiting_retry(venue.feeds[index]));
            *self.timers.values().min_by(|a, b| a.total_cmp(b)).unwrap()
        }
    }
    fn policy(auth: AuthMode) -> FeedPolicy {
        FeedPolicy {
            role: SocketChannel::Private,
            supervision: Config {
                auth_timeout_ms: (auth == AuthMode::Required).then_some(10_000.),
                idle_timeout_ms: 45_000.,
                ping_interval_ms: Some(15_000.),
                ..Config::default()
            },
            auth,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::PongOnly,
            controls: crate::socket_lifecycle::ControlPolicy::Native,
            invalidation: Invalidation::Required,
            subscription_interval_ms: 0.,
        }
    }
    fn bitunix() -> FeedPolicy {
        FeedPolicy {
            recovery: AuthRecovery::RetrySameSocket { delay_ms: 2_000. },
            traffic: TrafficPolicy::TextAfterFirstPong,
            ..policy(AuthMode::Observe)
        }
    }

    // Structural hot-path controls; no timing measurement.
    #[test]
    fn current_open_resolves_one_epoch_per_fresh_admission() {
        let mut h = Harness::new(vec![policy(AuthMode::None)], true);
        let before = h.engine.venue().driver.state.socket_lookup_count();
        h.open(0);
        assert_eq!(
            h.engine.venue().driver.state.socket_lookup_count() - before,
            1,
            "opening must resolve its exact current epoch only once"
        );
    }

    #[test]
    fn current_text_keeps_two_independent_single_lookup_admissions() {
        let mut h = Harness::new(vec![policy(AuthMode::None)], true);
        h.open(0);
        let before = h.engine.venue().driver.state.socket_lookup_count();
        h.text(0, "pong");
        assert_eq!(
            h.engine.venue().driver.state.socket_lookup_count() - before,
            2,
            "ingress and control must each resolve exactly one fresh epoch"
        );
        assert!(h.engine.venue().driver.state.current(h.epoch(0)));
    }

    #[test]
    fn retired_ingress_resolves_once_without_effect_or_callback() {
        let mut h = Harness::new(vec![policy(AuthMode::None)], true);
        h.open(0);
        let socket = h.epoch(0).socket();
        h.text(0, "stop");
        let before = h.engine.venue().driver.state.socket_lookup_count();
        let effects = h.engine.venue().effects.len();
        h.event(Event::WsMessage {
            socket,
            text: "pong",
        });
        assert_eq!(
            h.engine.venue().driver.state.socket_lookup_count() - before,
            1
        );
        assert_eq!(h.engine.venue().effects.len(), effects);
    }

    // Independent pre-optimization formula; keep the full interval list here.
    fn original_time_check(policy: &FeedPolicy, now: f64) -> bool {
        let c = policy.supervision;
        let advances = |delay: f64| (now + delay).is_finite() && now + delay > now;
        now.is_finite()
            && [
                5_000.,
                c.connect_timeout_ms,
                c.idle_timeout_ms,
                c.backoff_base_ms * 0.75,
            ]
            .into_iter()
            .all(advances)
            && c.auth_timeout_ms.is_none_or(advances)
            && c.ping_interval_ms.is_none_or(advances)
            && advances(c.backoff_cap_ms)
            && (policy.subscription_interval_ms == 0. || advances(policy.subscription_interval_ms))
            && match policy.recovery {
                AuthRecovery::Reconnect => true,
                AuthRecovery::RetrySameSocket { delay_ms } => advances(delay_ms),
            }
    }

    #[test]
    fn precomputed_time_extremes_match_original_formula_at_float_boundaries() {
        for delay in [
            f64::from_bits(1),
            f64::MIN_POSITIVE,
            1e-100,
            0.125,
            1.,
            5_000.,
            1e100,
            f64::MAX / 2.,
        ] {
            for field in 0..9 {
                let mut p = policy(AuthMode::None);
                match field {
                    0 => p.supervision.connect_timeout_ms = delay,
                    1 => p.supervision.idle_timeout_ms = delay,
                    2 => {
                        p.supervision.backoff_base_ms = delay;
                        p.supervision.backoff_cap_ms = delay.max(30_000.);
                    }
                    3 => {
                        p.supervision.backoff_base_ms = delay;
                        p.supervision.backoff_cap_ms = delay;
                    }
                    4 => {
                        p.auth = AuthMode::Required;
                        p.supervision.auth_timeout_ms = Some(delay);
                    }
                    5 => p.supervision.ping_interval_ms = Some(delay),
                    6 => p.subscription_interval_ms = delay,
                    7 => p.recovery = AuthRecovery::RetrySameSocket { delay_ms: delay },
                    _ => {
                        p.supervision.ping_interval_ms = None;
                        p.subscription_interval_ms = -0.;
                    }
                }
                let mut driver = Driver::default();
                let feed = driver.register(p.clone()).unwrap();
                let mut times = vec![
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::MAX,
                    -f64::MAX,
                    0.,
                    -0.,
                    delay,
                    -delay,
                    f64::from_bits(1),
                    -f64::MIN_POSITIVE,
                ];
                for exponent in [-1074, -1022, -100, -1, 0, 1, 52, 53, 100, 1000, 1023] {
                    let at = 2_f64.powi(exponent);
                    times.extend([
                        at.next_down(),
                        at,
                        at.next_up(),
                        -at.next_down(),
                        -at,
                        -at.next_up(),
                    ]);
                }
                for now in times {
                    assert_eq!(
                        driver.valid_time(feed, now),
                        original_time_check(&p, now),
                        "field={field} delay={delay} now={now}"
                    );
                }
            }
        }
    }

    #[test]
    fn contradictory_control_free_policy_refuses_before_runtime_registration() {
        use crate::socket_lifecycle::ControlPolicy;
        assert_eq!(ControlPolicy::default(), ControlPolicy::Native);
        for (auth, traffic) in [
            (AuthMode::Required, TrafficPolicy::AnyText),
            (AuthMode::Observe, TrafficPolicy::AnyText),
            (AuthMode::None, TrafficPolicy::PongOnly),
            (AuthMode::None, TrafficPolicy::TextAfterFirstPong),
        ] {
            let mut p = policy(auth);
            p.controls = ControlPolicy::None;
            p.traffic = traffic;
            let mut driver = Driver::default();
            assert_eq!(driver.register(p), Err(SocketError::InvalidPolicy));
            assert!(!driver.state.enabled());
            assert!(driver.runtimes.is_empty());
        }
    }

    #[test]
    fn required_auth_duplicate_open_and_ack_do_not_repeat_ready_effect() {
        let mut h = Harness::new(vec![policy(AuthMode::Required)], true);
        h.open(0);
        h.open(0);
        h.text(0, "accepted");
        h.text(0, "accepted");
        let effects = &h.engine.venue().effects;
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::Opened(_)))
                .count(),
            1
        );
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::Ready { .. }))
                .count(),
            1
        );
        let epoch = h.epoch(0);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Opened(e) if *e == epoch))
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Ready { epoch:e, reopened:false } if *e == epoch))
        );
        assert_eq!(
            effects
                .iter()
                .filter(|e| matches!(e, Effect::Authenticated { epoch:e, first:true } if *e==epoch))
                .count(),
            1
        );
        assert_eq!(
            effects
                .iter()
                .filter(
                    |e| matches!(e, Effect::Authenticated { epoch:e, first:false } if *e==epoch)
                )
                .count(),
            1
        );
    }

    #[test]
    fn late_auth_is_retired_before_timer_delivery_or_positive_evidence() {
        let mut h = Harness::new(vec![policy(AuthMode::Required)], true);
        h.open(0);
        let old = h.epoch(0);
        h.now = 10_000.; // Leave the previously armed watchdog undelivered.
        h.text(0, "accepted");
        assert!(!h.engine.venue().driver.state.current(old));
        assert!(
            !h.engine
                .venue()
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Authenticated { .. }))
        );
        assert!(h.engine.venue().effects.iter().any(|e| matches!(
            e,
            Effect::Retired {
                reason: RetireReason::Timeout(Timeout::Auth),
                ..
            }
        )));
        assert_eq!(h.closes, [old.socket()]);
        h.event(Event::WsMessage {
            socket: old.socket(),
            text: "accepted",
        });
        h.event(Event::WsClosed {
            socket: old.socket(),
            code: 1006,
        });
        assert_eq!(h.closes.len(), 1);
    }

    #[test]
    fn same_socket_retry_coalesces_and_positive_duplicate_cancels_it() {
        let mut h = Harness::new(vec![bitunix()], true);
        h.open(0);
        let epoch = h.epoch(0);
        h.text(0, "malformed");
        h.text(0, "rejected");
        let timers = h.timers.clone();
        h.text(0, "malformed");
        assert_eq!(h.timers, timers);
        h.advance(2_000.);
        assert_eq!(
            h.engine
                .venue()
                .effects
                .iter()
                .filter(|e| matches!(e, Effect::RetryLogin(e) if *e==epoch))
                .count(),
            1
        );
        h.text(0, "accepted");
        h.text(0, "malformed");
        h.text(0, "accepted");
        h.advance(4_000.);
        assert_eq!(
            h.engine
                .venue()
                .effects
                .iter()
                .filter(|e| matches!(e, Effect::RetryLogin(_)))
                .count(),
            1
        );
        assert_eq!(h.epoch(0), epoch);
        assert!(h.engine.venue().effects.iter().any(|e| matches!(e, Effect::AuthFailure { epoch:e, evidence:AuthEvidence::Malformed, .. } if *e==epoch)));
    }

    #[test]
    fn first_pong_idle_guard_resets_on_each_epoch_and_current_text_preserves_stability() {
        let mut h = Harness::new(vec![bitunix()], true);
        h.open(0);
        h.advance(100_000.);
        let old = h.epoch(0);
        h.text(0, "pong");
        // Expiry at the exact bound wins over a late pong, without timer delivery.
        h.now = 145_000.;
        h.text(0, "pong");
        assert!(!h.engine.venue().driver.state.current(old));
        let at = h.retry_at(0);
        h.advance(at);
        h.open(0);
        assert_ne!(h.epoch(0), old);
        h.advance(at + 100_000.);
        h.text(0, "ordinary-current-text");
        assert_eq!(h.closes.len(), 1); // New epoch has not seen its own pong.
        let current = h.epoch(0);
        h.event(Event::WsClosed {
            socket: current.socket(),
            code: 1006,
        });
        let delay = h.retry_at(0) - h.now;
        assert!(
            (750. ..=1_000.).contains(&delay),
            "stable text must reset backoff before pong"
        );
        assert!(
            h.engine
                .venue()
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Ping(e) if *e==old))
        );
    }

    #[test]
    fn failed_invalidation_acknowledgment_quarantines_without_retry_or_late_timer_effect() {
        let mut h = Harness::new(vec![policy(AuthMode::None)], false);
        h.open(0);
        let epoch = h.epoch(0);
        let old_timers: Vec<_> = h.timers.keys().copied().collect();
        h.text(0, "stop");
        assert!(!h.engine.venue().driver.state.current(epoch));
        assert!(h.timers.is_empty());
        let count = h.engine.venue().effects.len();
        for timer in old_timers {
            h.event(Event::TimerFired { timer });
        }
        h.advance(1_000_000.);
        assert_eq!(h.engine.venue().effects.len(), count);
    }

    #[test]
    fn terminal_group_cancels_inactive_retry_before_any_effect_and_never_regrants() {
        let group = FeedPolicy {
            permanent_refusal_group: Some(7),
            ..policy(AuthMode::Required)
        };
        let mut h = Harness::new(vec![group.clone(), group], true);
        h.open(0);
        h.open(1);
        let first = h.epoch(0);
        let second = h.epoch(1);
        h.event(Event::WsClosed {
            socket: second.socket(),
            code: 1006,
        });
        h.text(0, "rejected");
        assert!(!h.engine.venue().driver.state.current(first));
        assert!(h.timers.is_empty());
        h.advance(1_000_000.);
        assert!(
            !h.engine
                .venue()
                .effects
                .iter()
                .any(|e| matches!(e, Effect::Reconnect(_)))
        );
        assert_eq!(h.closes.len(), 2);
    }

    #[test]
    fn invalid_raw_time_refuses_begin_before_epoch_physical_connection_or_timer() {
        for now in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            -f64::MAX,
        ] {
            let h = Harness::at(vec![bitunix()], true, now);
            let venue = h.engine.venue();
            assert_eq!(venue.begin_errors, [SocketError::InvalidTime]);
            assert_eq!(venue.driver.state.epoch(venue.feeds[0]), None);
            assert!(h.timers.is_empty());
            assert!(h.closes.is_empty());
            assert!(h.connects.is_empty());
        }
        let h = Harness::at(vec![bitunix()], true, 0.125);
        assert!(h.engine.venue().begin_errors.is_empty());
        assert_eq!(h.timers.len(), 1);
    }

    #[test]
    fn invalid_ingress_time_cannot_accept_auth_or_refresh_or_consume_owned_timer() {
        let mut h = Harness::new(vec![policy(AuthMode::Required)], true);
        h.open(0);
        let epoch = h.epoch(0);
        let before = h.timers.clone();
        let timer = *before.keys().next().unwrap();
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
            h.now = now;
            assert_eq!(
                h.engine.venue().driver.timer_admissible(timer, now),
                Some(false)
            );
            // Final Engine timer ingress must query this BEFORE Io removal.
            h.text(0, "accepted");
            assert_eq!(h.epoch(0), epoch);
            assert!(!h.engine.venue().driver.state.authenticated(epoch.feed()));
            assert_eq!(h.timers, before);
            assert_eq!(h.engine.venue().effects.len(), 1); // First Opened only.
        }
        h.now = 1_000.;
        assert_eq!(
            h.engine.venue().driver.timer_admissible(timer, h.now),
            Some(true)
        );
        h.text(0, "accepted");
        assert!(h.engine.venue().driver.state.authenticated(epoch.feed()));
    }

    #[test]
    fn current_ingress_uses_its_own_supervisor_among_expired_neighbors() {
        let mut healthy = policy(AuthMode::None);
        healthy.traffic = TrafficPolicy::AnyText;
        healthy.supervision.idle_timeout_ms = 100_000.;
        let mut h = Harness::new(vec![policy(AuthMode::Required), healthy], true);
        h.open(0);
        h.open(1);
        let expired = h.epoch(0);
        let current = h.epoch(1);
        h.now = 10_000.; // Neighbor auth expires; healthy socket remains live.
        h.text(1, "ordinary");
        assert!(h.engine.venue().driver.state.current(current));
        assert!(h.engine.venue().driver.state.current(expired));
        assert!(h.closes.is_empty());
        h.text(0, "accepted");
        assert!(!h.engine.venue().driver.state.current(expired));
        assert!(h.engine.venue().driver.state.current(current));
        assert_eq!(h.closes, [expired.socket()]);
        h.text(1, "ordinary");
        assert_eq!(h.epoch(1), current);
    }

    #[test]
    fn invalid_policy_is_typed_and_does_not_partially_register_runtime() {
        let mut driver = Driver::default();
        let mut invalid = bitunix();
        invalid.supervision.connect_timeout_ms = f64::NAN;
        assert_eq!(driver.register(invalid), Err(SocketError::InvalidPolicy));
        assert!(!driver.state.enabled());
        assert!(driver.runtimes.is_empty());
    }
}

//! Managed socket boundaries and native work after final subscription handoff.
use super::*;
use crate::socket_driver::{CloseCause, ControlWork, Driver, Effect};
use crate::socket_lifecycle::{
    Admission, Control, ControlPolicy, EncodedBatch, Epoch, FeedId, FeedPolicy, Ingress, Interest,
    LifecycleEvent, MAX_FEEDS, MAX_INTEREST_BYTES, MAX_INTERESTS, RetireReason, SocketError,
};
const MAX_EFFECTS: usize = MAX_FEEDS * 16;
const MAX_PROPOSALS: usize = MAX_FEEDS;

pub(super) enum InterestUpdate {
    Desired(Vec<Interest>),
    Repair(Vec<String>),
}
pub(super) struct Proposal<P> {
    id: u64,
    feed: FeedId,
    update: InterestUpdate,
    bytes: usize,
    continuation: Option<Continuation<P>>,
}

pub(super) struct Continuation<P> {
    epoch: Epoch,
    proposal: u64,
    required: Vec<Interest>,
    bytes: usize,
    revision: u64,
    timer: Option<TimerId>,
    armed: bool,
    armed_at: f64,
    pending: P,
}

impl<P> Cx<'_, P> {
    fn with_socket_driver<R>(&mut self, f: impl FnOnce(&mut Driver, &mut Self) -> R) -> R {
        let mut driver = self.take_socket_driver();
        let result = f(&mut driver, self);
        self.restore_socket_driver(driver);
        result
    }
    // Shared out of line: every driver closure would otherwise inline the
    // driver's default construction and drop.
    #[inline(never)]
    fn take_socket_driver(&mut self) -> Driver {
        std::mem::take(&mut self.io.core.socket_driver)
    }
    #[inline(never)]
    fn restore_socket_driver(&mut self, driver: Driver) {
        self.io.core.socket_driver = driver;
    }
    #[inline(never)]
    pub(super) fn socket_effects(&mut self, effects: Vec<Effect>) {
        if self
            .io
            .core
            .socket_effects
            .len()
            .saturating_add(effects.len())
            > MAX_EFFECTS
        {
            self.io.core.socket_failed = true;
            // Revoked epochs still own physical/signing cleanup. Preserve
            // their retirements when dropping nonterminal work on exhaustion.
            self.io
                .core
                .socket_effects
                .retain(|e| matches!(e, Effect::Retired { .. }));
            self.io.core.socket_effects.extend(
                effects
                    .into_iter()
                    .filter(|e| matches!(e, Effect::Retired { .. })),
            );
            self.io.socket_proposals.clear();
            self.io.core.socket_proposal_bytes = 0;
            let stopped = self.with_socket_driver(|d, cx| d.stop_all(cx));
            self.io.core.socket_effects.extend(stopped);
            self.notice(
                Level::Error,
                log_code::SUBSCRIPTION_REJECTED,
                "Managed socket effect capacity exhausted",
            );
            return;
        }
        self.io.core.socket_effects.extend(effects);
    }
    pub fn register_socket_feed(&mut self, policy: FeedPolicy) -> Result<FeedId, SocketError> {
        if !self.io.core.managed_sockets_declared {
            return Err(SocketError::NotPermitted);
        }
        self.io.opaque_entry_activity();
        if self.io.core.socket_failed {
            return Err(SocketError::NotPermitted);
        }
        let feed = self.io.core.socket_driver.register(policy)?;
        self.io.core.managed_sockets = true;
        Ok(feed)
    }
    #[cfg(feature = "socket-native-timing")]
    pub fn register_socket_feed_with_native_timing(
        &mut self,
        policy: FeedPolicy,
        timing: crate::socket_lifecycle::NativeTiming,
    ) -> Result<FeedId, SocketError> {
        if !self.io.core.managed_sockets_declared {
            return Err(SocketError::NotPermitted);
        }
        self.io.assert_actions_safe();
        self.io.opaque_entry_activity();
        if self.io.core.socket_failed {
            return Err(SocketError::NotPermitted);
        }
        let feed = self
            .io
            .core
            .socket_driver
            .register_with_native_timing(policy, timing)?;
        self.io.core.managed_sockets = true;
        Ok(feed)
    }
    #[cfg(feature = "socket-native-timing")]
    pub fn complete_socket_initial_payload(&mut self) -> Result<(), SocketError> {
        self.io.assert_actions_safe();
        self.io.opaque_entry_activity();
        let (epoch, received_at) = self.io.core.socket_message.ok_or(SocketError::Stale)?;
        if self.now != received_at {
            return Err(SocketError::InvalidTime);
        }
        self.io
            .core
            .socket_driver
            .complete_initial_payload(epoch, self.now)?;
        self.io.core.socket_message = None;
        Ok(())
    }
    /// Keep login permission independent of a connect-origin native snapshot bound.
    pub fn register_socket_feed_with_snapshot(
        &mut self,
        policy: FeedPolicy,
    ) -> Result<FeedId, SocketError> {
        if !self.io.core.managed_sockets_declared {
            return Err(SocketError::NotPermitted);
        }
        self.io.assert_actions_safe();
        self.io.opaque_entry_activity();
        if self.io.core.socket_failed {
            return Err(SocketError::NotPermitted);
        }
        let feed = self
            .io
            .core
            .socket_driver
            .register_with_snapshot(policy, true)?;
        self.io.core.managed_sockets = true;
        Ok(feed)
    }
    /// Complete validated native snapshot proof from this exact text callback only.
    pub fn complete_socket_snapshot(&mut self) -> Result<(), SocketError> {
        self.io.assert_actions_safe();
        self.io.opaque_entry_activity();
        let (epoch, received_at) = self.io.core.socket_message.ok_or(SocketError::Stale)?;
        if self.now != received_at {
            return Err(SocketError::InvalidTime);
        }
        self.io
            .core
            .socket_driver
            .validate_snapshot_completion(epoch, self.now)?;
        if !self.io.core.next_id.available(1) {
            return Err(SocketError::Capacity);
        }
        self.with_socket_driver(|d, cx| d.complete_snapshot(cx, epoch))?;
        self.io.core.socket_message = None;
        Ok(())
    }
    #[inline]
    pub fn socket_epoch(&self, feed: FeedId) -> Option<Epoch> {
        self.core_ref().socket_epoch(feed)
    }
    pub fn socket_authenticated(&self, feed: FeedId) -> bool {
        self.io.core.socket_driver.state.authenticated(feed)
    }
    /// Native one-shot frames ordered after current interests, retained with
    /// their epoch until final drain. They are never persistent interests.
    pub fn send_after_socket_interests(
        &mut self,
        feed: FeedId,
        text: String,
    ) -> Result<(), SocketError> {
        self.io.opaque_entry_activity();
        let epoch = self.socket_epoch(feed).ok_or(SocketError::Stale)?;
        if self.io.core.socket_failed
            || self.io.core.socket_after.len() >= crate::socket_lifecycle::MAX_FRAMES
            || text.len() > crate::socket_lifecycle::MAX_FRAME_BYTES
        {
            return Err(SocketError::Capacity);
        }
        self.io
            .core
            .socket_driver
            .state
            .reserve_external(text.len())?;
        self.io.core.socket_after.push_back((epoch, text));
        Ok(())
    }
    pub fn connect_socket(
        &mut self,
        feed: FeedId,
        spec: impl Into<SocketSpec>,
    ) -> Result<Epoch, SocketError> {
        if self.io.core.socket_failed {
            return Err(SocketError::NotPermitted);
        }
        self.io.core.socket_driver.preflight_begin(feed, self.now)?;
        if self
            .io
            .core
            .socket_driver
            .snapshot_connect_ids(feed)
            .is_some_and(|count| !self.io.core.next_id.available(count))
        {
            return Err(SocketError::Capacity);
        }
        let role = self.io.core.socket_driver.state.policy(feed)?.role.clone();
        if role == SocketChannel::Public
            && self.io.core.public_feed_policy != PublicFeedPolicy::Direct
        {
            return Err(SocketError::NotPermitted);
        }
        let mut spec = spec.into();
        spec.channel = Some(role);
        let socket = SocketId(self.io.next());
        let epoch = self.with_socket_driver(|d, cx| d.begin(cx, feed, socket))?;
        self.queue_socket_transport(socket, spec);
        Ok(epoch)
    }
    pub fn connect_socket_signed(
        &mut self,
        feed: FeedId,
        spec: impl Into<SocketSpec>,
        step: Box<dyn crate::SignStep>,
    ) -> Result<Epoch, SocketError> {
        let epoch = self.connect_socket(feed, spec)?;
        self.sign_socket_upgrade(epoch.socket(), step);
        Ok(epoch)
    }
    pub fn retire_socket(&mut self, feed: FeedId, reason: RetireReason) {
        self.io.opaque_entry_activity();
        let effects =
            self.with_socket_driver(|d, cx| d.retire(cx, feed, reason, true).into_iter().collect());
        self.socket_effects(effects);
    }
    pub fn stop_socket_feed(&mut self, feed: FeedId) {
        self.io.opaque_entry_activity();
        let effects = self.with_socket_driver(|d, cx| d.stop_feed(cx, feed).into_iter().collect());
        self.socket_effects(effects);
    }
    fn propose_interests(
        &mut self,
        feed: FeedId,
        update: InterestUpdate,
        after: Option<(Vec<Interest>, P)>,
    ) -> Result<u64, SocketError> {
        self.io.opaque_entry_activity();
        if self.io.core.socket_failed {
            return Err(SocketError::NotPermitted);
        }
        self.io.core.socket_driver.state.policy(feed)?;
        let (count, bytes) = match &update {
            InterestUpdate::Desired(values) => (
                values.len(),
                values
                    .iter()
                    .try_fold(0usize, |n, i| {
                        n.checked_add(i.key.len())?.checked_add(i.wire_args.len())
                    })
                    .ok_or(SocketError::Capacity)?,
            ),
            InterestUpdate::Repair(keys) => (
                keys.len(),
                keys.iter()
                    .try_fold(0usize, |n, k| n.checked_add(k.len()))
                    .ok_or(SocketError::Capacity)?,
            ),
        };
        let required_bytes = after
            .as_ref()
            .map(|(required, _)| {
                required
                    .iter()
                    .try_fold(0usize, |bytes, i| {
                        bytes
                            .checked_add(i.key.len())?
                            .checked_add(i.wire_args.len())
                    })
                    .ok_or(SocketError::Capacity)
            })
            .transpose()?
            .unwrap_or(0);
        let epoch = if after.is_some() {
            Some(self.socket_epoch(feed).ok_or(SocketError::Stale)?)
        } else {
            None
        };
        if count > MAX_INTERESTS
            || bytes > MAX_INTEREST_BYTES / 2
            || self.io.socket_proposals.len() + self.io.socket_continuations.len() >= MAX_PROPOSALS
            || self
                .io
                .core
                .socket_proposal_bytes
                .saturating_add(self.io.core.socket_continuation_bytes)
                .saturating_add(bytes)
                .saturating_add(required_bytes)
                > MAX_INTEREST_BYTES
        {
            return Err(SocketError::Capacity);
        }
        let id = self
            .io
            .core
            .socket_proposal_next
            .checked_add(1)
            .ok_or(SocketError::Capacity)?;
        let continuation = if let Some((required, pending)) = after {
            let timer = TimerId(self.io.reserve_ids(1).ok_or(SocketError::Capacity)?.start);
            Some(Continuation {
                epoch: epoch.expect("continuation preflight checked epoch"),
                proposal: id,
                required,
                bytes: required_bytes,
                revision: 0,
                timer: Some(timer),
                armed: false,
                armed_at: 0.,
                pending,
            })
        } else {
            None
        };
        self.io.core.socket_proposal_next = id;
        let bytes = bytes + required_bytes;
        self.io.core.socket_proposal_bytes += bytes;
        self.io.socket_proposals.push_back(Proposal {
            id,
            feed,
            update,
            bytes,
            continuation,
        });
        Ok(id)
    }
    /// Admission only. Encoding/transaction completion is a LifecycleEvent.
    pub fn set_socket_interests(
        &mut self,
        feed: FeedId,
        values: Vec<Interest>,
    ) -> Result<u64, SocketError> {
        self.propose_interests(feed, InterestUpdate::Desired(values), None)
    }
    /// Admit native work behind final subscription handoff and a subsequent
    /// engine ingress. Required canonical interests must survive replacement.
    pub fn set_socket_interests_then(
        &mut self,
        feed: FeedId,
        values: Vec<Interest>,
        required_keys: &[&str],
        pending: P,
    ) -> Result<u64, SocketError> {
        // Before any activity, allocation or retention: an undeclared handoff
        // must leave no work for the venue's compiled-out continuation paths.
        if !self.io.core.socket_capabilities.handoff {
            return Err(SocketError::NotPermitted);
        }
        self.io.opaque_entry_activity();
        if values.len() > MAX_INTERESTS || required_keys.len() > MAX_INTERESTS {
            return Err(SocketError::Capacity);
        }
        // Inserted one by one: `collect` would add its own sort and bulk build.
        let mut values_by_key = std::collections::BTreeMap::new();
        for value in &values {
            if values_by_key.insert(value.key.as_ref(), value).is_some() {
                return Err(SocketError::InvalidInterest);
            }
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut required = Vec::with_capacity(required_keys.len());
        for key in required_keys {
            if !seen.insert(*key) {
                return Err(SocketError::InvalidInterest);
            }
            required.push((*values_by_key.get(key).ok_or(SocketError::InvalidInterest)?).clone());
        }
        self.propose_interests(
            feed,
            InterestUpdate::Desired(values),
            Some((required, pending)),
        )
    }
    /// Force native unsubscribe/subscribe for unchanged current interests.
    pub fn repair_socket_interests(
        &mut self,
        feed: FeedId,
        keys: Vec<String>,
    ) -> Result<u64, SocketError> {
        self.propose_interests(feed, InterestUpdate::Repair(keys), None)
    }
    pub(super) fn managed_send_allowed(&self, socket: SocketId) -> bool {
        !self.io.core.managed_sockets
            || !self.io.core.socket_failed
                && self
                    .io
                    .core
                    .socket_driver
                    .state
                    .socket_epoch(socket)
                    .is_some()
    }
}

impl crate::engine::CoreCx<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(feature = "managed-sockets", allow(dead_code))]
    #[inline]
    pub fn socket_epoch(&self, feed: FeedId) -> Option<Epoch> {
        self.core_ref().socket_epoch(feed)
    }
}

impl crate::engine::CoreRef<'_> {
    // Only some feature combinations call this.
    #[cfg_attr(feature = "managed-sockets", allow(dead_code))]
    pub fn socket_epoch(&self, feed: FeedId) -> Option<Epoch> {
        self.io.core.socket_driver.state.epoch(feed)
    }
}

impl<V: Venue> Engine<V> {
    pub(super) fn has_socket_work(&self) -> bool {
        if !V::managed_sockets() {
            return false;
        }
        !self.io.core.socket_effects.is_empty()
            || !self.io.socket_proposals.is_empty()
            || self
                .io
                .core
                .socket_driver
                .frames_ready(self.io.core.dispatch_now, |socket| {
                    self.io.core.signing.has_socket(socket)
                })
            || V::socket_capabilities().handoff
                && self.io.socket_continuations.iter().any(|work| {
                    !work.armed && self.continuation_ready(work, self.io.core.dispatch_now)
                })
            || self.io.core.socket_after.iter().any(|(epoch, _)| {
                self.io.core.socket_driver.one_shot_ready(
                    *epoch,
                    self.io.core.dispatch_now,
                    self.io.core.signing.has_socket(epoch.socket()),
                )
            })
    }
    fn socket_mutate<R>(
        &mut self,
        now: f64,
        f: impl FnOnce(&mut Driver, &mut Cx<'_, V::Pending>) -> R,
    ) -> R {
        self.assert_live();
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        let result = cx.with_socket_driver(f);
        self.assert_live();
        result
    }
    fn socket_native(&mut self, now: f64, event: LifecycleEvent) -> Result<(), SocketError> {
        self.assert_live();
        // Lifecycle work is independent of the HTTP codec that queued it.
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_history_failure = None;
        }
        let message = self.io.core.socket_message.take();
        let delivery = self.io.core.http_delivery.take();
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        let result = self.venue.on_socket_event(&mut cx, event);
        // A venue may catch an allocation panic. It cannot grant another
        // callback or hide invalid output behind a socket-retirement filter.
        self.io.assert_actions_safe();
        self.io.core.http_delivery = delivery;
        self.io.core.socket_message = message;
        result
    }
    fn socket_enqueue(&mut self, now: f64, effects: Vec<Effect>) {
        if !V::managed_sockets() {
            return;
        }
        let mut cx = Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        };
        cx.socket_effects(effects);
    }
    fn socket_replace(&mut self, feed: FeedId, update: InterestUpdate) -> Result<u64, SocketError> {
        let venue = &self.venue;
        let store = &self.store;
        let encode =
            |subscribe,
             items: &[Interest],
             emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>| {
                venue.encode_socket_interests(store, feed, subscribe, items, emit)
            };
        match update {
            InterestUpdate::Desired(items) => self
                .io
                .core
                .socket_driver
                .state
                .replace_interests(feed, items, encode),
            InterestUpdate::Repair(keys) => self
                .io
                .core
                .socket_driver
                .state
                .repair_interests(feed, &keys, encode),
        }
    }
    fn socket_replay(&mut self, feed: FeedId) -> Result<u64, SocketError> {
        let venue = &self.venue;
        let store = &self.store;
        self.io
            .core
            .socket_driver
            .state
            .replay_interests(feed, |subscribe, items, emit| {
                venue.encode_socket_interests(store, feed, subscribe, items, emit)
            })
    }
    #[inline(always)]
    pub(super) fn process_socket_work(&mut self, now: f64) {
        if !V::managed_sockets() {
            return;
        }
        self.assert_live();
        if !self.io.core.managed_sockets {
            return;
        }
        if self.io.core.socket_effects.is_empty() && self.io.socket_proposals.is_empty() {
            return;
        }
        self.process_pending_socket_work(now);
    }
    /// One shared notice for a refused lifecycle step.
    #[inline(never)]
    fn socket_refused(&mut self, now: f64, what: &str, error: SocketError) {
        Cx {
            now,
            store: &mut self.store,
            io: &mut self.io,
        }
        .notice(
            Level::Error,
            log_code::SUBSCRIPTION_REJECTED,
            format!("Managed socket {what} refused: {error:?}"),
        );
    }
    #[inline(never)]
    fn process_pending_socket_work(&mut self, now: f64) {
        let mut processed = 0usize;
        loop {
            self.assert_live();
            if processed >= MAX_EFFECTS && !self.io.core.socket_failed {
                self.io.core.socket_failed = true;
                self.io
                    .core
                    .socket_effects
                    .retain(|e| matches!(e, Effect::Retired { .. }));
                self.io.socket_proposals.clear();
                self.io.core.socket_proposal_bytes = 0;
                let mut cx = Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                };
                cx.notice(
                    Level::Error,
                    log_code::SUBSCRIPTION_REJECTED,
                    "Managed socket effect processing capacity exhausted",
                );
                let stopped = self.socket_mutate(now, |d, cx| d.stop_all(cx));
                self.io.core.socket_effects.extend(stopped);
            }
            if let Some(effect) = self.io.core.socket_effects.pop_front() {
                processed += 1;
                if let Effect::Retired {
                    retirement,
                    reason,
                    retry,
                    close_code,
                } = effect
                {
                    if V::socket_capabilities().handoff {
                        self.cancel_socket_continuations(retirement.epoch);
                    }
                    let pending = std::mem::take(&mut self.io.core.socket_after);
                    for (epoch, text) in pending {
                        if epoch == retirement.epoch {
                            self.io
                                .core
                                .socket_driver
                                .state
                                .release_external(text.len());
                        } else {
                            self.io.core.socket_after.push_back((epoch, text));
                        }
                    }
                    {
                        let mut cx = Cx {
                            now,
                            store: &mut self.store,
                            io: &mut self.io,
                        };
                        if reason == RetireReason::Closed
                            && cx.io.core.sockets.closed(retirement.epoch.socket(), true)
                        {
                            cx.io.core.health.count(log_code::WS_CLOSED);
                        }
                        cx.close_socket_transport(retirement.epoch.socket());
                    }
                    let result = self.socket_native(
                        now,
                        LifecycleEvent::Retired {
                            epoch: retirement.epoch,
                            reason,
                            close_code,
                        },
                    );
                    if let Err(error) = result {
                        self.socket_refused(now, "invalidation", error);
                    }
                    if let Err(error) = self.socket_mutate(now, |d, cx| {
                        d.acknowledge_retirement(cx, retirement, result.is_ok(), retry, close_code)
                    }) {
                        self.socket_refused(now, "retirement", error);
                    }
                    continue;
                }
                if self.io.core.socket_failed {
                    continue;
                }
                let (event, epoch) = match effect {
                    Effect::Opened(e) => (LifecycleEvent::Opened(e), Some(e)),
                    Effect::Ready { epoch, reopened } => {
                        if let Err(error) = self.socket_replay(epoch.feed()) {
                            self.socket_failed_feed(now, epoch.feed(), error);
                            continue;
                        }
                        if V::socket_capabilities().handoff {
                            self.rebase_socket_continuations(epoch.feed());
                        }
                        (LifecycleEvent::Ready { epoch, reopened }, Some(epoch))
                    }
                    Effect::Authenticated { epoch, first } => {
                        (LifecycleEvent::Authenticated { epoch, first }, Some(epoch))
                    }
                    Effect::AuthFailure {
                        epoch,
                        evidence,
                        terminal,
                    } => (
                        LifecycleEvent::AuthFailure {
                            epoch,
                            evidence,
                            terminal,
                        },
                        None,
                    ),
                    Effect::Ping(e) => (LifecycleEvent::Ping(e), Some(e)),
                    Effect::RetryLogin(e) => (LifecycleEvent::RetryLogin(e), Some(e)),
                    Effect::Reconnect(feed) => (LifecycleEvent::Reconnect(feed), None),
                    Effect::Retired { .. } => unreachable!(),
                };
                if epoch.is_some_and(|e| !self.io.core.socket_driver.state.current(e)) {
                    continue;
                }
                let feed = match &event {
                    LifecycleEvent::Reconnect(f) => *f,
                    LifecycleEvent::AuthFailure { epoch, .. } => epoch.feed(),
                    _ => epoch.expect("epoch effect").feed(),
                };
                if let Err(error) = self.socket_native(now, event) {
                    self.socket_failed_feed(now, feed, error);
                }
                continue;
            }
            let Some(proposal) = self.io.socket_proposals.pop_front() else {
                break;
            };
            processed += 1;
            self.io.core.socket_proposal_bytes -= proposal.bytes;
            if self.io.core.socket_failed {
                continue;
            }
            let event = match self.socket_replace(proposal.feed, proposal.update) {
                Ok(revision) => {
                    if V::socket_capabilities().handoff {
                        if let Some(mut work) = proposal.continuation {
                            work.revision = revision;
                            self.io.core.socket_continuation_bytes += work.bytes;
                            self.io.socket_continuations.push_back(work);
                        }
                        self.rebase_socket_continuations(proposal.feed);
                    } else {
                        // Admission refused every handoff, so none can exist here.
                        debug_assert!(proposal.continuation.is_none());
                    }
                    LifecycleEvent::InterestsCommitted {
                        feed: proposal.feed,
                        proposal: proposal.id,
                    }
                }
                Err(error) => LifecycleEvent::InterestsRefused {
                    feed: proposal.feed,
                    proposal: proposal.id,
                    error,
                },
            };
            if let Err(error) = self.socket_native(now, event) {
                self.socket_failed_feed(now, proposal.feed, error);
            }
        }
    }
    fn socket_failed_feed(&mut self, now: f64, feed: FeedId, error: SocketError) {
        self.socket_refused(now, "operation", error);
        let effects = self.socket_mutate(now, |d, cx| d.stop_feed(cx, feed).into_iter().collect());
        self.socket_enqueue(now, effects);
    }
    pub(super) fn handle_managed_socket(&mut self, now: f64, event: &Event<'_>) -> bool {
        if !V::managed_sockets() {
            return false;
        }
        if !self.io.core.managed_sockets {
            return false;
        }
        if let Event::TimerFired { timer } = event {
            if V::socket_capabilities().handoff && self.handle_socket_continuation(now, *timer) {
                return true;
            }
            match self.io.core.socket_driver.timer_admissible(*timer, now) {
                None => return false,
                Some(false) => return true,
                Some(true) => {
                    if self.io.core.timers.remove(timer).is_some() {
                        let effects = self
                            .socket_mutate(now, |d, cx| d.timer(cx, *timer).unwrap_or_default());
                        self.socket_enqueue(now, effects);
                    }
                    self.process_socket_work(now);
                    return true;
                }
            }
        }
        let (socket, kind, cause) = match event {
            Event::WsOpened { socket } => (*socket, Ingress::Open, None),
            Event::WsMessage { socket, .. } | Event::WsBinary { socket, .. } => {
                (*socket, Ingress::Message, None)
            }
            Event::WsClosed { socket, code } => {
                (*socket, Ingress::Close, Some(CloseCause::Code(*code)))
            }
            Event::WsRouteExhausted { socket } => {
                (*socket, Ingress::Close, Some(CloseCause::RouteExhausted))
            }
            _ => return false,
        };
        let admission = if kind == Ingress::Message {
            self.assert_live();
            let admission = self.io.core.socket_driver.admit_ingress(now, socket, kind);
            self.assert_live();
            if matches!(admission, Admission::Expired(..)) {
                let result =
                    self.socket_mutate(now, |d, cx| d.finish_ingress(cx, admission, kind, None));
                self.socket_enqueue(now, result.effects);
            } else if self.io.core.socket_effects.len() > MAX_EFFECTS {
                // Empty publication still owns exhaustion cleanup at this
                // exact boundary, before health or native classification.
                self.socket_enqueue(now, Vec::new());
            }
            admission
        } else {
            let result = self.socket_mutate(now, |d, cx| match cause {
                Some(cause) => d.close(cx, socket, cause),
                None => d.ingress(cx, socket, kind),
            });
            self.socket_enqueue(now, result.effects);
            result.admission
        };
        let Admission::Current(epoch) = admission else {
            self.process_socket_work(now);
            return true;
        };
        match event {
            Event::WsOpened { .. } => {
                self.io.core.health.ws_opened = self.io.core.health.ws_opened.saturating_add(1)
            }
            Event::WsMessage { text, .. } => {
                self.io
                    .core
                    .sockets
                    .received(socket, now, &mut self.io.core.health);
                let control = match self
                    .io
                    .core
                    .socket_driver
                    .state
                    .policy(epoch.feed())
                    .expect("current managed feed")
                    .controls
                {
                    ControlPolicy::None => Control::Other,
                    ControlPolicy::Native => self.venue.socket_control(
                        &self.store,
                        epoch.feed(),
                        self.io.core.socket_driver.state.authenticated(epoch.feed()),
                        text,
                    ),
                };
                let other = matches!(
                    control,
                    Control::Other | Control::Auth(crate::socket_lifecycle::AuthEvidence::Progress)
                );
                #[cfg(feature = "socket-native-timing")]
                let other = other || matches!(control, Control::Traffic);
                let ready = self.venue.socket_startup_ready(&self.store);
                self.assert_live();
                let work = self
                    .io
                    .core
                    .socket_driver
                    .observe_control(now, epoch, &control);
                self.assert_live();
                if matches!(work, ControlWork::Complete) {
                    if self.io.core.socket_effects.len() > MAX_EFFECTS {
                        self.socket_enqueue(now, Vec::new());
                    }
                } else {
                    let effects = self.socket_mutate(now, |d, cx| {
                        d.finish_control(cx, epoch, control, ready, work)
                    });
                    self.socket_enqueue(now, effects);
                }
                self.process_socket_work(now);
                if other && self.io.core.socket_driver.state.current(epoch) {
                    let mut cx = Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    };
                    assert!(cx.io.core.socket_message.is_none());
                    cx.io.core.socket_message = Some((epoch, now));
                    Self::deliver_socket_message(&mut self.venue, &mut cx, socket, text);
                    cx.io.core.socket_message = None;
                }
            }
            Event::WsBinary { data, .. } => {
                self.io
                    .core
                    .sockets
                    .received(socket, now, &mut self.io.core.health);
                let mut cx = Cx {
                    now,
                    store: &mut self.store,
                    io: &mut self.io,
                };
                self.venue.on_binary(&mut cx, socket, data);
            }
            _ => {}
        }
        self.process_socket_work(now);
        true
    }
    pub(super) fn socket_gate(&mut self, now: f64, socket: SocketId, connect: bool) -> bool {
        if !V::managed_sockets() {
            return true;
        }
        if !self.io.core.managed_sockets {
            return true;
        }
        if self.io.core.socket_failed {
            return false;
        }
        let result = self.socket_mutate(now, |d, cx| {
            d.ingress(
                cx,
                socket,
                if connect {
                    Ingress::Connect
                } else {
                    Ingress::Send
                },
            )
        });
        self.socket_enqueue(now, result.effects);
        self.process_socket_work(now);
        matches!(result.admission,Admission::Current(e)if self.io.core.socket_driver.state.current(e))
    }
    pub(super) fn gate_socket_action(&mut self, now: f64, action: &Action) -> bool {
        match action {
            Action::WsConnect { socket, .. } => self.socket_gate(now, *socket, true),
            Action::WsSend { socket, .. } => self.socket_gate(now, *socket, false),
            _ => true,
        }
    }
    pub(super) fn fence_socket_actions(&mut self, now: f64) {
        if !V::managed_sockets() {
            return;
        }
        if !self.io.core.managed_sockets {
            return;
        }
        self.process_socket_work(now);
        let io = &mut self.io;
        io.core.actions.retain(|a| match a {
            Action::WsConnect { socket, .. } | Action::WsSend { socket, .. } => {
                !io.core.socket_failed
                    && io.core.socket_driver.state.socket_epoch(*socket).is_some()
            }
            Action::Sign { attempt, .. } => io.core.signing.timer(*attempt).is_some(),
            _ => true,
        });
    }
    pub(super) fn drain_socket_frames(&mut self, now: f64) {
        if !V::managed_sockets() {
            return;
        }
        if !self.io.core.managed_sockets || self.io.core.socket_failed {
            return;
        }
        // Presence only: actual releases below still enforce every epoch,
        // time, auth and signing barrier. Idle ingress needs no feed allocation.
        if self.io.core.socket_after.is_empty()
            && !self.io.core.socket_driver.state.has_pending_frames()
        {
            return;
        }
        for feed in self.io.core.socket_driver.state.feeds() {
            let Some(epoch) = self.io.core.socket_driver.state.epoch(feed) else {
                continue;
            };
            let mut released_any = false;
            for _ in 0..crate::socket_lifecycle::MAX_FRAMES {
                let barrier = self.io.core.signing.has_socket(epoch.socket());
                let (frame, effects) =
                    self.socket_mutate(now, |d, cx| d.release_frame(cx, epoch, barrier));
                self.socket_enqueue(now, effects);
                self.process_socket_work(now);
                let Some(text) = frame else {
                    break;
                };
                // Registry-owned until this final handoff; no held plain queue.
                released_any = true;
                self.io.core.actions.push(Action::WsSend {
                    socket: epoch.socket(),
                    text,
                });
            }
            if self.io.core.socket_driver.state.ready(epoch)
                && (!self.io.core.socket_driver.state.has_batches(feed)
                    || released_any
                        && self
                            .io
                            .core
                            .socket_driver
                            .state
                            .policy(feed)
                            .is_ok_and(|p| p.subscription_interval_ms > 0.))
                && !self.io.core.signing.has_socket(epoch.socket())
                && self
                    .io
                    .core
                    .socket_after
                    .iter()
                    .any(|(owner, _)| *owner == epoch)
                && self.socket_gate(now, epoch.socket(), false)
            {
                let pending = std::mem::take(&mut self.io.core.socket_after);
                for (owner, text) in pending {
                    if owner == epoch {
                        self.io
                            .core
                            .socket_driver
                            .state
                            .release_external(text.len());
                        self.io.core.actions.push(Action::WsSend {
                            socket: epoch.socket(),
                            text,
                        });
                    } else {
                        self.io.core.socket_after.push_back((owner, text));
                    }
                }
            }
        }
    }
}

impl<V: Venue> Engine<V> {
    fn continuation_ready(&self, work: &Continuation<V::Pending>, now: f64) -> bool {
        #[cfg(feature = "http-plans")]
        if self.io.core.http_plan_expiring.is_some() {
            return false;
        }
        !self
            .io
            .socket_proposals
            .iter()
            .any(|proposal| proposal.feed == work.epoch.feed())
            && self
                .io
                .core
                .socket_driver
                .state
                .interest_revision(work.epoch, &work.required)
                == Some(work.revision)
            && self.io.core.socket_driver.handoff_ready(
                work.epoch,
                now,
                self.io.core.signing.has_socket(work.epoch.socket()),
            )
    }
    fn clear_continuation_timer(&mut self, work: &mut Continuation<V::Pending>) {
        if work.armed {
            if let Some(timer) = work.timer.take() {
                self.io.core.timers.remove(&timer);
                self.io.core.actions.push(Action::ClearTimer { timer });
            }
            work.armed = false;
        }
    }
    fn discard_socket_continuation(&mut self, mut work: Continuation<V::Pending>) {
        self.clear_continuation_timer(&mut work);
        self.io.core.socket_continuation_bytes -= work.bytes;
    }
    fn cancel_socket_continuations(&mut self, epoch: Epoch) {
        let count = self.io.socket_continuations.len();
        for _ in 0..count {
            let work = self
                .io
                .socket_continuations
                .pop_front()
                .expect("continuation snapshot");
            if work.epoch == epoch {
                self.discard_socket_continuation(work);
            } else {
                self.io.socket_continuations.push_back(work);
            }
        }
    }
    fn rebase_socket_continuations(&mut self, feed: FeedId) {
        let count = self.io.socket_continuations.len();
        for _ in 0..count {
            let mut work = self
                .io
                .socket_continuations
                .pop_front()
                .expect("continuation snapshot");
            if work.epoch.feed() != feed {
                self.io.socket_continuations.push_back(work);
                continue;
            }
            let Some(revision) = self
                .io
                .core
                .socket_driver
                .state
                .interest_revision(work.epoch, &work.required)
            else {
                self.discard_socket_continuation(work);
                continue;
            };
            if revision != work.revision {
                self.clear_continuation_timer(&mut work);
                work.revision = revision;
            }
            self.io.socket_continuations.push_back(work);
        }
    }
    pub(super) fn arm_socket_continuations(&mut self, now: f64) {
        if !V::managed_sockets() {
            return;
        }
        if self.io.socket_continuations.is_empty() || self.io.core.socket_failed {
            return;
        }
        let count = self.io.socket_continuations.len();
        for _ in 0..count {
            let mut work = self
                .io
                .socket_continuations
                .pop_front()
                .expect("continuation snapshot");
            if !work.armed && self.continuation_ready(&work, now) {
                if work.timer.is_none() {
                    work.timer = self.io.reserve_ids(1).map(|ids| TimerId(ids.start));
                }
                let Some(timer) = work.timer else {
                    let proposal = work.proposal;
                    self.discard_socket_continuation(work);
                    let mut cx = Cx {
                        now,
                        store: &mut self.store,
                        io: &mut self.io,
                    };
                    cx.notice(
                        Level::Error,
                        log_code::SUBSCRIPTION_REJECTED,
                        format!("Managed socket continuation {proposal} capacity exhausted"),
                    );
                    continue;
                };
                work.armed = true;
                work.armed_at = now;
                self.io.core.timers.insert(timer, ());
                self.io
                    .core
                    .actions
                    .push(Action::SetTimer { timer, at_ms: now });
            }
            self.io.socket_continuations.push_back(work);
        }
    }
    fn handle_socket_continuation(&mut self, now: f64, timer: TimerId) -> bool {
        let Some(index) = self
            .io
            .socket_continuations
            .iter()
            .position(|work| work.armed && work.timer == Some(timer))
        else {
            return false;
        };
        if !self.io.core.timers.contains_key(&timer) {
            let work = self
                .io
                .socket_continuations
                .remove(index)
                .expect("located cancelled continuation");
            self.discard_socket_continuation(work);
            return true;
        }
        #[cfg(feature = "http-plans")]
        if self.io.core.http_plan_expiring.is_some() {
            self.io.core.timers.remove(&timer);
            let work = &mut self.io.socket_continuations[index];
            work.armed = false;
            work.timer = None;
            return true;
        }
        if now < self.io.socket_continuations[index].armed_at {
            self.io.core.actions.push(Action::SetTimer {
                timer,
                at_ms: self.io.socket_continuations[index].armed_at,
            });
            return true;
        }
        self.io.core.timers.remove(&timer);
        let mut work = self
            .io
            .socket_continuations
            .remove(index)
            .expect("located continuation");
        work.armed = false;
        work.timer = None;
        if !self.socket_gate(now, work.epoch.socket(), false) {
            self.discard_socket_continuation(work);
            return true;
        }
        if !self.continuation_ready(&work, now) {
            if self
                .io
                .core
                .socket_driver
                .state
                .interest_revision(work.epoch, &work.required)
                == Some(work.revision)
            {
                self.io.socket_continuations.push_back(work);
            } else {
                self.discard_socket_continuation(work);
            }
            return true;
        }
        self.io.core.socket_continuation_bytes -= work.bytes;
        self.assert_live();
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_history_failure = None;
        }
        let message = self.io.core.socket_message.take();
        let delivery = self.io.core.http_delivery.take();
        let entry = self.io.core.command_entry.take();
        #[cfg(feature = "order-locks")]
        let order = self.io.core.order_response_token.take();
        #[cfg(feature = "http-plans")]
        let plan = self.io.core.http_plan_ready.take();
        let result = self.venue.on_socket_interests_released(
            &mut Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            },
            work.epoch,
            work.pending,
        );
        self.io.assert_actions_safe();
        self.io.core.http_delivery = delivery;
        self.io.core.socket_message = message;
        self.io.core.command_entry = entry;
        #[cfg(feature = "order-locks")]
        {
            self.io.core.order_response_token = order;
        }
        #[cfg(feature = "http-plans")]
        {
            self.io.core.http_plan_ready = plan;
        }
        self.assert_live();
        if let Err(error) = result {
            self.socket_failed_feed(now, work.epoch.feed(), error);
        }
        self.process_socket_work(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::socket_lifecycle::{AuthMode, AuthRecovery, Invalidation, TrafficPolicy};
    use crate::{book::Book, socket::Config};

    #[derive(Default)]
    struct Probe {
        feed: Option<FeedId>,
        controls: ControlPolicy,
        control_calls: std::cell::Cell<usize>,
        events: Vec<LifecycleEvent>,
        messages: usize,
        timers: usize,
        refuse_ready: bool,
        refuse_retirement: bool,
        receipts: usize,
        borrowed_http_delivery: bool,
        catch_fatal_on_ping: bool,
        caught_fatal: bool,
    }
    fn policy() -> FeedPolicy {
        FeedPolicy {
            role: SocketChannel::Private,
            supervision: Config::default(),
            auth: AuthMode::None,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::AnyText,
            controls: ControlPolicy::Native,
            invalidation: Invalidation::Required,
            subscription_interval_ms: 0.,
            close_floors: &[],
        }
    }
    impl Venue for Probe {
        fn managed_sockets() -> bool {
            true
        }
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            let mut declared = policy();
            declared.controls = self.controls;
            let feed = cx.register_socket_feed(declared).unwrap();
            self.feed = Some(feed);
            cx.connect_socket(feed, "wss://socket.test").unwrap();
        }
        fn socket_control(&self, _: &Store, _: FeedId, _: bool, _: &str) -> Control {
            self.control_calls.set(self.control_calls.get() + 1);
            Control::Other
        }
        fn on_socket_event(
            &mut self,
            cx: &mut Cx<'_, ()>,
            event: LifecycleEvent,
        ) -> Result<(), SocketError> {
            self.borrowed_http_delivery |= cx.io.core.http_delivery.is_some();
            self.events.push(event.clone());
            if self.catch_fatal_on_ping
                && !self.caught_fatal
                && matches!(event, LifecycleEvent::Ping(_))
            {
                cx.request(
                    Method::Get,
                    "https://socket.test/withheld".into(),
                    vec![],
                    None,
                    (),
                );
                let trapped =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cx.set_timer(1.)));
                self.caught_fatal = trapped.is_err();
            }
            match event {
                LifecycleEvent::Reconnect(feed) => {
                    cx.connect_socket(feed, "wss://socket.test")?;
                }
                LifecycleEvent::Ready { .. } if self.refuse_ready => {
                    return Err(SocketError::InvalidInterest);
                }
                LifecycleEvent::Retired { .. } if self.refuse_retirement => {
                    return Err(SocketError::NotPermitted);
                }
                _ => {}
            }
            Ok(())
        }
        fn encode_socket_interests(
            &self,
            _: &Store,
            _: FeedId,
            subscribe: bool,
            items: &[Interest],
            emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError> {
            for item in items {
                emit(EncodedBatch {
                    text: format!("{subscribe}:{}", item.wire_args),
                    covered_keys: vec![item.key.clone()],
                })?;
            }
            Ok(())
        }
        fn on_signing_receipt(&mut self, _: SigningTarget<'_, ()>, _: crate::signing::SignReceipt) {
            self.receipts += 1;
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {
            panic!("managed open reached legacy handler");
        }
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: &str) {
            self.messages += 1;
            cx.notice(Level::Error, log_code::AUTH_REJECTED, "probe refusal");
        }
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {
            panic!("managed close reached legacy handler");
        }
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {
            self.timers += 1;
        }
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    fn engine() -> (Engine<Probe>, SocketId) {
        let mut e = crate::test_support::seeded_engine(Probe::default());
        e.start(0.);
        let actions: Vec<_> = e.drain_actions().collect();
        let socket = actions
            .iter()
            .find_map(|a| {
                if let Action::WsConnect { socket, .. } = a {
                    Some(*socket)
                } else {
                    None
                }
            })
            .unwrap();
        (e, socket)
    }
    fn stop(e: &mut Engine<Probe>) {
        let feed = e.venue.feed.unwrap();
        let mut cx = Cx {
            now: e.io.core.dispatch_now,
            store: &mut e.store,
            io: &mut e.io,
        };
        cx.stop_socket_feed(feed);
    }
    #[test]
    fn declared_control_free_frames_skip_only_codec_and_keep_fresh_expiry() {
        for controls in [ControlPolicy::Native, ControlPolicy::None] {
            let mut e = crate::test_support::seeded_engine(Probe {
                controls,
                ..Probe::default()
            });
            e.start(0.);
            let socket = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::WsConnect { socket, .. } => Some(socket),
                    _ => None,
                })
                .unwrap();
            e.handle(0., Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            e.handle(
                100.,
                Event::WsMessage {
                    socket,
                    text: "ordinary-native-data",
                },
            );
            e.drain_actions().for_each(drop);
            assert_eq!(e.venue.messages, 1);
            let expected = usize::from(controls == ControlPolicy::Native);
            assert_eq!(e.venue.control_calls.get(), expected);
            for now in [f64::NAN, f64::INFINITY, f64::MAX] {
                e.handle(
                    now,
                    Event::WsMessage {
                        socket,
                        text: "invalid-clock",
                    },
                );
                assert_eq!(e.venue.messages, 1);
                assert_eq!(e.venue.control_calls.get(), expected);
            }
            // No watchdog timer is delivered: expiry must win at ingress.
            e.handle(
                60_100.,
                Event::WsMessage {
                    socket,
                    text: "too-late",
                },
            );
            let actions: Vec<_> = e.drain_actions().collect();
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::WsClose {socket:s} if *s==socket))
                    .count(),
                1
            );
            assert_eq!(e.venue.messages, 1);
            assert_eq!(e.venue.control_calls.get(), expected);
            e.handle(
                60_101.,
                Event::WsMessage {
                    socket,
                    text: "retired",
                },
            );
            assert_eq!(e.venue.messages, 1);
            assert_eq!(e.venue.control_calls.get(), expected);
        }
    }

    #[test]
    fn healthy_current_text_keeps_resource_ids_and_watchdog_timer() {
        for controls in [ControlPolicy::Native, ControlPolicy::None] {
            let mut e = crate::test_support::seeded_engine(Probe {
                controls,
                ..Probe::default()
            });
            e.start(0.);
            let socket = e
                .drain_actions()
                .find_map(|a| match a {
                    Action::WsConnect { socket, .. } => Some(socket),
                    _ => None,
                })
                .unwrap();
            e.handle(0., Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            let ids = e.io.core.next_id;
            let mut timers = e.io.core.timers.keys().copied().collect::<Vec<_>>();
            timers.sort_by_key(|timer| timer.0);
            assert!(!timers.is_empty(), "opened feed must own a watchdog");
            let queries = e.io.core.socket_driver.state.socket_lookup_count();
            for n in 1..=64 {
                e.handle(
                    n as f64,
                    Event::WsMessage {
                        socket,
                        text: "ordinary-data",
                    },
                );
                let actions = e.drain_actions().collect::<Vec<_>>();
                assert!(
                    actions
                        .iter()
                        .all(|action| matches!(action, Action::Notice(_)))
                );
            }
            let mut after = e.io.core.timers.keys().copied().collect::<Vec<_>>();
            after.sort_by_key(|timer| timer.0);
            assert_eq!(e.io.core.next_id, ids);
            assert_eq!(after, timers);
            assert_eq!(e.venue.messages, 64);
            assert_eq!(
                e.venue.control_calls.get(),
                if controls == ControlPolicy::Native {
                    64
                } else {
                    0
                }
            );
            assert_eq!(
                e.io.core.socket_driver.state.socket_lookup_count(),
                queries + 128
            );
        }
    }

    #[test]
    fn ordinary_message_keeps_overfull_effect_cleanup_before_codec_delivery() {
        for controls in [ControlPolicy::Native, ControlPolicy::None] {
            let mut e = crate::test_support::seeded_engine(Probe {
                controls,
                ..Probe::default()
            });
            e.start(0.);
            let socket = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::WsConnect { socket, .. } => Some(socket),
                    _ => None,
                })
                .unwrap();
            e.handle(0., Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            let epoch =
                e.io.core
                    .socket_driver
                    .state
                    .epoch(e.venue.feed.unwrap())
                    .unwrap();
            e.venue.events.clear();
            e.io.core
                .socket_effects
                .extend(std::iter::repeat_n(Effect::Ping(epoch), MAX_EFFECTS + 1));
            // Seed after the common ingress prelude: it normally processes
            // queued effects before this managed publication boundary.
            e.io.core.dispatch_now = 100.;
            assert!(e.handle_managed_socket(
                100.,
                &Event::WsMessage {
                    socket,
                    text: "ordinary-data",
                },
            ));
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert!(e.io.core.socket_failed);
            assert_eq!(e.venue.messages, 0);
            assert!(
                !e.venue
                    .events
                    .iter()
                    .any(|event| matches!(event, LifecycleEvent::Ping(_)))
            );
            assert_eq!(
                e.venue
                    .events
                    .iter()
                    .filter(|event| matches!(event, LifecycleEvent::Retired { .. }))
                    .count(),
                1
            );
            assert_eq!(actions.iter().filter(|action| matches!(action, Action::WsClose { socket: closed } if *closed == socket)).count(), 1);
            assert!(e.io.core.socket_effects.is_empty());
        }
    }

    #[test]
    fn retained_retirements_keep_both_empty_publication_capacity_checks() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        let retired = {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            let neighbor = cx.register_socket_feed(policy()).unwrap();
            cx.connect_socket(neighbor, "wss://neighbor.test").unwrap();
            cx.with_socket_driver(|driver, cx| {
                driver
                    .retire(cx, neighbor, RetireReason::Stopped, false)
                    .unwrap()
            })
        };
        // Retired effects survive exhaustion cleanup. The current feed is
        // stopped at the first boundary; the denied control observation must
        // still perform the second empty-publication capacity check.
        e.io.core
            .socket_effects
            .extend(std::iter::repeat_n(retired, MAX_EFFECTS + 1));
        e.io.core.dispatch_now = 100.;
        assert!(e.handle_managed_socket(
            100.,
            &Event::WsMessage {
                socket,
                text: "ordinary-data"
            }
        ));
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(e.io.core.socket_failed);
        assert_eq!(e.venue.messages, 0);
        assert_eq!(e.venue.control_calls.get(), 1);
        assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::Event(event)) if event.message == "Managed socket effect capacity exhausted")).count(), 2);
    }

    #[test]
    fn caught_fatal_socket_callback_stops_all_later_callbacks_and_output() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.venue.events.clear();
        e.venue.catch_fatal_on_ping = true;
        let epoch =
            e.io.core
                .socket_driver
                .state
                .epoch(e.venue.feed.unwrap())
                .unwrap();
        // One last unique HTTP identity is available; the following timer
        // allocation latches fatal without wrapping or aliasing a live socket.
        e.io.core.next_id =
            crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID);
        e.io.core
            .socket_effects
            .extend([Effect::Ping(epoch), Effect::Ping(epoch)]);
        let mut yielded = 0;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            e.drain_actions().for_each(|_| yielded += 1);
        }));
        assert!(result.is_err());
        assert!(e.failed());
        assert!(e.venue.caught_fatal);
        assert_eq!(
            e.venue.events.len(),
            1,
            "no callback may run after a swallowed fatal allocation"
        );
        assert_eq!(yielded, 0, "no partial callback output may escape");
    }

    #[test]
    fn socket_callback_cannot_borrow_http_delivery_and_restores_outer_scope() {
        use crate::request_context::{Attempt, Context, Operation, Owner};
        let (mut e, _) = engine();
        let epoch =
            e.io.core
                .socket_driver
                .state
                .epoch(e.venue.feed.unwrap())
                .unwrap();
        let context = Context::new(
            Owner::Command {
                id: 7,
                generation: 19,
            },
            3,
            Operation::Read,
            None,
            0.,
            5_000.,
            None,
        )
        .unwrap();
        e.io.core.http_delivery = Some(request_context_registry::Delivery {
            context,
            attempt: Attempt {
                request: RequestId(42),
                generation: 1,
            },
        });
        e.socket_native(1., LifecycleEvent::Ping(epoch)).unwrap();
        assert!(
            !e.venue.borrowed_http_delivery,
            "socket callback borrowed an HTTP delivery"
        );
        assert_eq!(
            e.io.core.http_delivery.as_ref().unwrap().attempt.request,
            RequestId(42)
        );
        e.io.core.socket_effects.push_back(Effect::Ping(epoch));
        e.finish_event_scope(2.);
        assert!(
            !e.venue.borrowed_http_delivery,
            "socket finalizer borrowed an HTTP delivery"
        );
        assert_eq!(
            e.io.core.http_delivery.as_ref().unwrap().attempt.request,
            RequestId(42)
        );
    }

    #[test]
    fn unexpected_close_counts_once_and_replacement_is_a_reconnect() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.handle(1., Event::WsClosed { socket, code: 1006 });
        let actions: Vec<_> = e.drain_actions().collect();
        let (timer, at) = actions
            .iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .unwrap();
        e.handle(1., Event::WsClosed { socket, code: 1006 });
        assert!(e.drain_actions().next().is_none());
        e.handle(at, Event::TimerFired { timer });
        assert_eq!(
            e.drain_actions()
                .filter(|a| matches!(a, Action::WsConnect { .. }))
                .count(),
            1
        );
        assert_eq!(e.io.core.health.reconnects, 1);
        assert_eq!(e.io.core.health.ws_closed, 1);
    }
    #[test]
    fn managed_frames_reset_auth_health_per_event_and_keep_common_epilogue() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        for now in [1., 2.] {
            e.handle(
                now,
                Event::WsMessage {
                    socket,
                    text: "refusal",
                },
            );
            e.drain_actions().for_each(drop);
        }
        assert_eq!(e.venue.messages, 2);
        assert_eq!(e.io.core.health.auth_failures, 2);
        assert_eq!(e.io.core.health.last_private_message_ms, Some(2.));
        assert!(!e.has_actions());
    }
    #[test]
    fn invalid_clock_does_not_consume_owned_timer() {
        let (mut e, _) = engine();
        let (&timer, _) = e.io.core.timers.iter().next().unwrap();
        e.handle(f64::NAN, Event::TimerFired { timer });
        assert!(e.io.core.timers.contains_key(&timer));
        e.handle(10_000., Event::TimerFired { timer });
        assert!(!e.io.core.timers.contains_key(&timer));
        assert_eq!(e.venue.timers, 0);
        assert!(
            e.drain_actions()
                .any(|a| matches!(a, Action::WsClose { .. }))
        );
    }
    #[test]
    fn stop_waiting_feed_cancels_retry_and_revokes_grant() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.handle(1., Event::WsClosed { socket, code: 1006 });
        let actions: Vec<_> = e.drain_actions().collect();
        let (timer, at) = actions
            .iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((*timer, *at_ms))
                } else {
                    None
                }
            })
            .unwrap();
        stop(&mut e);
        assert!(
            e.drain_actions()
                .any(|a| matches!(a,Action::ClearTimer { timer:t } if t==timer))
        );
        e.handle(at, Event::TimerFired { timer });
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::WsConnect { .. }))
        );
        let feed = e.venue.feed.unwrap();
        assert_eq!(
            e.io.core.socket_driver.preflight_begin(feed, at),
            Err(SocketError::NotPermitted)
        );
    }
    #[test]
    fn stop_granted_feed_blocks_already_queued_reconnect() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.handle(1., Event::WsClosed { socket, code: 1006 });
        let (timer, at) = e
            .drain_actions()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a {
                    Some((timer, at_ms))
                } else {
                    None
                }
            })
            .unwrap();
        let effects = e.socket_mutate(at, |d, cx| d.timer(cx, timer).unwrap());
        assert!(effects.iter().any(|e| matches!(e, Effect::Reconnect(_))));
        e.socket_enqueue(at, effects);
        stop(&mut e);
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::WsConnect { .. }))
        );
        assert!(
            e.io.core.actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::Event(LogEvent {
                    code: log_code::SUBSCRIPTION_REJECTED,
                    ..
                }))
            )) || e
                .io
                .core
                .socket_driver
                .preflight_begin(e.venue.feed.unwrap(), at)
                .is_err()
        );
    }
    #[test]
    fn overflow_keeps_already_revoked_cleanup_authority() {
        let (mut e, socket) = engine();
        let feed = e.venue.feed.unwrap();
        let retired = e.socket_mutate(0., |d, cx| {
            d.retire(cx, feed, RetireReason::Stopped, false).unwrap()
        });
        let epoch = match &retired {
            Effect::Retired { retirement, .. } => retirement.epoch,
            _ => unreachable!(),
        };
        e.io.core.socket_effects.push_back(retired);
        e.socket_enqueue(0., vec![Effect::Ping(epoch); MAX_EFFECTS]);
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(e.io.core.socket_failed);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
                .count(),
            1
        );
        assert_eq!(
            e.venue
                .events
                .iter()
                .filter(|e| matches!(e,LifecycleEvent::Retired{epoch:e,..}if e.socket()==socket))
                .count(),
            1
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsSend { .. }))
        );
    }
    #[test]
    fn refused_native_callback_logs_and_stops_feed() {
        let (mut e, socket) = engine();
        e.venue.refuse_ready = true;
        e.handle(0., Event::WsOpened { socket });
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::Event(LogEvent {
                code: log_code::SUBSCRIPTION_REJECTED,
                ..
            }))
        )));
        assert!(actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
        assert!(
            e.io.core
                .socket_driver
                .preflight_begin(e.venue.feed.unwrap(), 1.)
                .is_err()
        );
    }
    #[test]
    fn same_interest_repair_unsubscribes_before_subscribing_without_changing_intent() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        let feed = e.venue.feed.unwrap();
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            cx.set_socket_interests(
                feed,
                vec![Interest {
                    key: "book".into(),
                    wire_args: "native-book".into(),
                }],
            )
            .unwrap();
        }
        assert_eq!(
            e.drain_actions()
                .filter_map(|a| if let Action::WsSend { text, .. } = a {
                    Some(text)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            vec!["true:native-book"]
        );
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            cx.repair_socket_interests(feed, vec!["book".into()])
                .unwrap();
        }
        assert_eq!(
            e.drain_actions()
                .filter_map(|a| if let Action::WsSend { text, .. } = a {
                    Some(text)
                } else {
                    None
                })
                .collect::<Vec<_>>(),
            vec!["false:native-book", "true:native-book"]
        );
        assert_eq!(
            e.io.core.socket_driver.state.desired(feed).unwrap().len(),
            1
        );
    }
    #[test]
    fn auth_progress_invalid_deadlines_leave_watch_and_auth_bound_unchanged() {
        for now in [f64::NAN, f64::INFINITY, f64::MAX] {
            let (mut e, _) = engine();
            let mut supervisor = crate::socket::Supervisor::new(Config {
                auth_timeout_ms: Some(15_000.),
                ..Config::default()
            });
            {
                let mut cx = Cx {
                    now: 0.,
                    store: &mut e.store,
                    io: &mut e.io,
                };
                supervisor.connecting(&mut cx);
                supervisor.opened(&mut cx);
            }
            let actions = e.io.core.actions.len();
            let timers = e.io.core.timers.clone();
            supervisor.authentication_progress(&mut Cx {
                now,
                store: &mut e.store,
                io: &mut e.io,
            });
            assert_eq!(e.io.core.actions.len(), actions);
            assert_eq!(e.io.core.timers, timers);
            assert_eq!(
                supervisor.expired(15_000.),
                Some(crate::socket::Timeout::Auth)
            );
        }
    }

    #[test]
    fn auth_progress_overflow_cannot_replace_the_existing_watch() {
        let (mut e, _) = engine();
        let mut supervisor = crate::socket::Supervisor::new(Config {
            auth_timeout_ms: Some(f64::MAX),
            ..Config::default()
        });
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            supervisor.connecting(&mut cx);
            supervisor.opened(&mut cx);
        }
        let actions = e.io.core.actions.len();
        let timers = e.io.core.timers.clone();
        supervisor.authentication_progress(&mut Cx {
            now: f64::MAX / 2.,
            store: &mut e.store,
            io: &mut e.io,
        });
        assert_eq!(e.io.core.actions.len(), actions);
        assert_eq!(e.io.core.timers, timers);
        assert_eq!(
            supervisor.expired(60_000.),
            Some(crate::socket::Timeout::Idle)
        );
    }

    #[test]
    fn auth_progress_timer_exhaustion_is_fatal_before_replacing_the_old_bound() {
        let (mut e, _) = engine();
        let mut supervisor = crate::socket::Supervisor::new(Config {
            auth_timeout_ms: Some(15_000.),
            ..Config::default()
        });
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            supervisor.connecting(&mut cx);
            supervisor.opened(&mut cx);
        }
        let actions = e.io.core.actions.len();
        let timers = e.io.core.timers.clone();
        e.io.core.next_id =
            crate::resource_ids::ResourceIds::starting_at(crate::resource_ids::MAX_RESOURCE_ID + 1);
        let trapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            supervisor.authentication_progress(&mut Cx {
                now: 14_000.,
                store: &mut e.store,
                io: &mut e.io,
            });
        }));
        assert!(trapped.is_err());
        assert!(e.failed());
        assert_eq!(e.io.core.actions.len(), actions);
        assert_eq!(e.io.core.timers, timers);
        assert_eq!(
            supervisor.expired(15_000.),
            Some(crate::socket::Timeout::Auth)
        );
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                e.drain_actions().for_each(drop);
            }))
            .is_err()
        );
    }

    struct Step;
    impl crate::SignStep for Step {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &crate::DispatchContext<'_>,
        ) -> Result<crate::signing::SignRequest, crate::signing::SignError> {
            use crate::signing::{Algorithm, Context, KeyRef, Operation, PayloadPart, SignRequest};
            Ok(SignRequest {
                receipt: Default::default(),
                context: Context::default(),
                operations: vec![Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: KeyRef("ref:probe".into()),
                    payload: vec![PayloadPart::Bytes {
                        bytes: b"native-auth".to_vec(),
                    }],
                }],
            })
        }
        fn freshness_ms(&self) -> f64 {
            5_000.
        }
        fn apply(
            &mut self,
            _: &mut Action,
            _: &crate::signing::SignResult,
        ) -> Result<(), crate::signing::SignError> {
            Ok(())
        }
    }
    #[test]
    fn retired_parked_signer_and_ready_outbound_never_publish_or_resurrect() {
        for complete_before_retire in [false, true] {
            let (mut e, socket) = engine();
            e.handle(0., Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            {
                let mut cx = Cx {
                    now: 0.,
                    store: &mut e.store,
                    io: &mut e.io,
                };
                cx.send_signed(socket, "native-auth".into(), Box::new(Step));
            }
            let attempt = e
                .drain_actions()
                .find_map(|a| {
                    if let Action::Sign { attempt, .. } = a {
                        Some(attempt)
                    } else {
                        None
                    }
                })
                .unwrap();
            let result = || {
                Ok(crate::signing::SignResult::new(
                    vec![vec![0; 32]],
                    Default::default(),
                ))
            };
            if complete_before_retire {
                e.handle(
                    1.,
                    Event::Signed {
                        attempt,
                        result: result(),
                    },
                );
            }
            stop(&mut e);
            let actions: Vec<_> = e.drain_actions().collect();
            assert!(actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
            assert!(!actions.iter().any(|a| matches!(
                a,
                Action::Sign { .. } | Action::WsSend { .. } | Action::WsConnect { .. }
            )));
            e.handle(
                2.,
                Event::Signed {
                    attempt,
                    result: result(),
                },
            );
            assert!(e.drain_actions().next().is_none());
            assert_eq!(e.venue.receipts, 0);
        }
    }
    #[test]
    fn failed_invalidation_is_coded_and_cannot_authorize_retry() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        e.venue.refuse_retirement = true;
        e.handle(1., Event::WsClosed { socket, code: 1006 });
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::WsClose { .. }))
                .count(),
            1
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::Event(LogEvent {
                level: Level::Error,
                code: log_code::SUBSCRIPTION_REJECTED,
                ..
            }))
        )));
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::SetTimer { .. } | Action::WsConnect { .. }))
        );
        assert!(
            e.io.core
                .socket_driver
                .preflight_begin(e.venue.feed.unwrap(), 2.)
                .is_err()
        );
    }
    #[test]
    fn standalone_ready_one_shot_wakes_drain_without_spinning_after_release() {
        let (mut e, socket) = engine();
        e.handle(0., Event::WsOpened { socket });
        e.drain_actions().for_each(drop);
        assert!(!e.has_actions());
        let feed = e.venue.feed.unwrap();
        {
            let mut cx = Cx {
                now: 0.,
                store: &mut e.store,
                io: &mut e.io,
            };
            cx.send_after_socket_interests(feed, "native-follow-up".into())
                .unwrap();
        }
        assert!(e.has_actions());
        for invalid in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::MAX,
            -f64::MAX,
        ] {
            e.handle(invalid, Event::PollScaleChanged { scale: 1. });
            assert!(!e.drain_actions().any(|a| matches!(
                a,
                Action::WsSend { .. } | Action::WsConnect { .. } | Action::Sign { .. }
            )));
            assert!(!e.has_actions());
            assert!(e.drain_actions().next().is_none());
        }
        e.prepare_dispatch(0.);
        assert!(e.has_actions());
        let sent = e
            .drain_actions()
            .filter_map(|a| {
                if let Action::WsSend { text, .. } = a {
                    Some(text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(sent, vec!["native-follow-up"]);
        assert!(!e.has_actions());
    }
    #[test]
    fn snapshot_connect_preflights_exact_owned_resource_count() {
        for (timeout, ceiling, admitted) in
            [(20_000., 2, true), (1_000., 2, false), (1_000., 3, true)]
        {
            let mut store = Store::default();
            let mut io = Io::<()> {
                core: IoCore {
                    next_id: crate::resource_ids::ResourceIds::with_ceiling(ceiling),
                    managed_sockets_declared: true,
                    ..IoCore::default()
                },
                ..Default::default()
            };
            let mut cx = Cx {
                now: 0.,
                store: &mut store,
                io: &mut io,
            };
            let mut declared = policy();
            declared.auth = AuthMode::Required;
            declared.supervision.auth_timeout_ms = Some(timeout);
            let feed = cx.register_socket_feed_with_snapshot(declared).unwrap();
            let result = cx.connect_socket(feed, "wss://snapshot.test");
            if admitted {
                assert!(result.is_ok());
                assert!(!cx.io.core.next_id.available(1));
                assert!(cx.socket_epoch(feed).is_some());
            } else {
                assert_eq!(result, Err(SocketError::Capacity));
                assert!(cx.io.core.actions.is_empty());
                assert!(cx.io.core.next_id.available(ceiling));
                assert!(cx.socket_epoch(feed).is_none());
            }
        }
    }
    #[test]
    fn snapshot_invalid_policy_does_not_register_a_feed() {
        let mut store = Store::default();
        let mut io = Io::<()>::default();
        io.core.managed_sockets_declared = true;
        let mut cx = Cx {
            now: 0.,
            store: &mut store,
            io: &mut io,
        };
        assert_eq!(
            cx.register_socket_feed_with_snapshot(policy()),
            Err(SocketError::InvalidPolicy)
        );
        assert!(cx.io.core.actions.is_empty());
        assert_eq!(cx.register_socket_feed(policy()).unwrap().index(), 0);
    }
    #[test]
    fn snapshot_completion_capacity_preserves_proof_and_duplicate_is_inert() {
        for ceiling in [3, 4] {
            let mut store = Store::default();
            let mut io = Io::<()> {
                core: IoCore {
                    next_id: crate::resource_ids::ResourceIds::with_ceiling(ceiling),
                    managed_sockets_declared: true,
                    ..IoCore::default()
                },
                ..Default::default()
            };
            let mut cx = Cx {
                now: 0.,
                store: &mut store,
                io: &mut io,
            };
            let mut declared = policy();
            declared.auth = AuthMode::Required;
            declared.supervision.auth_timeout_ms = Some(20_000.);
            let feed = cx.register_socket_feed_with_snapshot(declared).unwrap();
            let epoch = cx.connect_socket(feed, "wss://snapshot.test").unwrap();
            cx.with_socket_driver(|d, cx| d.ingress(cx, epoch.socket(), Ingress::Open));
            cx.with_socket_driver(|d, cx| {
                d.control(
                    cx,
                    epoch,
                    crate::socket_lifecycle::Control::Auth(
                        crate::socket_lifecycle::AuthEvidence::Accepted,
                    ),
                    false,
                )
            });
            cx.io.core.socket_message = Some((epoch, 0.));
            if ceiling == 3 {
                let actions = cx.io.core.actions.len();
                assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Capacity));
                assert_eq!(cx.io.core.actions.len(), actions);
                assert_eq!(cx.io.core.socket_message, Some((epoch, 0.)));
                cx.io.core.next_id = crate::resource_ids::ResourceIds::starting_at(4);
            }
            assert_eq!(cx.complete_socket_snapshot(), Ok(()));
            cx.io.core.socket_message = Some((epoch, 0.));
            let actions = cx.io.core.actions.len();
            assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
            assert_eq!(cx.io.core.actions.len(), actions);
        }
    }
}

#[cfg(test)]
#[path = "socket_handoff_tests.rs"]
mod handoff_tests;

#[cfg(test)]
#[path = "socket_capability_tests.rs"]
mod capability_tests;

#[cfg(test)]
#[path = "socket_declaration_tests.rs"]
mod declaration_tests;

#[cfg(test)]
#[path = "socket_close_floor_tests.rs"]
mod close_floor_tests;

#[cfg(all(test, feature = "socket-native-timing"))]
#[path = "socket_native_timing_tests.rs"]
mod native_timing_tests;

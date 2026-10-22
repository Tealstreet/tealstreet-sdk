//! Engine-owned socket identity and transactional subscription state.
//!
//! This module is the state component of the managed socket driver. It is not
//! a venue helper: only the engine may issue epochs, consume transitions or
//! commit subscription release. The driver supplies the existing Supervisor's
//! expiry result; this module does not implement a second clock/backoff loop.
use crate::socket::{Config, Timeout};
use crate::{ErrorKind, SocketChannel, SocketId, VenueError};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_FEEDS: usize = 512;
pub const MAX_INTERESTS: usize = 8192;
pub const MAX_FRAMES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_INTEREST_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeedId(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Epoch {
    feed: FeedId,
    generation: u64,
    socket: SocketId,
}
impl Epoch {
    pub fn feed(self) -> FeedId {
        self.feed
    }
    pub fn socket(self) -> SocketId {
        self.socket
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SocketError {
    Capacity,
    InvalidPolicy,
    InvalidTime,
    InvalidInterest,
    InvalidCoverage,
    Stale,
    NotPermitted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthMode {
    None,
    Observe,
    Required,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AuthRecovery {
    Reconnect,
    RetrySameSocket { delay_ms: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficPolicy {
    PongOnly,
    AnyText,
    TextAfterFirstPong,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalidation {
    None,
    Required,
}

#[derive(Clone, Debug)]
pub struct FeedPolicy {
    pub role: SocketChannel,
    pub supervision: Config,
    pub auth: AuthMode,
    pub recovery: AuthRecovery,
    pub permanent_refusal_group: Option<u16>,
    pub traffic: TrafficPolicy,
    pub invalidation: Invalidation,
    pub subscription_interval_ms: f64,
}
impl FeedPolicy {
    fn validate(&self) -> Result<(), SocketError> {
        let c = self.supervision;
        let positive = |x: f64| x.is_finite() && x > 0.;
        if !positive(c.connect_timeout_ms)
            || !c.auth_timeout_ms.is_none_or(positive)
            || !positive(c.idle_timeout_ms)
            || !c.ping_interval_ms.is_none_or(positive)
            || !positive(c.backoff_base_ms)
            || !positive(c.backoff_cap_ms)
            || c.backoff_cap_ms < c.backoff_base_ms
            || !c.stable_ms.is_finite()
            || c.stable_ms < 0.
            || !self.subscription_interval_ms.is_finite()
            || self.subscription_interval_ms < 0.
            || matches!(self.recovery, AuthRecovery::RetrySameSocket { delay_ms } if !positive(delay_ms))
            || self.auth == AuthMode::None && c.auth_timeout_ms.is_some()
        {
            return Err(SocketError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum AuthEvidence {
    Accepted,
    Rejected(VenueError),
    Malformed,
}
#[derive(Clone, Debug)]
pub enum Control {
    Other,
    Pong,
    Auth(AuthEvidence),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetireReason {
    Closed,
    Timeout(Timeout),
    Authentication,
    Stopped,
}

#[derive(Clone, Debug)]
pub enum LifecycleEvent {
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
        epoch: Epoch,
        reason: RetireReason,
    },
    Ping(Epoch),
    RetryLogin(Epoch),
    Reconnect(FeedId),
    InterestsCommitted {
        feed: FeedId,
        proposal: u64,
    },
    InterestsRefused {
        feed: FeedId,
        proposal: u64,
        error: SocketError,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Interest {
    pub key: String,
    pub wire_args: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedBatch {
    pub text: String,
    pub covered_keys: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Initial,
    Connecting,
    Authenticating,
    Ready,
    Quarantined,
    Waiting,
    Granted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Retirement {
    pub epoch: Epoch,
    pub invalidation: Invalidation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ingress {
    Open,
    Message,
    Close,
    Connect,
    Send,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Admission {
    Legacy,
    Denied,
    Current(Epoch),
    Expired(Retirement, Timeout),
}
#[derive(Clone, Debug)]
pub(crate) enum AuthTransition {
    Ignored,
    Accepted {
        epoch: Epoch,
        ready: bool,
        reopened: bool,
    },
    Failed {
        epoch: Epoch,
        evidence: AuthEvidence,
        recovery: AuthRecovery,
    },
    Permanent {
        epoch: Epoch,
        group: u16,
        error: VenueError,
    },
}

#[derive(Clone, Debug)]
struct QueuedBatch {
    revision: u64,
    subscribe: bool,
    batch: EncodedBatch,
}
#[derive(Default)]
struct Interests {
    desired: BTreeMap<String, Interest>,
    released: BTreeMap<String, Interest>,
    revision: u64,
    queue: VecDeque<QueuedBatch>,
}
impl Interests {
    fn bytes(&self) -> usize {
        self.desired
            .values()
            .chain(self.released.values())
            .map(|i| i.key.len() + i.wire_args.len())
            .sum()
    }
    // Unsubscribes precede subscribes. Reserve both the current retained state
    // and the fully released desired state, so another feed cannot consume the
    // byte budget between staging and final frame release.
    fn budget_bytes(&self) -> usize {
        let desired: usize = self
            .desired
            .values()
            .map(|i| i.key.len() + i.wire_args.len())
            .sum();
        self.bytes().max(desired.saturating_mul(2))
    }
    fn queued_bytes(&self) -> usize {
        self.queue
            .iter()
            .map(|b| {
                b.batch.text.len() + b.batch.covered_keys.iter().map(String::len).sum::<usize>()
            })
            .sum()
    }
}
struct Feed {
    policy: FeedPolicy,
    generation: u64,
    epoch: Option<Epoch>,
    retiring: Option<Epoch>,
    phase: Phase,
    stopped: bool,
    authenticated: bool,
    ever_ready: bool,
    pong_seen: bool,
    interests: Interests,
}

/// Internal state of one engine. No venue can construct or mutate this registry.
#[derive(Default)]
pub(crate) struct Lifecycle {
    feeds: Vec<Feed>,
    external_queue_bytes: usize,
}
impl Lifecycle {
    pub(crate) fn reserve_external(&mut self, bytes: usize) -> Result<(), SocketError> {
        let total = self
            .feeds
            .iter()
            .try_fold(self.external_queue_bytes, |n, f| {
                n.checked_add(f.interests.queued_bytes())
            })
            .and_then(|n| n.checked_add(bytes))
            .ok_or(SocketError::Capacity)?;
        if total > MAX_QUEUED_BYTES {
            return Err(SocketError::Capacity);
        }
        self.external_queue_bytes += bytes;
        Ok(())
    }
    pub(crate) fn release_external(&mut self, bytes: usize) {
        self.external_queue_bytes = self
            .external_queue_bytes
            .checked_sub(bytes)
            .expect("owned deferred bytes");
    }
    pub(crate) fn enabled(&self) -> bool {
        !self.feeds.is_empty()
    }
    pub(crate) fn feeds(&self) -> Vec<FeedId> {
        (0..self.feeds.len()).map(FeedId).collect()
    }
    pub(crate) fn socket_epoch(&self, socket: SocketId) -> Option<Epoch> {
        self.feeds
            .iter()
            .find_map(|f| f.epoch.filter(|e| e.socket == socket))
    }
    pub(crate) fn has_pending_frames(&self) -> bool {
        self.feeds
            .iter()
            .any(|feed| !feed.interests.queue.is_empty())
    }
    pub(crate) fn has_batches(&self, feed: FeedId) -> bool {
        self.feeds
            .get(feed.0)
            .is_some_and(|f| !f.interests.queue.is_empty())
    }
    pub(crate) fn ready(&self, epoch: Epoch) -> bool {
        self.feeds
            .get(epoch.feed.0)
            .is_some_and(|f| f.epoch == Some(epoch) && f.phase == Phase::Ready)
    }
    pub(crate) fn stop_feed(&mut self, feed: FeedId) -> Option<Retirement> {
        let f = self.feeds.get_mut(feed.0)?;
        f.stopped = true;
        f.phase = Phase::Quarantined;
        f.authenticated = false;
        f.interests.queue.clear();
        f.interests.released.clear();
        self.retire(feed)
    }
    pub(crate) fn stop_all(&mut self) -> Vec<Retirement> {
        let feeds = self.feeds();
        for f in &mut self.feeds {
            f.stopped = true;
        }
        feeds
            .into_iter()
            .filter_map(|feed| self.retire(feed))
            .collect()
    }
    pub(crate) fn register(&mut self, policy: FeedPolicy) -> Result<FeedId, SocketError> {
        policy.validate()?;
        if policy.permanent_refusal_group.is_some_and(|group| {
            self.feeds
                .iter()
                .any(|f| f.stopped && f.policy.permanent_refusal_group == Some(group))
        }) {
            return Err(SocketError::NotPermitted);
        }
        if self.feeds.len() >= MAX_FEEDS {
            return Err(SocketError::Capacity);
        }
        let id = FeedId(self.feeds.len());
        self.feeds.push(Feed {
            policy,
            generation: 0,
            epoch: None,
            retiring: None,
            phase: Phase::Initial,
            stopped: false,
            authenticated: false,
            ever_ready: false,
            pong_seen: false,
            interests: Interests::default(),
        });
        Ok(id)
    }
    pub(crate) fn policy(&self, feed: FeedId) -> Result<&FeedPolicy, SocketError> {
        self.feeds
            .get(feed.0)
            .map(|f| &f.policy)
            .ok_or(SocketError::Stale)
    }
    pub(crate) fn begin(&mut self, feed: FeedId, socket: SocketId) -> Result<Epoch, SocketError> {
        self.can_begin(feed)?;
        if socket.0 == 0
            || self
                .feeds
                .iter()
                .any(|f| f.epoch.is_some_and(|e| e.socket == socket))
        {
            return Err(SocketError::NotPermitted);
        }
        let f = self.feeds.get_mut(feed.0).ok_or(SocketError::Stale)?;
        let generation = f.generation.checked_add(1).ok_or(SocketError::Capacity)?;
        let epoch = Epoch {
            feed,
            generation,
            socket,
        };
        f.generation = generation;
        f.epoch = Some(epoch);
        f.phase = Phase::Connecting;
        f.authenticated = false;
        f.pong_seen = false;
        f.interests.released.clear();
        f.interests.queue.clear();
        Ok(epoch)
    }
    pub(crate) fn can_begin(&self, feed: FeedId) -> Result<(), SocketError> {
        let f = self.feeds.get(feed.0).ok_or(SocketError::Stale)?;
        if f.stopped || !matches!(f.phase, Phase::Initial | Phase::Granted) {
            return Err(SocketError::NotPermitted);
        }
        f.generation.checked_add(1).ok_or(SocketError::Capacity)?;
        Ok(())
    }
    pub(crate) fn epoch(&self, feed: FeedId) -> Option<Epoch> {
        self.feeds.get(feed.0)?.epoch
    }
    pub(crate) fn current(&self, epoch: Epoch) -> bool {
        self.epoch(epoch.feed) == Some(epoch)
    }
    pub(crate) fn authenticated(&self, feed: FeedId) -> bool {
        self.feeds
            .get(feed.0)
            .is_some_and(|f| f.epoch.is_some() && f.authenticated)
    }
    pub(crate) fn awaiting_retry(&self, feed: FeedId) -> bool {
        self.feeds
            .get(feed.0)
            .is_some_and(|f| !f.stopped && f.phase == Phase::Waiting)
    }
    pub(crate) fn admit(
        &mut self,
        socket: SocketId,
        ingress: Ingress,
        expired: Option<Timeout>,
    ) -> Admission {
        if !self.enabled() {
            return Admission::Legacy;
        }
        let Some(index) = self
            .feeds
            .iter()
            .position(|f| f.epoch.is_some_and(|e| e.socket == socket))
        else {
            return Admission::Denied;
        };
        let f = &self.feeds[index];
        if matches!(ingress, Ingress::Open | Ingress::Connect) && f.phase != Phase::Connecting
            || matches!(ingress, Ingress::Message | Ingress::Send)
                && !matches!(f.phase, Phase::Authenticating | Phase::Ready)
        {
            return Admission::Denied;
        }
        if let Some(timeout) = expired {
            let retired = self.retire(FeedId(index)).expect("admitted epoch");
            return Admission::Expired(retired, timeout);
        }
        Admission::Current(f.epoch.expect("current epoch"))
    }
    pub(crate) fn opened(&mut self, epoch: Epoch) -> Result<Option<bool>, SocketError> {
        let f = self.feeds.get_mut(epoch.feed.0).ok_or(SocketError::Stale)?;
        if f.epoch != Some(epoch) || f.phase != Phase::Connecting {
            return Err(SocketError::Stale);
        }
        if f.policy.auth == AuthMode::Required {
            f.phase = Phase::Authenticating;
            Ok(None)
        } else {
            let reopened = f.ever_ready;
            f.ever_ready = true;
            f.phase = Phase::Ready;
            Ok(Some(reopened))
        }
    }
    pub(crate) fn auth(
        &mut self,
        epoch: Epoch,
        evidence: AuthEvidence,
        startup_ready: bool,
    ) -> AuthTransition {
        let Some(f) = self.feeds.get_mut(epoch.feed.0) else {
            return AuthTransition::Ignored;
        };
        if f.epoch != Some(epoch)
            || !matches!(f.phase, Phase::Authenticating | Phase::Ready)
            || f.policy.auth == AuthMode::None
        {
            return AuthTransition::Ignored;
        }
        if matches!(evidence, AuthEvidence::Accepted) {
            if f.authenticated {
                // Positive evidence must still cancel a coalesced native login
                // retry after an intervening rejection/malformed response.
                return AuthTransition::Accepted {
                    epoch,
                    ready: false,
                    reopened: false,
                };
            }
            f.authenticated = true;
            let ready = f.phase != Phase::Ready;
            let reopened = f.ever_ready;
            f.ever_ready = true;
            f.phase = Phase::Ready;
            return AuthTransition::Accepted {
                epoch,
                ready,
                reopened,
            };
        }
        if let AuthEvidence::Rejected(ref error) = evidence
            && !startup_ready
            && !f.authenticated
            && matches!(
                error.kind,
                Some(ErrorKind::AuthInvalid | ErrorKind::AuthPermission | ErrorKind::AuthExpired)
            )
            && let Some(group) = f.policy.permanent_refusal_group
        {
            return AuthTransition::Permanent {
                epoch,
                group,
                error: error.clone(),
            };
        }
        AuthTransition::Failed {
            epoch,
            evidence,
            recovery: f.policy.recovery,
        }
    }
    /// Called only after ingress admission. Malformed current text may be an
    /// idle-refresh proof when declared, but is never an auth proof.
    pub(crate) fn traffic(&mut self, epoch: Epoch, text: bool, pong: bool) -> bool {
        let Some(f) = self.feeds.get_mut(epoch.feed.0) else {
            return false;
        };
        if f.epoch != Some(epoch) {
            return false;
        }
        f.pong_seen |= pong;
        match f.policy.traffic {
            TrafficPolicy::PongOnly => pong,
            TrafficPolicy::AnyText => text,
            TrafficPolicy::TextAfterFirstPong => text && f.pong_seen,
        }
    }
    pub(crate) fn retire(&mut self, feed: FeedId) -> Option<Retirement> {
        let f = self.feeds.get_mut(feed.0)?;
        let epoch = f.epoch.take()?;
        f.retiring = Some(epoch);
        f.phase = Phase::Quarantined;
        f.authenticated = false;
        f.interests.queue.clear();
        f.interests.released.clear();
        Some(Retirement {
            epoch,
            invalidation: f.policy.invalidation,
        })
    }
    /// Terminal group stop: revoke every member's retry/start grant before ANY
    /// effect. Successful invalidation does not make this group reconnectable.
    pub(crate) fn retire_group(&mut self, group: u16) -> Vec<Retirement> {
        let feeds: Vec<_> = self
            .feeds
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                (f.policy.permanent_refusal_group == Some(group)).then_some(FeedId(i))
            })
            .collect();
        for feed in &feeds {
            let f = &mut self.feeds[feed.0];
            f.stopped = true;
            f.phase = Phase::Quarantined;
            f.authenticated = false;
            f.interests.queue.clear();
            f.interests.released.clear();
        }
        feeds
            .into_iter()
            .filter_map(|feed| self.retire(feed))
            .collect()
    }
    pub(crate) fn retired(&mut self, epoch: Epoch, acknowledged: bool) -> Result<(), SocketError> {
        let f = self.feeds.get_mut(epoch.feed.0).ok_or(SocketError::Stale)?;
        if f.retiring != Some(epoch) {
            return Err(SocketError::Stale);
        }
        f.retiring = None;
        if acknowledged && !f.stopped {
            f.phase = Phase::Waiting;
        }
        Ok(())
    }
    pub(crate) fn grant_retry(&mut self, feed: FeedId) -> Result<(), SocketError> {
        let f = self.feeds.get_mut(feed.0).ok_or(SocketError::Stale)?;
        if f.stopped || f.phase != Phase::Waiting {
            return Err(SocketError::NotPermitted);
        }
        f.phase = Phase::Granted;
        Ok(())
    }
    pub(crate) fn desired(&self, feed: FeedId) -> Result<Vec<Interest>, SocketError> {
        Ok(self
            .feeds
            .get(feed.0)
            .ok_or(SocketError::Stale)?
            .interests
            .desired
            .values()
            .cloned()
            .collect())
    }
    /// Pure transactional preparation. The engine invokes its native encoder
    /// after restoring registry borrows, then commits only this complete result.
    pub(crate) fn replace_interests(
        &mut self,
        feed: FeedId,
        interests: Vec<Interest>,
        encode: impl FnMut(
            bool,
            &[Interest],
            &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError>,
    ) -> Result<u64, SocketError> {
        self.replace_with_repair(feed, interests, &[], encode)
    }
    pub(crate) fn repair_interests(
        &mut self,
        feed: FeedId,
        keys: &[String],
        encode: impl FnMut(
            bool,
            &[Interest],
            &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError>,
    ) -> Result<u64, SocketError> {
        if keys.len() > MAX_INTERESTS {
            return Err(SocketError::Capacity);
        }
        self.replace_with_repair(feed, self.desired(feed)?, keys, encode)
    }
    fn replace_with_repair(
        &mut self,
        feed: FeedId,
        interests: Vec<Interest>,
        repair: &[String],
        mut encode: impl FnMut(
            bool,
            &[Interest],
            &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError>,
    ) -> Result<u64, SocketError> {
        let f = self.feeds.get(feed.0).ok_or(SocketError::Stale)?;
        if f.stopped {
            return Err(SocketError::NotPermitted);
        }
        if interests.len() > MAX_INTERESTS {
            return Err(SocketError::Capacity);
        }
        // Bound all retained input and eventual released copies before cloning
        // argument state or invoking a native encoder.
        let mut unique = BTreeSet::new();
        let mut desired_bytes = 0usize;
        for item in &interests {
            if item.key.is_empty()
                || item.key.len() > 1024
                || item.wire_args.len() > MAX_FRAME_BYTES
                || !unique.insert(item.key.as_str())
            {
                return Err(SocketError::InvalidInterest);
            }
            desired_bytes = desired_bytes
                .checked_add(item.key.len())
                .and_then(|v| v.checked_add(item.wire_args.len()))
                .ok_or(SocketError::Capacity)?;
            if desired_bytes > MAX_INTEREST_BYTES / 2 {
                return Err(SocketError::Capacity);
            }
        }
        let old_released: usize = f
            .interests
            .released
            .values()
            .map(|i| i.key.len() + i.wire_args.len())
            .sum();
        let reserved = desired_bytes
            .checked_add(old_released)
            .ok_or(SocketError::Capacity)?
            .max(desired_bytes.checked_mul(2).ok_or(SocketError::Capacity)?);
        let other_interest = self
            .feeds
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != feed.0)
            .try_fold(0usize, |sum, (_, f)| {
                sum.checked_add(f.interests.budget_bytes())
            })
            .ok_or(SocketError::Capacity)?;
        if other_interest
            .checked_add(reserved)
            .ok_or(SocketError::Capacity)?
            > MAX_INTEREST_BYTES
        {
            return Err(SocketError::Capacity);
        }
        let other_queue = self
            .feeds
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != feed.0)
            .try_fold(0usize, |sum, (_, f)| {
                sum.checked_add(f.interests.queued_bytes())
            })
            .ok_or(SocketError::Capacity)?;
        let available_queue = MAX_QUEUED_BYTES
            .checked_sub(other_queue)
            .and_then(|n| n.checked_sub(self.external_queue_bytes))
            .ok_or(SocketError::Capacity)?;
        let revision = f
            .interests
            .revision
            .checked_add(1)
            .ok_or(SocketError::Capacity)?;
        let desired: BTreeMap<_, _> = interests.into_iter().map(|i| (i.key.clone(), i)).collect();
        let forced: BTreeSet<_> = repair.iter().collect();
        if forced.len() != repair.len() || forced.iter().any(|k| !desired.contains_key(*k)) {
            return Err(SocketError::InvalidInterest);
        }
        let removed: Vec<_> = f
            .interests
            .released
            .values()
            .filter(|i| desired.get(&i.key) != Some(i) || forced.contains(&i.key))
            .cloned()
            .collect();
        let added: Vec<_> = desired
            .values()
            .filter(|i| f.interests.released.get(&i.key) != Some(i) || forced.contains(&i.key))
            .cloned()
            .collect();
        let mut queue = VecDeque::new();
        let mut queued_bytes = 0usize;
        for (subscribe, items) in [(false, removed), (true, added)] {
            if items.is_empty() {
                continue;
            }
            let expected: BTreeSet<_> = items.iter().map(|i| i.key.as_str()).collect();
            let mut covered = BTreeSet::new();
            let mut failure = None;
            let result = {
                let mut emit = |batch: EncodedBatch| {
                    if let Some(error) = failure {
                        return Err(error);
                    }
                    let result = (|| {
                        if batch.text.is_empty()
                            || batch.text.len() > MAX_FRAME_BYTES
                            || batch.covered_keys.is_empty()
                            || batch.covered_keys.iter().any(|key| {
                                !expected.contains(key.as_str()) || !covered.insert(key.clone())
                            })
                        {
                            return Err(SocketError::InvalidCoverage);
                        }
                        let bytes = batch
                            .covered_keys
                            .iter()
                            .try_fold(batch.text.len(), |sum, key| sum.checked_add(key.len()))
                            .ok_or(SocketError::Capacity)?;
                        let next = queued_bytes
                            .checked_add(bytes)
                            .ok_or(SocketError::Capacity)?;
                        if queue.len() >= MAX_FRAMES || next > available_queue {
                            return Err(SocketError::Capacity);
                        }
                        queued_bytes = next;
                        queue.push_back(QueuedBatch {
                            revision,
                            subscribe,
                            batch,
                        });
                        Ok(())
                    })();
                    if let Err(error) = result {
                        failure = Some(error);
                    }
                    result
                };
                encode(subscribe, &items, &mut emit)
            };
            result?;
            if let Some(error) = failure {
                return Err(error);
            }
            if covered.iter().map(String::as_str).collect::<BTreeSet<_>>() != expected {
                return Err(SocketError::InvalidCoverage);
            }
        }
        self.feeds[feed.0].interests = Interests {
            desired,
            released: f.interests.released.clone(),
            revision,
            queue,
        };
        Ok(revision)
    }
    /// Only the engine's final action handoff calls this, after all expiry and
    /// signer barriers are checked. Staging a frame is NOT release progress.
    pub(crate) fn release_batch(&mut self, epoch: Epoch, signer_barrier: bool) -> Option<String> {
        let f = self.feeds.get_mut(epoch.feed.0)?;
        if f.epoch != Some(epoch) || f.phase != Phase::Ready || signer_barrier {
            return None;
        }
        let queued = f.interests.queue.pop_front()?;
        if queued.revision != f.interests.revision {
            return None;
        }
        for key in &queued.batch.covered_keys {
            if queued.subscribe {
                f.interests
                    .released
                    .insert(key.clone(), f.interests.desired.get(key)?.clone());
            } else {
                f.interests.released.remove(key);
            }
        }
        Some(queued.batch.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(auth: AuthMode) -> FeedPolicy {
        FeedPolicy {
            role: SocketChannel::Private,
            supervision: Config {
                auth_timeout_ms: (auth == AuthMode::Required).then_some(10_000.),
                ..Config::default()
            },
            auth,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::PongOnly,
            invalidation: Invalidation::Required,
            subscription_interval_ms: 0.,
        }
    }
    fn started(auth: AuthMode) -> (Lifecycle, FeedId, Epoch) {
        let mut l = Lifecycle::default();
        let feed = l.register(policy(auth)).unwrap();
        let epoch = l.begin(feed, SocketId(1)).unwrap();
        l.opened(epoch).unwrap();
        (l, feed, epoch)
    }
    fn interest(key: &str) -> Interest {
        Interest {
            key: key.into(),
            wire_args: key.into(),
        }
    }
    fn encode(
        subscribe: bool,
        values: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for i in values {
            emit(EncodedBatch {
                text: format!("{}:{}", if subscribe { "+" } else { "-" }, i.wire_args),
                covered_keys: vec![i.key.clone()],
            })?;
        }
        Ok(())
    }
    #[test]
    fn legacy_and_unknown_managed_ingress_are_distinct() {
        let mut l = Lifecycle::default();
        assert_eq!(
            l.admit(SocketId(3), Ingress::Message, None),
            Admission::Legacy
        );
        let f = l.register(policy(AuthMode::None)).unwrap();
        assert!(matches!(l.policy(f).unwrap().role, SocketChannel::Private));
        assert_eq!(l.admit(SocketId(3), Ingress::Open, None), Admission::Denied);
        assert_eq!(l.begin(f, SocketId(0)), Err(SocketError::NotPermitted));
        let e = l.begin(f, SocketId(3)).unwrap();
        assert_eq!(e.feed(), f);
        assert_eq!(e.socket(), SocketId(3));
        assert_eq!(
            l.admit(SocketId(3), Ingress::Open, None),
            Admission::Current(e)
        );
        assert_eq!(
            l.admit(SocketId(3), Ingress::Message, None),
            Admission::Denied
        );
        assert_eq!(l.opened(e), Ok(Some(false)));
        assert_eq!(l.admit(SocketId(3), Ingress::Open, None), Admission::Denied);
        assert_eq!(
            l.admit(SocketId(3), Ingress::Close, None),
            Admission::Current(e)
        );
    }
    #[test]
    fn accepted_auth_grants_readiness_once_per_epoch() {
        let (mut l, f, e) = started(AuthMode::Required);
        assert!(!l.authenticated(f));
        assert!(
            matches!(l.auth(e, AuthEvidence::Accepted, false), AuthTransition::Accepted { epoch, ready: true, reopened: false } if epoch == e)
        );
        assert!(l.authenticated(f));
        assert!(matches!(
            l.auth(e, AuthEvidence::Accepted, false),
            AuthTransition::Accepted {
                ready: false,
                reopened: false,
                ..
            }
        ));
        l.retire(f).unwrap();
        l.retired(e, true).unwrap();
        l.grant_retry(f).unwrap();
        let replacement = l.begin(f, SocketId(2)).unwrap();
        l.opened(replacement).unwrap();
        assert!(matches!(
            l.auth(e, AuthEvidence::Accepted, false),
            AuthTransition::Ignored
        ));
        assert!(matches!(
            l.auth(replacement, AuthEvidence::Accepted, false),
            AuthTransition::Accepted {
                ready: true,
                reopened: true,
                ..
            }
        ));
    }
    #[test]
    fn observe_only_auth_does_not_gate_push_or_repeat_ready() {
        let (mut l, _, e) = started(AuthMode::Observe);
        assert!(matches!(
            l.admit(e.socket, Ingress::Message, None),
            Admission::Current(_)
        ));
        assert!(matches!(
            l.auth(e, AuthEvidence::Accepted, false),
            AuthTransition::Accepted { ready: false, .. }
        ));
        let (mut l, _, e) = started(AuthMode::None);
        assert!(matches!(
            l.auth(e, AuthEvidence::Accepted, false),
            AuthTransition::Ignored
        ));
    }
    #[test]
    fn malformed_and_refused_auth_keep_declared_recovery_and_typed_error() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::Observe);
        p.recovery = AuthRecovery::RetrySameSocket { delay_ms: 2000. };
        let f = l.register(p).unwrap();
        let e = l.begin(f, SocketId(7)).unwrap();
        l.opened(e).unwrap();
        assert!(
            matches!(l.auth(e, AuthEvidence::Malformed, false), AuthTransition::Failed { epoch, evidence: AuthEvidence::Malformed, recovery: AuthRecovery::RetrySameSocket { delay_ms: 2000. } } if epoch == e)
        );
        let error = VenueError::new("native refusal")
            .kind(ErrorKind::AuthInvalid)
            .venue_code("60005");
        assert!(
            matches!(l.auth(e, AuthEvidence::Rejected(error.clone()), false), AuthTransition::Failed { evidence: AuthEvidence::Rejected(found), .. } if found == error)
        );
        let control = Control::Auth(AuthEvidence::Malformed);
        assert!(matches!(control, Control::Auth(AuthEvidence::Malformed)));
        assert!(matches!(Control::Pong, Control::Pong));
        assert!(matches!(Control::Other, Control::Other));
    }
    #[test]
    fn permanent_group_refusal_depends_on_native_startup_and_prior_auth() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::Required);
        p.permanent_refusal_group = Some(5);
        let f = l.register(p).unwrap();
        let e = l.begin(f, SocketId(7)).unwrap();
        l.opened(e).unwrap();
        let refusal =
            || AuthEvidence::Rejected(VenueError::new("refused").kind(ErrorKind::AuthInvalid));
        assert!(
            matches!(l.auth(e, refusal(), false), AuthTransition::Permanent { epoch, group: 5, error } if epoch == e && error.kind == Some(ErrorKind::AuthInvalid))
        );
        assert!(matches!(
            l.auth(e, refusal(), true),
            AuthTransition::Failed { .. }
        ));
        l.auth(e, AuthEvidence::Accepted, false);
        assert!(matches!(
            l.auth(e, refusal(), false),
            AuthTransition::Failed { .. }
        ));
    }
    #[test]
    fn retirement_commits_all_group_epochs_before_any_effect() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::Required);
        p.permanent_refusal_group = Some(2);
        let a = l.register(p.clone()).unwrap();
        let b = l.register(p).unwrap();
        let ea = l.begin(a, SocketId(1)).unwrap();
        let eb = l.begin(b, SocketId(2)).unwrap();
        let effects = l.retire_group(2);
        assert_eq!(effects.len(), 2);
        for effect in effects {
            assert_eq!(effect.invalidation, Invalidation::Required);
            assert!(!l.current(ea) && !l.current(eb));
            assert_eq!(
                l.begin(effect.epoch.feed, SocketId(3)),
                Err(SocketError::NotPermitted)
            );
            l.retired(effect.epoch, true).unwrap();
        }
    }
    #[test]
    fn failed_or_duplicate_retirement_ack_cannot_grant_replacement() {
        let (mut l, f, e) = started(AuthMode::None);
        assert!(l.retire(f).is_some());
        assert!(l.retire(f).is_none());
        assert_eq!(l.admit(e.socket, Ingress::Message, None), Admission::Denied);
        assert_eq!(l.grant_retry(f), Err(SocketError::NotPermitted));
        l.retired(e, false).unwrap();
        assert_eq!(l.grant_retry(f), Err(SocketError::NotPermitted));
        assert_eq!(l.retired(e, true), Err(SocketError::Stale));
    }
    #[test]
    fn expired_ingress_retires_before_auth_or_traffic() {
        let (mut l, f, e) = started(AuthMode::Required);
        assert_eq!(
            l.admit(e.socket, Ingress::Message, Some(Timeout::Auth)),
            Admission::Expired(
                Retirement {
                    epoch: e,
                    invalidation: Invalidation::Required
                },
                Timeout::Auth
            )
        );
        assert!(!l.traffic(e, true, true));
        assert!(matches!(
            l.auth(e, AuthEvidence::Accepted, false),
            AuthTransition::Ignored
        ));
        assert!(!l.authenticated(f));
    }
    #[test]
    fn native_idle_activation_is_distinct_from_authentication() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::Observe);
        p.traffic = TrafficPolicy::TextAfterFirstPong;
        let f = l.register(p).unwrap();
        let e = l.begin(f, SocketId(1)).unwrap();
        l.opened(e).unwrap();
        assert!(!l.traffic(e, true, false));
        assert!(l.traffic(e, true, true));
        assert!(l.traffic(e, true, false), "includes malformed current text");
        assert!(!l.traffic(e, false, false));
        assert!(!l.authenticated(f));
        let (mut l, _, e) = started(AuthMode::None);
        assert!(!l.traffic(e, true, false));
        assert!(l.traffic(e, true, true));
        let mut p = policy(AuthMode::None);
        p.traffic = TrafficPolicy::AnyText;
        let f = l.register(p).unwrap();
        let e = l.begin(f, SocketId(2)).unwrap();
        l.opened(e).unwrap();
        assert!(l.traffic(e, true, false));
    }
    #[test]
    fn partially_released_update_retains_unchanged_unsent_interest() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
        l.replace_interests(f, vec![interest("B"), interest("C")], encode)
            .unwrap();
        let frames: Vec<_> = std::iter::from_fn(|| l.release_batch(e, false)).collect();
        assert_eq!(frames, ["-:A", "+:B", "+:C"]);
        assert_eq!(l.desired(f).unwrap(), [interest("B"), interest("C")]);
    }
    #[test]
    fn same_key_argument_replacement_tracks_only_actual_released_coverage() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        l.release_batch(e, false).unwrap();
        let changed = Interest {
            key: "A".into(),
            wire_args: "new-A".into(),
        };
        l.replace_interests(f, vec![changed, interest("B")], encode)
            .unwrap();
        let frames: Vec<_> = std::iter::from_fn(|| l.release_batch(e, false)).collect();
        assert_eq!(frames, ["-:A", "+:new-A", "+:B"]);
    }
    #[test]
    fn signer_barrier_retains_frames_and_unreleased_progress() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        assert_eq!(l.release_batch(e, true), None);
        l.replace_interests(f, vec![interest("B")], encode).unwrap();
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:B"));
        l.retire(f).unwrap();
        assert_eq!(l.release_batch(e, false), None);
    }
    #[test]
    fn refusal_and_invalid_batch_coverage_leave_transaction_unchanged() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        let revision = l.feeds[f.0].interests.revision;
        assert_eq!(
            l.replace_interests(f, vec![interest("B")], |_, _, _| Err(SocketError::Capacity)),
            Err(SocketError::Capacity)
        );
        for keys in [vec![], vec!["B".into(), "B".into()], vec!["wrong".into()]] {
            assert_eq!(
                l.replace_interests(f, vec![interest("B")], |_, _, emit| emit(EncodedBatch {
                    text: "+B".into(),
                    covered_keys: keys.clone()
                })),
                Err(SocketError::InvalidCoverage)
            );
        }
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        assert_eq!(l.desired(f).unwrap(), [interest("A")]);
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
    }
    #[test]
    fn bounded_interest_and_queue_refusals_preserve_old_state() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        assert_eq!(
            l.replace_interests(f, vec![interest("B"), interest("B")], encode),
            Err(SocketError::InvalidInterest)
        );
        let excessive = (0..129)
            .map(|n| Interest {
                key: n.to_string(),
                wire_args: "x".repeat(MAX_FRAME_BYTES),
            })
            .collect();
        assert_eq!(
            l.replace_interests(f, excessive, |_, items, emit| emit(EncodedBatch {
                text: "bounded".into(),
                covered_keys: items.iter().map(|i| i.key.clone()).collect()
            })),
            Err(SocketError::Capacity)
        );
        let values = (0..65).map(|n| interest(&n.to_string())).collect();
        assert_eq!(
            l.replace_interests(f, values, |_, items, emit| {
                for i in items {
                    emit(EncodedBatch {
                        text: "x".repeat(MAX_FRAME_BYTES),
                        covered_keys: vec![i.key.clone()],
                    })?;
                }
                Ok(())
            }),
            Err(SocketError::Capacity)
        );
        assert_eq!(l.desired(f).unwrap(), [interest("A")]);
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
    }
    #[test]
    fn exhausted_epoch_or_interest_revision_does_not_partially_mutate() {
        let mut l = Lifecycle::default();
        let f = l.register(policy(AuthMode::None)).unwrap();
        l.feeds[f.0].generation = u64::MAX;
        assert_eq!(l.begin(f, SocketId(1)), Err(SocketError::Capacity));
        assert_eq!(l.epoch(f), None);
        l.feeds[f.0].generation = 0;
        let e = l.begin(f, SocketId(1)).unwrap();
        l.opened(e).unwrap();
        l.feeds[f.0].interests.revision = u64::MAX;
        assert_eq!(
            l.replace_interests(f, vec![interest("A")], encode),
            Err(SocketError::Capacity)
        );
        assert!(l.desired(f).unwrap().is_empty());
        assert_eq!(l.release_batch(e, false), None);
    }
    #[test]
    fn invalid_policy_and_feed_capacity_are_refused_before_registration() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::None);
        p.subscription_interval_ms = f64::NAN;
        assert_eq!(l.register(p), Err(SocketError::InvalidPolicy));
        assert!(!l.enabled());
        for _ in 0..MAX_FEEDS {
            let mut p = policy(AuthMode::None);
            p.invalidation = Invalidation::None;
            l.register(p).unwrap();
        }
        assert_eq!(
            l.register(policy(AuthMode::None)),
            Err(SocketError::Capacity)
        );
    }
    #[test]
    fn terminal_group_stop_revokes_waiting_granted_initial_and_acknowledged_members() {
        for already_granted in [false, true] {
            let mut l = Lifecycle::default();
            let mut p = policy(AuthMode::Required);
            p.permanent_refusal_group = Some(9);
            let a = l.register(p.clone()).unwrap();
            let b = l.register(p.clone()).unwrap();
            let initial = l.register(p.clone()).unwrap();
            l.begin(a, SocketId(1)).unwrap();
            let eb = l.begin(b, SocketId(2)).unwrap();
            l.retire(b).unwrap();
            l.retired(eb, true).unwrap();
            if already_granted {
                l.grant_retry(b).unwrap();
            }
            let effects = l.retire_group(9);
            for effect in effects {
                l.retired(effect.epoch, true).unwrap();
            }
            for f in [a, b, initial] {
                assert_eq!(l.grant_retry(f), Err(SocketError::NotPermitted));
                assert_eq!(l.begin(f, SocketId(3)), Err(SocketError::NotPermitted));
            }
            assert_eq!(l.register(p), Err(SocketError::NotPermitted));
        }
    }
    #[test]
    fn aggregate_interest_refusal_happens_before_encoder_work() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        let revision = l.feeds[f.0].interests.revision;
        let values = (0..129)
            .map(|i| Interest {
                key: i.to_string(),
                wire_args: "x".repeat(MAX_FRAME_BYTES),
            })
            .collect();
        let mut calls = 0;
        let result = l.replace_interests(f, values, |op, items, emit| {
            calls += 1;
            encode(op, items, emit)
        });
        assert_eq!(result, Err(SocketError::Capacity));
        assert_eq!(calls, 0, "reject aggregate bounds before encoding");
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
    }

    #[test]
    fn positive_auth_after_same_socket_failure_remains_visible_for_retry_cleanup() {
        for failure in [
            AuthEvidence::Malformed,
            AuthEvidence::Rejected(VenueError::new("refused")),
        ] {
            let (mut l, _, e) = started(AuthMode::Observe);
            l.auth(e, AuthEvidence::Accepted, false);
            l.auth(e, failure, false);
            assert!(
                matches!(
                    l.auth(e, AuthEvidence::Accepted, false),
                    AuthTransition::Accepted { ready: false, .. }
                ),
                "positive duplicate still cancels retry, never replays"
            );
        }
    }
    #[test]
    fn bounded_emitter_stops_queue_work_and_cannot_ignore_admission_errors() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        let mut emitted = 0;
        let result = l.replace_interests(
            f,
            (0..65).map(|i| interest(&i.to_string())).collect(),
            |_, items, emit| {
                for i in items {
                    emitted += 1;
                    emit(EncodedBatch {
                        text: "x".repeat(MAX_FRAME_BYTES),
                        covered_keys: vec![i.key.clone()],
                    })?;
                }
                Ok(())
            },
        );
        assert_eq!(result, Err(SocketError::Capacity));
        assert!(emitted < 65, "bounded output stops the codec incrementally");
        let result = l.replace_interests(f, vec![interest("B")], |_, items, emit| {
            let _ = emit(EncodedBatch {
                text: String::new(),
                covered_keys: vec!["wrong".into()],
            });
            let _ = encode(true, items, emit);
            Ok(())
        });
        assert_eq!(result, Err(SocketError::InvalidCoverage));
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
    }
    #[test]
    fn reserved_released_copies_bound_aggregate_feeds_before_encoding() {
        let (mut l, first, e) = started(AuthMode::None);
        let second = l.register(policy(AuthMode::None)).unwrap();
        let items = (0..61)
            .map(|i| Interest {
                key: i.to_string(),
                wire_args: "x".repeat(MAX_FRAME_BYTES),
            })
            .collect();
        l.replace_interests(first, items, |_, items, emit| {
            emit(EncodedBatch {
                text: "batch".into(),
                covered_keys: items.iter().map(|i| i.key.clone()).collect(),
            })
        })
        .unwrap();
        let mut calls = 0;
        let items = (0..4)
            .map(|i| Interest {
                key: i.to_string(),
                wire_args: "x".repeat(MAX_FRAME_BYTES),
            })
            .collect();
        assert_eq!(
            l.replace_interests(second, items, |_, _, _| {
                calls += 1;
                Ok(())
            }),
            Err(SocketError::Capacity)
        );
        assert_eq!(calls, 0);
        assert_eq!(l.release_batch(e, false).as_deref(), Some("batch"));
        assert!(l.feeds.iter().map(|f| f.interests.bytes()).sum::<usize>() <= MAX_INTEREST_BYTES);
    }
    #[test]
    fn reconnect_retains_desired_interests_and_replays_from_empty_released_state() {
        let (mut l, f, old) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        l.release_batch(old, false).unwrap();
        l.retire(f).unwrap();
        l.retired(old, true).unwrap();
        l.grant_retry(f).unwrap();
        let current = l.begin(f, SocketId(2)).unwrap();
        assert_eq!(l.opened(current), Ok(Some(true)));
        let desired = l.desired(f).unwrap();
        l.replace_interests(f, desired, encode).unwrap();
        assert_eq!(l.release_batch(old, false), None);
        assert_eq!(l.release_batch(current, false).as_deref(), Some("+:A"));
        assert_eq!(l.release_batch(current, false).as_deref(), Some("+:B"));
        assert_eq!(l.release_batch(current, false), None);
    }
}

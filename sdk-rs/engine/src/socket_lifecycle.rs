//! Engine-owned socket identity and transactional subscription state.
//!
//! This module is the state component of the managed socket driver. It is not
//! a venue helper: only the engine may issue epochs, consume transitions or
//! commit subscription release. The driver supplies the existing Supervisor's
//! expiry result; this module does not implement a second clock/backoff loop.
use crate::socket::{Config, Timeout};
use crate::{ErrorKind, SocketChannel, SocketId, VenueError};
use sorted::{Map, Set};
use std::collections::VecDeque;
use std::sync::Arc;

pub const MAX_FEEDS: usize = 512;
pub const MAX_INTERESTS: usize = 8192;
pub const MAX_FRAMES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = 65_536;
pub const MAX_QUEUED_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_INTEREST_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FeedId(usize);
impl FeedId {
    /// Feeds and their driver runtimes are append-only for one engine lifetime.
    pub(crate) fn index(self) -> usize {
        self.0
    }
}

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
/// Whether this feed carries native control messages. No control decoding is
/// needed only when every text frame is ordinary traffic without auth evidence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlPolicy {
    #[default]
    Native,
    None,
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
    pub controls: ControlPolicy,
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
            || self.controls == ControlPolicy::None
                && (self.auth != AuthMode::None || self.traffic != TrafficPolicy::AnyText)
        {
            return Err(SocketError::InvalidPolicy);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub enum AuthEvidence {
    Accepted,
    /// Accepted intermediate proof; renews authentication without readiness.
    Progress,
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
    pub key: Arc<str>,
    pub wire_args: Arc<str>,
}
impl Interest {
    /// Use one immutable allocation when the opaque native argument is its key.
    pub fn from_wire_args(args: impl Into<Arc<str>>) -> Self {
        let args = args.into();
        Self {
            key: args.clone(),
            wire_args: args,
        }
    }
    fn canonicalize(mut self) -> Self {
        if self.key == self.wire_args {
            self.wire_args = self.key.clone();
        }
        self
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncodedBatch {
    pub text: String,
    pub covered_keys: Vec<Arc<str>>,
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
    Progress(Epoch),
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
    desired: Map<Arc<str>, Arc<Interest>>,
    released: Map<Arc<str>, Arc<Interest>>,
    revision: u64,
    queue: VecDeque<QueuedBatch>,
}
impl Interests {
    fn clear_queue(&mut self, pending_frames: &mut usize) {
        *pending_frames = pending_frames
            .checked_sub(self.queue.len())
            .expect("owned queued subscription frames");
        self.queue.clear();
    }
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
                b.batch.text.len()
                    + b.batch
                        .covered_keys
                        .iter()
                        .map(|key| key.len())
                        .sum::<usize>()
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

/// A native subscription encoder, called through one shared vtable.
type Encode<'a> = dyn FnMut(
        bool,
        &[Interest],
        &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError>
    + 'a;

// A canonical replay retains immutable rows without recreating their strings.
// Both sources still pass through the same validation and transaction below.
enum InterestInput {
    Proposed(Vec<Interest>),
    Retained(Vec<Arc<Interest>>),
}
impl InterestInput {
    fn len(&self) -> usize {
        match self {
            Self::Proposed(rows) => rows.len(),
            Self::Retained(rows) => rows.len(),
        }
    }
    fn rows(&self) -> impl Iterator<Item = &Interest> {
        let (owned, shared): (&[Interest], &[Arc<Interest>]) = match self {
            Self::Proposed(rows) => (rows, &[]),
            Self::Retained(rows) => (&[], rows),
        };
        owned.iter().chain(shared.iter().map(Arc::as_ref))
    }
}

/// Live physical epochs by socket id, sorted for binary search. At most one
/// per registered feed, so a vector replaces a tree's rebalancing code.
#[derive(Default)]
struct SocketIndex(Vec<(u64, Epoch)>);

impl SocketIndex {
    fn find(&self, socket: u64) -> Result<usize, usize> {
        self.0.binary_search_by_key(&socket, |(id, _)| *id)
    }
    fn get(&self, socket: &u64) -> Option<&Epoch> {
        self.find(*socket).ok().map(|index| &self.0[index].1)
    }
    fn contains_key(&self, socket: &u64) -> bool {
        self.find(*socket).is_ok()
    }
    fn insert(&mut self, socket: u64, epoch: Epoch) {
        match self.find(socket) {
            Ok(index) => self.0[index].1 = epoch,
            Err(index) => self.0.insert(index, (socket, epoch)),
        }
    }
    fn remove(&mut self, socket: &u64) -> Option<Epoch> {
        let index = self.find(*socket).ok()?;
        Some(self.0.remove(index).1)
    }
    #[cfg(test)]
    fn keys(&self) -> impl Iterator<Item = &u64> {
        self.0.iter().map(|(id, _)| id)
    }
    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Internal state of one engine. No venue can construct or mutate this registry.
#[derive(Default)]
pub(crate) struct Lifecycle {
    feeds: Vec<Feed>,
    // Only live physical epochs. Central retirement removes entries before any
    // native invalidation/retry callback; size is bounded by registered feeds.
    current_sockets: SocketIndex,
    #[cfg(test)]
    socket_lookups: std::cell::Cell<usize>,
    pending_frames: usize,
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
        #[cfg(test)]
        self.socket_lookups.set(self.socket_lookups.get() + 1);
        self.current_sockets
            .get(&socket.0)
            .copied()
            .filter(|epoch| self.current(*epoch))
    }
    #[cfg(test)]
    pub(crate) fn socket_lookup_count(&self) -> usize {
        self.socket_lookups.get()
    }
    pub(crate) fn has_pending_frames(&self) -> bool {
        self.pending_frames != 0
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
    pub(crate) fn interest_revision(&self, epoch: Epoch, required: &[Interest]) -> Option<u64> {
        let feed = self.feeds.get(epoch.feed.0)?;
        if feed.epoch != Some(epoch)
            || required.iter().any(|i| {
                feed.interests
                    .desired
                    .get(&i.key)
                    .is_none_or(|current| current.as_ref() != i)
            })
        {
            return None;
        }
        Some(feed.interests.revision)
    }
    pub(crate) fn stop_feed(&mut self, feed: FeedId) -> Option<Retirement> {
        let f = self.feeds.get_mut(feed.0)?;
        f.stopped = true;
        f.phase = Phase::Quarantined;
        f.authenticated = false;
        f.interests.clear_queue(&mut self.pending_frames);
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
        if socket.0 == 0 || self.current_sockets.contains_key(&socket.0) {
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
        f.interests.clear_queue(&mut self.pending_frames);
        self.current_sockets.insert(socket.0, epoch);
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
    #[cfg(test)]
    pub(crate) fn admit(
        &mut self,
        socket: SocketId,
        ingress: Ingress,
        expired: Option<Timeout>,
    ) -> Admission {
        self.admit_checked(socket, ingress, |_| Ok(expired))
    }
    /// Resolve one current physical epoch and query its fresh supervision state
    /// synchronously. The caller cannot supply an epoch or cache admission.
    pub(crate) fn admit_checked(
        &mut self,
        socket: SocketId,
        ingress: Ingress,
        check: impl FnOnce(Epoch) -> Result<Option<Timeout>, SocketError>,
    ) -> Admission {
        if !self.enabled() {
            return Admission::Legacy;
        }
        let Some(epoch) = self.socket_epoch(socket) else {
            return Admission::Denied;
        };
        let Ok(expired) = check(epoch) else {
            return Admission::Denied;
        };
        let f = &self.feeds[epoch.feed.0];
        if f.epoch != Some(epoch) {
            return Admission::Denied;
        }
        if matches!(ingress, Ingress::Open | Ingress::Connect) && f.phase != Phase::Connecting
            || matches!(ingress, Ingress::Message | Ingress::Send)
                && !matches!(f.phase, Phase::Authenticating | Phase::Ready)
        {
            return Admission::Denied;
        }
        if let Some(timeout) = expired {
            let retired = self.retire(epoch.feed).expect("admitted epoch");
            return Admission::Expired(retired, timeout);
        }
        Admission::Current(epoch)
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
        if matches!(evidence, AuthEvidence::Progress) {
            return if f.policy.auth == AuthMode::Required
                && f.phase == Phase::Authenticating
                && !f.authenticated
            {
                AuthTransition::Progress(epoch)
            } else {
                AuthTransition::Ignored
            };
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
        let removed = self.current_sockets.remove(&epoch.socket.0);
        debug_assert_eq!(removed, Some(epoch));
        f.retiring = Some(epoch);
        f.phase = Phase::Quarantined;
        f.authenticated = false;
        f.interests.clear_queue(&mut self.pending_frames);
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
            f.interests.clear_queue(&mut self.pending_frames);
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
    #[cfg(test)]
    pub(crate) fn desired(&self, feed: FeedId) -> Result<Vec<Interest>, SocketError> {
        Ok(self
            .feeds
            .get(feed.0)
            .ok_or(SocketError::Stale)?
            .interests
            .desired
            .values()
            .map(|i| i.as_ref().clone())
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
        let mut encode = encode;
        self.replace_with_repair(feed, InterestInput::Proposed(interests), &[], &mut encode)
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
        let mut encode = encode;
        self.replay_with_repair(feed, keys, &mut encode)
    }
    pub(crate) fn replay_interests(
        &mut self,
        feed: FeedId,
        encode: impl FnMut(
            bool,
            &[Interest],
            &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
        ) -> Result<(), SocketError>,
    ) -> Result<u64, SocketError> {
        let mut encode = encode;
        self.replay_with_repair(feed, &[], &mut encode)
    }
    fn replay_with_repair(
        &mut self,
        feed: FeedId,
        keys: &[String],
        encode: &mut Encode<'_>,
    ) -> Result<u64, SocketError> {
        let rows = self
            .feeds
            .get(feed.0)
            .ok_or(SocketError::Stale)?
            .interests
            .desired
            .values()
            .cloned()
            .collect();
        self.replace_with_repair(feed, InterestInput::Retained(rows), keys, encode)
    }
    /// Non-generic: every native encoder shares this one copy.
    fn replace_with_repair(
        &mut self,
        feed: FeedId,
        interests: InterestInput,
        repair: &[String],
        encode: &mut Encode<'_>,
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
        // Duplicate keys are found by one sort, not a per-row ordered insert
        // (quadratic for up to MAX_INTERESTS rows in descending order). A
        // duplicate still refuses wherever the per-row check did: before a
        // later row's Capacity failure, and before the budget checks below.
        let mut keys = Vec::with_capacity(interests.len());
        let mut desired_bytes = 0usize;
        for item in interests.rows() {
            if item.key.is_empty()
                || item.key.len() > 1024
                || item.wire_args.len() > MAX_FRAME_BYTES
            {
                return Err(SocketError::InvalidInterest);
            }
            keys.push(item.key.as_ref());
            match desired_bytes
                .checked_add(item.key.len())
                .and_then(|v| v.checked_add(item.wire_args.len()))
            {
                Some(bytes) if bytes <= MAX_INTEREST_BYTES / 2 => desired_bytes = bytes,
                _ if has_duplicate(&mut keys) => return Err(SocketError::InvalidInterest),
                _ => return Err(SocketError::Capacity),
            }
        }
        if has_duplicate(&mut keys) {
            return Err(SocketError::InvalidInterest);
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
        // Reuse immutable rows only when both key and native arguments match.
        // Changed arguments keep a distinct released row until unsubscribe.
        // Collected, then sorted once into the map; has_duplicate refused any
        // repeated key above, so the sort's order among equal keys is moot.
        let mut entries = Vec::new();
        match interests {
            InterestInput::Proposed(rows) => {
                for i in rows {
                    let i = i.canonicalize();
                    let row = f
                        .interests
                        .desired
                        .get(&i.key)
                        .filter(|previous| previous.as_ref() == &i)
                        .cloned()
                        .unwrap_or_else(|| Arc::new(i));
                    entries.push((row.key.clone(), row));
                }
            }
            InterestInput::Retained(rows) => {
                for row in rows {
                    entries.push((row.key.clone(), row));
                }
            }
        }
        let desired = Map::from_unique(entries);
        let mut forced = Set::default();
        for key in repair {
            forced.insert(key.as_str());
        }
        if forced.len() != repair.len() || forced.iter().any(|k| !desired.contains_key(*k)) {
            return Err(SocketError::InvalidInterest);
        }
        let removed: Vec<_> = f
            .interests
            .released
            .values()
            .filter(|i| desired.get(&i.key) != Some(i) || forced.contains(i.key.as_ref()))
            .map(|i| i.as_ref().clone())
            .collect();
        let added: Vec<_> = desired
            .values()
            .filter(|i| {
                f.interests.released.get(&i.key) != Some(i) || forced.contains(i.key.as_ref())
            })
            .map(|i| i.as_ref().clone())
            .collect();
        let mut queue = VecDeque::new();
        let mut queued_bytes = 0usize;
        for (subscribe, items) in [(false, removed), (true, added)] {
            if items.is_empty() {
                continue;
            }
            let mut expected = Set::default();
            for item in &items {
                expected.insert(item.key.as_ref());
            }
            let mut covered = Set::default();
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
                                !expected.contains(key.as_ref()) || !covered.insert(key.clone())
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
            // Each covered key was checked against expected and inserted
            // exactly once. Equal cardinality proves full coverage without
            // allocating another tree of the same canonical keys.
            if covered.len() != expected.len() {
                return Err(SocketError::InvalidCoverage);
            }
        }
        let pending_frames = self
            .pending_frames
            .checked_sub(f.interests.queue.len())
            .and_then(|n| n.checked_add(queue.len()))
            .ok_or(SocketError::Capacity)?;
        // No fallible work remains. Move prior release evidence instead of
        // cloning its map while old/new desired state and frames are live.
        let state = &mut self.feeds[feed.0].interests;
        let released = std::mem::take(&mut state.released);
        *state = Interests {
            desired,
            released,
            revision,
            queue,
        };
        self.pending_frames = pending_frames;
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
        self.pending_frames = self
            .pending_frames
            .checked_sub(1)
            .expect("owned queued subscription frame");
        if queued.revision != f.interests.revision {
            return None;
        }
        for key in queued.batch.covered_keys {
            if queued.subscribe {
                let row = f.interests.desired.get(&key)?.clone();
                f.interests.released.insert(key, row);
            } else {
                f.interests.released.remove(&key);
            }
        }
        Some(queued.batch.text)
    }
}

/// Whether two keys are equal; sorts `keys` in place.
fn has_duplicate(keys: &mut [&str]) -> bool {
    keys.sort_unstable();
    keys.windows(2).any(|pair| pair[0] == pair[1])
}

/// Sorted, deduplicated storage with the `BTreeMap`/`BTreeSet` operations and
/// key order the interest state uses, without each tree's node code: that code
/// is a fixed managed-sockets link cost in every family that enables the feature.
/// A feed's interests (up to `MAX_INTERESTS`) are built with one sort, not one
/// insert at a time; single updates use binary search.
mod sorted {
    use std::borrow::Borrow;
    use std::ops::Index;

    #[derive(Clone, Debug)]
    pub(super) struct Map<K, V>(Vec<(K, V)>);

    impl<K, V> Default for Map<K, V> {
        fn default() -> Self {
            Self(Vec::new())
        }
    }

    impl<K: Ord, V> Map<K, V> {
        fn find<Q: Ord + ?Sized>(&self, key: &Q) -> Result<usize, usize>
        where
            K: Borrow<Q>,
        {
            self.0.binary_search_by(|(k, _)| k.borrow().cmp(key))
        }
        pub(super) fn get<Q: Ord + ?Sized>(&self, key: &Q) -> Option<&V>
        where
            K: Borrow<Q>,
        {
            self.find(key).ok().map(|i| &self.0[i].1)
        }
        pub(super) fn contains_key<Q: Ord + ?Sized>(&self, key: &Q) -> bool
        where
            K: Borrow<Q>,
        {
            self.find(key).is_ok()
        }
        /// As `BTreeMap::insert`: an existing key keeps its original key value
        /// and only the value is replaced.
        pub(super) fn insert(&mut self, key: K, value: V) -> Option<V> {
            match self.find(&key) {
                Ok(i) => Some(std::mem::replace(&mut self.0[i].1, value)),
                Err(i) => {
                    self.0.insert(i, (key, value));
                    None
                }
            }
        }
        pub(super) fn remove<Q: Ord + ?Sized>(&mut self, key: &Q) -> Option<V>
        where
            K: Borrow<Q>,
        {
            self.find(key).ok().map(|i| self.0.remove(i).1)
        }
        pub(super) fn values(&self) -> impl Iterator<Item = &V> {
            self.0.iter().map(|(_, v)| v)
        }
        pub(super) fn clear(&mut self) {
            self.0.clear();
        }
        /// From entries whose keys are already known to be distinct. A small
        /// heapsort instead of a std sort: ~4K less code per element type.
        pub(super) fn from_unique(mut entries: Vec<(K, V)>) -> Self {
            heapsort_by_key(&mut entries);
            debug_assert!(entries.windows(2).all(|p| p[0].0 < p[1].0));
            Self(entries)
        }
    }

    fn heapsort_by_key<K: Ord, V>(v: &mut [(K, V)]) {
        fn sift<K: Ord, V>(v: &mut [(K, V)], mut root: usize) {
            loop {
                let mut child = 2 * root + 1;
                if child >= v.len() {
                    return;
                }
                if child + 1 < v.len() && v[child].0 < v[child + 1].0 {
                    child += 1;
                }
                if v[root].0 >= v[child].0 {
                    return;
                }
                v.swap(root, child);
                root = child;
            }
        }
        for root in (0..v.len() / 2).rev() {
            sift(v, root);
        }
        for end in (1..v.len()).rev() {
            v.swap(0, end);
            sift(&mut v[..end], 0);
        }
    }

    #[cfg(test)]
    impl<K: Ord, V> Map<K, V> {
        pub(super) fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
            self.0.iter().map(|(k, v)| (k, v))
        }
        pub(super) fn keys(&self) -> impl Iterator<Item = &K> {
            self.0.iter().map(|(k, _)| k)
        }
        pub(super) fn len(&self) -> usize {
            self.0.len()
        }
        pub(super) fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
    }

    #[cfg(test)]
    impl<K, V> IntoIterator for Map<K, V> {
        type Item = (K, V);
        type IntoIter = std::vec::IntoIter<(K, V)>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<K: Ord + Borrow<Q>, V, Q: Ord + ?Sized> Index<&Q> for Map<K, V> {
        type Output = V;
        fn index(&self, key: &Q) -> &V {
            self.get(key).expect("no entry found for key")
        }
    }

    #[derive(Debug)]
    pub(super) struct Set<T>(Vec<T>);

    impl<T> Default for Set<T> {
        fn default() -> Self {
            Self(Vec::new())
        }
    }

    impl<T: Ord> Set<T> {
        /// As `BTreeSet::insert`: false when the value was already present.
        pub(super) fn insert(&mut self, value: T) -> bool {
            match self.0.binary_search(&value) {
                Ok(_) => false,
                Err(i) => {
                    self.0.insert(i, value);
                    true
                }
            }
        }
        pub(super) fn contains<Q: Ord + ?Sized>(&self, value: &Q) -> bool
        where
            T: Borrow<Q>,
        {
            self.0.binary_search_by(|v| v.borrow().cmp(value)).is_ok()
        }
        pub(super) fn iter(&self) -> std::slice::Iter<'_, T> {
            self.0.iter()
        }
        pub(super) fn len(&self) -> usize {
            self.0.len()
        }
    }

    #[cfg(test)]
    mod parity {
        use super::{Map, Set};
        use std::collections::{BTreeMap, BTreeSet};
        use std::sync::Arc;

        /// A fixed xorshift stream: the same operations every run.
        fn ops(seed: u64, n: usize) -> impl Iterator<Item = (u8, String, u32)> {
            let mut x = seed;
            (0..n).map(move |i| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                ((x % 5) as u8, format!("k{}", x % 23), i as u32)
            })
        }

        #[test]
        fn map_matches_btreemap_for_every_operation_and_order() {
            for seed in 1..40u64 {
                let mut ours: Map<Arc<str>, u32> = Map::default();
                let mut tree: BTreeMap<Arc<str>, u32> = BTreeMap::new();
                for (op, key, value) in ops(seed, 300) {
                    let key: Arc<str> = key.into();
                    match op {
                        0 | 1 => {
                            let fresh: Arc<str> = key.to_string().into();
                            assert_eq!(
                                ours.insert(fresh.clone(), value),
                                tree.insert(fresh, value)
                            );
                        }
                        2 => assert_eq!(ours.remove(key.as_ref()), tree.remove(key.as_ref())),
                        3 => assert_eq!(ours.get(key.as_ref()), tree.get(key.as_ref())),
                        _ => assert_eq!(ours.contains_key(&key), tree.contains_key(&key)),
                    }
                    let a: Vec<_> = ours.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    let b: Vec<_> = tree.iter().map(|(k, v)| (k.clone(), *v)).collect();
                    assert_eq!(a, b, "seed {seed}");
                    // BTreeMap keeps a repeated key's first allocation.
                    for ((ka, _), (kb, _)) in ours.iter().zip(tree.iter()) {
                        assert!(Arc::ptr_eq(ka, kb) || ka == kb);
                    }
                }
                let ours_values: Vec<_> = ours.values().copied().collect();
                let tree_values: Vec<_> = tree.values().copied().collect();
                assert_eq!(ours_values, tree_values);
            }
        }

        #[test]
        fn from_unique_matches_a_btreemap_for_distinct_keys() {
            let check = |entries: Vec<(Arc<str>, u32)>, label: &str| {
                let tree: BTreeMap<_, _> = entries.iter().cloned().collect();
                let keys: Vec<_> = entries.iter().map(|(k, _)| k.clone()).collect();
                let ours = Map::from_unique(entries);
                let a: Vec<_> = ours.iter().map(|(k, v)| (k.clone(), *v)).collect();
                assert_eq!(a, tree.into_iter().collect::<Vec<_>>(), "{label}");
                for (k, _) in ours.iter() {
                    assert!(
                        keys.iter().any(|key| Arc::ptr_eq(k, key)),
                        "{label}: key moved"
                    );
                }
            };
            for seed in 1..40u64 {
                let mut seen = BTreeSet::new();
                let entries: Vec<(Arc<str>, u32)> = ops(seed, 200)
                    .filter(|(_, k, _)| seen.insert(k.clone()))
                    .map(|(_, k, v)| (k.into(), v))
                    .collect();
                check(entries, &format!("seed {seed}"));
            }
            for len in [0usize, 1, 2, 3, 8192] {
                let ascending: Vec<(Arc<str>, u32)> = (0..len as u32)
                    .map(|i| (format!("k{i:05}").into(), i))
                    .collect();
                let descending = ascending.iter().rev().cloned().collect();
                check(ascending, &format!("ascending {len}"));
                check(descending, &format!("descending {len}"));
            }
        }

        #[test]
        fn set_matches_btreeset() {
            for seed in 1..40u64 {
                let mut ours = Set::default();
                let mut tree = BTreeSet::new();
                for (op, key, _) in ops(seed, 300) {
                    if op < 3 {
                        assert_eq!(ours.insert(key.clone()), tree.insert(key.clone()));
                    } else {
                        assert_eq!(ours.contains(key.as_str()), tree.contains(key.as_str()));
                    }
                    assert_eq!(ours.len(), tree.len());
                    assert!(ours.iter().eq(tree.iter()));
                }
            }
        }
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
            controls: ControlPolicy::Native,
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
    #[test]
    fn checked_admission_queries_only_one_current_epoch() {
        let mut l = Lifecycle::default();
        let mut calls = 0;
        assert_eq!(
            l.admit_checked(SocketId(7), Ingress::Message, |_| {
                calls += 1;
                Ok(None)
            }),
            Admission::Legacy
        );
        let f = l.register(policy(AuthMode::None)).unwrap();
        let e = l.begin(f, SocketId(7)).unwrap();
        assert_eq!(
            l.admit_checked(SocketId(8), Ingress::Message, |_| {
                calls += 1;
                Ok(None)
            }),
            Admission::Denied
        );
        l.opened(e).unwrap();
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Message, |exact| {
                calls += 1;
                assert_eq!(exact, e);
                Ok(None)
            }),
            Admission::Current(e)
        );
        assert_eq!(calls, 1);
        l.retire(f).unwrap();
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Message, |_| {
                calls += 1;
                Ok(None)
            }),
            Admission::Denied
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn checked_expiry_does_not_retire_wrong_phase_events() {
        let mut l = Lifecycle::default();
        let f = l.register(policy(AuthMode::None)).unwrap();
        let e = l.begin(f, SocketId(7)).unwrap();
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Message, |_| Ok(Some(Timeout::Connect))),
            Admission::Denied
        );
        assert!(l.current(e));
        l.opened(e).unwrap();
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Open, |_| Ok(Some(Timeout::Idle))),
            Admission::Denied
        );
        assert!(l.current(e));
        assert!(matches!(
            l.admit_checked(e.socket(), Ingress::Message, |_| Ok(Some(Timeout::Idle))),
            Admission::Expired(retired, Timeout::Idle) if retired.epoch == e
        ));
        assert!(!l.current(e));
        let mut called = false;
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Message, |_| {
                called = true;
                Ok(None)
            }),
            Admission::Denied
        );
        assert!(!called);
    }

    #[test]
    fn failed_fresh_query_preserves_epoch_and_queued_interests() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("unchanged")], encode)
            .unwrap();
        let bytes = l.feeds[f.0].interests.bytes();
        let frames = l.pending_frames;
        assert_eq!(
            l.admit_checked(e.socket(), Ingress::Message, |_| Err(
                SocketError::InvalidTime
            )),
            Admission::Denied
        );
        assert!(l.current(e));
        assert_eq!(l.pending_frames, frames);
        assert_eq!(l.feeds[f.0].interests.bytes(), bytes);
        assert!(l.release_batch(e, false).is_some());
    }
    /// fe7lgm on #2127: MAX_INTERESTS rows in descending key order (the worst
    /// case for an ordered insert) complete, and a duplicate among them refuses.
    #[test]
    fn max_interests_in_reverse_order_complete_and_refuse_a_duplicate() {
        let rows: Vec<Interest> = (0..MAX_INTERESTS)
            .rev()
            .map(|i| interest(&format!("k{i:05}")))
            .collect();
        let (mut l, f, _) = started(AuthMode::None);
        // Still MAX_INTERESTS rows: the last (k00000) becomes a second k08191.
        let mut duplicated = rows.clone();
        duplicated[MAX_INTERESTS - 1] = interest("k08191");
        assert_eq!(
            l.replace_interests(f, duplicated, encode),
            Err(SocketError::InvalidInterest)
        );
        assert_eq!(
            l.feeds[f.0].interests.revision, 0,
            "a refusal has no effect"
        );
        // One frame per 64 keys keeps the 8192 rows under MAX_FRAMES.
        let batched =
            |subscribe: bool,
             values: &[Interest],
             emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>| {
                for chunk in values.chunks(64) {
                    emit(EncodedBatch {
                        text: format!("{}{}", if subscribe { "+" } else { "-" }, chunk.len()),
                        covered_keys: chunk.iter().map(|i| i.key.clone()).collect(),
                    })?;
                }
                Ok(())
            };
        assert!(l.replace_interests(f, rows, batched).is_ok());
        assert_eq!(l.feeds[f.0].interests.desired.len(), MAX_INTERESTS);
    }

    /// A duplicate refuses as InvalidInterest exactly where the per-row check
    /// did: ahead of a later row's Capacity failure, and Capacity first when
    /// the overflowing row comes before the duplicate.
    #[test]
    fn duplicate_and_capacity_refusals_keep_their_precedence() {
        let wide = |key: &str| Interest {
            key: key.into(),
            wire_args: "x".repeat(MAX_FRAME_BYTES).into(),
        };
        let filler: Vec<Interest> = (0..MAX_INTEREST_BYTES / 2 / MAX_FRAME_BYTES)
            .map(|i| wide(&format!("w{i:05}")))
            .collect();
        let (mut l, f, _) = started(AuthMode::None);
        let mut duplicate_first = vec![interest("A"), interest("A")];
        duplicate_first.extend(filler.iter().cloned());
        assert_eq!(
            l.replace_interests(f, duplicate_first, encode),
            Err(SocketError::InvalidInterest)
        );
        let mut overflow_first = filler.clone();
        overflow_first.push(wide("over"));
        overflow_first.push(interest("A"));
        overflow_first.push(interest("A"));
        assert_eq!(
            l.replace_interests(f, overflow_first, encode),
            Err(SocketError::Capacity)
        );
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
    fn retained_text_allocations() -> [usize; 6] {
        let (mut l, f, e) = started(AuthMode::None);
        let key = "shared-canonical-nonempty-native-argument";
        let mut encoded_key = 0;
        l.replace_interests(f, vec![interest(key)], |op, rows, emit| {
            encoded_key = rows[0].key.as_ptr() as usize;
            encode(op, rows, emit)
        })
        .unwrap();
        let (map_key, row) = l.feeds[f.0].interests.desired.iter().next().unwrap();
        let desired_key = row.key.as_ptr() as usize;
        let desired_arg = row.wire_args.as_ptr() as usize;
        let desired_map = map_key.as_ptr() as usize;
        let coverage = l.feeds[f.0]
            .interests
            .queue
            .front()
            .unwrap()
            .batch
            .covered_keys[0]
            .as_ptr() as usize;
        assert!(l.release_batch(e, false).is_some());
        let released_map = l.feeds[f.0]
            .interests
            .released
            .keys()
            .next()
            .unwrap()
            .as_ptr() as usize;
        [
            desired_key,
            desired_arg,
            desired_map,
            coverage,
            released_map,
            encoded_key,
        ]
    }

    #[test]
    fn equal_native_key_and_argument_share_canonical_text_storage() {
        let addresses = retained_text_allocations();
        assert_eq!(
            addresses[0], addresses[1],
            "equal immutable native text must not be retained twice"
        );
    }
    #[test]
    fn desired_and_released_map_keys_share_the_canonical_row_key() {
        let addresses = retained_text_allocations();
        assert_eq!(
            addresses[0], addresses[2],
            "desired map key must not duplicate row text"
        );
        assert_eq!(
            addresses[0], addresses[4],
            "released map key must share the same canonical text"
        );
    }
    #[test]
    fn queued_coverage_uses_the_canonical_row_key_storage() {
        let addresses = retained_text_allocations();
        assert_eq!(
            addresses[0], addresses[3],
            "queued coverage must retain a handle, not duplicate native text"
        );
    }
    #[test]
    fn encoder_scratch_reuses_canonical_interest_text() {
        let addresses = retained_text_allocations();
        assert_eq!(
            addresses[0], addresses[5],
            "transaction scratch must retain a handle, not deep-clone text"
        );
    }

    #[test]
    fn final_release_transfers_queued_coverage_key_storage() {
        let (mut l, f, e) = started(AuthMode::None);
        let key = "a-nonempty-owned-coverage-key";
        l.replace_interests(f, vec![interest(key)], encode).unwrap();
        let queued = &l.feeds[f.0]
            .interests
            .queue
            .front()
            .unwrap()
            .batch
            .covered_keys[0];
        let allocation = queued.as_ptr();
        assert_eq!(
            l.release_batch(e, false).as_deref(),
            Some("+:a-nonempty-owned-coverage-key")
        );
        let released = l.feeds[f.0].interests.released.keys().next().unwrap();
        assert_eq!(
            released.as_ptr(),
            allocation,
            "move the consumed coverage key instead of cloning it"
        );
    }

    #[test]
    fn canonical_replay_rechecks_queue_budget_and_encoder_failure_atomically() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
        let desired = l.feeds[f.0].interests.desired.clone();
        let revision = l.feeds[f.0].interests.revision;
        let front = l.feeds[f.0]
            .interests
            .queue
            .front()
            .unwrap()
            .batch
            .text
            .clone();
        l.external_queue_bytes = MAX_QUEUED_BYTES;
        assert_eq!(l.replay_interests(f, encode), Err(SocketError::Capacity));
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        assert_eq!(
            l.feeds[f.0].interests.queue.front().unwrap().batch.text,
            front
        );
        l.external_queue_bytes = 0;
        let mut calls = 0;
        assert_eq!(
            l.replay_interests(f, |op, rows, emit| {
                calls += 1;
                encode(op, rows, emit)?;
                Err(SocketError::InvalidCoverage)
            }),
            Err(SocketError::InvalidCoverage)
        );
        assert_eq!(calls, 1);
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        for (key, row) in desired {
            assert!(Arc::ptr_eq(&row, &l.feeds[f.0].interests.desired[&key]));
        }
        assert_eq!(l.feeds[f.0].interests.released.len(), 1);
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:B"));
        l.replay_interests(f, encode).unwrap();
        assert_eq!(
            l.release_batch(e, false),
            None,
            "already released rows do not replay twice"
        );
    }

    #[test]
    fn canonical_repair_retains_row_identity_and_unsubscribe_before_subscribe() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        while l.release_batch(e, false).is_some() {}
        let desired = l.feeds[f.0].interests.desired.clone();
        let revision = l.feeds[f.0].interests.revision;
        assert_eq!(
            l.repair_interests(f, &["missing".into()], encode),
            Err(SocketError::InvalidInterest)
        );
        assert_eq!(
            l.repair_interests(f, &["A".into(), "A".into()], encode),
            Err(SocketError::InvalidInterest)
        );
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        l.repair_interests(f, &["A".into()], encode).unwrap();
        for (key, row) in desired {
            assert!(Arc::ptr_eq(&row, &l.feeds[f.0].interests.desired[&key]));
        }
        assert_eq!(l.release_batch(e, false).as_deref(), Some("-:A"));
        assert!(!l.feeds[f.0].interests.released.contains_key("A"));
        assert!(l.feeds[f.0].interests.released.contains_key("B"));
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
        assert!(Arc::ptr_eq(
            &l.feeds[f.0].interests.desired["A"],
            &l.feeds[f.0].interests.released["A"]
        ));
        assert_eq!(l.release_batch(e, false), None);
    }

    #[test]
    fn release_barrier_and_stale_epoch_preserve_queued_coverage_storage() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("owned-key")], encode)
            .unwrap();
        let allocation = l.feeds[f.0]
            .interests
            .queue
            .front()
            .unwrap()
            .batch
            .covered_keys[0]
            .as_ptr();
        assert_eq!(l.release_batch(e, true), None);
        assert_eq!(
            l.feeds[f.0]
                .interests
                .queue
                .front()
                .unwrap()
                .batch
                .covered_keys[0]
                .as_ptr(),
            allocation
        );
        let stale = Epoch {
            socket: SocketId(99),
            ..e
        };
        assert_eq!(l.release_batch(stale, false), None);
        assert_eq!(
            l.feeds[f.0]
                .interests
                .queue
                .front()
                .unwrap()
                .batch
                .covered_keys[0]
                .as_ptr(),
            allocation
        );
        assert!(l.feeds[f.0].interests.released.is_empty());
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:owned-key"));
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
    fn current_socket_lookup_is_revoked_before_retirement_ack_and_never_resurrects() {
        let mut l = Lifecycle::default();
        let a = l.register(policy(AuthMode::None)).unwrap();
        let b = l.register(policy(AuthMode::None)).unwrap();
        let neighbor = l.begin(b, SocketId(9000)).unwrap();
        l.opened(neighbor).unwrap();
        for id in 1..=128 {
            let old = l.begin(a, SocketId(id)).unwrap();
            assert_eq!(l.socket_epoch(old.socket()), Some(old));
            assert_eq!(l.begin(b, old.socket()), Err(SocketError::NotPermitted));
            l.opened(old).unwrap();
            l.retire(a).unwrap();
            assert_eq!(l.socket_epoch(old.socket()), None);
            assert_eq!(
                l.admit(old.socket(), Ingress::Message, None),
                Admission::Denied
            );
            assert_eq!(l.socket_epoch(neighbor.socket()), Some(neighbor));
            l.retired(old, true).unwrap();
            l.grant_retry(a).unwrap();
        }
        l.stop_all();
        assert_eq!(l.socket_epoch(neighbor.socket()), None);
        assert_eq!(
            l.admit(neighbor.socket(), Ingress::Message, None),
            Admission::Denied
        );
    }

    #[test]
    fn current_socket_index_sorts_full_width_ids_and_preserves_shifted_neighbors() {
        let mut l = Lifecycle::default();
        let ids = [u64::MAX, 2, 1u64 << 40, 1, 999, u32::MAX as u64 + 1];
        let epochs: Vec<_> = ids
            .into_iter()
            .map(|id| {
                let feed = l.register(policy(AuthMode::None)).unwrap();
                l.begin(feed, SocketId(id)).unwrap()
            })
            .collect();
        let mut expected = ids;
        expected.sort_unstable();
        assert_eq!(
            l.current_sockets.keys().copied().collect::<Vec<_>>(),
            expected
        );
        for epoch in &epochs {
            assert_eq!(l.socket_epoch(epoch.socket()), Some(*epoch));
        }
        // Remove from the middle, then reinsert a smaller ID on the same feed.
        let old = epochs[2];
        l.retire(old.feed()).unwrap();
        l.retired(old, true).unwrap();
        l.grant_retry(old.feed()).unwrap();
        let new = l.begin(old.feed(), SocketId(3)).unwrap();
        assert_eq!(l.socket_epoch(old.socket()), None);
        assert_eq!(l.socket_epoch(new.socket()), Some(new));
        for epoch in epochs.into_iter().filter(|e| *e != old) {
            assert_eq!(l.socket_epoch(epoch.socket()), Some(epoch));
        }
        assert!(l.current_sockets.keys().is_sorted());
    }

    #[test]
    fn refused_begin_preserves_current_socket_and_other_feed_generation() {
        let mut l = Lifecycle::default();
        let a = l.register(policy(AuthMode::None)).unwrap();
        let b = l.register(policy(AuthMode::None)).unwrap();
        let current = l.begin(a, SocketId(41)).unwrap();
        assert_eq!(l.begin(b, current.socket()), Err(SocketError::NotPermitted));
        assert_eq!(l.begin(b, SocketId(0)), Err(SocketError::NotPermitted));
        assert_eq!(l.feeds[b.0].generation, 0);
        assert_eq!(l.socket_epoch(current.socket()), Some(current));
        assert_eq!(l.epoch(b), None);
        l.feeds[b.0].generation = u64::MAX;
        assert_eq!(l.begin(b, SocketId(42)), Err(SocketError::Capacity));
        assert_eq!(l.socket_epoch(SocketId(42)), None);
        assert_eq!(l.socket_epoch(current.socket()), Some(current));
        l.retire_group(1); // Unrelated group cannot clear another current socket.
        assert_eq!(l.socket_epoch(current.socket()), Some(current));
    }

    #[test]
    fn current_socket_index_is_bounded_and_all_retire_paths_remove_ownership() {
        let mut l = Lifecycle::default();
        let mut epochs = Vec::new();
        for id in 1..=MAX_FEEDS {
            let mut p = policy(AuthMode::None);
            p.permanent_refusal_group = (id % 2 == 0).then_some(7);
            let feed = l.register(p).unwrap();
            epochs.push(l.begin(feed, SocketId(id as u64)).unwrap());
        }
        assert_eq!(l.current_sockets.len(), MAX_FEEDS);
        assert_eq!(
            l.register(policy(AuthMode::None)),
            Err(SocketError::Capacity)
        );
        let group = l.retire_group(7);
        assert_eq!(group.len(), MAX_FEEDS / 2);
        assert_eq!(l.current_sockets.len(), MAX_FEEDS / 2);
        for retirement in group {
            assert_eq!(l.socket_epoch(retirement.epoch.socket()), None);
            l.retired(retirement.epoch, true).unwrap();
            assert_eq!(
                l.grant_retry(retirement.epoch.feed()),
                Err(SocketError::NotPermitted)
            );
        }
        l.stop_feed(epochs[0].feed()).unwrap();
        assert_eq!(l.socket_epoch(epochs[0].socket()), None);
        let expired = epochs[2];
        l.opened(expired).unwrap();
        assert!(matches!(
            l.admit(expired.socket(), Ingress::Message, Some(Timeout::Idle)),
            Admission::Expired(_, Timeout::Idle)
        ));
        assert_eq!(l.socket_epoch(expired.socket()), None);
        l.retired(expired, false).unwrap();
        assert_eq!(
            l.grant_retry(expired.feed()),
            Err(SocketError::NotPermitted)
        );
        l.stop_all();
        assert!(l.current_sockets.is_empty());
        for epoch in epochs {
            assert_eq!(
                l.admit(epoch.socket(), Ingress::Message, None),
                Admission::Denied
            );
        }
    }

    fn assert_pending_frames(l: &Lifecycle, expected: usize) {
        let actual: usize = l.feeds.iter().map(|f| f.interests.queue.len()).sum();
        assert_eq!(actual, expected);
        assert_eq!(l.pending_frames, actual);
        assert_eq!(l.has_pending_frames(), expected != 0);
    }

    #[test]
    fn pending_frame_count_preserves_refusal_and_double_clear_paths() {
        let mut l = Lifecycle::default();
        let mut p = policy(AuthMode::None);
        p.permanent_refusal_group = Some(7);
        let a = l.register(p).unwrap();
        l.replace_interests(a, vec![interest("A")], encode).unwrap();
        assert_pending_frames(&l, 1);
        assert_eq!(l.begin(a, SocketId(0)), Err(SocketError::NotPermitted));
        assert_pending_frames(&l, 1);
        l.feeds[a.0].generation = u64::MAX;
        assert_eq!(l.begin(a, SocketId(1)), Err(SocketError::Capacity));
        assert_pending_frames(&l, 1);
        l.feeds[a.0].generation = 0;
        let e = l.begin(a, SocketId(1)).unwrap();
        assert_pending_frames(&l, 0); // Successful begin clears staged old queue.
        l.opened(e).unwrap();
        l.replay_interests(a, encode).unwrap();
        assert_pending_frames(&l, 1);
        l.feeds[a.0].interests.queue.front_mut().unwrap().revision = 0;
        assert_eq!(l.release_batch(e, false), None);
        assert_pending_frames(&l, 0); // A discarded stale frame was still popped.
        l.replay_interests(a, encode).unwrap();
        assert_pending_frames(&l, 1);
        assert_eq!(l.retire_group(7).len(), 1); // Group clear then retire clear.
        assert_pending_frames(&l, 0);
        l.stop_feed(a);
        assert_pending_frames(&l, 0);
    }

    #[test]
    fn pending_frames_follow_partial_release_failed_transaction_and_reconnect() {
        let (mut l, a, ea) = started(AuthMode::None);
        let b = l.register(policy(AuthMode::None)).unwrap();
        let eb = l.begin(b, SocketId(2)).unwrap();
        l.opened(eb).unwrap();
        l.replace_interests(a, vec![interest("A"), interest("B")], encode)
            .unwrap();
        l.replace_interests(b, vec![interest("C")], encode).unwrap();
        assert!(l.has_pending_frames());
        assert_pending_frames(&l, 3);
        assert_eq!(l.release_batch(ea, true), None);
        assert_pending_frames(&l, 3);
        assert!(l.has_pending_frames());
        assert_eq!(l.release_batch(ea, false).as_deref(), Some("+:A"));
        assert_pending_frames(&l, 2);
        assert_eq!(
            l.replace_interests(a, vec![interest("D")], |op, rows, emit| {
                encode(op, rows, emit)?;
                Err(SocketError::InvalidCoverage)
            }),
            Err(SocketError::InvalidCoverage)
        );
        assert_pending_frames(&l, 2);
        assert_eq!(l.release_batch(ea, false).as_deref(), Some("+:B"));
        assert_pending_frames(&l, 1); // Other feed still owns C.
        l.retire(b).unwrap();
        assert!(!l.has_pending_frames());
        assert_pending_frames(&l, 0);
        l.retired(eb, true).unwrap();
        l.grant_retry(b).unwrap();
        let fresh = l.begin(b, SocketId(3)).unwrap();
        l.opened(fresh).unwrap();
        l.replay_interests(b, encode).unwrap();
        assert_pending_frames(&l, 1);
        assert_eq!(l.release_batch(eb, false), None);
        assert_pending_frames(&l, 1);
        assert!(l.has_pending_frames());
        assert_eq!(l.release_batch(fresh, false).as_deref(), Some("+:C"));
        assert!(!l.has_pending_frames());
        assert_pending_frames(&l, 0);
    }

    #[test]
    fn inactive_pending_frames_keep_existing_stop_all_cleanup_contract() {
        let mut l = Lifecycle::default();
        let feed = l.register(policy(AuthMode::None)).unwrap();
        l.replace_interests(feed, vec![interest("A")], encode)
            .unwrap();
        assert!(l.has_pending_frames());
        assert_pending_frames(&l, 1);
        assert!(l.stop_all().is_empty());
        assert_pending_frames(&l, 1);
        assert!(l.has_pending_frames()); // No active epoch existed to retire.
        assert_eq!(l.begin(feed, SocketId(1)), Err(SocketError::NotPermitted));
        l.stop_feed(feed); // Explicit feed stop also clears an inactive queue.
        assert!(!l.has_pending_frames());
        assert_pending_frames(&l, 0);
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
    fn immutable_interest_storage_is_shared_only_after_actual_release() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        let pointer = l.feeds[f.0].interests.desired["A"].wire_args.as_ptr();
        assert!(l.feeds[f.0].interests.released.is_empty());
        assert_eq!(l.release_batch(e, true), None);
        assert!(l.feeds[f.0].interests.released.is_empty());
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            pointer
        );
        let bytes = l.feeds[f.0].interests.bytes();
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        assert_eq!(
            l.feeds[f.0].interests.desired["A"].wire_args.as_ptr(),
            pointer
        );
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            pointer
        );
        assert!(
            l.feeds[f.0].interests.bytes() > bytes,
            "logical budgets still count both maps"
        );
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:B"));
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        assert_eq!(
            l.feeds[f.0].interests.desired["A"].wire_args.as_ptr(),
            pointer
        );
        assert_eq!(l.release_batch(e, false), None);
    }

    #[test]
    fn shared_interest_replacement_retains_old_arguments_until_unsubscribe_release() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        l.release_batch(e, false).unwrap();
        let pointer = l.feeds[f.0].interests.released["A"].wire_args.as_ptr();
        l.replace_interests(
            f,
            vec![Interest {
                key: "A".into(),
                wire_args: "new-A".into(),
            }],
            encode,
        )
        .unwrap();
        assert_eq!(l.feeds[f.0].interests.released["A"].wire_args.as_ref(), "A");
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            pointer
        );
        assert_ne!(
            l.feeds[f.0].interests.desired["A"].wire_args.as_ptr(),
            pointer
        );
        assert_eq!(l.release_batch(e, false).as_deref(), Some("-:A"));
        assert!(l.feeds[f.0].interests.released.is_empty());
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:new-A"));
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            l.feeds[f.0].interests.desired["A"].wire_args.as_ptr()
        );
    }

    #[test]
    fn partially_encoded_failure_preserves_shared_rows_revision_and_old_queue() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        l.release_batch(e, false).unwrap();
        let pointer = l.feeds[f.0].interests.released["A"].wire_args.as_ptr();
        let desired = l.feeds[f.0].interests.desired["B"].wire_args.as_ptr();
        let revision = l.feeds[f.0].interests.revision;
        let mut emitted = 0;
        assert_eq!(
            l.replace_interests(f, vec![interest("C")], |op, rows, emit| {
                encode(op, rows, emit)?;
                emitted += 1;
                Err(SocketError::Capacity)
            }),
            Err(SocketError::Capacity)
        );
        assert_eq!(emitted, 1);
        assert_eq!(l.feeds[f.0].interests.revision, revision);
        assert_eq!(
            l.feeds[f.0].interests.desired["B"].wire_args.as_ptr(),
            desired
        );
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            pointer
        );
        assert_eq!(l.release_batch(e, false).as_deref(), Some("+:B"));
        assert_eq!(l.release_batch(e, false), None);
    }

    #[test]
    fn shared_interest_reconnect_reuses_desired_without_publishing_released_progress() {
        let (mut l, f, old) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A")], encode).unwrap();
        l.release_batch(old, false).unwrap();
        let pointer = l.feeds[f.0].interests.desired["A"].wire_args.as_ptr();
        l.retire(f).unwrap();
        l.retired(old, true).unwrap();
        l.grant_retry(f).unwrap();
        let current = l.begin(f, SocketId(2)).unwrap();
        l.opened(current).unwrap();
        l.replay_interests(f, encode).unwrap();
        assert!(l.feeds[f.0].interests.released.is_empty());
        assert_eq!(
            l.feeds[f.0].interests.desired["A"].wire_args.as_ptr(),
            pointer
        );
        assert_eq!(l.release_batch(old, false), None);
        assert_eq!(l.release_batch(current, false).as_deref(), Some("+:A"));
        assert_eq!(
            l.feeds[f.0].interests.released["A"].wire_args.as_ptr(),
            pointer
        );
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
    fn coverage_accepts_reverse_order_and_resets_between_directions() {
        let (mut l, f, e) = started(AuthMode::None);
        l.replace_interests(f, vec![interest("A"), interest("B")], encode)
            .unwrap();
        while l.release_batch(e, false).is_some() {}
        let changed: Vec<_> = ["A", "B"]
            .into_iter()
            .map(|key| Interest {
                key: key.into(),
                wire_args: format!("new-{key}").into(),
            })
            .collect();
        let mut directions = Vec::new();
        l.replace_interests(f, changed.clone(), |subscribe, items, emit| {
            directions.push(subscribe);
            for item in items.iter().rev() {
                emit(EncodedBatch {
                    text: format!("{}:{}", if subscribe { "+" } else { "-" }, item.wire_args),
                    covered_keys: vec![item.key.clone()],
                })?;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(directions, [false, true]);
        assert_eq!(
            std::iter::from_fn(|| l.release_batch(e, false)).collect::<Vec<_>>(),
            ["-:B", "-:A", "+:new-B", "+:new-A"]
        );
        assert_eq!(l.desired(f).unwrap(), changed);
    }

    #[test]
    fn coverage_latches_unknown_and_cross_batch_duplicate_after_valid_output() {
        for invalid in ["B", "unknown"] {
            let (mut l, f, e) = started(AuthMode::None);
            l.replace_interests(f, vec![interest("A")], encode).unwrap();
            let revision = l.feeds[f.0].interests.revision;
            let queued_bytes = l.feeds[f.0].interests.queued_bytes();
            let result =
                l.replace_interests(f, vec![interest("B"), interest("C")], |_, _, emit| {
                    assert_eq!(
                        emit(EncodedBatch {
                            text: "first".into(),
                            covered_keys: vec!["B".into()]
                        }),
                        Ok(())
                    );
                    assert_eq!(
                        emit(EncodedBatch {
                            text: "invalid".into(),
                            covered_keys: vec![invalid.into()]
                        }),
                        Err(SocketError::InvalidCoverage)
                    );
                    assert_eq!(
                        emit(EncodedBatch {
                            text: "remaining".into(),
                            covered_keys: vec!["C".into()]
                        }),
                        Err(SocketError::InvalidCoverage)
                    );
                    Ok(()) // A codec cannot erase an earlier admission error.
                });
            assert_eq!(result, Err(SocketError::InvalidCoverage));
            assert_eq!(l.feeds[f.0].interests.revision, revision);
            assert_eq!(l.feeds[f.0].interests.queued_bytes(), queued_bytes);
            assert_eq!(l.desired(f).unwrap(), [interest("A")]);
            assert_pending_frames(&l, 1);
            assert_eq!(l.release_batch(e, false).as_deref(), Some("+:A"));
        }
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
                key: n.to_string().into(),
                wire_args: "x".repeat(MAX_FRAME_BYTES).into(),
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
                key: i.to_string().into(),
                wire_args: "x".repeat(MAX_FRAME_BYTES).into(),
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
                key: i.to_string().into(),
                wire_args: "x".repeat(MAX_FRAME_BYTES).into(),
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
                key: i.to_string().into(),
                wire_args: "x".repeat(MAX_FRAME_BYTES).into(),
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
        l.replay_interests(f, encode).unwrap();
        assert_eq!(l.release_batch(old, false), None);
        assert_eq!(l.release_batch(current, false).as_deref(), Some("+:A"));
        assert_eq!(l.release_batch(current, false).as_deref(), Some("+:B"));
        assert_eq!(l.release_batch(current, false), None);
    }
}

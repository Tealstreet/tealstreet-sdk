//! Scheduling for caller-declared, retry-safe logical reads.
//!
//! A token owns an entire snapshot, including parallel legs and pagination. It
//! conveys no authority to publish rows, authenticate credentials or replay a
//! write. Callers retain those fences and rebuild each request at issue time.
//! Times are finite raw host milliseconds, never calibrated protocol time.

/// Why a caller wants a read. Periodic polls do not perpetually dirty a slow read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Demand {
    Refresh,
    Invalidate,
}

/// Unique within a scheduler, including after cancellation and key retirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidTime,
    InvalidPolicy,
    Capacity,
    Duplicate,
    UnknownKey,
    UnknownGroup,
    TokenExhausted,
}

/// Retry arithmetic shared with the startup compatibility facade.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    pub initial_ms: f64,
    pub max_steps: u32,
    pub cap_ms: f64,
}
impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            initial_ms: 1_000.0,
            max_steps: 5,
            cap_ms: 30_000.0,
        }
    }
}
impl RetryPolicy {
    fn valid(self) -> bool {
        self.initial_ms.is_finite()
            && self.initial_ms > 0.0
            && self.cap_ms.is_finite()
            && self.cap_ms >= self.initial_ms
    }
}

#[derive(Debug, Default)]
pub struct RetryState {
    failures: u32,
}
impl RetryState {
    pub fn failures(&self) -> u32 {
        self.failures
    }

    pub fn reset(&mut self) {
        self.failures = 0;
    }

    /// Exactly one draw per new failed attempt, none for coalescing or pauses.
    pub fn failed(&mut self, jitter: impl FnOnce(f64) -> f64) -> f64 {
        self.failed_with(RetryPolicy::default(), jitter)
    }

    fn failed_with(&mut self, policy: RetryPolicy, jitter: impl FnOnce(f64) -> f64) -> f64 {
        self.failures = self.failures.saturating_add(1);
        let nominal =
            crate::backoff::Backoff::new(policy.initial_ms, policy.max_steps, policy.cap_ms)
                .delay(self.failures - 1);
        let sampled = jitter(nominal);
        if sampled.is_finite() {
            sampled.clamp(0.0, nominal)
        } else {
            nominal
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpiryPolicy {
    /// An unanswered issued read is a failed attempt and consumes retry jitter.
    Retry,
    /// Replace a lost read immediately, retaining the existing failure count.
    ReplaceImmediately,
}

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// First notification-repair deadline, retained under repeated invalidation.
    pub debounce_ms: f64,
    /// Poll/startup refresh delay, independent of notification repair timing.
    pub refresh_debounce_ms: f64,
    /// Bound on a ready reservation which has not reached the network.
    pub admission_timeout_ms: f64,
    /// Bound for the complete logical read, anchored at actual issue.
    pub response_timeout_ms: f64,
    pub response_expiry: ExpiryPolicy,
    pub retry: RetryPolicy,
}
impl Policy {
    fn valid(self) -> bool {
        self.debounce_ms.is_finite()
            && self.debounce_ms >= 0.0
            && self.refresh_debounce_ms.is_finite()
            && self.refresh_debounce_ms >= 0.0
            && self.admission_timeout_ms.is_finite()
            && self.admission_timeout_ms > 0.0
            && self.response_timeout_ms.is_finite()
            && self.response_timeout_ms > 0.0
            && self.retry.valid()
    }
}

#[derive(Clone, Copy, Debug)]
struct Active {
    token: Attempt,
    deadline: f64,
    issued: bool,
    #[cfg(feature = "http-plans")]
    state: ActiveState,
}

impl Active {
    fn live(&self) -> bool {
        #[cfg(feature = "http-plans")]
        {
            self.state == ActiveState::Live
        }
        #[cfg(not(feature = "http-plans"))]
        {
            true
        }
    }
}

#[cfg(feature = "http-plans")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActiveState {
    Live,
    Retiring(AfterRetirement),
}

#[cfg(feature = "http-plans")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AfterRetirement {
    Pause,
    Cancel,
    Remove,
    Superseded,
    Finished(bool),
    Expired(bool),
}

struct Read<K> {
    key: K,
    group: usize,
    policy: Policy,
    /// Refresh, retry or revoked-reservation demand. Notifications retain their
    /// own first deadline, including while this logical read is active.
    pending: Option<f64>,
    /// Fresh demand may advance a notification deadline, never a retry floor.
    retry_not_before: f64,
    invalidation_due: Option<f64>,
    active: Option<Active>,
    retry: RetryState,
    #[cfg(feature = "http-plans")]
    retain_until_retired: bool,
}

impl<K> Read<K> {
    fn due(&self, pause_until: f64) -> Option<f64> {
        let due = match (self.pending, self.invalidation_due) {
            (Some(refresh), Some(notification)) => refresh.min(notification),
            (Some(due), None) | (None, Some(due)) => due,
            (None, None) => return None,
        };
        Some(due.max(self.retry_not_before).max(pause_until))
    }
}

struct Group<G> {
    key: G,
    until: f64,
}

/// Bounded registry; least-recently-reserved order breaks equal-deadline ties.
///
/// Groups are fixed at construction so retiring the last key cannot erase a
/// pause and let a newly registered key escape it. Separate instances do not
/// share pause, retry, token or financial state.
pub struct Scheduler<K, G> {
    reads: Vec<Read<K>>,
    groups: Vec<Group<G>>,
    capacity: usize,
    serial: u64,
}

fn time(now: f64) -> Result<(), Error> {
    if now.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidTime)
    }
}

// Finite saturation keeps a malformed/astronomical host input from producing an
// unserviceable infinite deadline. Negative explicit clocks remain supported.
fn after(now: f64, delay: f64) -> f64 {
    (now + delay).min(f64::MAX)
}

fn lease_deadline(now: f64, duration: f64) -> Result<f64, Error> {
    let deadline = now + duration;
    if deadline.is_finite() && deadline > now {
        Ok(deadline)
    } else {
        Err(Error::InvalidTime)
    }
}

impl<K: PartialEq, G: PartialEq> Scheduler<K, G> {
    pub fn new(capacity: usize, groups: Vec<G>) -> Result<Self, Error> {
        if capacity == 0 || groups.is_empty() || groups.len() > capacity {
            return Err(Error::Capacity);
        }
        for (index, key) in groups.iter().enumerate() {
            if groups[..index].contains(key) {
                return Err(Error::Duplicate);
            }
        }
        Ok(Self {
            reads: Vec::new(),
            groups: groups
                .into_iter()
                .map(|key| Group {
                    key,
                    until: f64::NEG_INFINITY,
                })
                .collect(),
            capacity,
            serial: 0,
        })
    }

    pub fn register(&mut self, key: K, group: &G, policy: Policy) -> Result<(), Error> {
        self.register_inner(key, group, policy, false)
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn register_task(&mut self, key: K, group: &G, policy: Policy) -> Result<(), Error> {
        self.register_inner(key, group, policy, true)
    }

    fn register_inner(
        &mut self,
        key: K,
        group: &G,
        policy: Policy,
        retain_until_retired: bool,
    ) -> Result<(), Error> {
        #[cfg(not(feature = "http-plans"))]
        let _ = retain_until_retired;
        if !policy.valid() {
            return Err(Error::InvalidPolicy);
        }
        if self.reads.iter().any(|read| read.key == key) {
            return Err(Error::Duplicate);
        }
        if self.reads.len() == self.capacity {
            return Err(Error::Capacity);
        }
        let group = self
            .groups
            .iter()
            .position(|candidate| &candidate.key == group)
            .ok_or(Error::UnknownGroup)?;
        #[cfg(feature = "http-plans")]
        if retain_until_retired {
            self.reads.try_reserve(1).map_err(|_| Error::Capacity)?;
        }
        self.reads.push(Read {
            key,
            group,
            policy,
            pending: None,
            retry_not_before: f64::NEG_INFINITY,
            invalidation_due: None,
            active: None,
            retry: RetryState::default(),
            #[cfg(feature = "http-plans")]
            retain_until_retired,
        });
        Ok(())
    }

    pub fn request(&mut self, key: &K, now: f64, demand: Demand) -> Result<(), Error> {
        time(now)?;
        let read = self
            .reads
            .iter_mut()
            .find(|read| &read.key == key)
            .ok_or(Error::UnknownKey)?;
        let due = after(
            now,
            match demand {
                Demand::Refresh => read.policy.refresh_debounce_ms,
                Demand::Invalidate => read.policy.debounce_ms,
            },
        );
        match demand {
            Demand::Invalidate => {
                // Retain the first notification even across raw-clock rollback.
                read.invalidation_due.get_or_insert(due);
            }
            Demand::Refresh if read.active.is_none() => {
                // A fresh read covers pending notification demand too. Neither
                // kind of demand may bypass a separate retry or pause floor.
                read.pending = Some(read.pending.map_or(due, |old| old.min(due)));
            }
            Demand::Refresh => {}
        }
        Ok(())
    }

    /// Extend one explicit budget's pause. Unsent reservations are invalidated;
    /// already issued ordinary reads retain their response deadlines. Task-bound
    /// reads retain cleanup custody and retire before replacement.
    pub fn pause(&mut self, group: &G, until: f64) -> Result<(), Error> {
        time(until)?;
        let index = self
            .groups
            .iter()
            .position(|entry| &entry.key == group)
            .ok_or(Error::UnknownGroup)?;
        if until <= self.groups[index].until {
            return Ok(());
        }
        self.groups[index].until = until;
        for read in &mut self.reads {
            if read.group != index {
                continue;
            }
            #[cfg(feature = "http-plans")]
            if read.retain_until_retired
                && let Some(active) = read.active.as_mut()
            {
                if active.state == ActiveState::Live {
                    active.state = ActiveState::Retiring(AfterRetirement::Pause);
                }
                continue;
            }
            if read.active.is_some_and(|active| !active.issued) {
                read.active = None;
                read.pending = Some(until);
                // The replacement has not been prepared, so it covers dirty
                // demand as well. No failed network attempt or jitter draw.
                read.invalidation_due = None;
            }
        }
        Ok(())
    }

    /// Earliest timer needed for pending admission or active attempt expiry.
    /// Call again after every state change; stale/early timer callbacks are safe.
    pub fn next_deadline(&self) -> Option<f64> {
        self.reads
            .iter()
            .filter_map(|read| {
                if let Some(active) = read.active {
                    active.live().then_some(active.deadline)
                } else {
                    read.due(self.groups[read.group].until)
                }
            })
            .min_by(f64::total_cmp)
    }

    /// Retire expired attempts and reserve the earliest due logical read.
    /// `jitter` uses the caller's existing entropy source, once per issued timeout.
    pub fn take_ready(
        &mut self,
        now: f64,
        mut jitter: impl FnMut(f64) -> f64,
    ) -> Result<Option<(&K, Attempt)>, Error> {
        time(now)?;
        // Preflight candidates before retiring an attempt, drawing entropy or
        // allocating a token. Idle/future/paused keys must not block other work.
        // Positive sub-ULP durations can otherwise expire immediately.
        for read in &self.reads {
            let eligible = self.groups[read.group].until <= now
                && if let Some(active) = read.active {
                    active.live() && active.deadline <= now
                } else {
                    read.due(self.groups[read.group].until)
                        .is_some_and(|due| due <= now)
                };
            if eligible {
                lease_deadline(now, read.policy.admission_timeout_ms)?;
            }
        }
        for read in &mut self.reads {
            if let Some(active) = read.active
                && active.live()
                && active.deadline <= now
            {
                #[cfg(feature = "http-plans")]
                if read.retain_until_retired {
                    let retry = active.issued && read.policy.response_expiry == ExpiryPolicy::Retry;
                    read.active.as_mut().expect("checked active token").state =
                        ActiveState::Retiring(AfterRetirement::Expired(retry));
                    continue;
                }
                read.active = None;
                let retry = active.issued && read.policy.response_expiry == ExpiryPolicy::Retry;
                let delay = if retry {
                    read.retry.failed_with(read.policy.retry, &mut jitter)
                } else {
                    0.0
                };
                read.pending = Some(after(now, delay));
                read.retry_not_before = if retry {
                    after(now, delay)
                } else {
                    f64::NEG_INFINITY
                };
                read.invalidation_due = None;
            }
        }
        let index = self
            .reads
            .iter()
            .enumerate()
            .filter_map(|(index, read)| {
                let due = read.due(self.groups[read.group].until)?;
                (read.active.is_none() && due <= now).then_some((index, due))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index);
        let Some(index) = index else {
            return Ok(None);
        };
        self.serial = self.serial.checked_add(1).ok_or(Error::TokenExhausted)?;
        // Rotate successful reservations behind their peers. With a zero
        // debounce and repeated same-time callbacks, a hot key must not win
        // every equal-deadline tie and starve another ready key.
        let read = self.reads.remove(index);
        self.reads.push(read);
        let read = self.reads.last_mut().expect("reserved registered read");
        let token = Attempt(self.serial);
        read.pending = None;
        read.invalidation_due = None;
        read.retry_not_before = f64::NEG_INFINITY;
        read.active = Some(Active {
            token,
            deadline: lease_deadline(now, read.policy.admission_timeout_ms)?,
            issued: false,
            #[cfg(feature = "http-plans")]
            state: ActiveState::Live,
        });
        Ok(Some((&read.key, token)))
    }

    /// Called only when the logical read actually leaves the host. Returns false
    /// for cancelled, expired or superseded reservations. Check before sending.
    pub fn mark_issued(&mut self, token: Attempt, now: f64) -> Result<bool, Error> {
        self.admit_inner(token, now, false)
            .map(|deadline| deadline.is_some())
    }

    /// Shared HTTP-release gate for the first request and all parallel legs or
    /// continuation pages of one logical attempt. Only the first release starts
    /// its response lease; later releases validate without renewing it. Pauses
    /// gate every new physical release, while existing responses may progress.
    /// The returned raw deadline must also be capped by the request lifecycle.
    pub fn admit(&mut self, token: Attempt, now: f64) -> Result<Option<f64>, Error> {
        self.admit_inner(token, now, true)
    }

    /// Typed final-release adapter. `None` from scheduler admission is a denial,
    /// never permission to send an unleased read. Arithmetic is checked before
    /// the existing admission path mutates its first-release state.
    pub fn admit_http(
        &mut self,
        token: Attempt,
        release: &crate::request_context::HttpRelease,
    ) -> Result<crate::request_context::Admission, Error> {
        use crate::request_context::{Admission, ResponseLease};
        match self.admit_inner(token, release.time.raw_ms, true)? {
            Some(deadline) => Ok(Admission::Scheduled(ResponseLease::from_scheduler(
                deadline,
            ))),
            None => Ok(Admission::Denied),
        }
    }

    fn admit_inner(
        &mut self,
        token: Attempt,
        now: f64,
        allow_issued: bool,
    ) -> Result<Option<f64>, Error> {
        time(now)?;
        for read in &mut self.reads {
            if let Some(active) = &mut read.active
                && active.token == token
            {
                if !active.live()
                    || (!allow_issued && active.issued)
                    || now >= active.deadline
                    || now < self.groups[read.group].until
                {
                    return Ok(None);
                }
                if !active.issued {
                    let deadline = lease_deadline(now, read.policy.response_timeout_ms)?;
                    active.issued = true;
                    active.deadline = deadline;
                }
                return Ok(Some(active.deadline));
            }
        }
        Ok(None)
    }

    /// Renew a page/leg budget only after validated progress from this issued
    /// logical read. Never call for polls, WS notifications or signing progress.
    pub fn progress(&mut self, token: Attempt, now: f64) -> Result<bool, Error> {
        time(now)?;
        for read in &mut self.reads {
            if let Some(active) = &mut read.active
                && active.token == token
            {
                if !active.live() || !active.issued || now >= active.deadline {
                    return Ok(false);
                }
                active.deadline = lease_deadline(now, read.policy.response_timeout_ms)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Finish the whole logical read, never an intermediate page or join leg.
    /// Failure also releases an unsent reservation (e.g. signing rejection).
    /// A late response after expiry cannot complete an old attempt.
    pub fn finish(
        &mut self,
        token: Attempt,
        now: f64,
        success: bool,
        jitter: impl FnOnce(f64) -> f64,
    ) -> Result<bool, Error> {
        time(now)?;
        let Some(read) = self
            .reads
            .iter_mut()
            .find(|read| read.active.is_some_and(|active| active.token == token))
        else {
            return Ok(false);
        };
        let active = read.active.expect("matching active attempt");
        if !active.live() || now >= active.deadline || (success && !active.issued) {
            return Ok(false);
        }
        #[cfg(feature = "http-plans")]
        if read.retain_until_retired {
            read.active.as_mut().expect("checked active token").state =
                ActiveState::Retiring(AfterRetirement::Finished(success));
            return Ok(true);
        }
        read.active = None;
        if success {
            read.retry.reset();
            // Keep the retained first notification until its next reservation,
            // so a later notification cannot restart it after completion.
        } else {
            let delay = read.retry.failed_with(read.policy.retry, jitter);
            read.pending = Some(after(now, delay));
            read.retry_not_before = after(now, delay);
            read.invalidation_due = None;
        }
        Ok(true)
    }

    /// A valid response was superseded by publication authority (for example an
    /// accepted write), not by a network failure. Retain retry history and one
    /// repair demand without drawing entropy or reporting authentication evidence.
    pub fn superseded(&mut self, token: Attempt, now: f64) -> Result<bool, Error> {
        time(now)?;
        let Some(read) = self.reads.iter_mut().find(|read| {
            read.active.is_some_and(|active| {
                active.token == token && active.live() && active.issued && now < active.deadline
            })
        }) else {
            return Ok(false);
        };
        #[cfg(feature = "http-plans")]
        if read.retain_until_retired {
            read.active.as_mut().expect("checked active token").state =
                ActiveState::Retiring(AfterRetirement::Superseded);
            return Ok(true);
        }
        read.active = None;
        read.invalidation_due
            .get_or_insert_with(|| after(now, read.policy.debounce_ms));
        Ok(true)
    }

    /// Cancel demand and invalidate callbacks without removing registration.
    pub fn cancel(&mut self, key: &K) -> Result<(), Error> {
        let read = self
            .reads
            .iter_mut()
            .find(|read| &read.key == key)
            .ok_or(Error::UnknownKey)?;
        #[cfg(feature = "http-plans")]
        if read.retain_until_retired
            && let Some(active) = &mut read.active
        {
            if active.state != ActiveState::Retiring(AfterRetirement::Remove) {
                active.state = ActiveState::Retiring(AfterRetirement::Cancel);
            }
            return Ok(());
        }
        read.active = None;
        read.pending = None;
        read.retry_not_before = f64::NEG_INFINITY;
        read.invalidation_due = None;
        read.retry.reset();
        Ok(())
    }

    pub fn retire(&mut self, key: &K) -> Result<(), Error> {
        let index = self
            .reads
            .iter()
            .position(|read| &read.key == key)
            .ok_or(Error::UnknownKey)?;
        #[cfg(feature = "http-plans")]
        if self.reads[index].retain_until_retired
            && let Some(active) = &mut self.reads[index].active
        {
            active.state = ActiveState::Retiring(AfterRetirement::Remove);
            return Ok(());
        }
        self.reads.remove(index);
        Ok(())
    }

    /// Exact task settlement after the Plan terminal proof. Expiry cannot
    /// release its replacement before the separate cleanup acknowledgement.
    #[cfg(feature = "http-plans")]
    pub(crate) fn finish_task(
        &mut self,
        token: Attempt,
        now: f64,
        success: bool,
    ) -> Result<bool, Error> {
        time(now)?;
        let Some(read) = self.reads.iter_mut().find(|read| {
            read.retain_until_retired
                && read
                    .active
                    .is_some_and(|active| active.token == token && active.issued)
        }) else {
            return Ok(false);
        };
        let active = read.active.as_mut().expect("exact task token");
        if active.live() {
            active.state = ActiveState::Retiring(AfterRetirement::Finished(success));
        }
        Ok(true)
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn retiring(&self, key: &K) -> Option<Attempt> {
        self.reads
            .iter()
            .find(|read| &read.key == key)?
            .active
            .filter(|active| matches!(active.state, ActiveState::Retiring(_)))
            .map(|active| active.token)
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn ack_retired(
        &mut self,
        token: Attempt,
        now: f64,
        jitter: impl FnOnce(f64) -> f64,
    ) -> Result<bool, Error> {
        time(now)?;
        let Some(index) = self.reads.iter().position(|read| {
            read.retain_until_retired
                && read.active.is_some_and(|active| {
                    active.token == token && matches!(active.state, ActiveState::Retiring(_))
                })
        }) else {
            return Ok(false);
        };
        let read = &mut self.reads[index];
        let ActiveState::Retiring(disposition) = read.active.expect("exact retiring token").state
        else {
            unreachable!()
        };
        read.active = None;
        match disposition {
            AfterRetirement::Remove => {
                self.reads.remove(index);
            }
            AfterRetirement::Cancel => {
                read.pending = None;
                read.retry_not_before = f64::NEG_INFINITY;
                read.invalidation_due = None;
                read.retry.reset();
            }
            AfterRetirement::Finished(true) => read.retry.reset(),
            AfterRetirement::Finished(false) | AfterRetirement::Expired(true) => {
                let delay = read.retry.failed_with(read.policy.retry, jitter);
                read.pending = Some(after(now, delay));
                read.retry_not_before = after(now, delay);
                read.invalidation_due = None;
            }
            AfterRetirement::Pause | AfterRetirement::Expired(false) => {
                read.pending = Some(now.max(self.groups[read.group].until));
                read.retry_not_before = f64::NEG_INFINITY;
                read.invalidation_due = None;
            }
            AfterRetirement::Superseded => {
                read.invalidation_due
                    .get_or_insert_with(|| after(now, read.policy.debounce_ms));
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy {
            debounce_ms: 10.0,
            refresh_debounce_ms: 10.0,
            admission_timeout_ms: 50.0,
            response_timeout_ms: 100.0,
            response_expiry: ExpiryPolicy::Retry,
            retry: RetryPolicy::default(),
        }
    }

    fn scheduler() -> Scheduler<&'static str, &'static str> {
        let mut scheduler = Scheduler::new(3, vec!["private", "public"]).unwrap();
        scheduler.register("orders", &"private", policy()).unwrap();
        scheduler
            .register("positions", &"private", policy())
            .unwrap();
        scheduler.register("markets", &"public", policy()).unwrap();
        scheduler
    }

    fn ready(scheduler: &mut Scheduler<&str, &str>, now: f64) -> Attempt {
        scheduler
            .take_ready(now, |_| panic!("unexpected jitter draw"))
            .unwrap()
            .unwrap()
            .1
    }

    fn immediate_policy() -> Policy {
        Policy {
            refresh_debounce_ms: 0.,
            response_expiry: ExpiryPolicy::ReplaceImmediately,
            ..policy()
        }
    }

    fn single(policy: Policy) -> Scheduler<&'static str, &'static str> {
        let mut scheduler = Scheduler::new(1, vec!["private"]).unwrap();
        scheduler.register("balance", &"private", policy).unwrap();
        scheduler
    }

    #[cfg(feature = "http-plans")]
    fn retained() -> Scheduler<&'static str, &'static str> {
        let mut scheduler = Scheduler::new(1, vec!["private"]).unwrap();
        let mut terms = policy();
        terms.refresh_debounce_ms = 0.;
        scheduler
            .register_task("orders", &"private", terms)
            .unwrap();
        scheduler.request(&"orders", 0., Demand::Refresh).unwrap();
        scheduler
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn retained_pause_cannot_replace_an_unsent_token_before_exact_cleanup_ack() {
        let mut scheduler = retained();
        let first = ready(&mut scheduler, 0.);
        scheduler.pause(&"private", 100.).unwrap();
        assert!(scheduler.retiring(&"orders") == Some(first));
        assert!(scheduler.next_deadline().is_none());
        assert!(
            scheduler
                .take_ready(200., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert!(scheduler.admit(first, 10.).unwrap().is_none());
        assert!(
            !scheduler
                .finish(first, 10., false, |_| unreachable!())
                .unwrap()
        );
        assert!(!scheduler.progress(first, 10.).unwrap());
        assert!(
            scheduler
                .ack_retired(first, 500., |_| unreachable!())
                .unwrap()
        );
        assert!(
            !scheduler
                .ack_retired(first, 500., |_| unreachable!())
                .unwrap()
        );
        let next = ready(&mut scheduler, 500.);
        assert!(next != first);
        assert!(!scheduler.mark_issued(first, 500.).unwrap());
        assert!(scheduler.mark_issued(next, 500.).unwrap());
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn retained_pause_retires_an_admitted_child_before_further_release() {
        let mut scheduler = retained();
        let first = ready(&mut scheduler, 0.);
        assert!(scheduler.mark_issued(first, 0.).unwrap());
        scheduler.pause(&"private", 100.).unwrap();
        assert!(scheduler.retiring(&"orders") == Some(first));
        assert!(scheduler.admit(first, 101.).unwrap().is_none());
        assert!(
            scheduler
                .take_ready(101., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert!(
            scheduler
                .ack_retired(first, 200., |_| unreachable!())
                .unwrap()
        );
        assert!(ready(&mut scheduler, 200.) != first);
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn retained_expiry_starts_backoff_after_cleanup_even_past_its_deadline() {
        let mut scheduler = retained();
        let first = ready(&mut scheduler, 0.);
        assert!(scheduler.mark_issued(first, 0.).unwrap());
        assert!(
            scheduler
                .take_ready(100., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert!(scheduler.retiring(&"orders") == Some(first));
        assert!(scheduler.next_deadline().is_none());
        assert!(
            !scheduler
                .finish(first, 101., true, |_| unreachable!())
                .unwrap()
        );
        assert!(scheduler.ack_retired(first, 150., |delay| delay).unwrap());
        assert!(scheduler.next_deadline() == Some(1_150.));
        assert!(
            scheduler
                .take_ready(1_149., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert!(ready(&mut scheduler, 1_150.) != first);
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn retained_cancel_and_removal_hold_the_registration_until_cleanup() {
        for remove in [false, true] {
            let mut scheduler = retained();
            let first = ready(&mut scheduler, 0.);
            assert!(scheduler.mark_issued(first, 0.).unwrap());
            if remove {
                scheduler.retire(&"orders").unwrap();
            } else {
                scheduler.cancel(&"orders").unwrap();
            }
            assert!(scheduler.retiring(&"orders") == Some(first));
            assert!(scheduler.register("new", &"private", policy()) == Err(Error::Capacity));
            assert!(
                scheduler
                    .take_ready(1_000., |_| unreachable!())
                    .unwrap()
                    .is_none()
            );
            assert!(
                scheduler
                    .ack_retired(first, 1_000., |_| unreachable!())
                    .unwrap()
            );
            assert!(scheduler.next_deadline().is_none());
            if remove {
                scheduler.register("new", &"private", policy()).unwrap();
            } else {
                scheduler
                    .request(&"orders", 1_000., Demand::Refresh)
                    .unwrap();
            }
        }
    }

    #[cfg(feature = "http-plans")]
    #[test]
    fn retained_completion_and_supersession_never_release_a_live_child_early() {
        for superseded in [false, true] {
            let mut scheduler = retained();
            let first = ready(&mut scheduler, 0.);
            assert!(scheduler.mark_issued(first, 0.).unwrap());
            if superseded {
                assert!(scheduler.superseded(first, 10.).unwrap());
            } else {
                assert!(
                    scheduler
                        .finish(first, 10., false, |_| unreachable!())
                        .unwrap()
                );
            }
            assert!(scheduler.retiring(&"orders") == Some(first));
            assert!(
                scheduler
                    .take_ready(200., |_| unreachable!())
                    .unwrap()
                    .is_none()
            );
            assert!(scheduler.ack_retired(first, 500., |delay| delay).unwrap());
            assert!(scheduler.next_deadline() == Some(if superseded { 510. } else { 1_500. }));
        }
    }

    #[test]
    fn immediate_refresh_covers_notification_without_restarting_first_debounce() {
        let mut scheduler = single(immediate_policy());
        scheduler
            .request(&"balance", 100., Demand::Invalidate)
            .unwrap();
        scheduler
            .request(&"balance", 90., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(110.));
        scheduler
            .request(&"balance", 105., Demand::Refresh)
            .unwrap();
        scheduler
            .request(&"balance", 90., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(105.));
        let first = ready(&mut scheduler, 105.);
        assert!(scheduler.mark_issued(first, 105.).unwrap());
        for now in 106..120 {
            scheduler
                .request(&"balance", f64::from(now), Demand::Refresh)
                .unwrap();
        }
        assert!(
            scheduler
                .finish(first, 120., true, |_| unreachable!())
                .unwrap()
        );
        assert_eq!(scheduler.next_deadline(), None);
    }

    #[test]
    fn first_notification_can_preempt_slow_refresh_but_repeats_retain_its_deadline() {
        let mut policy = immediate_policy();
        policy.refresh_debounce_ms = 100.;
        let mut scheduler = single(policy);
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(100.));
        scheduler
            .request(&"balance", 1., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(11.));
        scheduler
            .request(&"balance", 2., Demand::Invalidate)
            .unwrap();
        scheduler
            .request(&"balance", -100., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(11.));
        let token = ready(&mut scheduler, 11.);
        scheduler.mark_issued(token, 11.).unwrap();
        scheduler
            .request(&"balance", 12., Demand::Invalidate)
            .unwrap();
        scheduler
            .finish(token, 13., true, |_| unreachable!())
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(22.));
        scheduler
            .request(&"balance", -100., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(22.));
        let followup = ready(&mut scheduler, 22.);
        scheduler.mark_issued(followup, 22.).unwrap();
        scheduler
            .finish(followup, 23., true, |_| unreachable!())
            .unwrap();
        assert_eq!(
            scheduler.next_deadline(),
            None,
            "one fresh read covers both original demands"
        );
    }

    #[test]
    fn immediate_refresh_cannot_pull_retry_or_pause_floors_forward() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let first = ready(&mut scheduler, 0.);
        scheduler.mark_issued(first, 0.).unwrap();
        scheduler
            .finish(first, 1., false, |delay| delay / 2.)
            .unwrap();
        for now in 2..20 {
            scheduler
                .request(&"balance", f64::from(now), Demand::Refresh)
                .unwrap();
            scheduler
                .request(&"balance", f64::from(now), Demand::Invalidate)
                .unwrap();
        }
        assert_eq!(scheduler.next_deadline(), Some(501.));
        assert!(
            scheduler
                .take_ready(500., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        scheduler.pause(&"private", 700.).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(700.));
        assert!(
            scheduler
                .take_ready(699., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        let second = ready(&mut scheduler, 700.);
        scheduler.mark_issued(second, 700.).unwrap();
        scheduler
            .finish(second, 701., false, |delay| {
                assert_eq!(delay, 2000.);
                delay
            })
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(2701.));
        scheduler.cancel(&"balance").unwrap();
        scheduler
            .request(&"balance", 702., Demand::Refresh)
            .unwrap();
        assert!(
            scheduler
                .take_ready(702., |_| unreachable!())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn immediate_lost_read_replacement_keeps_failure_streak_without_entropy() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let first = ready(&mut scheduler, 0.);
        scheduler.mark_issued(first, 0.).unwrap();
        scheduler.finish(first, 1., false, |_| 0.).unwrap();
        let lost = ready(&mut scheduler, 1.);
        scheduler.mark_issued(lost, 1.).unwrap();
        scheduler
            .request(&"balance", 20., Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(101.));
        let replacement = ready(&mut scheduler, 101.);
        assert_ne!(lost, replacement);
        assert!(
            !scheduler
                .finish(lost, 101., false, |_| unreachable!())
                .unwrap()
        );
        assert!(!scheduler.superseded(lost, 101.).unwrap());
        scheduler.mark_issued(replacement, 101.).unwrap();
        scheduler
            .finish(replacement, 102., false, |delay| {
                assert_eq!(
                    delay, 2000.,
                    "expiry neither increments nor resets failures"
                );
                delay
            })
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(2102.));
    }

    #[test]
    fn immediate_expiry_replacement_waits_for_the_shared_pause() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let old = ready(&mut scheduler, 0.);
        scheduler.mark_issued(old, 0.).unwrap();
        scheduler.pause(&"private", 500.).unwrap();
        assert!(
            scheduler
                .take_ready(100., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert_eq!(scheduler.next_deadline(), Some(500.));
        scheduler
            .request(&"balance", 101., Demand::Refresh)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(500.));
        assert!(
            !scheduler
                .finish(old, 101., true, |_| unreachable!())
                .unwrap()
        );
        let replacement = ready(&mut scheduler, 500.);
        assert_ne!(old, replacement);
        assert!(scheduler.mark_issued(replacement, 500.).unwrap());
    }

    #[test]
    fn superseded_completion_preserves_first_dirty_deadline_and_failure_streak() {
        for (dirty, completed, expected) in [(true, 5., 12.), (true, 20., 12.), (false, 20., 30.)] {
            let mut scheduler = single(immediate_policy());
            scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
            let failed = ready(&mut scheduler, 0.);
            scheduler.mark_issued(failed, 0.).unwrap();
            scheduler.finish(failed, 1., false, |_| 0.).unwrap();
            let stale = ready(&mut scheduler, 1.);
            scheduler.mark_issued(stale, 1.).unwrap();
            if dirty {
                scheduler
                    .request(&"balance", 2., Demand::Invalidate)
                    .unwrap();
                scheduler
                    .request(&"balance", -100., Demand::Invalidate)
                    .unwrap();
            }
            assert!(scheduler.superseded(stale, completed).unwrap());
            assert_eq!(scheduler.next_deadline(), Some(expected));
            assert!(!scheduler.superseded(stale, completed).unwrap());
            assert!(
                !scheduler
                    .finish(stale, completed, false, |_| unreachable!())
                    .unwrap()
            );
            let next = ready(&mut scheduler, completed.max(expected));
            scheduler
                .mark_issued(next, completed.max(expected))
                .unwrap();
            scheduler
                .finish(next, completed.max(expected) + 1., false, |delay| {
                    assert_eq!(
                        delay, 2000.,
                        "neutral completion retains the previous failure"
                    );
                    delay
                })
                .unwrap();
        }
    }

    #[test]
    fn superseded_rejects_unsent_expired_replaced_and_cancelled_tokens() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let old = ready(&mut scheduler, 0.);
        assert!(!scheduler.superseded(old, 1.).unwrap());
        assert_eq!(scheduler.next_deadline(), Some(50.));
        scheduler.mark_issued(old, 1.).unwrap();
        assert_eq!(scheduler.superseded(old, f64::NAN), Err(Error::InvalidTime));
        assert!(!scheduler.superseded(old, 101.).unwrap());
        assert_eq!(scheduler.next_deadline(), Some(101.));
        let current = ready(&mut scheduler, 101.);
        assert!(!scheduler.superseded(old, 101.).unwrap());
        assert_eq!(scheduler.next_deadline(), Some(151.));
        scheduler.cancel(&"balance").unwrap();
        assert!(!scheduler.superseded(current, 102.).unwrap());
        assert_eq!(scheduler.next_deadline(), None);
    }

    #[test]
    fn superseded_and_cancelled_reads_cannot_erase_a_shared_pause() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let failed = ready(&mut scheduler, 0.);
        scheduler.mark_issued(failed, 0.).unwrap();
        scheduler.finish(failed, 1., false, |_| 0.).unwrap();
        let stale = ready(&mut scheduler, 1.);
        scheduler.mark_issued(stale, 1.).unwrap();
        scheduler
            .request(&"balance", 2., Demand::Invalidate)
            .unwrap();
        scheduler.pause(&"private", 50.).unwrap();
        assert!(scheduler.superseded(stale, 5.).unwrap());
        scheduler.request(&"balance", 6., Demand::Refresh).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(50.));
        assert!(
            scheduler
                .take_ready(49., |_| unreachable!())
                .unwrap()
                .is_none()
        );
        let next = ready(&mut scheduler, 50.);
        scheduler.mark_issued(next, 50.).unwrap();
        scheduler
            .finish(next, 51., false, |delay| {
                assert_eq!(delay, 2000.);
                delay
            })
            .unwrap();
        scheduler.pause(&"private", 3000.).unwrap();
        scheduler.cancel(&"balance").unwrap();
        scheduler.request(&"balance", 52., Demand::Refresh).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(3000.));
        let fresh = ready(&mut scheduler, 3000.);
        scheduler.mark_issued(fresh, 3000.).unwrap();
        scheduler
            .finish(fresh, 3001., false, |delay| {
                assert_eq!(delay, 1000., "cancel resets the key, not its shared pause");
                delay
            })
            .unwrap();
    }

    #[test]
    fn physical_release_admission_is_idempotent_without_renewing_logical_lease() {
        let mut scheduler = single(immediate_policy());
        scheduler.request(&"balance", 0., Demand::Refresh).unwrap();
        let token = ready(&mut scheduler, 0.);
        assert_eq!(scheduler.admit(token, 20.).unwrap(), Some(120.));
        assert_eq!(scheduler.admit(token, 30.).unwrap(), Some(120.));
        assert!(!scheduler.mark_issued(token, 30.).unwrap());
        scheduler.pause(&"private", 60.).unwrap();
        assert_eq!(scheduler.admit(token, 40.).unwrap(), None);
        assert_eq!(scheduler.next_deadline(), Some(120.));
        assert!(scheduler.progress(token, 50.).unwrap());
        assert_eq!(scheduler.admit(token, 59.).unwrap(), None);
        assert_eq!(scheduler.admit(token, 60.).unwrap(), Some(150.));
        assert_eq!(
            scheduler.admit(token, f64::INFINITY),
            Err(Error::InvalidTime)
        );
        assert_eq!(scheduler.next_deadline(), Some(150.));
        assert_eq!(scheduler.admit(token, 150.).unwrap(), None);
        scheduler.cancel(&"balance").unwrap();
        assert_eq!(scheduler.admit(token, 151.).unwrap(), None);
        scheduler
            .request(&"balance", 200., Demand::Refresh)
            .unwrap();
        let unsent = ready(&mut scheduler, 200.);
        scheduler.pause(&"private", 250.).unwrap();
        assert_eq!(scheduler.admit(unsent, 210.).unwrap(), None);
        assert_eq!(scheduler.next_deadline(), Some(250.));
    }

    #[test]
    fn retry_floor_preflight_does_not_let_an_ineligible_key_block_a_peer() {
        let big = 9_007_199_254_740_992.;
        let mut scheduler = Scheduler::new(2, vec!["private"]).unwrap();
        let mut tiny = immediate_policy();
        tiny.admission_timeout_ms = 1.;
        scheduler.register("tiny", &"private", tiny).unwrap();
        scheduler
            .register("healthy", &"private", immediate_policy())
            .unwrap();
        scheduler
            .request(&"tiny", big - 1., Demand::Refresh)
            .unwrap();
        let token = ready(&mut scheduler, big - 1.);
        scheduler.mark_issued(token, big - 1.).unwrap();
        scheduler
            .finish(token, big - 1., false, |delay| delay)
            .unwrap();
        scheduler.request(&"tiny", big, Demand::Refresh).unwrap();
        scheduler.request(&"healthy", big, Demand::Refresh).unwrap();
        let (key, _) = scheduler
            .take_ready(big, |_| unreachable!())
            .unwrap()
            .unwrap();
        assert_eq!(*key, "healthy");
    }

    #[test]
    fn invalid_refresh_debounce_is_rejected_before_registration() {
        for delay in [-1., f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut scheduler = Scheduler::new(1, vec!["private"]).unwrap();
            let mut invalid = immediate_policy();
            invalid.refresh_debounce_ms = delay;
            assert_eq!(
                scheduler.register("balance", &"private", invalid),
                Err(Error::InvalidPolicy)
            );
            assert!(scheduler.next_deadline().is_none());
            assert!(
                scheduler
                    .register("balance", &"private", immediate_policy())
                    .is_ok()
            );
        }
    }

    #[test]
    fn notifications_retain_first_debounce_and_polls_do_not_dirty_active_reads() {
        let mut scheduler = scheduler();
        for now in 0..10 {
            scheduler
                .request(&"orders", f64::from(now), Demand::Invalidate)
                .unwrap();
        }
        assert_eq!(scheduler.next_deadline(), Some(10.0));
        assert!(
            scheduler
                .take_ready(9.0, |_| unreachable!())
                .unwrap()
                .is_none()
        );
        let token = ready(&mut scheduler, 10.0);
        assert!(scheduler.mark_issued(token, 10.0).unwrap());
        for now in 11..30 {
            scheduler
                .request(&"orders", f64::from(now), Demand::Refresh)
                .unwrap();
        }
        assert_eq!(scheduler.next_deadline(), Some(110.0));
        assert!(
            scheduler
                .finish(token, 30.0, true, |_| unreachable!())
                .unwrap()
        );
        assert_eq!(scheduler.next_deadline(), None);
    }

    #[test]
    fn many_invalidations_produce_one_followup_and_do_not_extend_lease() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let token = ready(&mut scheduler, 10.0);
        assert!(scheduler.mark_issued(token, 10.0).unwrap());
        for now in 20..100 {
            scheduler
                .request(&"orders", f64::from(now), Demand::Invalidate)
                .unwrap();
        }
        assert_eq!(scheduler.next_deadline(), Some(110.0));
        assert!(
            scheduler
                .finish(token, 100.0, true, |_| unreachable!())
                .unwrap()
        );
        assert_eq!(scheduler.next_deadline(), Some(30.0));
        let next = ready(&mut scheduler, 100.0);
        assert_ne!(token, next);
        assert!(scheduler.mark_issued(next, 100.0).unwrap());
        assert!(
            scheduler
                .finish(next, 101.0, true, |_| unreachable!())
                .unwrap()
        );
        assert_eq!(scheduler.next_deadline(), None);
    }

    #[test]
    fn unsent_reservation_expires_without_response_lease_or_retry_entropy() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let old = ready(&mut scheduler, 10.0);
        assert_eq!(scheduler.next_deadline(), Some(60.0));
        assert!(!scheduler.mark_issued(old, 60.0).unwrap());
        let replacement = ready(&mut scheduler, 60.0);
        assert!(!scheduler.mark_issued(old, 61.0).unwrap());
        assert!(scheduler.mark_issued(replacement, 100.0).unwrap());
        assert_eq!(scheduler.next_deadline(), Some(200.0));
        assert!(!scheduler.mark_issued(replacement, 101.0).unwrap());
        assert!(
            !scheduler
                .finish(old, 101.0, false, |_| unreachable!())
                .unwrap()
        );
    }

    #[test]
    fn lost_read_retries_and_late_success_or_failure_cannot_release_replacement() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let old = ready(&mut scheduler, 10.0);
        scheduler.mark_issued(old, 10.0).unwrap();
        let mut draws = Vec::new();
        assert!(
            scheduler
                .take_ready(110.0, |delay| {
                    draws.push(delay);
                    delay
                })
                .unwrap()
                .is_none()
        );
        assert_eq!(draws, [1_000.0]);
        assert_eq!(scheduler.next_deadline(), Some(1_110.0));
        let replacement = ready(&mut scheduler, 1_110.0);
        scheduler.mark_issued(replacement, 1_110.0).unwrap();
        for success in [true, false] {
            assert!(
                !scheduler
                    .finish(old, 1_111.0, success, |_| unreachable!())
                    .unwrap()
            );
        }
        assert_eq!(scheduler.next_deadline(), Some(1_210.0));
        assert!(
            scheduler
                .finish(replacement, 1_112.0, true, |_| unreachable!())
                .unwrap()
        );
    }

    #[test]
    fn matching_page_progress_renews_only_issued_live_logical_read() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let token = ready(&mut scheduler, 10.0);
        assert!(!scheduler.progress(token, 11.0).unwrap());
        scheduler.mark_issued(token, 20.0).unwrap();
        assert!(scheduler.progress(token, 119.0).unwrap());
        assert_eq!(scheduler.next_deadline(), Some(219.0));
        assert!(!scheduler.progress(token, 219.0).unwrap());
        assert!(
            !scheduler
                .finish(token, 219.0, true, |_| unreachable!())
                .unwrap()
        );
        scheduler.take_ready(219.0, |delay| delay).unwrap();
        assert!(!scheduler.progress(token, 220.0).unwrap());
    }

    #[test]
    fn scoped_pause_gates_fresh_retry_and_dirty_admission_and_revokes_unsent() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let unsent = ready(&mut scheduler, 10.0);
        scheduler
            .request(&"positions", 10.0, Demand::Refresh)
            .unwrap();
        scheduler.pause(&"private", 500.0).unwrap();
        scheduler.pause(&"private", 400.0).unwrap();
        assert!(!scheduler.mark_issued(unsent, 11.0).unwrap());
        scheduler
            .request(&"markets", 10.0, Demand::Refresh)
            .unwrap();
        let public = ready(&mut scheduler, 20.0);
        scheduler.mark_issued(public, 20.0).unwrap();
        scheduler
            .finish(public, 21.0, true, |_| unreachable!())
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(500.0));
        assert!(
            scheduler
                .take_ready(499.0, |_| unreachable!())
                .unwrap()
                .is_none()
        );
        let first = ready(&mut scheduler, 500.0);
        scheduler.mark_issued(first, 500.0).unwrap();
        scheduler
            .request(&"positions", 501.0, Demand::Invalidate)
            .unwrap();
        let second = ready(&mut scheduler, 501.0);
        scheduler.mark_issued(second, 501.0).unwrap();
        scheduler
            .request(&"orders", 502.0, Demand::Invalidate)
            .unwrap();
        scheduler
            .finish(first, 503.0, true, |_| unreachable!())
            .unwrap();
        scheduler
            .finish(second, 503.0, false, |delay| delay)
            .unwrap();
        scheduler.pause(&"private", 2_000.0).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(2_000.0));
        assert!(
            scheduler
                .take_ready(1_999.0, |_| unreachable!())
                .unwrap()
                .is_none()
        );
        assert!(
            scheduler
                .take_ready(2_000.0, |_| unreachable!())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn retries_share_startup_sequence_draw_once_and_reset_independently() {
        let mut retry = RetryState::default();
        let mut draws = Vec::new();
        for expected in [
            1_000.0, 2_000.0, 4_000.0, 8_000.0, 16_000.0, 30_000.0, 30_000.0,
        ] {
            assert_eq!(
                retry.failed(|delay| {
                    draws.push(delay);
                    delay * 0.75
                }),
                expected * 0.75
            );
        }
        assert_eq!(draws.len(), 7);
        assert_eq!(retry.failures(), 7);
        retry.reset();
        assert_eq!(retry.failed(|delay| delay), 1_000.0);

        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let first = ready(&mut scheduler, 10.0);
        // Synchronous signing failure releases the reservation with one retry.
        assert!(scheduler.finish(first, 11.0, false, |delay| delay).unwrap());
        scheduler
            .request(&"orders", 12.0, Demand::Invalidate)
            .unwrap();
        assert_eq!(scheduler.next_deadline(), Some(1_011.0));
        let second = ready(&mut scheduler, 1_011.0);
        scheduler.mark_issued(second, 1_011.0).unwrap();
        scheduler
            .finish(second, 1_012.0, true, |_| unreachable!())
            .unwrap();
        scheduler
            .request(&"orders", 1_013.0, Demand::Refresh)
            .unwrap();
        let third = ready(&mut scheduler, 1_023.0);
        scheduler
            .finish(third, 1_024.0, false, |delay| {
                assert_eq!(delay, 1_000.0);
                delay
            })
            .unwrap();
    }

    #[test]
    fn capacity_cancel_retire_and_key_reuse_do_not_evict_or_reuse_tokens() {
        let mut scheduler = scheduler();
        assert_eq!(
            scheduler.register("fees", &"private", policy()),
            Err(Error::Capacity)
        );
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        let old = ready(&mut scheduler, 10.0);
        scheduler.cancel(&"orders").unwrap();
        scheduler.request(&"orders", 11.0, Demand::Refresh).unwrap();
        let replacement = ready(&mut scheduler, 21.0);
        assert_ne!(old, replacement);
        assert!(!scheduler.mark_issued(old, 21.0).unwrap());
        scheduler.pause(&"private", 500.0).unwrap();
        scheduler.retire(&"orders").unwrap();
        scheduler.retire(&"positions").unwrap();
        scheduler.register("orders", &"private", policy()).unwrap();
        scheduler.request(&"orders", 30.0, Demand::Refresh).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(500.0));
        let final_token = ready(&mut scheduler, 500.0);
        assert_ne!(replacement, final_token);
        assert!(
            !scheduler
                .finish(replacement, 501.0, false, |_| unreachable!())
                .unwrap()
        );
    }

    #[test]
    fn equal_deadlines_keep_registration_order_and_instances_are_independent() {
        let mut scheduler = scheduler();
        for key in ["markets", "positions", "orders"] {
            scheduler.request(&key, -100.0, Demand::Refresh).unwrap();
        }
        for expected in ["orders", "positions", "markets"] {
            let (key, token) = scheduler
                .take_ready(-90.0, |_| unreachable!())
                .unwrap()
                .unwrap();
            assert_eq!(*key, expected);
            scheduler.mark_issued(token, -90.0).unwrap();
        }
        let mut other = super::tests::scheduler();
        other.request(&"orders", -100.0, Demand::Refresh).unwrap();
        other.pause(&"private", 1_000.0).unwrap();
        assert_eq!(scheduler.next_deadline(), Some(10.0));
        assert_eq!(other.next_deadline(), Some(1_000.0));
    }

    #[test]
    fn invalid_inputs_and_counter_exhaustion_leave_work_owned() {
        let mut scheduler = scheduler();
        scheduler.request(&"orders", 0.0, Demand::Refresh).unwrap();
        for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                scheduler.request(&"orders", invalid, Demand::Invalidate),
                Err(Error::InvalidTime)
            );
            assert_eq!(
                scheduler.pause(&"private", invalid),
                Err(Error::InvalidTime)
            );
            assert_eq!(
                scheduler.take_ready(invalid, |_| unreachable!()),
                Err(Error::InvalidTime)
            );
        }
        assert_eq!(scheduler.next_deadline(), Some(10.0));
        scheduler.serial = u64::MAX;
        assert_eq!(
            scheduler.take_ready(10.0, |_| unreachable!()),
            Err(Error::TokenExhausted)
        );
        assert_eq!(scheduler.next_deadline(), Some(10.0));
        let mut invalid = policy();
        invalid.response_timeout_ms = 0.0;
        assert_eq!(
            scheduler.register("invalid", &"private", invalid),
            Err(Error::InvalidPolicy)
        );
        assert_eq!(
            scheduler.request(&"unknown", 0.0, Demand::Refresh),
            Err(Error::UnknownKey)
        );
    }

    #[test]
    fn unrepresentable_admission_leases_fail_without_state_or_entropy_changes() {
        for (now, duration) in [(1_760_000_000_000.0, 1e-9), (f64::MAX, 50.0)] {
            let mut scheduler = scheduler();
            scheduler.reads[0].policy.admission_timeout_ms = duration;
            scheduler
                .request(&"orders", now - 10.0, Demand::Refresh)
                .unwrap();
            let pending = scheduler.next_deadline();
            for _ in 0..2 {
                assert_eq!(
                    scheduler.take_ready(now, |_| unreachable!()),
                    Err(Error::InvalidTime)
                );
                assert_eq!(scheduler.serial, 0);
                assert_eq!(scheduler.next_deadline(), pending);
                assert!(scheduler.reads[0].active.is_none());
            }
        }
    }

    #[test]
    fn unrepresentable_response_lease_keeps_the_unsent_reservation_unchanged() {
        let mut scheduler = scheduler();
        let now = 1_760_000_000_000.0;
        scheduler.reads[0].policy.response_timeout_ms = 1e-9;
        scheduler
            .request(&"orders", now - 10.0, Demand::Refresh)
            .unwrap();
        let token = ready(&mut scheduler, now);
        let deadline = scheduler.next_deadline();
        assert_eq!(scheduler.mark_issued(token, now), Err(Error::InvalidTime));
        assert_eq!(scheduler.next_deadline(), deadline);
        let read = scheduler
            .reads
            .iter_mut()
            .find(|read| read.key == "orders")
            .unwrap();
        assert!(!read.active.unwrap().issued);
        read.policy.response_timeout_ms = 100.0;
        assert!(scheduler.mark_issued(token, now).unwrap());
    }

    #[test]
    fn idle_future_and_paused_keys_cannot_block_a_healthy_ready_peer() {
        let now = 1_760_000_000_000.0;
        for demand in [None, Some(now + 100.0), Some(now - 10.0)] {
            let mut scheduler = scheduler();
            scheduler.reads[2].policy.admission_timeout_ms = 1e-9;
            if let Some(at) = demand {
                scheduler.request(&"markets", at, Demand::Refresh).unwrap();
                if at < now {
                    scheduler.pause(&"public", now + 100.0).unwrap();
                }
            }
            scheduler
                .request(&"positions", now - 10.0, Demand::Refresh)
                .unwrap();
            let (key, token) = scheduler
                .take_ready(now, |_| unreachable!())
                .unwrap()
                .unwrap();
            assert_eq!(*key, "positions");
            assert!(scheduler.mark_issued(token, now).unwrap());
        }
    }

    #[test]
    fn repeated_same_time_dirty_reads_cannot_starve_an_equal_deadline_peer() {
        let mut scheduler = scheduler();
        for read in &mut scheduler.reads {
            read.policy.debounce_ms = 0.0;
            read.policy.refresh_debounce_ms = 0.0;
        }
        for key in ["orders", "positions"] {
            scheduler.request(&key, 0.0, Demand::Refresh).unwrap();
        }
        let first = ready(&mut scheduler, 0.0);
        scheduler.mark_issued(first, 0.0).unwrap();
        scheduler
            .request(&"orders", 0.0, Demand::Invalidate)
            .unwrap();
        scheduler
            .finish(first, 0.0, true, |_| unreachable!())
            .unwrap();
        let (key, second) = scheduler
            .take_ready(0.0, |_| unreachable!())
            .unwrap()
            .unwrap();
        assert_eq!(
            *key, "positions",
            "hot orders may not win every equal-time tie"
        );
        scheduler.mark_issued(second, 0.0).unwrap();
        scheduler
            .finish(second, 0.0, true, |_| unreachable!())
            .unwrap();
        let (key, _) = scheduler
            .take_ready(0.0, |_| unreachable!())
            .unwrap()
            .unwrap();
        assert_eq!(*key, "orders", "the coalesced repair is retained");
    }
}

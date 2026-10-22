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

#[derive(Clone, Copy, Debug)]
pub struct Policy {
    pub debounce_ms: f64,
    /// Bound on a ready reservation which has not reached the network.
    pub admission_timeout_ms: f64,
    /// Bound for the complete logical read, anchored at actual issue.
    pub response_timeout_ms: f64,
    pub retry: RetryPolicy,
}
impl Policy {
    fn valid(self) -> bool {
        self.debounce_ms.is_finite()
            && self.debounce_ms >= 0.0
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
}

struct Read<K> {
    key: K,
    group: usize,
    policy: Policy,
    pending: Option<f64>,
    dirty: Option<f64>,
    active: Option<Active>,
    retry: RetryState,
}

struct Group<G> {
    key: G,
    until: f64,
}

/// Bounded registry; stable registration order breaks equal-deadline ties.
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
        self.reads.push(Read {
            key,
            group,
            policy,
            pending: None,
            dirty: None,
            active: None,
            retry: RetryState::default(),
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
        let due = after(now, read.policy.debounce_ms);
        if read.active.is_some() {
            if demand == Demand::Invalidate {
                read.dirty.get_or_insert(due);
            }
        } else {
            // A fresh notification must not move a retry earlier or postpone a
            // repair. Its pending read will already cover the new demand.
            read.pending.get_or_insert(due);
        }
        Ok(())
    }

    /// Extend one explicit budget's pause. Unsent reservations are invalidated;
    /// already issued reads retain their original response deadlines.
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
            if read.group == index && read.active.is_some_and(|active| !active.issued) {
                read.active = None;
                read.pending = Some(until);
                // The replacement has not been prepared, so it covers dirty
                // demand as well. No failed network attempt or jitter draw.
                read.dirty = None;
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
                    Some(active.deadline)
                } else {
                    read.pending
                        .map(|due| due.max(self.groups[read.group].until))
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
                    active.deadline <= now
                } else {
                    read.pending.is_some_and(|due| due <= now)
                };
            if eligible {
                lease_deadline(now, read.policy.admission_timeout_ms)?;
            }
        }
        for read in &mut self.reads {
            if let Some(active) = read.active
                && active.deadline <= now
            {
                read.active = None;
                let delay = if active.issued {
                    read.retry.failed_with(read.policy.retry, &mut jitter)
                } else {
                    0.0
                };
                read.pending = Some(after(now, delay));
                read.dirty = None;
            }
        }
        let index = self
            .reads
            .iter()
            .enumerate()
            .filter_map(|(index, read)| {
                let due = read.pending?.max(self.groups[read.group].until);
                (read.active.is_none() && due <= now).then_some((index, due))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(index, _)| index);
        let Some(index) = index else {
            return Ok(None);
        };
        self.serial = self.serial.checked_add(1).ok_or(Error::TokenExhausted)?;
        let read = &mut self.reads[index];
        let token = Attempt(self.serial);
        read.pending = None;
        read.active = Some(Active {
            token,
            deadline: lease_deadline(now, read.policy.admission_timeout_ms)?,
            issued: false,
        });
        Ok(Some((&read.key, token)))
    }

    /// Called only when the logical read actually leaves the host. Returns false
    /// for cancelled, expired or superseded reservations. Check before sending.
    pub fn mark_issued(&mut self, token: Attempt, now: f64) -> Result<bool, Error> {
        time(now)?;
        for read in &mut self.reads {
            if let Some(active) = &mut read.active
                && active.token == token
            {
                if active.issued || now >= active.deadline || now < self.groups[read.group].until {
                    return Ok(false);
                }
                let deadline = lease_deadline(now, read.policy.response_timeout_ms)?;
                active.issued = true;
                active.deadline = deadline;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Renew a page/leg budget only after validated progress from this issued
    /// logical read. Never call for polls, WS notifications or signing progress.
    pub fn progress(&mut self, token: Attempt, now: f64) -> Result<bool, Error> {
        time(now)?;
        for read in &mut self.reads {
            if let Some(active) = &mut read.active
                && active.token == token
            {
                if !active.issued || now >= active.deadline {
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
        if now >= active.deadline || (success && !active.issued) {
            return Ok(false);
        }
        read.active = None;
        if success {
            read.retry.reset();
            read.pending = read.dirty.take();
        } else {
            let delay = read.retry.failed_with(read.policy.retry, jitter);
            read.pending = Some(after(now, delay));
            read.dirty = None;
        }
        Ok(true)
    }

    /// Cancel demand and invalidate callbacks without removing registration.
    pub fn cancel(&mut self, key: &K) -> Result<(), Error> {
        let read = self
            .reads
            .iter_mut()
            .find(|read| &read.key == key)
            .ok_or(Error::UnknownKey)?;
        read.active = None;
        read.pending = None;
        read.dirty = None;
        read.retry.reset();
        Ok(())
    }

    pub fn retire(&mut self, key: &K) -> Result<(), Error> {
        let index = self
            .reads
            .iter()
            .position(|read| &read.key == key)
            .ok_or(Error::UnknownKey)?;
        self.reads.remove(index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        Policy {
            debounce_ms: 10.0,
            admission_timeout_ms: 50.0,
            response_timeout_ms: 100.0,
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
        assert!(!scheduler.reads[0].active.unwrap().issued);
        scheduler.reads[0].policy.response_timeout_ms = 100.0;
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
}

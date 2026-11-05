//! Router ownership assertions and generation-bound account IO authority.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

pub const DEFAULT_LEASE_MS: u64 = 90_000;
pub const CLOCK_SKEW_MS: u64 = 5_000;
pub const MAX_USERS: usize = 256;
const MAX_TRACKED_USERS: usize = 4096;
const MAX_SAFE_INTEGER: u64 = (1 << 53) - 1;

#[derive(Clone, Debug)]
pub struct Config {
    pub box_id: Option<String>,
    pub display_name: Option<String>,
    pub lease_ms: u64,
    pub enforce: bool,
    pub enable_grace_until: Option<u64>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            box_id: None,
            display_name: None,
            lease_ms: DEFAULT_LEASE_MS,
            enforce: false,
            enable_grace_until: None,
        }
    }
}

impl Config {
    pub fn parse(
        box_id: Option<String>,
        display_name: Option<String>,
        lease_ms: u64,
        enforce: Option<&str>,
        enable_grace_until: Option<&str>,
    ) -> Result<Self, String> {
        let enforce = match enforce {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err("TEALSTREET_ROUTER_LEASE_ENFORCE must be 0 or 1".into()),
        };
        let enable_grace_until = enable_grace_until
            .map(|value| {
                value.parse::<u64>().ok()
                    .filter(|parsed| value.bytes().all(|b| b.is_ascii_digit()) && *parsed <= MAX_SAFE_INTEGER)
                    .ok_or_else(|| "TEALSTREET_ROUTER_LEASE_ENABLE_GRACE_UNTIL must be a JS-safe Unix-millisecond integer".to_owned())
            }).transpose()?;
        let box_id = box_id.filter(|id| {
            !id.trim().is_empty() && id.len() <= 128 && !id.chars().any(char::is_control)
        });
        if enforce && box_id.is_none() {
            return Err(
                "TEALSTREET_ROUTER_BOX_ID is required when TEALSTREET_ROUTER_LEASE_ENFORCE=1"
                    .into(),
            );
        }
        Ok(Self {
            box_id,
            display_name,
            lease_ms,
            enforce,
            enable_grace_until,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Heartbeat {
    pub r#box: String,
    pub issued_at: u64,
    pub users: Vec<Assertion>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Assertion {
    pub user_id: String,
    pub owner_epoch: u64,
    pub sequence: u64,
    pub not_after: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ack {
    pub accepted: usize,
    pub checked_at: u64,
    pub enforcing: bool,
    pub grace_until: Option<u64>,
    pub boot_id: &'static str,
}

struct Grace {
    until: u64,
    deadline: Instant,
}
struct Policy {
    enforce: bool,
    grace: Option<Grace>,
}
impl Policy {
    fn grace_until(&mut self, now: u64) -> Option<u64> {
        if self
            .grace
            .as_ref()
            .is_some_and(|grace| now > grace.until || Instant::now() > grace.deadline)
        {
            self.grace = None;
        }
        self.grace.as_ref().map(|grace| grace.until)
    }
}
struct State {
    accepted: Option<(u64, u64)>,
    not_after: Option<u64>,
    deadline: Option<Instant>,
    generation: u64,
    retired_at: Option<Instant>,
    fenced: bool,
    permit_issued: bool,
}

pub struct User {
    policy: Arc<Mutex<Policy>>,
    state: Mutex<State>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}

#[derive(Clone)]
pub struct Permit {
    user: Arc<User>,
    generation: u64,
}
impl Permit {
    pub fn restore_generation(&self) -> Option<u64> {
        self.user.restore_generation()
    }
    pub fn allowed(&self) -> bool {
        self.user.allowed(self.generation)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub enforced: bool,
    pub would_fence: bool,
    pub fenced: bool,
    pub owner_epoch: Option<u64>,
    pub sequence: Option<u64>,
    pub not_after: Option<u64>,
    pub generation: u64,
    pub missing_heartbeat_grace_ms: u64,
}

impl User {
    fn expired(&self, state: &State) -> bool {
        state.not_after.is_none_or(|until| (self.clock)() > until)
            || state.deadline.is_none_or(|until| Instant::now() > until)
    }
    fn observe(&self, policy: &mut Policy, state: &mut State) {
        let missing_grace =
            state.accepted.is_none() && policy.grace_until((self.clock)()).is_some();
        if policy.enforce && !missing_grace && self.expired(state) && !state.fenced {
            state.fenced = true;
            state.generation = state.generation.saturating_add(1);
            state.retired_at = Some(Instant::now());
        }
    }
    pub fn permit(self: &Arc<Self>) -> Permit {
        let mut policy = self.policy.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        self.observe(&mut policy, &mut state);
        state.permit_issued = true;
        Permit {
            user: self.clone(),
            generation: state.generation,
        }
    }
    fn allowed(&self, generation: u64) -> bool {
        let mut policy = self.policy.lock().unwrap();
        if !policy.enforce {
            return true;
        }
        let mut state = self.state.lock().unwrap();
        self.observe(&mut policy, &mut state);
        !state.fenced && state.generation == generation
    }
    fn restore_generation(&self) -> Option<u64> {
        let policy = self.policy.lock().unwrap();
        let state = self.state.lock().unwrap();
        (policy.enforce
            && state
                .retired_at
                .is_some_and(|at| at.elapsed() < crate::interruption::RETENTION))
        .then_some(state.generation)
    }
    pub fn status(&self) -> Status {
        let mut policy = self.policy.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        self.observe(&mut policy, &mut state);
        Status {
            enforced: policy.enforce,
            would_fence: self.expired(&state),
            fenced: policy.enforce && state.fenced,
            owner_epoch: state.accepted.map(|v| v.0),
            sequence: state.accepted.map(|v| v.1),
            not_after: state.not_after,
            generation: state.generation,
            missing_heartbeat_grace_ms: if policy.enforce && state.accepted.is_none() {
                policy.grace.as_ref().map_or(0, |grace| {
                    grace.until.saturating_sub((self.clock)()).min(
                        grace
                            .deadline
                            .saturating_duration_since(Instant::now())
                            .as_millis() as u64,
                    )
                })
            } else {
                0
            },
        }
    }
}

pub struct Users {
    config: Config,
    boot_id: &'static str,
    policy: Arc<Mutex<Policy>>,
    entries: Mutex<HashMap<String, Arc<User>>>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}
impl Users {
    pub fn new(config: Config) -> Self {
        Self::with_clock(config, Arc::new(crate::accounts::now_ms))
    }
    fn with_clock(config: Config, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        let now = clock();
        let grace = config
            .enable_grace_until
            .filter(|until| *until > now)
            .and_then(|until| {
                Instant::now()
                    .checked_add(Duration::from_millis(until - now))
                    .map(|deadline| Grace { until, deadline })
            });
        Self {
            boot_id: crate::state::boot_id(),
            policy: Arc::new(Mutex::new(Policy {
                enforce: config.enforce,
                grace,
            })),
            config,
            entries: Mutex::default(),
            clock,
        }
    }
    pub fn boot_id(&self) -> &'static str {
        self.boot_id
    }
    pub fn box_id(&self) -> Option<&str> {
        self.config.box_id.as_deref()
    }
    pub fn enforced(&self) -> bool {
        self.policy.lock().unwrap().enforce
    }
    pub fn get(&self, user: &str) -> Option<Arc<User>> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(user) {
            return Some(entry.clone());
        }
        // Epoch high-water marks are never evicted while this process can accept assertions.
        if entries.len() >= MAX_TRACKED_USERS {
            return None;
        }
        let entry = Arc::new(User {
            policy: self.policy.clone(),
            state: Mutex::new(State {
                accepted: None,
                not_after: None,
                deadline: None,
                generation: 0,
                retired_at: None,
                fenced: false,
                permit_issued: false,
            }),
            clock: self.clock.clone(),
        });
        entries.insert(user.to_owned(), entry.clone());
        Some(entry)
    }
    pub fn accept(&self, heartbeat: &Heartbeat) -> Result<Ack, &'static str> {
        let now = (self.clock)();
        if self.config.box_id.as_deref() != Some(&heartbeat.r#box)
            || heartbeat.r#box.is_empty()
            || now.abs_diff(heartbeat.issued_at) > CLOCK_SKEW_MS
            || heartbeat.users.is_empty()
            || heartbeat.users.len() > MAX_USERS
        {
            return Err("invalid router heartbeat");
        }
        let mut names = std::collections::HashSet::new();
        for assertion in &heartbeat.users {
            if assertion.user_id.is_empty()
                || assertion.user_id.len() > 512
                || assertion.user_id.chars().any(char::is_control)
                || !names.insert(&assertion.user_id)
                || assertion.owner_epoch == 0
                || assertion.owner_epoch > MAX_SAFE_INTEGER
                || assertion.sequence == 0
                || assertion.sequence > MAX_SAFE_INTEGER
                || assertion.not_after <= now
                || assertion.not_after <= heartbeat.issued_at
                || assertion.not_after.saturating_sub(heartbeat.issued_at)
                    > self.config.lease_ms / 2
            {
                return Err("invalid router assertion");
            }
        }
        let mut accepted = 0;
        for assertion in &heartbeat.users {
            let entry = self
                .get(&assertion.user_id)
                .ok_or("router lease inventory full")?;
            let mut policy = self.policy.lock().unwrap();
            let mut state = entry.state.lock().unwrap();
            if state.accepted.is_some() || state.permit_issued {
                entry.observe(&mut policy, &mut state);
            }
            let incoming = (assertion.owner_epoch, assertion.sequence);
            if state.accepted.is_some_and(|previous| incoming <= previous) {
                continue;
            }
            if state
                .accepted
                .is_some_and(|previous| previous.0 != incoming.0)
                && !state.fenced
            {
                state.generation = state.generation.saturating_add(1);
                state.retired_at = Some(Instant::now());
            }
            state.accepted = Some(incoming);
            state.not_after = Some(assertion.not_after);
            state.deadline =
                Some(Instant::now() + Duration::from_millis(assertion.not_after - now));
            state.fenced = false;
            accepted += 1;
        }
        let mut policy = self.policy.lock().unwrap();
        let checked_at = (self.clock)();
        // Readiness stays conservative for every user until the explicit grace ends.
        let grace_until = if policy.enforce {
            policy.grace_until(checked_at)
        } else {
            None
        };
        Ok(Ack {
            accepted,
            checked_at,
            enforcing: policy.enforce && grace_until.is_none(),
            grace_until,
            boot_id: self.boot_id,
        })
    }
    #[cfg(test)]
    fn enforce(&self, enabled: bool) {
        let mut policy = self.policy.lock().unwrap();
        policy.enforce = enabled;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn router_identity_is_separate_from_display_and_required_only_for_enforcement() {
        assert!(
            Config::parse(
                None,
                Some("exchange-01".into()),
                DEFAULT_LEASE_MS,
                Some("1"),
                None
            )
            .is_err()
        );
        let shadow = Config::parse(
            None,
            Some("exchange-01".into()),
            DEFAULT_LEASE_MS,
            None,
            None,
        )
        .unwrap();
        assert!(shadow.box_id.is_none());
        let active = Config::parse(
            Some("01".into()),
            Some("exchange-01".into()),
            DEFAULT_LEASE_MS,
            Some("1"),
            None,
        )
        .unwrap();
        let users = Users::with_clock(active, Arc::new(|| 1_000_000));
        let mut heartbeat = beat(1_000_000, 1, 1);
        assert!(users.accept(&heartbeat).is_err());
        heartbeat.r#box = "01".into();
        assert_eq!(users.accept(&heartbeat).unwrap().accepted, 1);
    }

    fn setup(enforce: bool) -> (Users, Arc<AtomicU64>) {
        setup_with_grace(enforce, None)
    }
    fn setup_with_grace(enforce: bool, until: Option<u64>) -> (Users, Arc<AtomicU64>) {
        let now = Arc::new(AtomicU64::new(1_000_000));
        let clock = now.clone();
        (
            Users::with_clock(
                Config {
                    box_id: Some("exchange-01".into()),
                    enforce,
                    enable_grace_until: until,
                    ..Config::default()
                },
                Arc::new(move || clock.load(Ordering::Relaxed)),
            ),
            now,
        )
    }
    fn beat(now: u64, epoch: u64, sequence: u64) -> Heartbeat {
        Heartbeat {
            r#box: "exchange-01".into(),
            issued_at: now,
            users: vec![Assertion {
                user_id: "u".into(),
                owner_epoch: epoch,
                sequence,
                not_after: now + 45_000,
            }],
        }
    }
    async fn advance(now: &AtomicU64, ms: u64) {
        now.fetch_add(ms, Ordering::Relaxed);
        tokio::time::advance(Duration::from_millis(ms)).await;
    }
    fn ack(users: &Users, now: &AtomicU64, sequence: u64) -> serde_json::Value {
        serde_json::to_value(
            users
                .accept(&beat(now.load(Ordering::Relaxed), 1, sequence))
                .unwrap(),
        )
        .unwrap()
    }
    #[tokio::test(start_paused = true)]
    async fn heartbeat_ack_reports_shadow_and_enforces_without_implicit_grace() {
        let (users, now) = setup(false);
        let shadow = ack(&users, &now, 1);
        assert_eq!(shadow["accepted"], 1);
        assert_eq!(shadow["enforcing"], false);
        assert_eq!(shadow["graceUntil"], serde_json::Value::Null);
        assert_eq!(shadow["bootId"], users.boot_id());
        advance(&now, 300_000).await;
        users.enforce(true);
        let active = ack(&users, &now, 2);
        assert_eq!(active["enforcing"], true);
        assert_eq!(active["graceUntil"], serde_json::Value::Null);
        assert_eq!(active["checkedAt"], now.load(Ordering::Relaxed));
        assert!(!users.get("unknown").unwrap().permit().allowed());
        users.enforce(false);
        assert_eq!(ack(&users, &now, 3)["enforcing"], false);
        users.enforce(true);
        assert_eq!(ack(&users, &now, 4)["enforcing"], true);
    }
    #[tokio::test(start_paused = true)]
    async fn heartbeat_ack_preserves_the_explicit_absolute_grace_across_restart() {
        let until = 1_000_000 + DEFAULT_LEASE_MS;
        let (users, now) = setup_with_grace(true, Some(until));
        let first = ack(&users, &now, 1);
        assert_eq!(first["enforcing"], false);
        assert_eq!(first["graceUntil"], until);
        let unknown = users.get("unknown").unwrap().permit();
        assert!(unknown.allowed());
        advance(&now, 1_000).await;
        let clock = now.clone();
        let restarted = Users::with_clock(
            users.config.clone(),
            Arc::new(move || clock.load(Ordering::Relaxed)),
        );
        let within = ack(&restarted, &now, 1);
        assert_eq!(within["enforcing"], false);
        assert_eq!(within["graceUntil"], until);
        assert_eq!(
            restarted
                .get("unknown")
                .unwrap()
                .status()
                .missing_heartbeat_grace_ms,
            DEFAULT_LEASE_MS - 1000
        );
        advance(&now, DEFAULT_LEASE_MS - 1000).await;
        assert_eq!(ack(&restarted, &now, 2)["enforcing"], false);
        advance(&now, 1).await;
        assert_eq!(ack(&restarted, &now, 3)["enforcing"], true);
        assert!(!unknown.allowed());
        let clock = now.clone();
        let past = Users::with_clock(
            users.config.clone(),
            Arc::new(move || clock.load(Ordering::Relaxed)),
        );
        assert!(!past.get("unknown").unwrap().permit().allowed());
        let expired = ack(&past, &now, 1);
        assert_eq!(expired["enforcing"], true);
        assert!(expired["graceUntil"].is_null());
    }
    #[tokio::test(start_paused = true)]
    async fn explicit_grace_expires_on_either_clock_and_never_reopens() {
        let (users, now) = setup_with_grace(true, Some(1_000_000 + DEFAULT_LEASE_MS));
        now.store(1_000_000 + DEFAULT_LEASE_MS + 1, Ordering::Relaxed);
        assert!(!users.get("unknown").unwrap().permit().allowed());
        assert_eq!(ack(&users, &now, 1)["enforcing"], true);
        now.store(1_000_000, Ordering::Relaxed);
        assert_eq!(ack(&users, &now, 2)["enforcing"], true);
        assert!(!users.get("another").unwrap().permit().allowed());
        let (users, now) = setup_with_grace(true, Some(1_000_000 + DEFAULT_LEASE_MS));
        tokio::time::advance(Duration::from_millis(DEFAULT_LEASE_MS + 1)).await;
        assert!(!users.get("unknown").unwrap().permit().allowed());
        assert_eq!(ack(&users, &now, 1)["enforcing"], true);
    }
    #[test]
    fn explicit_grace_environment_is_strict_and_absent_means_none() {
        for invalid in [
            "",
            "-1",
            "+123",
            "1.5",
            " 123",
            "9007199254740992",
            "undefined",
        ] {
            assert!(
                Config::parse(
                    Some("01".into()),
                    None,
                    DEFAULT_LEASE_MS,
                    Some("1"),
                    Some(invalid)
                )
                .is_err()
            );
        }
        let parsed = Config::parse(
            Some("01".into()),
            None,
            DEFAULT_LEASE_MS,
            Some("1"),
            Some("1090000"),
        )
        .unwrap();
        assert_eq!(parsed.enable_grace_until, Some(1_090_000));
        assert_eq!(Config::default().enable_grace_until, None);
    }
    #[tokio::test(start_paused = true)]
    async fn explicit_grace_never_extends_an_already_granted_lease_or_revives_work() {
        let (users, now) = setup_with_grace(true, Some(1_000_000 + DEFAULT_LEASE_MS));
        let unknown = users.get("before-first-heartbeat").unwrap().permit();
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 1))
            .unwrap();
        let known = users.get("u").unwrap().permit();
        assert_eq!(known.restore_generation(), None);
        advance(&now, 45_001).await;
        assert!(!known.allowed());
        assert!(unknown.allowed());
        advance(&now, 45_000).await;
        let mut heartbeat = beat(now.load(Ordering::Relaxed), 1, 1);
        heartbeat.users[0].user_id = "before-first-heartbeat".into();
        users.accept(&heartbeat).unwrap();
        assert!(!unknown.allowed());
        assert!(
            users
                .get("before-first-heartbeat")
                .unwrap()
                .permit()
                .allowed()
        );
    }
    #[tokio::test(start_paused = true)]
    async fn heartbeat_ack_is_conservative_for_known_and_new_users_in_a_batch() {
        let (users, now) = setup_with_grace(false, Some(1_000_000 + DEFAULT_LEASE_MS));
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 1))
            .unwrap();
        users.enforce(true);
        let mut batch = beat(now.load(Ordering::Relaxed), 1, 2);
        batch.users.push(Assertion {
            user_id: "new-user".into(),
            owner_epoch: 1,
            sequence: 1,
            not_after: batch.issued_at + 45_000,
        });
        let grace = serde_json::to_value(users.accept(&batch).unwrap()).unwrap();
        assert_eq!(grace["accepted"], 2);
        assert_eq!(grace["enforcing"], false);
        assert_eq!(
            grace["graceUntil"],
            now.load(Ordering::Relaxed) + DEFAULT_LEASE_MS
        );
        advance(&now, DEFAULT_LEASE_MS + 1).await;
        batch.issued_at = now.load(Ordering::Relaxed);
        for user in &mut batch.users {
            user.sequence += 1;
            user.not_after = batch.issued_at + 45_000;
        }
        let active = serde_json::to_value(users.accept(&batch).unwrap()).unwrap();
        assert_eq!(active["accepted"], 2);
        assert_eq!(active["enforcing"], true);
        assert_eq!(active["graceUntil"], serde_json::Value::Null);
    }
    #[tokio::test(start_paused = true)]
    async fn accepted_lease_fences_at_its_half_window_and_old_permits_never_revive() {
        let (users, now) = setup(true);
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 1))
            .unwrap();
        let user = users.get("u").unwrap();
        let before = user.permit();
        advance(&now, 45_000).await;
        assert!(before.allowed());
        advance(&now, 1).await;
        assert!(!before.allowed());
        assert!(user.status().fenced);
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 2))
            .unwrap();
        assert!(!user.status().fenced);
        assert!(user.permit().allowed());
        assert!(!before.allowed());
        assert!(!users.get("other").unwrap().status().owner_epoch.is_some());
    }
    #[tokio::test(start_paused = true)]
    async fn same_box_restore_generation_expires_after_one_day_without_changing_io_authority() {
        let (users, now) = setup(true);
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 1))
            .unwrap();
        advance(&now, 45_001).await;
        assert!(users.get("u").unwrap().status().fenced);
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 2))
            .unwrap();
        let permit = users.get("u").unwrap().permit();
        assert_eq!(permit.restore_generation(), Some(1));
        tokio::time::advance(crate::interruption::RETENTION).await;
        assert_eq!(permit.restore_generation(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn regular_contact_keeps_a_live_generation_and_parked_users_are_explicit() {
        let (users, now) = setup(true);
        users
            .accept(&beat(now.load(Ordering::Relaxed), 1, 1))
            .unwrap();
        let user = users.get("u").unwrap();
        let permit = user.permit();
        for sequence in 2..=21 {
            users
                .accept(&beat(now.load(Ordering::Relaxed), 1, sequence))
                .unwrap();
            advance(&now, 10_000).await;
            assert!(permit.allowed());
        }
        assert!(users.get("other").unwrap().status().fenced);
    }
    #[tokio::test(start_paused = true)]
    async fn shadow_never_gates_and_enabling_without_an_explicit_deadline_fences() {
        let (users, now) = setup(false);
        let user = users.get("u").unwrap();
        let permit = user.permit();
        advance(&now, 300_000).await;
        assert!(user.status().would_fence);
        assert!(!user.status().fenced);
        assert!(permit.allowed());
        users.enforce(true);
        assert!(!permit.allowed());
    }
    #[tokio::test(start_paused = true)]
    async fn rejected_replays_and_clocks_cannot_extend_not_after() {
        let (users, now) = setup(true);
        let start = now.load(Ordering::Relaxed);
        let first = beat(start, 2, 3);
        users.accept(&first).unwrap();
        let user = users.get("u").unwrap();
        let mut old = beat(start, 1, 100);
        assert_eq!(users.accept(&old).unwrap().accepted, 0);
        old.users[0].owner_epoch = 2;
        old.users[0].sequence = 2;
        assert_eq!(users.accept(&old).unwrap().accepted, 0);
        old.users[0].sequence = 3;
        assert_eq!(users.accept(&old).unwrap().accepted, 0);
        assert_eq!(user.status().not_after, Some(start + 45_000));
        now.store(start - 10_000, Ordering::Relaxed);
        assert!(users.accept(&beat(start, 3, 1)).is_err());
        tokio::time::advance(Duration::from_millis(45_001)).await;
        assert!(!user.permit().allowed());
    }
    #[tokio::test(start_paused = true)]
    async fn wrong_box_future_stale_and_overlong_assertions_fail_closed() {
        let (users, now) = setup(true);
        let start = now.load(Ordering::Relaxed);
        let mut heartbeat = beat(start, 1, 1);
        heartbeat.r#box = "exchange-02".into();
        assert!(users.accept(&heartbeat).is_err());
        heartbeat.r#box = "exchange-01".into();
        heartbeat.issued_at = start + CLOCK_SKEW_MS + 1;
        assert!(users.accept(&heartbeat).is_err());
        heartbeat.issued_at = start - CLOCK_SKEW_MS - 1;
        assert!(users.accept(&heartbeat).is_err());
        heartbeat.issued_at = start;
        heartbeat.users[0].not_after += 1;
        assert!(users.accept(&heartbeat).is_err());
        assert_eq!(users.get("u").unwrap().status().owner_epoch, None);
    }
    #[tokio::test(start_paused = true)]
    async fn review_enforcing_ack_fences_every_named_user_even_after_wall_rollback() {
        let (users, now) = setup(true);
        advance(&now, DEFAULT_LEASE_MS + 1).await;
        let mut heartbeat = beat(now.load(Ordering::Relaxed), 1, 1);
        heartbeat.users.push(Assertion {
            user_id: "second".into(),
            owner_epoch: 1,
            sequence: 1,
            not_after: heartbeat.users[0].not_after,
        });
        let ack = users.accept(&heartbeat).unwrap();
        assert!(ack.enforcing);
        assert!(ack.grace_until.is_none());
        let first = users.get("u").unwrap().permit();
        let second = users.get("second").unwrap().permit();
        assert!(first.allowed() && second.allowed());
        now.fetch_sub(20_000, Ordering::Relaxed);
        tokio::time::advance(Duration::from_millis(45_001)).await;
        assert!(!first.allowed() && !second.allowed());
        assert!(!users.get("never-heartbeated").unwrap().permit().allowed());
    }

    #[tokio::test(start_paused = true)]
    async fn review_true_ack_cannot_authorize_takeover_while_restarted_old_box_allows_io() {
        let (users, now) = setup(true);
        advance(&now, DEFAULT_LEASE_MS + 1).await;
        let heartbeat = beat(now.load(Ordering::Relaxed), 1, 1);
        let not_after = heartbeat.users[0].not_after;
        let ack = users.accept(&heartbeat).unwrap();
        assert!(ack.enforcing);
        assert!(ack.grace_until.is_none());
        let old_process_permit = users.get("u").unwrap().permit();
        advance(&now, 1_000).await;
        let clock = now.clone();
        let restarted = Users::with_clock(
            users.config.clone(),
            Arc::new(move || clock.load(Ordering::Relaxed)),
        );
        let restarted_old_box = restarted.get("u").unwrap();
        let reconnect_permit = restarted_old_box.permit();
        assert!(
            !reconnect_permit.allowed(),
            "restart without an explicit grace must refuse IO immediately"
        );
        advance(&now, 49_001).await;
        assert!(now.load(Ordering::Relaxed) > not_after + CLOCK_SKEW_MS);
        assert!(
            !old_process_permit.allowed(),
            "same-process control must fence"
        );
        assert_eq!(restarted_old_box.status().missing_heartbeat_grace_ms, 0);
        assert!(
            !reconnect_permit.allowed(),
            "previous true ACK cannot authorize no-proof takeover while restarted source still admits native IO"
        );
    }
}

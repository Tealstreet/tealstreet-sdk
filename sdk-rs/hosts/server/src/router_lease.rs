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
}
impl Default for Config {
    fn default() -> Self {
        Self {
            box_id: None,
            display_name: None,
            lease_ms: DEFAULT_LEASE_MS,
            enforce: false,
        }
    }
}

impl Config {
    pub fn parse(
        box_id: Option<String>,
        display_name: Option<String>,
        lease_ms: u64,
        enforce: Option<&str>,
    ) -> Result<Self, String> {
        let enforce = match enforce {
            None | Some("0") => false,
            Some("1") => true,
            _ => return Err("TEALSTREET_ROUTER_LEASE_ENFORCE must be 0 or 1".into()),
        };
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

struct Policy {
    enforce: bool,
    grace_until: Instant,
}
struct State {
    accepted: Option<(u64, u64)>,
    not_after: Option<u64>,
    deadline: Option<Instant>,
    generation: u64,
    fenced: bool,
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
    pub fn restores_must_pause(&self) -> bool {
        self.user.restores_must_pause()
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
    fn observe(&self, policy: &Policy, state: &mut State) {
        let missing_grace = state.accepted.is_none() && Instant::now() <= policy.grace_until;
        if policy.enforce && !missing_grace && self.expired(state) && !state.fenced {
            state.fenced = true;
            state.generation = state.generation.saturating_add(1);
        }
    }
    pub fn permit(self: &Arc<Self>) -> Permit {
        let policy = self.policy.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        self.observe(&policy, &mut state);
        Permit {
            user: self.clone(),
            generation: state.generation,
        }
    }
    fn allowed(&self, generation: u64) -> bool {
        let policy = self.policy.lock().unwrap();
        if !policy.enforce {
            return true;
        }
        let mut state = self.state.lock().unwrap();
        self.observe(&policy, &mut state);
        !state.fenced && state.generation == generation
    }
    pub fn restores_must_pause(&self) -> bool {
        self.policy.lock().unwrap().enforce && self.state.lock().unwrap().generation > 0
    }
    pub fn status(&self) -> Status {
        let policy = self.policy.lock().unwrap();
        let mut state = self.state.lock().unwrap();
        self.observe(&policy, &mut state);
        Status {
            enforced: policy.enforce,
            would_fence: self.expired(&state),
            fenced: policy.enforce && state.fenced,
            owner_epoch: state.accepted.map(|v| v.0),
            sequence: state.accepted.map(|v| v.1),
            not_after: state.not_after,
            generation: state.generation,
            missing_heartbeat_grace_ms: if state.accepted.is_none() {
                policy
                    .grace_until
                    .saturating_duration_since(Instant::now())
                    .as_millis() as u64
            } else {
                0
            },
        }
    }
}

pub struct Users {
    config: Config,
    policy: Arc<Mutex<Policy>>,
    entries: Mutex<HashMap<String, Arc<User>>>,
    clock: Arc<dyn Fn() -> u64 + Send + Sync>,
}
impl Users {
    pub fn new(config: Config) -> Self {
        Self::with_clock(config, Arc::new(crate::accounts::now_ms))
    }
    fn with_clock(config: Config, clock: Arc<dyn Fn() -> u64 + Send + Sync>) -> Self {
        Self {
            policy: Arc::new(Mutex::new(Policy {
                enforce: config.enforce,
                grace_until: Instant::now() + Duration::from_millis(config.lease_ms),
            })),
            config,
            entries: Mutex::default(),
            clock,
        }
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
                fenced: false,
            }),
            clock: self.clock.clone(),
        });
        entries.insert(user.to_owned(), entry.clone());
        Some(entry)
    }
    pub fn accept(&self, heartbeat: &Heartbeat) -> Result<usize, &'static str> {
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
            let policy = self.policy.lock().unwrap();
            let mut state = entry.state.lock().unwrap();
            entry.observe(&policy, &mut state);
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
            }
            state.accepted = Some(incoming);
            state.not_after = Some(assertion.not_after);
            state.deadline =
                Some(Instant::now() + Duration::from_millis(assertion.not_after - now));
            state.fenced = false;
            accepted += 1;
        }
        Ok(accepted)
    }
    #[cfg(test)]
    fn enforce(&self, enabled: bool) {
        let mut policy = self.policy.lock().unwrap();
        if enabled && !policy.enforce {
            policy.grace_until = Instant::now() + Duration::from_millis(self.config.lease_ms);
        }
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
                Some("1")
            )
            .is_err()
        );
        let shadow =
            Config::parse(None, Some("exchange-01".into()), DEFAULT_LEASE_MS, None).unwrap();
        assert!(shadow.box_id.is_none());
        let active = Config::parse(
            Some("01".into()),
            Some("exchange-01".into()),
            DEFAULT_LEASE_MS,
            Some("1"),
        )
        .unwrap();
        let users = Users::with_clock(active, Arc::new(|| 1_000_000));
        let mut heartbeat = beat(1_000_000, 1, 1);
        assert!(users.accept(&heartbeat).is_err());
        heartbeat.r#box = "01".into();
        assert_eq!(users.accept(&heartbeat).unwrap(), 1);
    }

    fn setup(enforce: bool) -> (Users, Arc<AtomicU64>) {
        let now = Arc::new(AtomicU64::new(1_000_000));
        let clock = now.clone();
        (
            Users::with_clock(
                Config {
                    box_id: Some("exchange-01".into()),
                    enforce,
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
    async fn regular_contact_keeps_a_live_generation_and_parked_users_are_explicit() {
        let (users, now) = setup(true);
        let user = users.get("u").unwrap();
        let permit = user.permit();
        for sequence in 1..=20 {
            users
                .accept(&beat(now.load(Ordering::Relaxed), 1, sequence))
                .unwrap();
            advance(&now, 10_000).await;
            assert!(permit.allowed());
        }
        assert!(users.get("other").unwrap().status().fenced);
    }
    #[tokio::test(start_paused = true)]
    async fn shadow_never_gates_and_enabling_grants_missing_users_one_full_lease() {
        let (users, now) = setup(false);
        let user = users.get("u").unwrap();
        let permit = user.permit();
        advance(&now, 300_000).await;
        assert!(user.status().would_fence);
        assert!(!user.status().fenced);
        assert!(permit.allowed());
        users.enforce(true);
        advance(&now, DEFAULT_LEASE_MS).await;
        assert!(permit.allowed());
        advance(&now, 1).await;
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
        assert_eq!(users.accept(&old), Ok(0));
        old.users[0].owner_epoch = 2;
        old.users[0].sequence = 2;
        assert_eq!(users.accept(&old), Ok(0));
        old.users[0].sequence = 3;
        assert_eq!(users.accept(&old), Ok(0));
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
}

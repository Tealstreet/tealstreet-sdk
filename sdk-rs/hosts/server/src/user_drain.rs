//! Per-user admission fences. Actor dispatch and the release proof share this lock.
use crate::{accounts::Account, algos::Running};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;
use tokio::time::Instant;

pub const DEFAULT_IDLE_RELEASE_TTL_MS: u64 = 20_000;
pub(crate) const MAX_IDLE_REQUESTS: usize = 64;

pub const DEFAULT_CAP_MS: u64 = 1_800_000;
pub const MAX_CAP_MS: u64 = 86_400_000;

pub fn starts_job(op: &str) -> bool {
    matches!(
        op,
        "chaser:start" | "twap:start" | "twap:resume" | "twap:state" | "exec"
    )
}

pub fn refusal() -> Value {
    json!({"name":"ServerDraining", "kind":"venueUnavailable", "message":"The server is draining; new algo jobs are not accepted", "outcome":"notSent"})
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Job {
    pub kind: &'static str,
    pub id: String,
    #[serde(skip)]
    pub parked: bool,
}

struct Tracked {
    name: String,
    running: Weak<Running>,
    account: Weak<Account>,
}

struct Drain {
    id: String,
    started: Instant,
    cap_ms: u64,
    capped: bool,
    ready: bool,
    forcing: bool,
    forced: bool,
    idle: Option<IdleRelease>,
}

struct IdleRelease {
    request_id: String,
    checked_at: u64,
    expires_at: u64,
    deadline: Instant,
    ttl: Duration,
}

struct SpentRequest {
    id: String,
    forget_at: Instant,
}

#[derive(Debug)]
pub(crate) enum IdleReleaseError {
    Refused(Value),
    Capacity,
}

fn prune_spent(state: &mut Activity) {
    let now = Instant::now();
    state.spent_requests.retain(|spent| spent.forget_at > now);
}

fn retire_idle(state: &mut Activity) {
    prune_spent(state);
    if let Some(idle) = state.drain.as_mut().and_then(|drain| drain.idle.take()) {
        // Issuance reserves this slot; unexpired tombstones are never evicted.
        state.spent_requests.push_back(SpentRequest {
            id: idle.request_id,
            forget_at: Instant::now() + idle.ttl * 2,
        });
    }
}

fn expire_idle(state: &mut Activity) {
    if state.sockets == 0
        && state
            .drain
            .as_ref()
            .and_then(|drain| drain.idle.as_ref())
            .is_some_and(|idle| {
                Instant::now() >= idle.deadline || crate::accounts::now_ms() >= idle.expires_at
            })
    {
        retire_idle(state);
        state.drain = None;
    }
}

#[derive(Default)]
pub(crate) struct Activity {
    drain: Option<Drain>,
    sockets: usize,
    tracked: Vec<Tracked>,
    spent_requests: VecDeque<SpentRequest>,
}

pub struct User {
    state: Mutex<Activity>,
    box_draining: Arc<AtomicBool>,
}

impl User {
    /// Held through synchronous dispatch and publication of its running-work snapshot.
    pub(crate) fn admit(&self) -> Result<MutexGuard<'_, Activity>, Value> {
        let mut state = self.state.lock().unwrap();
        expire_idle(&mut state);
        if state.drain.is_some() || self.box_draining.load(Ordering::Acquire) {
            return Err(refusal());
        }
        Ok(state)
    }

    pub(crate) fn admit_cleanup(&self) -> Result<MutexGuard<'_, Activity>, Value> {
        let mut state = self.state.lock().unwrap();
        expire_idle(&mut state);
        if state
            .drain
            .as_ref()
            .is_some_and(|drain| drain.ready || drain.forcing)
        {
            return Err(refusal());
        }
        Ok(state)
    }

    pub fn latched(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        expire_idle(&mut state);
        state.drain.is_some()
    }

    pub fn should_close(&self) -> bool {
        if !self.latched() {
            return false;
        }
        self.snapshot()["state"]
            .as_str()
            .is_some_and(|state| matches!(state, "closing" | "drained"))
    }

    pub(crate) fn forced(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .drain
            .as_ref()
            .is_some_and(|drain| drain.forced)
    }

    pub(crate) fn planned_move(&self) -> bool {
        self.should_close()
            && self
                .state
                .lock()
                .unwrap()
                .drain
                .as_ref()
                .is_some_and(|drain| !drain.forced)
    }

    fn snapshot(&self) -> Value {
        let mut state = self.state.lock().unwrap();
        expire_idle(&mut state);
        let mut jobs = Vec::new();
        let mut pending = 0;
        state
            .tracked
            .retain(|tracked| tracked.running.strong_count() > 0);
        for tracked in &state.tracked {
            if let Some(running) = tracked.running.upgrade() {
                let (rows, waiting) = running.jobs();
                pending += waiting;
                for row in rows {
                    jobs.push(json!({"accountName":tracked.name,"kind":row.kind,"id":row.id}));
                }
            }
        }
        jobs.sort_by_cached_key(Value::to_string);
        let done = jobs.is_empty() && pending == 0;
        let live = state.sockets;
        let mut result = json!({"drainId":null,"state":"off","capMs":null,"elapsedMs":0,
            "hasRunningAlgo":!done,"algosDone":done,"pendingStarts":pending,
            "runningJobs":jobs,"liveSockets":live,"released":false,"closeCode":4100});
        if let Some(drain) = &mut state.drain {
            let elapsed = drain
                .started
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            if !drain.ready && !drain.forcing && elapsed >= drain.cap_ms {
                drain.capped = true;
            }
            if done && !drain.capped {
                drain.ready = true;
            }
            let released = drain.ready && done && live == 0;
            result["drainId"] = drain.id.clone().into();
            result["capMs"] = drain.cap_ms.into();
            result["elapsedMs"] = elapsed.into();
            result["released"] = released.into();
            result["state"] = if released {
                "drained"
            } else if drain.ready && done {
                "closing"
            } else if drain.forcing {
                "forcing"
            } else if drain.capped {
                "capped"
            } else {
                "waiting"
            }
            .into();
        }
        result
    }
}

#[derive(Default)]
pub struct Users {
    entries: Mutex<HashMap<String, Arc<User>>>,
    box_draining: Arc<AtomicBool>,
}

impl Users {
    pub fn get(&self, user: &str) -> Arc<User> {
        let mut entries = self.entries.lock().unwrap();
        if let Some(entry) = entries.get(user) {
            return entry.clone();
        }
        entries.retain(|_, entry| {
            if Arc::strong_count(entry) != 1 {
                return true;
            }
            let mut state = entry.state.lock().unwrap();
            prune_spent(&mut state);
            !state.spent_requests.is_empty()
                || state.drain.is_some()
                || state.sockets > 0
                || state.tracked.iter().any(|a| a.running.strong_count() > 0)
        });
        entries
            .entry(user.to_owned())
            .or_insert_with(|| {
                Arc::new(User {
                    state: Mutex::default(),
                    box_draining: self.box_draining.clone(),
                })
            })
            .clone()
    }

    fn find(&self, user: &str) -> Option<Arc<User>> {
        self.entries.lock().unwrap().get(user).cloned()
    }

    pub fn box_draining(&self, value: bool) {
        self.box_draining.store(value, Ordering::Release);
    }

    pub fn track(&self, user: &str, name: &str, running: &Arc<Running>, account: Weak<Account>) {
        if user.is_empty() {
            return;
        }
        let entry = self.get(user);
        running.attach(entry.clone());
        let mut state = entry.state.lock().unwrap();
        if let Some(existing) = state
            .tracked
            .iter_mut()
            .find(|tracked| tracked.running.ptr_eq(&Arc::downgrade(running)))
        {
            if account.strong_count() > 0 {
                if let Some(account) = account.upgrade() {
                    existing.name = account.name.clone();
                }
                existing.account = account;
            }
        } else {
            state.tracked.push(Tracked {
                name: account
                    .upgrade()
                    .map_or_else(|| name.to_owned(), |account| account.name.clone()),
                running: Arc::downgrade(running),
                account,
            });
        }
    }

    pub fn socket(&self, user: &str) -> Socket {
        let entry = self.get(user);
        entry.state.lock().unwrap().sockets += 1;
        Socket(entry)
    }

    /// Reserve before returning HTTP 101; waiting operator drains still need signers.
    pub(crate) fn reserve_socket(&self, user: &str) -> Result<Socket, ()> {
        let entry = self.get(user);
        {
            let mut state = entry.state.lock().unwrap();
            expire_idle(&mut state);
            if state.drain.as_ref().is_some_and(|drain| drain.ready) {
                return Err(());
            }
            state.sockets += 1;
        }
        Ok(Socket(entry))
    }

    /// The same critical section as job dispatch: no socket/start can cross this proof.
    pub(crate) fn idle_release(
        &self,
        user: &str,
        request: &str,
        ttl: Duration,
    ) -> Result<Value, IdleReleaseError> {
        let entry = self.get(user);
        let mut state = entry.state.lock().unwrap();
        expire_idle(&mut state);
        prune_spent(&mut state);
        if let Some(drain) = &state.drain
            && let Some(idle) = &drain.idle
            && idle.request_id == request
        {
            return Ok(idle_proof(user, drain, idle));
        }
        let reason = if state.spent_requests.iter().any(|spent| spent.id == request) {
            Some("request-spent")
        } else {
            idle_refusal(&state)
        };
        if let Some(reason) = reason {
            return Err(IdleReleaseError::Refused(
                json!({"released":false,"reason":reason,"checkedAt":crate::accounts::now_ms()}),
            ));
        }
        if state.spent_requests.len() >= MAX_IDLE_REQUESTS {
            return Err(IdleReleaseError::Capacity);
        }
        let checked_at = crate::accounts::now_ms();
        let deadline = Instant::now() + ttl;
        let drain = Drain {
            id: crate::accounts::resume_id(),
            started: Instant::now(),
            cap_ms: DEFAULT_CAP_MS,
            capped: false,
            ready: true,
            forcing: false,
            forced: false,
            idle: Some(IdleRelease {
                request_id: request.into(),
                checked_at,
                expires_at: checked_at
                    + u64::try_from(ttl.as_millis()).expect("validated idle TTL"),
                deadline,
                ttl,
            }),
        };
        let proof = idle_proof(user, &drain, drain.idle.as_ref().unwrap());
        let id = drain.id.clone();
        state.drain = Some(drain);
        drop(state);
        let entry = Arc::downgrade(&entry);
        tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            if let Some(entry) = entry.upgrade() {
                let mut state = entry.state.lock().unwrap();
                if state.drain.as_ref().is_some_and(|drain| drain.id == id) {
                    expire_idle(&mut state);
                }
            }
        });
        Ok(proof)
    }

    pub fn start(&self, user: &str, cap_ms: u64) -> Value {
        let entry = self.get(user);
        {
            let mut state = entry.state.lock().unwrap();
            expire_idle(&mut state);
            // Explicit operator takeover retains the request history but never expires.
            retire_idle(&mut state);
            state.drain.get_or_insert_with(|| Drain {
                id: crate::accounts::resume_id(),
                started: Instant::now(),
                cap_ms,
                capped: false,
                ready: false,
                forcing: false,
                forced: false,
                idle: None,
            });
        }
        self.status(user)
    }

    pub fn status(&self, user: &str) -> Value {
        let entry = self.find(user).unwrap_or_else(|| {
            Arc::new(User {
                state: Mutex::default(),
                box_draining: self.box_draining.clone(),
            })
        });
        let mut status = entry.snapshot();
        status["userTag"] = crate::accounts::user_tag(user).into();
        status
    }

    pub fn cancel(&self, user: &str, id: &str) -> bool {
        let Some(entry) = self.find(user) else {
            return false;
        };
        let mut state = entry.state.lock().unwrap();
        expire_idle(&mut state);
        if state.sockets > 0 && state.drain.as_ref().is_some_and(|drain| drain.ready) {
            return false;
        }
        if !state
            .drain
            .as_ref()
            .is_some_and(|drain| drain.id == id && !drain.forcing)
        {
            return false;
        }
        retire_idle(&mut state);
        state.drain = None;
        true
    }

    pub fn begin_force(&self, user: &str, id: &str) -> Option<Vec<Arc<Account>>> {
        let entry = self.find(user)?;
        let mut state = entry.state.lock().unwrap();
        expire_idle(&mut state);
        let drain = state.drain.as_mut()?;
        if drain.id != id || drain.forcing {
            return None;
        }
        drain.capped = false;
        drain.forcing = true;
        drain.forced = true;
        retire_idle(&mut state);
        Some(
            state
                .tracked
                .iter()
                .filter_map(|tracked| tracked.account.upgrade())
                .collect(),
        )
    }

    pub fn end_force(&self, user: &str, id: &str) {
        if let Some(entry) = self.find(user) {
            let mut state = entry.state.lock().unwrap();
            if let Some(drain) = &mut state.drain
                && drain.id == id
            {
                drain.forcing = false;
            }
        }
    }

    pub fn proof(&self, user: &str, id: &str) -> Value {
        let status = self.status(user);
        json!({"userTag":crate::accounts::user_tag(user),"drainId":id,
            "released":status["drainId"] == id && status["released"] == true,
            "checkedAt":crate::accounts::now_ms()})
    }

    pub(crate) fn users_with_sockets(&self) -> std::collections::HashSet<String> {
        self.entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, entry)| entry.state.lock().unwrap().sockets > 0)
            .map(|(user, _)| user.clone())
            .collect()
    }

    pub fn list(&self) -> Result<Value, &'static str> {
        let entries: Vec<_> = self
            .entries
            .lock()
            .unwrap()
            .iter()
            .map(|(id, entry)| (id.clone(), entry.clone()))
            .collect();
        if entries.len() > 4096 {
            return Err("user inventory exceeds 4096 entries; drain automation must stop");
        }
        let mut users = Vec::new();
        for (id, entry) in entries {
            let summary = entry.snapshot();
            if summary["liveSockets"] == 0
                && summary["drainId"].is_null()
                && !entry
                    .state
                    .lock()
                    .unwrap()
                    .tracked
                    .iter()
                    .any(|a| a.running.strong_count() > 0)
            {
                continue;
            }
            users.push(json!({"userId":id,"userTag":crate::accounts::user_tag(&id),"liveSockets":summary["liveSockets"],
                "hasRunningAlgo":summary["hasRunningAlgo"],"runningJobs":summary["runningJobs"],
                "drain":if summary["drainId"].is_null() {Value::Null} else {self.status(&id)}}));
        }
        users.sort_by(|a, b| {
            a["userTag"]
                .as_str()
                .cmp(&b["userTag"].as_str())
                .then_with(|| a["userId"].as_str().cmp(&b["userId"].as_str()))
        });
        Ok(json!({"users":users}))
    }
}

fn idle_proof(user: &str, drain: &Drain, idle: &IdleRelease) -> Value {
    json!({"userTag":crate::accounts::user_tag(user),"drainId":drain.id,
        "released":true,"checkedAt":idle.checked_at,"expiresAt":idle.expires_at})
}

fn idle_refusal(state: &Activity) -> Option<&'static str> {
    if state.drain.is_some() {
        return Some("already-draining");
    }
    if state.sockets > 0 {
        return Some("live-socket");
    }
    let mut active = false;
    let mut parked = false;
    for tracked in &state.tracked {
        if let Some(running) = tracked.running.upgrade() {
            let (jobs, pending) = running.jobs();
            if (!running.io_allowed() || running.parked_by_lease())
                && (!jobs.is_empty() || pending > 0)
            {
                return Some("parked-jobs");
            }
            if pending > 0 {
                return Some("pending-starts");
            }
            let account_parked = tracked
                .account
                .upgrade()
                .is_some_and(|account| account.has_parked_holders());
            for job in jobs {
                if job.parked || account_parked {
                    parked = true
                } else {
                    active = true
                }
            }
        }
    }
    if active {
        Some("running-jobs")
    } else if parked {
        Some("parked-jobs")
    } else {
        None
    }
}

pub struct Socket(Arc<User>);
impl Socket {
    pub fn latched(&self) -> bool {
        self.0.latched()
    }
    pub fn should_close(&self) -> bool {
        self.0.should_close()
    }
}
impl Drop for Socket {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap();
        state.sockets -= 1;
        expire_idle(&mut state);
    }
}

pub const FORCE_DEADLINE: Duration = Duration::from_secs(25);

#[cfg(test)]
mod tests {
    use super::*;

    impl IdleReleaseError {
        fn refusal(self) -> Value {
            match self {
                Self::Refused(body) => body,
                Self::Capacity => panic!("expected a refusal, not request history capacity"),
            }
        }
    }

    fn tracked(users: &Users, user: &str) -> Arc<Running> {
        let running = Arc::new(Running::default());
        users.track(user, "acct", &running, Weak::new());
        running
    }

    const REQUEST: &str = "12345678-1234-4234-8234-123456789abc";
    const OTHER_REQUEST: &str = "87654321-1234-4234-8234-123456789abc";
    const TTL: Duration = Duration::from_millis(DEFAULT_IDLE_RELEASE_TTL_MS);

    #[tokio::test(start_paused = true)]
    async fn idle_release_latches_atomically_and_retries_return_the_original_proof() {
        let users = Users::default();
        let before = crate::accounts::now_ms();
        let proof = users.idle_release("u", REQUEST, TTL).unwrap();
        assert_eq!(proof.as_object().unwrap().len(), 5);
        assert_eq!(proof["userTag"], crate::accounts::user_tag("u"));
        assert_eq!(proof["released"], true);
        assert!(proof["checkedAt"].as_u64().unwrap() >= before);
        assert_eq!(
            proof["expiresAt"].as_u64().unwrap() - proof["checkedAt"].as_u64().unwrap(),
            DEFAULT_IDLE_RELEASE_TTL_MS
        );
        assert!(users.get("u").admit().is_err());
        assert!(users.get("u").admit_cleanup().is_err());
        assert!(users.reserve_socket("u").is_err());
        assert!(users.get("other").admit().is_ok());
        tokio::time::advance(Duration::from_secs(10)).await;
        assert_eq!(
            users.idle_release("u", REQUEST, TTL).unwrap(),
            proof,
            "a retry must not renew either timestamp"
        );
        let refusal = users
            .idle_release("u", OTHER_REQUEST, TTL)
            .unwrap_err()
            .refusal();
        assert_eq!(refusal["reason"], "already-draining");
        assert_eq!(users.status("u")["drainId"], proof["drainId"]);
        assert!(!users.cancel("u", "wrong-id"));
        assert!(users.cancel("u", proof["drainId"].as_str().unwrap()));
        assert!(users.get("u").admit().is_ok());
        assert!(users.reserve_socket("u").is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn every_idle_refusal_preserves_the_latch_socket_and_work_state() {
        let users = Users::default();
        let live = users.socket("live");
        let pending_socket = users.reserve_socket("pending-socket").unwrap();
        let active = tracked(&users, "active");
        active.refresh(&[], &[json!({"id":"active-job","status":"running"})], 0);
        let parked = tracked(&users, "parked");
        parked.refresh(&[], &[json!({"id":"paused-job","status":"paused"})], 0);
        let pending = tracked(&users, "pending");
        pending.refresh(&[], &[], 1);
        users.start("operator", DEFAULT_CAP_MS);
        for (user, reason) in [
            ("live", "live-socket"),
            ("pending-socket", "live-socket"),
            ("active", "running-jobs"),
            ("parked", "parked-jobs"),
            ("pending", "pending-starts"),
            ("operator", "already-draining"),
        ] {
            let before = users.status(user);
            let refusal = users
                .idle_release(user, REQUEST, TTL)
                .unwrap_err()
                .refusal();
            assert_eq!(refusal["released"], false);
            assert_eq!(refusal["reason"], reason);
            assert_eq!(refusal.as_object().unwrap().len(), 3);
            assert!(refusal["checkedAt"].is_u64());
            assert_eq!(
                users.status(user),
                before,
                "{user} must be observation-only"
            );
        }
        assert_eq!(active.jobs().0[0].id, "active-job");
        assert_eq!(parked.jobs().0[0].id, "paused-job");
        assert_eq!(pending.jobs().1, 1);
        drop(live);
        drop(pending_socket);
        assert!(users.idle_release("pending-socket", REQUEST, TTL).is_ok());
    }

    #[tokio::test]
    async fn composed_upgrade_reserves_before_101_and_keeps_idle_release_refused() {
        let registry = crate::accounts::tests::registry();
        let app = Arc::new(crate::http::App {
            registry: registry.clone(),
            metrics: Arc::default(),
            verifier: Arc::new(crate::auth::Verifier::unsigned()),
            config: crate::config::Config {
                port: 0,
                region: None,
                release: None,
                engine_threads: 1,
                max_connections: 16,
                max_public_per_ip: 2,
                idle_timeout: Duration::from_secs(30),
                resume_grace: Duration::from_secs(60),
                account_flush: Duration::from_millis(60),
                access: None,
                router_lease: Default::default(),
                idle_release_ttl: TTL,
            },
            started: std::time::Instant::now(),
            public_per_ip: Mutex::default(),
            closing: tokio::sync::watch::channel(false).0,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = crate::http::router(app);
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let client = reqwest::Client::new();
        assert_eq!(
            client
                .get(format!("http://{address}/health"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        let entry = registry.user_drains.get("u");
        let target = registry.clone();
        let (done, finished) = tokio::sync::oneshot::channel();
        let peer = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let mut socket = std::net::TcpStream::connect(address).unwrap();
            let token = crate::auth::unsigned_token("u", crate::accounts::now_ms() / 1000 + 3600);
            let admission = entry.state.lock().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_millis(100)))
                .unwrap();
            write!(socket,"GET /account?proxyToken={token} HTTP/1.1\r\nHost: {address}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").unwrap();
            let mut bytes = [0; 4096];
            let before = socket.read(&mut bytes);
            drop(admission);
            let blocked = before.as_ref().is_err_and(|error| {
                matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                )
            });
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let received = match before {
                Ok(n) => n,
                Err(_) => socket.read(&mut bytes).unwrap(),
            };
            let response = String::from_utf8_lossy(&bytes[..received]).into_owned();
            let refusal = target
                .user_drains
                .idle_release("u", REQUEST, TTL)
                .unwrap_err()
                .refusal();
            done.send((blocked, response, refusal)).unwrap();
        });
        let (blocked, response, refusal) = finished.await.unwrap();
        peer.join().unwrap();
        assert!(
            blocked,
            "HTTP101 must wait for the admission reservation, not just Session::new after the upgrade"
        );
        assert!(response.starts_with("HTTP/1.1 101"), "{response}");
        assert_eq!(refusal["reason"], "live-socket");
        server.abort();
    }

    #[test]
    fn summary_socket_users_are_deduplicated_and_exclude_latches_and_closed_sockets() {
        let users = Users::default();
        let first = users.socket("connected");
        let second = users.socket("connected");
        users.start("operator-latch-only", DEFAULT_CAP_MS);
        let pending = users.reserve_socket("pending-upgrade").unwrap();
        assert_eq!(
            users.users_with_sockets(),
            std::collections::HashSet::from(["connected".into(), "pending-upgrade".into()])
        );
        drop(first);
        assert!(users.users_with_sockets().contains("connected"));
        drop(second);
        drop(pending);
        assert!(users.users_with_sockets().is_empty());
    }
    #[tokio::test(start_paused = true)]
    async fn lease_fenced_and_parked_work_refuses_idle_release_without_mutating_work() {
        let users = Users::default();
        let running = tracked(&users, "u");
        let leases = crate::router_lease::Users::new(crate::router_lease::Config {
            box_id: Some("01".into()),
            enforce: true,
            ..Default::default()
        });
        let now = crate::accounts::now_ms();
        leases
            .accept(&crate::router_lease::Heartbeat {
                r#box: "01".into(),
                issued_at: now,
                users: vec![crate::router_lease::Assertion {
                    user_id: "u".into(),
                    owner_epoch: 1,
                    sequence: 1,
                    not_after: now + 1000,
                }],
            })
            .unwrap();
        running.attach_lease(leases.get("u").unwrap().permit());
        running.refresh(&[], &[json!({"id":"schedule","status":"running"})], 0);
        assert_eq!(
            users.idle_release("u", REQUEST, TTL).unwrap_err().refusal()["reason"],
            "running-jobs"
        );
        tokio::time::advance(Duration::from_millis(1001)).await;
        assert!(!running.io_allowed());
        for (jobs, pending) in [
            (vec![json!({"id":"schedule","status":"running"})], 0),
            (vec![], 1),
            (vec![json!({"id":"schedule","status":"paused"})], 0),
        ] {
            running.refresh(&[], &jobs, pending);
            let before = users.status("u");
            assert_eq!(
                users.idle_release("u", REQUEST, TTL).unwrap_err().refusal()["reason"],
                "parked-jobs"
            );
            assert_eq!(users.status("u"), before);
            assert!(!users.get("u").latched());
        }
        assert!(running.claim_lease_park());
        assert_eq!(
            users.idle_release("u", REQUEST, TTL).unwrap_err().refusal()["reason"],
            "parked-jobs"
        );
        assert!(users.reserve_socket("other").is_ok());
    }

    #[tokio::test]
    async fn an_admitted_start_publishes_its_reservation_before_the_idle_check() {
        let users = Arc::new(Users::default());
        let running = tracked(&users, "u");
        let entry = users.get("u");
        let admission = entry.admit().unwrap();
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let contender = {
            let users = users.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                users.idle_release("u", REQUEST, TTL)
            })
        };
        barrier.wait();
        running.refresh(&[], &[], 1);
        drop(admission);
        assert_eq!(
            contender.join().unwrap().unwrap_err().refusal()["reason"],
            "pending-starts"
        );
        assert!(!entry.latched());
        running.refresh(&[], &[], 0);
        assert!(users.idle_release("u", REQUEST, TTL).is_ok());
        assert!(
            entry.admit().is_err(),
            "a start arriving after the proof cannot run"
        );
        assert!(
            users.reserve_socket("u").is_err(),
            "a socket arriving after the proof cannot join"
        );
    }

    #[tokio::test]
    async fn racing_socket_reservation_and_idle_release_cannot_both_succeed() {
        for n in 0..32 {
            let users = Arc::new(Users::default());
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let socket = {
                let users = users.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    users.reserve_socket("u")
                })
            };
            barrier.wait();
            let proof = users.idle_release("u", REQUEST, TTL);
            let socket = socket.join().unwrap();
            match (proof, socket) {
                (Ok(proof), Err(())) => assert_eq!(proof["released"], true),
                (Err(refusal), Ok(_socket)) => {
                    assert_eq!(refusal.refusal()["reason"], "live-socket")
                }
                _ => {
                    panic!("socket admission and release must choose exactly one winner, race {n}")
                }
            }
        }
    }

    #[tokio::test]
    async fn an_algo_queued_after_idle_release_is_refused_at_real_actor_dispatch() {
        tokio::task::LocalSet::new().run_until(async {
            let registry = crate::accounts::tests::registry();
            let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_|true));
            let (account, placed) = crate::accounts::tests::held_load_account(link, Some(0.));
            crate::accounts::testing::register(&registry, &account);
            let (sent, mut frames) = tokio::sync::mpsc::unbounded_channel();
            crate::accounts::tests::hold_live(&account, 7, Arc::new(move |frame| sent.send(frame).is_ok()));
            tokio::time::sleep(Duration::from_millis(50)).await;
            let proof = registry.user_drains.idle_release("u", REQUEST, TTL).unwrap();
            crate::accounts::testing::algo_op(&account, "chaser:start", r#"{"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"}"#);
            let refusal = tokio::time::timeout(Duration::from_secs(2), async {
                loop {
                    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
                    if frame["d"]["op"] == "error" { break frame["d"]["data"].clone(); }
                }
            }).await.unwrap();
            assert_eq!(refusal[0]["name"], "ServerDraining", "{refusal}");
            assert_eq!(refusal[0]["outcome"], "notSent");
            assert!(!account.running.any());
            assert_eq!(placed.load(Ordering::Relaxed), 0);
            assert_eq!(registry.user_drains.status("u")["drainId"], proof["drainId"]);
        }).await;
    }

    #[tokio::test(start_paused = true)]
    async fn idle_expiry_runs_without_traffic_and_cannot_clear_a_newer_or_operator_latch() {
        let users = Users::default();
        let proof = users.idle_release("expires", REQUEST, TTL).unwrap();
        let entry = users.get("expires");
        let replaced = users.idle_release("replaced", REQUEST, TTL).unwrap();
        assert!(users.cancel("replaced", replaced["drainId"].as_str().unwrap()));
        let newer = users
            .idle_release("replaced", OTHER_REQUEST, TTL + Duration::from_secs(10))
            .unwrap();
        users.idle_release("operator", REQUEST, TTL).unwrap();
        let operator = users.start("operator", DEFAULT_CAP_MS);
        let permanent = users.start("permanent", DEFAULT_CAP_MS);
        tokio::task::yield_now().await;
        tokio::time::advance(TTL).await;
        tokio::task::yield_now().await;
        assert!(
            entry.state.lock().unwrap().drain.is_none(),
            "timer alone must clear the idle latch even when wall time has not advanced"
        );
        assert!(crate::accounts::now_ms() < proof["expiresAt"].as_u64().unwrap());
        assert_eq!(users.status("replaced")["drainId"], newer["drainId"]);
        assert_eq!(users.status("operator")["drainId"], operator["drainId"]);
        assert_eq!(users.status("permanent")["drainId"], permanent["drainId"]);
        assert!(users.get("expires").admit().is_ok());
        drop(entry);
        // Pruning another user must not erase this user's spent request history.
        users.get("another-user");
        let spent = users
            .idle_release("expires", REQUEST, TTL)
            .unwrap_err()
            .refusal();
        assert_eq!(spent["reason"], "request-spent");
        assert_eq!(users.status("expires")["state"], "off");
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_idle_request_cannot_install_a_new_latch_on_a_delayed_duplicate() {
        let users = Users::default();
        let original = users.idle_release("u", REQUEST, TTL).unwrap();
        assert!(users.cancel("u", original["drainId"].as_str().unwrap()));
        users.get("another-user");
        let spent = users.idle_release("u", REQUEST, TTL).unwrap_err().refusal();
        assert_eq!(spent["reason"], "request-spent");
        assert_eq!(spent.as_object().unwrap().len(), 3);
        assert_eq!(users.status("u")["state"], "off");
        assert!(users.get("u").admit().is_ok());
        let fresh = users.idle_release("u", OTHER_REQUEST, TTL).unwrap();
        assert_eq!(users.idle_release("u", OTHER_REQUEST, TTL).unwrap(), fresh);
    }

    #[tokio::test(start_paused = true)]
    async fn spent_request_capacity_never_evicts_a_protected_id_and_recovers_after_retention() {
        let users = Users::default();
        let request = |n: usize| format!("12345678-1234-4234-8234-{n:012x}");
        for n in 0..MAX_IDLE_REQUESTS {
            let proof = users.idle_release("u", &request(n), TTL).unwrap();
            assert!(users.cancel("u", proof["drainId"].as_str().unwrap()));
        }
        assert_eq!(
            users.get("u").state.lock().unwrap().spent_requests.len(),
            MAX_IDLE_REQUESTS
        );
        assert!(matches!(
            users.idle_release("u", REQUEST, TTL),
            Err(IdleReleaseError::Capacity)
        ));
        assert_eq!(users.status("u")["state"], "off");
        assert!(users.get("u").admit().is_ok());
        assert!(users.idle_release("other-user", REQUEST, TTL).is_ok());
        tokio::time::advance(TTL * 2 - Duration::from_millis(1)).await;
        for n in [0, MAX_IDLE_REQUESTS - 1] {
            assert_eq!(
                users
                    .idle_release("u", &request(n), TTL)
                    .unwrap_err()
                    .refusal()["reason"],
                "request-spent"
            );
        }
        assert!(matches!(
            users.idle_release("u", REQUEST, TTL),
            Err(IdleReleaseError::Capacity)
        ));
        tokio::time::advance(Duration::from_millis(2)).await;
        users.get("prune-after-retention");
        assert!(
            users.idle_release("u", REQUEST, TTL).is_ok(),
            "expired history must not exhaust capacity forever"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_retains_spent_ids_for_twice_the_original_ttl_from_retirement() {
        let users = Users::default();
        let original_ttl = TTL * 2;
        let proof = users.idle_release("u", REQUEST, original_ttl).unwrap();
        tokio::time::advance(original_ttl).await;
        // Admission performs expiry even if the timer has not yet run.
        assert!(users.get("u").admit().is_ok());
        tokio::time::advance(original_ttl * 2 - Duration::from_millis(1)).await;
        users.get("another-user");
        assert_eq!(
            users
                .idle_release("u", REQUEST, Duration::from_millis(1))
                .unwrap_err()
                .refusal()["reason"],
            "request-spent"
        );
        assert_eq!(users.status("u")["state"], "off");
        assert_eq!(
            users.proof("u", proof["drainId"].as_str().unwrap())["released"],
            false
        );
        tokio::time::advance(Duration::from_millis(2)).await;
        assert!(users.idle_release("u", OTHER_REQUEST, TTL).is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn operator_takeover_and_force_preserve_spent_request_identity() {
        for force in [false, true] {
            let users = Users::default();
            let proof = users.idle_release("u", REQUEST, TTL).unwrap();
            let id = proof["drainId"].as_str().unwrap();
            if force {
                assert!(users.begin_force("u", id).is_some());
                users.end_force("u", id);
            } else {
                assert_eq!(users.start("u", DEFAULT_CAP_MS)["drainId"], id);
            }
            assert!(users.cancel("u", id));
            users.get("another-user");
            assert_eq!(
                users.idle_release("u", REQUEST, TTL).unwrap_err().refusal()["reason"],
                "request-spent"
            );
            assert_eq!(users.status("u")["state"], "off");
        }
    }

    #[tokio::test(start_paused = true)]
    async fn expiry_never_clears_a_latch_with_a_socket_and_drop_finishes_expiry() {
        let users = Users::default();
        let proof = users.idle_release("u", REQUEST, TTL).unwrap();
        // Internal conversion can only follow a reserved upgrade; exercise the expiry backstop.
        let socket = users.socket("u");
        tokio::task::yield_now().await;
        tokio::time::advance(TTL).await;
        tokio::task::yield_now().await;
        assert_eq!(users.status("u")["drainId"], proof["drainId"]);
        assert!(users.get("u").admit().is_err());
        drop(socket);
        assert!(users.get("u").admit().is_ok());
    }

    #[tokio::test]
    async fn a_real_parked_account_job_refuses_without_stopping_or_moving_it() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, _) = crate::accounts::tests::held_load_account(link, Some(0.));
                crate::accounts::testing::register(&registry, &account);
                crate::accounts::tests::hold_live(&account, 7, Arc::new(|_| true));
                account.running.refresh(
                    &[],
                    &[json!({"id":"parked-schedule","status":"running"})],
                    0,
                );
                registry.park("resume".into(), "u", vec![(account.clone(), 7)]);
                let before = registry.user_drains.status("u");
                assert_eq!(
                    registry
                        .user_drains
                        .idle_release("u", REQUEST, TTL)
                        .unwrap_err()
                        .refusal()["reason"],
                    "parked-jobs"
                );
                assert_eq!(registry.user_drains.status("u"), before);
                assert!(account.has_parked_holders());
                assert!(account.running.any());
            })
            .await;
    }

    #[test]
    fn admission_and_release_are_one_fence_including_pending_exec_handoffs() {
        let users = Arc::new(Users::default());
        let running = tracked(&users, "u");
        let entry = users.get("u");
        let admitted = entry.admit().unwrap();
        let another = users.clone();
        let start = std::thread::spawn(move || another.start("u", DEFAULT_CAP_MS));
        running.refresh(&[], &[], 1);
        drop(admitted);
        let status = start.join().unwrap();
        assert_eq!(status["hasRunningAlgo"], true);
        assert_eq!(status["pendingStarts"], 1);
        assert_eq!(status["released"], false);
        assert!(entry.admit().is_err());
        running.refresh(&[], &[json!({"id":"handoff"})], 0);
        assert_eq!(
            users.status("u")["runningJobs"],
            json!([{"accountName":"acct","kind":"twap","id":"handoff"}])
        );
        running.refresh(&[], &[], 0);
        assert_eq!(
            users.proof("u", status["drainId"].as_str().unwrap())["released"],
            true
        );
    }

    #[tokio::test(start_paused = true)]
    async fn cap_never_stops_jobs_or_unlatches_and_generations_cannot_cross_cancel() {
        let users = Users::default();
        let running = tracked(&users, "u");
        running.refresh(&[], &[json!({"id":"paused"})], 0);
        let first = users.start("u", 10);
        let id = first["drainId"].as_str().unwrap();
        tokio::time::advance(Duration::from_millis(11)).await;
        let capped = users.status("u");
        assert_eq!(capped["state"], "capped");
        assert_eq!(capped["runningJobs"], first["runningJobs"]);
        assert_eq!(users.start("u", 1000)["drainId"], id);
        assert!(!users.cancel("u", "old"));
        assert!(users.cancel("u", id));
        assert!(users.get("u").admit().is_ok());
        let next = users.start("u", DEFAULT_CAP_MS);
        assert_ne!(next["drainId"], id);
        assert!(!users.cancel("u", id));
        assert!(users.begin_force("u", id).is_none());
        running.refresh(&[], &[], 0);
        assert_eq!(users.proof("u", id)["released"], false);
        assert_eq!(
            users.proof("u", next["drainId"].as_str().unwrap())["released"],
            true
        );
    }

    #[test]
    fn socket_counts_are_raii_and_parked_or_removed_accounts_still_hold_the_fence() {
        let users = Users::default();
        let running = tracked(&users, "u");
        let one = users.socket("u");
        let two = users.socket("u");
        running.refresh(&[], &[json!({"id":"paused"})], 0);
        let status = users.start("u", DEFAULT_CAP_MS);
        let id = status["drainId"].as_str().unwrap();
        drop(one);
        drop(two);
        assert_eq!(users.status("u")["liveSockets"], 0);
        assert_eq!(users.proof("u", id)["released"], false);
        let list = users.list().unwrap();
        assert_eq!(list["users"][0]["userId"], "u");
        assert_eq!(list["users"][0]["hasRunningAlgo"], true);
        running.refresh(&[], &[], 0);
        let proof = users.proof("u", id);
        assert_eq!(proof["released"], true);
        assert_eq!(proof.as_object().unwrap().len(), 4);
        assert!(!proof.to_string().contains("accountName"));
        assert!(!proof.to_string().contains("userId"));
        assert_eq!(users.proof("missing", id)["released"], false);
    }

    #[test]
    fn box_and_user_drains_share_refusal_without_affecting_other_users_or_stop_ops() {
        let users = Users::default();
        let _running = tracked(&users, "u");
        let other = users.get("other");
        users.start("u", DEFAULT_CAP_MS);
        assert!(other.admit().is_ok());
        let refusal = users.get("u").admit().err().unwrap();
        users.box_draining(true);
        assert_eq!(other.admit().err().unwrap(), refusal);
        users.box_draining(false);
        assert!(other.admit().is_ok());
        assert!(users.get("u").admit().is_err());
        for op in [
            "chaser:start",
            "twap:start",
            "twap:state",
            "twap:resume",
            "exec",
        ] {
            assert!(starts_job(op));
        }
        for op in [
            "chaser:terminate",
            "chaser:terminateAll",
            "twap:stop",
            "twap:pause",
            "twap:cleanup",
        ] {
            assert!(!starts_job(op));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn force_timeout_reports_work_and_cannot_be_aborted_during_cleanup() {
        let registry = Arc::new(crate::accounts::Registry::new(
            crate::threads::EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        ));
        let running = tracked(&registry.user_drains, "u");
        running.refresh(&[], &[json!({"id":"still-running"})], 0);
        let started = registry.start_user_drain("u", 1);
        let id = started["drainId"].as_str().unwrap();
        let force = registry.force_user_drain("u", id).unwrap();
        tokio::task::yield_now().await;
        assert!(!registry.user_drains.cancel("u", id));
        tokio::time::advance(FORCE_DEADLINE + Duration::from_secs(1)).await;
        assert!(!force.await.unwrap());
        let status = registry.user_drains.status("u");
        assert_eq!(status["state"], "capped");
        assert_eq!(status["runningJobs"], started["runningJobs"]);
        assert_eq!(status["released"], false);
        assert!(registry.user_drains.get("u").admit().is_err());
        assert!(registry.user_drains.cancel("u", id));
    }

    #[test]
    fn cancel_cannot_unlatch_between_a_move_decision_and_its_socket_close() {
        let users = Users::default();
        let socket = users.socket("u");
        let started = users.start("u", DEFAULT_CAP_MS);
        let id = started["drainId"].as_str().unwrap();
        assert_eq!(started["state"], "closing");
        assert!(!users.cancel("u", id));
        drop(socket);
        assert_eq!(users.proof("u", id)["released"], true);
        assert!(users.cancel("u", id));
        assert_eq!(users.proof("u", id)["released"], false);
    }

    #[test]
    fn inventory_is_sorted_and_refuses_oversize_instead_of_returning_a_partial_list() {
        let users = Users::default();
        let _anonymous = tracked(&users, "");
        assert_eq!(users.list().unwrap(), json!({"users":[]}));
        let _sockets: Vec<_> = ["third", "first", "second"]
            .into_iter()
            .map(|user| users.socket(user))
            .collect();
        let inventory = users.list().unwrap();
        let tags: Vec<_> = inventory["users"]
            .as_array()
            .unwrap()
            .iter()
            .map(|user| user["userTag"].as_str().unwrap())
            .collect();
        assert!(tags.windows(2).all(|pair| pair[0] <= pair[1]));
        let mut entries = users.entries.lock().unwrap();
        for n in 0..4096 {
            entries.insert(
                format!("u{n}"),
                Arc::new(User {
                    state: Mutex::default(),
                    box_draining: users.box_draining.clone(),
                }),
            );
        }
        drop(entries);
        assert!(users.list().is_err());
    }

    #[tokio::test]
    async fn a_stopped_twap_with_a_slice_still_on_the_wire_cannot_release_the_user() {
        tokio::task::LocalSet::new().run_until(async {
            let registry = crate::accounts::tests::registry();
            let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_|true));
            let (account, placed) = crate::accounts::tests::held_load_account(link, Some(0.));
            crate::accounts::testing::register(&registry, &account);
            let (sent, mut frames) = tokio::sync::mpsc::unbounded_channel();
            crate::accounts::tests::hold_live(&account,7,Arc::new(move |frame|sent.send(frame).is_ok()));
            tokio::time::sleep(Duration::from_millis(50)).await;
            let market = json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true});
            let options = json!({"accountName":"acct","activeMarket":market,"size":0.1,"duration":600,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"});
            crate::accounts::testing::algo_op(&account,"twap:start",&options.to_string());
            let id = tokio::time::timeout(Duration::from_secs(2),async {
                loop {
                    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
                    if let Some(id) = frame["d"]["data"]["activeTWAPs"][0]["id"].as_str() {break id.to_owned();}
                }
            }).await.unwrap();
            tokio::time::timeout(Duration::from_secs(2),async {
                while placed.load(Ordering::Relaxed)==0 {tokio::time::sleep(Duration::from_millis(5)).await;}
            }).await.expect("a real slice was physically handed to the test venue");
            crate::accounts::testing::algo_op(&account,"twap:stop",&json!(id).to_string());
            tokio::time::timeout(Duration::from_secs(2),async {
                while account.running.counts() != (0,0) {tokio::time::sleep(Duration::from_millis(5)).await;}
            }).await.unwrap();
            let status = registry.start_user_drain("u",DEFAULT_CAP_MS);
            assert_eq!(status["released"],false,"the visible TWAP row ended, but its placement has no answer: {status}");
            assert!(status["pendingStarts"].as_u64().unwrap()>0);
        }).await;
    }

    #[tokio::test]
    async fn a_queued_nuke_cannot_start_after_the_user_has_committed_to_move() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, placed) = crate::accounts::tests::held_load_account(link, Some(0.));
                crate::accounts::testing::register(&registry, &account);
                let (sent, mut frames) = tokio::sync::mpsc::unbounded_channel();
                crate::accounts::tests::hold_live(
                    &account,
                    7,
                    Arc::new(move |frame| sent.send(frame).is_ok()),
                );
                tokio::time::sleep(Duration::from_millis(50)).await;
                account
                    .algos
                    .as_ref()
                    .unwrap()
                    .send(crate::algos::AlgoInput::Nuke {
                        mode: "positions",
                        request_id: json!("nuke"),
                        action: None,
                    })
                    .unwrap();
                let started = registry.start_user_drain("u", DEFAULT_CAP_MS);
                assert_eq!(started["released"], true);
                let reply = tokio::time::timeout(Duration::from_secs(2), async {
                    loop {
                        let frame: Value =
                            serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
                        if frame["d"]["op"] == "callMethod:done" {
                            break frame["d"]["data"].clone();
                        }
                    }
                })
                .await
                .unwrap();
                assert_eq!(reply["success"], false);
                assert_eq!(reply["result"]["name"], "ServerDraining");
                assert_eq!(reply["result"]["outcome"], "notSent");
                assert_eq!(placed.load(Ordering::Relaxed), 0);
            })
            .await;
    }
}

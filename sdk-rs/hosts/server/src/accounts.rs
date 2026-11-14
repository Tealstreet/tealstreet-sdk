//! Hosted account registry with identity keys for enabled private Bybit accounts.
//! Connections keep their labels; live and parked holders share the account actor.
use crate::delegated::{DelegatedPair, PreparedDelegated, ResidentCopy, prepare_delegated};
use crate::envelope::InitPayload;
use crate::state::Log;
use crate::threads::EngineThreads;
use safe_cex_native::DisposalCause;
use safe_cex_native::resolve::{self, AccountTransport, Provider};
use safe_cex_native::{
    AccountHandle, NetworkTransport, ProxyTokenProvider, RoutingPlan, ServerConfig,
};
use safe_cex_remote_signer::{Outbox, SignerLink, remote_account};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

mod admission;
mod proof_queue;
pub(crate) use admission::ProbePermit;
pub(crate) mod identity;

#[cfg(test)]
mod identity_tests;

/// The unauthenticated public pseudo-account, as in NX `server.ts`.
pub const PUBLIC_ACCOUNT: &str = "__pub_bybit";
pub const PUBLIC_EXCHANGE: &str = "bybit";

enum DelegatedSetup {
    Pair(DelegatedPair),
    Resident(Box<ResidentCopy>),
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// The proxy token of the socket that owns an account. The transport reads it
/// per operation and refuses once it has expired, so it never outlives its socket.
#[derive(Clone)]
pub struct ProxyToken(Arc<RwLock<Option<(String, u64)>>>);

impl ProxyToken {
    fn new() -> Self {
        Self(Arc::new(RwLock::new(None)))
    }
    pub fn set(&self, token: Option<(String, u64)>) {
        *self.0.write().unwrap() = token;
    }
    fn provider(&self) -> ProxyTokenProvider {
        let cell = self.0.clone();
        Arc::new(move || {
            let current = cell.read().unwrap().clone();
            Box::pin(async move {
                match current {
                    Some((token, expires_at_ms)) if expires_at_ms > now_ms() => Ok(token),
                    Some(_) => Err("Tealstreet proxy token expired".to_owned()),
                    None => Err("Tealstreet proxy token unavailable".to_owned()),
                }
            })
        })
    }
}

pub struct Account {
    pub user: String,
    pub name: String,
    identity: Option<String>,
    pub venue: String,
    pub handle: AccountHandle,
    /// Explicit construction mode, independent of a remote signer link.
    public_only: bool,
    /// Present for a DELEGATED private account.
    pub delegated: Option<ResidentCopy>,
    /// Present for COSIGNED private accounts only.
    pub link: Option<Arc<SignerLink>>,
    pub token: ProxyToken,
    pub fingerprint: String,
    /// Live sockets carry their outbox and token; park holders carry none.
    holders: Mutex<Holders>,
    /// Private accounts' algo host (chaser, TWAP, exec); it also owns disposal.
    pub algos: Option<mpsc::UnboundedSender<crate::algos::AlgoInput>>,
    /// Lets algo replies reach the account's signing socket without a cycle.
    algo_owner: Arc<Mutex<Option<std::sync::Weak<Account>>>>,
    /// Every symbol a placement was attempted on, by a client or an algo.
    pub placed: PlacedSymbols,
    /// A live socket holds the account (not only parked resumes); account-only
    /// background reads run just while it does.
    pub held: Arc<std::sync::atomic::AtomicBool>,
    /// Frame-built account reads in flight; each is a whole engine snapshot.
    pub frame_reads: Arc<tokio::sync::Semaphore>,
    /// Other store reads in flight (balance, position mode), which may wait for load.
    pub reads: Arc<tokio::sync::Semaphore>,
    /// Metadata refreshes waiting for load, one per selected symbol.
    pub metadata_refreshes: Arc<tokio::sync::Semaphore>,
    /// Chases and TWAPs its algo host is running; none for public accounts.
    pub running: Arc<crate::algos::Running>,
    pub twap_owner: Arc<crate::twap_owner::Binding>,
}

/// Concurrent waiting store reads per account; more are refused, not queued.
pub const MAX_READS: usize = 4;

/// Own pool: a selection during load must not be refused, nor crowd out other reads.
pub const MAX_METADATA_REFRESHES: usize = 32;

/// Concurrent `fetchOrders`/`fetchPositions` per account; more are refused, not queued.
pub const MAX_FRAME_READS: usize = 2;

/// Symbols a placement was attempted on, kept for the account's lifetime, as
/// `rust.exchange.ts` `attemptedPlacementSymbols`: on `eachPlacedSymbol` venues a
/// cancel-all also cancels them, since frames can lag a placement's ACK.
/// Only the account's own markets are kept, and at most `MAX_PLACED_SYMBOLS`:
/// client text must never widen a cancel-all fan-out through the shared egress.
#[derive(Clone, Default)]
pub struct PlacedSymbols(Arc<Mutex<Placed>>);

pub const MAX_PLACED_SYMBOLS: usize = 256;

#[derive(Default)]
struct Placed {
    symbols: std::collections::BTreeSet<String>,
    markets: std::collections::HashSet<String>,
    warned: bool,
}

impl PlacedSymbols {
    fn lock(&self) -> std::sync::MutexGuard<'_, Placed> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The account's engine markets; recorded symbols outside them are dropped.
    pub fn sync_markets<'a>(&self, markets: impl ExactSizeIterator<Item = &'a String> + Clone) {
        let mut placed = self.lock();
        if placed.markets.len() == markets.len()
            && markets
                .clone()
                .all(|market| placed.markets.contains(market))
        {
            return;
        }
        placed.markets = markets.cloned().collect();
        let Placed {
            symbols, markets, ..
        } = &mut *placed;
        symbols.retain(|symbol| markets.contains(symbol));
    }

    pub fn record<'a>(&self, symbols: impl IntoIterator<Item = &'a str>) {
        let mut placed = self.lock();
        for symbol in symbols {
            if placed.symbols.contains(symbol) || !placed.markets.contains(symbol) {
                continue;
            }
            if placed.symbols.len() >= MAX_PLACED_SYMBOLS {
                if !placed.warned {
                    placed.warned = true;
                    Log::warn("placed-symbol scopes full; later placements are not recorded");
                }
                continue;
            }
            placed.symbols.insert(symbol.to_owned());
        }
    }

    /// The symbols of an engine `placeOrders` command, if it is one.
    pub fn record_command(&self, command: &Value) {
        if command["op"] == "placeOrders"
            && let Some(orders) = command["orders"].as_array()
        {
            self.record(orders.iter().filter_map(|order| order["symbol"].as_str()));
        }
    }

    pub fn symbols(&self) -> Vec<String> {
        self.lock().symbols.iter().cloned().collect()
    }
}

/// Why an account was disposed, as `disposal=<kind>` on its audit line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disposal {
    /// Re-inited on public data: the page saw its key refused, or its start fail.
    PublicData,
    /// Re-inited with other key or connection settings.
    SettingsChanged,
    /// A concurrent init of the same name won the race.
    Replaced,
    /// Remote signing failed repeatedly.
    SignerFailures,
    /// No browser came back within the resume grace.
    NoBrowser,
    /// The page disposed it: the user removed or deactivated it, or the page dropped it.
    PageDisposed,
    /// The page's ping stopped listing it.
    Orphaned,
    /// It never joined the socket that built it.
    JoinFailed,
    /// Its last socket closed without resuming.
    SocketClosed,
    /// Its last socket deliberately moved under an active drain.
    Moving,
}

impl Disposal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PublicData => "public_data",
            Self::SettingsChanged => "settings_changed",
            Self::Replaced => "replaced",
            Self::SignerFailures => "signer_failures",
            Self::NoBrowser => "no_browser",
            Self::PageDisposed => "page_disposed",
            Self::Orphaned => "orphaned",
            Self::JoinFailed => "join_failed",
            Self::SocketClosed => "socket_closed",
            Self::Moving => "moving",
        }
    }

    /// A rebuild for a new init: onto public data only when a keyed account loses its key.
    pub fn rebuild(was_public: bool, now_public: bool) -> Self {
        if now_public && !was_public {
            Self::PublicData
        } else {
            Self::SettingsChanged
        }
    }
}

/// A live socket holding an account: where its signing and proxy token go.
#[derive(Clone)]
pub struct Live {
    pub outbox: Outbox,
    pub token: Option<(String, u64)>,
}

#[derive(Default)]
struct Holders {
    by_id: HashMap<u64, Option<Live>>,
    /// Live holders in init/resume order; socket IDs do not order later reinits.
    init_order: Vec<u64>,
    labels: HashMap<u64, std::collections::BTreeMap<String, Option<Live>>>,
    /// The socket the signer and proxy token follow.
    signing: Option<u64>,
}

impl Holders {
    fn insert(&mut self, id: u64, live: Option<Live>) {
        self.init_order.retain(|holder| *holder != id);
        if live.is_some() {
            self.init_order.push(id);
        }
        self.by_id.insert(id, live);
    }

    fn remove(&mut self, id: u64) {
        self.init_order.retain(|holder| *holder != id);
        self.by_id.remove(&id);
    }

    /// `None` when `leaving` was not signing; otherwise the newest live holder
    /// to sign through next, or `Some(None)` when none is live.
    fn successor(&self, leaving: u64) -> Option<Option<(u64, Live)>> {
        if self.signing != Some(leaving) {
            return None;
        }
        Some(
            self.init_order
                .iter()
                .rev()
                .filter(|id| **id != leaving)
                .find_map(|id| self.by_id.get(id)?.as_ref().map(|live| (*id, live.clone()))),
        )
    }
}

impl Account {
    pub(crate) fn registry_name(&self) -> &str {
        self.identity.as_deref().unwrap_or(&self.name)
    }

    pub(crate) fn matches_init(&self, user: &str, init: &InitPayload) -> bool {
        self.user == user
            && identity::registry_name(user, init).as_deref() == Ok(self.registry_name())
    }

    pub(crate) fn moving(&self) -> bool {
        self.handle.disposal_cause() == Some(DisposalCause::Moving)
    }

    pub fn is_public(&self) -> bool {
        self.public_only
    }

    /// Live (not parked) sockets holding this account, other than `holder`.
    pub(crate) fn live_holders_besides(&self, holder: u64) -> Vec<u64> {
        self.holders
            .lock()
            .unwrap()
            .by_id
            .iter()
            .filter(|(id, live)| **id != holder && live.is_some())
            .map(|(id, _)| *id)
            .collect()
    }

    /// Signing and the proxy token follow `id` (the newest init or resume).
    fn sign_through(&self, holders: &mut Holders, id: u64, live: &Live) {
        self.twap_owner
            .rebind_owner(self.identity.as_deref().unwrap_or(&self.user));
        holders.signing = Some(id);
        self.held.store(true, Ordering::Relaxed);
        self.token.set(live.token.clone());
        if let Some(link) = &self.link {
            link.attach(id, live.outbox.clone());
        }
    }

    /// The signing socket left: hand over to the most recent live holder, or
    /// suspend when only parked holders remain.
    fn hand_over(&self, holders: &mut Holders, leaving: u64) {
        match holders.successor(leaving) {
            None => {}
            Some(Some((id, live))) => self.sign_through(holders, id, &live),
            Some(None) => {
                holders.signing = None;
                self.held.store(false, Ordering::Relaxed);
                if let Some(link) = &self.link {
                    link.suspend();
                }
            }
        }
    }

    fn hold(&self, id: u64, live: Live) {
        self.hold_label(id, live, &self.name);
    }

    fn hold_label(&self, id: u64, live: Live, label: &str) {
        let mut holders = self.holders.lock().unwrap();
        holders
            .labels
            .entry(id)
            .or_default()
            .insert(label.to_owned(), Some(live.clone()));
        holders.insert(id, Some(live.clone()));
        self.sign_through(&mut holders, id, &live);
    }

    pub(crate) fn labels(&self, id: u64) -> Vec<String> {
        self.holders
            .lock()
            .unwrap()
            .labels
            .get(&id)
            .map(|labels| labels.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn hold_registration(
        &self,
        id: u64,
        outbox: Outbox,
        token: Option<(String, u64)>,
        label: &str,
    ) {
        self.hold_label(id, Live { outbox, token }, label);
    }

    pub(crate) fn has_parked_holders(&self) -> bool {
        self.holders
            .lock()
            .unwrap()
            .by_id
            .values()
            .any(Option::is_none)
    }

    /// Sends a frame, if given, to every live socket holding this account.
    fn reach_live(&self, text: Option<String>) -> crate::algos::Holding {
        let holders = self.holders.lock().unwrap();
        let mut holding = crate::algos::Holding::default();
        for (id, holder) in &holders.by_id {
            match holder {
                Some(live) => {
                    holding.live.push(*id);
                    if let Some(text) = &text {
                        if self.identity.is_some()
                            && let Some(labels) = holders.labels.get(id)
                        {
                            for live in labels.values().flatten() {
                                (live.outbox)(text.clone());
                            }
                        } else {
                            (live.outbox)(text.clone());
                        }
                    }
                }
                None => holding.parked = true,
            }
        }
        holding
    }

    /// The current signing socket's outbox, for notices about the account itself.
    pub fn notify(&self, text: String) {
        let holders = self.holders.lock().unwrap();
        if let Some(Some(live)) = holders.signing.and_then(|id| holders.by_id.get(&id)) {
            (live.outbox)(text);
        }
    }

    fn live_outboxes(&self) -> Vec<Outbox> {
        let holders = self.holders.lock().unwrap();
        let mut outboxes = Vec::new();
        for (id, live) in &holders.by_id {
            if let Some(labels) = holders.labels.get(id).filter(|labels| !labels.is_empty()) {
                outboxes.extend(labels.values().flatten().map(|live| live.outbox.clone()));
            } else if let Some(live) = live {
                outboxes.push(live.outbox.clone());
            }
        }
        outboxes
    }

    /// Signing and the proxy token follow this socket, which already holds the account.
    pub fn sign_through_socket(&self, id: u64, outbox: Outbox, token: Option<(String, u64)>) {
        self.hold(id, Live { outbox, token });
    }

    pub(crate) fn update_reply_outbox(&self, id: u64, outbox: Outbox, label: &str) {
        let mut holders = self.holders.lock().unwrap();
        if let Some(Some(live)) = holders.by_id.get_mut(&id) {
            live.outbox = outbox.clone();
        }
        if let Some(Some(live)) = holders
            .labels
            .get_mut(&id)
            .and_then(|labels| labels.get_mut(label))
        {
            live.outbox = outbox;
        }
    }
}

/// How an account's algo host reaches its sockets, without a cycle back to it.
fn algo_reach(
    owner: Arc<Mutex<Option<std::sync::Weak<Account>>>>,
) -> (crate::algos::Emit, crate::algos::Broadcast) {
    let upgrade = |owner: &Mutex<Option<std::sync::Weak<Account>>>| {
        owner
            .lock()
            .unwrap()
            .as_ref()
            .and_then(std::sync::Weak::upgrade)
    };
    let emit_owner = owner.clone();
    (
        Box::new(move |text| {
            if let Some(account) = upgrade(&emit_owner) {
                account.notify(text);
            }
        }),
        Box::new(move |text| {
            upgrade(&owner).map_or_else(Default::default, |account| account.reach_live(text))
        }),
    )
}

struct Parked {
    user: String,
    accounts: Vec<(Arc<Account>, u64)>,
    alias_until: Option<u64>,
}

type AccountEntries = HashMap<(String, String), Arc<Account>>;

type PendingInits = Arc<Mutex<HashMap<(String, String), InitIncarnation>>>;

struct InitIncarnation {
    fingerprint: String,
    public_only: bool,
    attempts: Vec<Arc<AtomicBool>>,
    probes: Vec<Arc<AtomicBool>>,
}

#[derive(Clone)]
struct InitAuthority {
    current: Arc<AtomicBool>,
    lease: Option<crate::router_lease::Permit>,
    probe_lease: Option<crate::router_lease::Permit>,
    admission_expires_at: Option<u64>,
    probe_current: Option<Arc<AtomicBool>>,
}

impl InitAuthority {
    fn check(&self) -> Result<(), String> {
        if !self.current.load(Ordering::Acquire) {
            return Err("Account init superseded; initialization stopped".into());
        }
        if self
            .probe_current
            .as_ref()
            .is_some_and(|current| !current.load(Ordering::Acquire))
        {
            return Err("Account settings changed during verification".into());
        }
        if self.lease.as_ref().is_some_and(|lease| !lease.allowed()) {
            return Err("Router lease expired; initialization stopped".into());
        }
        if self
            .probe_lease
            .as_ref()
            .is_some_and(|lease| !lease.allowed())
        {
            return Err("Account verification expired; initialization stopped".into());
        }
        Ok(())
    }

    fn check_admission(&self) -> Result<(), String> {
        self.check()?;
        if self
            .admission_expires_at
            .is_some_and(|expiry| expiry <= now_ms())
        {
            return Err("Account verification expired; initialization stopped".into());
        }
        Ok(())
    }
}

struct InitAttempt<'a> {
    pending: &'a PendingInits,
    key: (String, String),
    authority: InitAuthority,
    /// Admission only: an account can subsequently be held by other tokens.
    expires_at_ms: Option<u64>,
    published: bool,
    /// Keep the reset registration alive through construction and publication.
    _probe: Option<ProbePermit>,
}

impl InitAttempt<'_> {
    fn forget(&self, pending: &mut HashMap<(String, String), InitIncarnation>) {
        if let Some(incarnation) = pending.get_mut(&self.key) {
            incarnation
                .attempts
                .retain(|current| !Arc::ptr_eq(current, &self.authority.current));
            if incarnation.attempts.is_empty() && incarnation.probes.is_empty() {
                pending.remove(&self.key);
            }
        }
    }

    fn publish(
        &mut self,
        accounts: &Mutex<HashMap<(String, String), Arc<Account>>>,
        account: &Arc<Account>,
    ) -> Result<Option<Arc<Account>>, String> {
        // Serialize the final check and insertion with a newer init's admission.
        let mut pending = self.pending.lock().unwrap();
        self.authority.check_admission()?;
        crate::auth::Verifier::check_expiry(self.expires_at_ms, now_ms())?;
        let replaced = accounts
            .lock()
            .unwrap()
            .insert(self.key.clone(), account.clone());
        self.forget(&mut pending);
        self.published = true;
        Ok(replaced)
    }
}

impl Drop for InitAttempt<'_> {
    fn drop(&mut self) {
        if !self.published {
            self.authority.current.store(false, Ordering::Release);
        }
        let mut pending = self.pending.lock().unwrap();
        self.forget(&mut pending);
    }
}

pub struct Registry {
    pub public_topics: Arc<crate::public_topics::PublicTopics>,
    pub user_drains: crate::user_drain::Users,
    pub router_leases: Arc<crate::router_lease::Users>,
    pub interruptions: Arc<crate::interruption::Notices>,
    accounts: Arc<Mutex<AccountEntries>>,
    pending_inits: PendingInits,
    proof_queues: proof_queue::ProofQueues,
    parked: Mutex<HashMap<String, Parked>>,
    /// Resumable sessions still open, by resume id: a resume of one whose drop
    /// the server has not seen yet (no close frame) takes it over.
    open: Mutex<HashMap<String, OpenSession>>,
    threads: EngineThreads,
    ids: AtomicU64,
    engine_config: ServerConfig,
    pub resume_grace: Duration,
    /// The process is going away: no new account starts.
    stopping: AtomicBool,
    /// Sticky before forced cleanup starts, until explicit undrain.
    box_forced: AtomicBool,
    /// The operator's drain before a restart (see DESIGN "Drain and roll").
    drain: tokio::sync::watch::Sender<Drain>,
    /// Bumped as any account's chases or TWAPs end.
    algos_ended: Arc<tokio::sync::watch::Sender<u64>>,
    /// 5-10 account starts per second per egress IP, with jitter (DESIGN sizing).
    pacer: crate::pacing::Pacer,
    /// Why algos stopped while no browser was there to see it, one per
    /// (user, account), for that account's next init.
    notices: Notices,
    /// When each user last opened a socket, and was last logged as not returning.
    presence: Arc<Mutex<HashMap<String, Presence>>>,
    /// The last forced-reset repair of each (user, account, topic) in this process.
    repairs: Mutex<HashMap<(String, String, String), std::time::Instant>>,
    /// Seeded test accounts can retain their pre-admission fixture setup.
    #[cfg(any(test, feature = "test-support"))]
    join_admission_override: Option<bool>,
    /// Unit-test loopback routing; the normal construction and transport run unchanged.
    #[cfg(test)]
    construction_test_plans: HashMap<String, RoutingPlan>,
    #[cfg(test)]
    e2e_venue: Option<crate::delegated_e2e_io::MockVenue>,
}

/// A chart's forced reset repairs its upstream stream at most this often per account and topic.
pub const REPAIR_EVERY: Duration = Duration::from_secs(30);

#[derive(Clone)]
struct Presence {
    socket_at: std::time::Instant,
    told_at: Option<std::time::Instant>,
    chat_handle: Option<crate::auth::ChatHandle>,
}

/// How long after its session is disposed for no browser a user may stay away unlogged.
const RETURN_WAIT: Duration = Duration::from_secs(5 * 60);
/// A user absent again within this is not logged again.
const RETURN_LOG_EVERY: Duration = Duration::from_secs(60 * 60);

/// An opaque, stable tag for a user in logs: never the raw id. The same form as the
/// routing Worker's tag (the first six bytes of SHA-256, hex), so the journals line up.
pub fn user_tag(user: &str) -> String {
    safe_cex_crypto::sha256(user.as_bytes())[..6]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn user_log_label(user: &str, chat_handle: Option<&crate::auth::ChatHandle>) -> String {
    let tag = user_tag(user);
    match chat_handle {
        Some(handle) => format!("{tag} chat={}", handle.as_str()),
        None => tag,
    }
}

/// The operator's drain of this box before a restart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Drain {
    #[default]
    Off,
    /// Sockets move as their accounts' chases and TWAPs end; users new here are moved.
    Waiting,
    /// Break-glass: algos were stopped and told; every socket moves.
    Now,
}

/// A still-open resumable session, as a resume of its id reaches it.
struct OpenSession {
    user: String,
    /// Tells its socket loop to close now, as though the drop had been seen.
    close: Arc<tokio::sync::Notify>,
    /// Told once its accounts are parked.
    parked: Vec<tokio::sync::oneshot::Sender<()>>,
}

/// Why an account's algos stopped, until its owning session has been told.
struct StopRecord {
    kept: crate::algos::Kept,
    at: tokio::time::Instant,
    /// The resume id of the session whose expiry stopped them.
    owner: String,
    /// Another session has been told: a tab reopened, or another device.
    told_other: bool,
}

/// Stop notices by (user, account, source fingerprint), even while unregistered.
type NoticeKey = (String, String, String);
type Notices = Arc<Mutex<HashMap<NoticeKey, StopRecord>>>;

/// How long an undelivered stop notice waits for the account's next init.
pub const NOTICE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Engine options for a hosted account: non-secret init fields only.
/// HyroTrader's origins are browser build configuration the server does not have;
/// without them its engine would start with no endpoints.
pub const HYROTRADER_UNHOSTED: &str = "HyroTrader accounts cannot run on the hosted server: its HyroTrader endpoints are not configured. Turn hosted mode off for this account.";

/// A venue the server cannot run correctly is refused at init, never started degraded.
pub fn refuse_unhosted(venue: &str) -> Result<(), String> {
    if venue == "hyrotrader" {
        return Err(HYROTRADER_UNHOSTED.into());
    }
    Ok(())
}

/// As NX `BYBIT_SITE_ID_EXCHANGES`: only these venues have a regional site.
const BYBIT_SITE_VENUES: [&str; 3] = ["bybit", "bybitinverse", "bybitspot"];
/// Venues that read the Hyperliquid builder and vault; the browser sends the
/// builder with every account's init, whatever its venue.
const HYPERLIQUID_VENUES: [&str; 2] = ["hyperliquid", "hyperliquidspot"];

/// A Bybit site on another venue would change nothing the browser engine does,
/// so the init is refused rather than trusted to route.
pub fn refuse_misplaced_site(init: &InitPayload) -> Result<(), String> {
    let has_site = init
        .bybit_site_id
        .as_ref()
        .is_some_and(|site| !site.is_empty());
    if has_site && !BYBIT_SITE_VENUES.contains(&init.exchange_name.as_str()) {
        return Err(format!(
            "bybitSiteId applies only to Bybit accounts, not {}",
            init.exchange_name
        ));
    }
    Ok(())
}

pub fn engine_options(init: &InitPayload, public_only: bool) -> Value {
    let mut options = Map::new();
    if !public_only {
        options.insert("key".into(), init.key.clone().into());
    }
    let text = [
        ("passphrase", &init.passphrase),
        ("applicationId", &init.application_id),
        ("apiKeyIndex", &init.api_key_index),
        ("paradexAddress", &init.paradex_address),
        ("subkeyAddress", &init.subkey_address),
        ("subaccount", &init.subaccount),
        ("orderlyExchangeId", &init.orderly_exchange_id),
        ("grvtTradingAccountId", &init.grvt_trading_account_id),
    ];
    for (field, value) in text {
        if let Some(value) = value.as_ref().filter(|value| !value.is_empty())
            && !public_only
        {
            options.insert(field.into(), value.clone().into());
        }
    }
    if init.exchange_name == "grvt"
        && !public_only
        && let Some(address) = init.grvt_signer_address.as_ref().filter(|v| !v.is_empty())
    {
        options.insert("grvtSignerAddress".into(), address.clone().into());
    }
    if init.testnet == Some(true) {
        options.insert("testnet".into(), true.into());
    }
    // The page sets this in production; BloFin refuses orders without its broker id.
    options.insert("brokerAttribution".into(), true.into());
    let hyperliquid = HYPERLIQUID_VENUES.contains(&init.exchange_name.as_str());
    if let Some(vault) = init.vault.as_ref().filter(|vault| !vault.is_empty())
        && !public_only
        && hyperliquid
    {
        options.insert("vault".into(), vault.clone().into());
    }
    // As the browser engine: the site and the market toggles shape public data too.
    if let Some(site) = init.bybit_site_id.as_ref().filter(|site| !site.is_empty()) {
        options.insert("bybitSiteId".into(), site.clone().into());
    }
    for (field, enabled) in [
        ("enableSpotMarkets", init.enable_spot_markets),
        ("enableInverseMarkets", init.enable_inverse_markets),
        ("enableOptionMarkets", init.enable_option_markets),
    ] {
        if enabled == Some(true) {
            options.insert(field.into(), true.into());
        }
    }
    // As `isUsableBuilder`: an unusable builder falls back to the default.
    // The registry parses `feeTenthsBps` as u32, so it must go out as an integer.
    if let Some(builder) = init
        .hyperliquid_builder
        .as_ref()
        .filter(|builder| hyperliquid && usable_builder(builder))
    {
        options.insert(
            "hyperliquidBuilder".into(),
            json!({"address": builder.address, "feeTenthsBps": builder.fee_tenths_bps as u32}),
        );
    }
    // Lighter promotes a failing direct socket to read-only, never a routed one.
    if init.exchange_name.starts_with("lighter") && init.tealstreet_ws_route == Some(true) {
        options.insert("lighterWsRoute".into(), true.into());
    }
    if public_only {
        options.insert("publicDataOnly".into(), true.into());
    }
    Value::Object(options)
}

/// What an init resolves to before its engine thread: the routing plan and the
/// keyless engine options, rewritten for remote signing when `link` is given.
pub struct Prepared {
    pub plan: RoutingPlan,
    pub options: String,
    pub remote: Option<safe_cex_remote_signer::RemoteAccount>,
}

/// The account's resolved routing, as its engine thread receives it.
pub(crate) fn routing_plan(init: &InitPayload, public_only: bool) -> Result<RoutingPlan, String> {
    let venue = &init.exchange_name;
    let transport_flags = AccountTransport {
        tealstreet_proxy: init.tealstreet_proxy,
        tealstreet_tunnel: init.tealstreet_tunnel,
        tealstreet_ws_route: init.tealstreet_ws_route,
        proxy_region: init.proxy_region.clone(),
        testnet: init.testnet,
    };
    refuse_unhosted(venue)?;
    refuse_misplaced_site(init)?;
    let mut inputs =
        resolve::account_transport(venue, &transport_flags).map_err(|error| error.to_string())?;
    // The Brazil site has its own host and `x-site-id` header.
    inputs.bybit_site_id = init.bybit_site_id.clone().filter(|site| !site.is_empty());
    // WOO's private relay embeds the same public application ID as its engine.
    if venue == "woo" {
        inputs.application_id = init.application_id.clone().filter(|id| !id.is_empty());
    }
    if public_only {
        inputs.public_data_only = Some(true);
        inputs.has_key = false;
    }
    let base_url = resolve::rest_base_url(venue, init.testnet.unwrap_or(false)).map_err(|_| {
        format!("{venue} cannot run on the Hosted server. Turn Hosted off to use this account.")
    })?;
    resolve::resolve_routing_plan(venue, &inputs, base_url, Provider::Tealstreet)
        .map_err(|error| error.to_string())
}

/// What an account's engine is built from: its venue, its venue-scoped engine
/// options and its resolved routing, as one flat JSON object (so the names of
/// differing fields stay readable). Two inits that would build the same engine
/// share it, whatever else differs between the clients that sent them: a field
/// another venue reads, a default sent explicitly, flags resolving alike.
/// Hands a disposed account's stop notices and chase ends to its sockets. A later,
/// fuller `Kept` repeats what came before it, so only what is new is sent.
fn tell_through(account: &Account, outboxes: Vec<Outbox>) -> crate::algos::StopNotice {
    let name = account.name.clone();
    let told = Mutex::new((0, 0));
    Box::new(move |kept: crate::algos::Kept| {
        let mut told = told.lock().unwrap();
        let new = crate::algos::Kept {
            notices: kept.notices.get(told.0..).unwrap_or_default().to_vec(),
            ends: kept.ends.get(told.1..).unwrap_or_default().to_vec(),
        };
        *told = (kept.notices.len(), kept.ends.len());
        for frame in new.frames(&name) {
            for outbox in &outboxes {
                outbox(frame.clone());
            }
        }
    })
}

pub fn build_fingerprint(init: &InitPayload) -> String {
    let public_only = init.public_only == Some(true) || init.account_name.starts_with("__pub_");
    let mut fields = match engine_options(init, public_only) {
        Value::Object(options) => options,
        _ => Map::new(),
    };
    fields.insert("venue".into(), init.exchange_name.clone().into());
    // An init that cannot route is refused at build; its raw flags still tell inits apart.
    let routing = match routing_plan(init, public_only) {
        Ok(plan) => serde_json::to_value(plan).unwrap_or(Value::Null),
        Err(_) => json!([
            init.tealstreet_proxy,
            init.tealstreet_tunnel,
            init.tealstreet_ws_route,
            init.proxy_region
        ]),
    };
    fields.insert("routing".into(), routing);
    for field in ["key", "passphrase"] {
        if identity::enabled(init) {
            fields.remove(field);
            continue;
        }
        if let Some(Value::String(text)) = fields.get_mut(field) {
            *text = fingerprint_digest(text);
        }
    }
    Value::Object(fields).to_string()
}

/// A fingerprint only compares credentials, so it keeps none: this digest is
/// salted per process, and nothing outlives the process to compare against.
fn fingerprint_digest(text: &str) -> String {
    static SALT: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();
    let salt = SALT.get_or_init(|| {
        let mut salt = [0u8; 32];
        getrandom::getrandom(&mut salt).expect("OS entropy");
        salt
    });
    let digest = safe_cex_crypto::sha256(&[&salt[..], text.as_bytes()].concat());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn prepare(
    init: &InitPayload,
    public_only: bool,
    link: Option<Arc<SignerLink>>,
) -> Result<Prepared, String> {
    let venue = &init.exchange_name;
    let plan = routing_plan(init, public_only)?;
    let options = engine_options(init, public_only).to_string();
    let Some(link) = link else {
        return Ok(Prepared {
            plan,
            options,
            remote: None,
        });
    };
    let remote = remote_account(link, venue, Some(init.account_name.clone()), &options)
        .map_err(|error| error.to_string())?;
    Ok(Prepared {
        plan,
        options: remote.options_json.clone(),
        remote: Some(remote),
    })
}

fn usable_builder(builder: &crate::envelope::HyperliquidBuilder) -> bool {
    let address = builder.address.as_bytes();
    address.len() == 42
        && address.starts_with(b"0x")
        && address[2..].iter().all(u8::is_ascii_hexdigit)
        && builder.fee_tenths_bps.fract() == 0.0
        && builder.fee_tenths_bps > 0.0
        && builder.fee_tenths_bps <= f64::from(u32::MAX)
}

impl Registry {
    pub fn new(threads: EngineThreads, account_flush: Duration, resume_grace: Duration) -> Self {
        Self {
            public_topics: Arc::default(),
            user_drains: crate::user_drain::Users::default(),
            router_leases: Arc::new(crate::router_lease::Users::new(Default::default())),
            interruptions: Default::default(),
            accounts: Arc::default(),
            pending_inits: Arc::default(),
            proof_queues: Default::default(),
            parked: Mutex::default(),
            open: Mutex::default(),
            threads,
            ids: AtomicU64::new(1),
            engine_config: ServerConfig {
                flush_every: account_flush,
                ..ServerConfig::default()
            },
            resume_grace,
            stopping: AtomicBool::new(false),
            box_forced: AtomicBool::new(false),
            drain: tokio::sync::watch::channel(Drain::Off).0,
            algos_ended: Arc::new(tokio::sync::watch::channel(0).0),
            pacer: crate::pacing::Pacer::new(7, Duration::from_millis(60)),
            notices: Arc::default(),
            presence: Arc::default(),
            repairs: Mutex::default(),
            #[cfg(any(test, feature = "test-support"))]
            join_admission_override: None,
            #[cfg(test)]
            construction_test_plans: HashMap::new(),
            #[cfg(test)]
            e2e_venue: None,
        }
    }

    pub fn with_router_lease(mut self, config: crate::router_lease::Config) -> Self {
        self.interruptions = Arc::new(crate::interruption::Notices::with_box(
            config.box_id.clone(),
            config.display_name.clone(),
        ));
        self.router_leases = Arc::new(crate::router_lease::Users::new(config));
        self
    }

    pub fn user_lease_fenced(&self, user: &str) -> bool {
        self.router_leases.fenced(user)
    }

    pub fn park_expired_router_leases(self: &Arc<Self>) {
        let accounts: Vec<_> = self.accounts.lock().unwrap().values().cloned().collect();
        for account in accounts {
            if !account.running.claim_lease_park() {
                continue;
            }
            let tag = user_tag(&account.user);
            Log::error(
                &format!("router lease fenced user_tag={tag}"),
                &crate::telemetry::Report {
                    kind: "router_lease.fenced",
                    summary: "router lease fenced; the account's algos stop on this box",
                    user_tag: Some(tag.clone()),
                    venue: Some(&account.venue),
                    ..Default::default()
                },
            );
            account.handle.begin_disposal(DisposalCause::Stopped);
            let stopped = self.keep_notice(&account, "router-lease");
            if !account.algos.as_ref().is_some_and(|sender| {
                sender
                    .send(crate::algos::AlgoInput::RouterFence { stopped })
                    .is_ok()
            }) {
                account.handle.dispose();
                if let Some(link) = &account.link {
                    link.mark_unavailable();
                }
            }
            let mut cached = self.accounts.lock().unwrap();
            let key = (account.user.clone(), account.registry_name().to_owned());
            if cached
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, &account))
            {
                cached.remove(&key);
            }
        }
    }

    /// Whether a forced reset may repair `topic` on `account` now, and records it if so.
    /// Each user's engine has its own venue socket, so the limit is per account, not per topic.
    pub fn allow_repair(
        &self,
        user: &str,
        account: &str,
        topic: &str,
        now: std::time::Instant,
    ) -> bool {
        let mut repairs = self.repairs.lock().unwrap();
        repairs.retain(|_, at| now.duration_since(*at) < REPAIR_EVERY);
        let key = (user.to_owned(), account.to_owned(), topic.to_owned());
        if repairs.contains_key(&key) {
            return false;
        }
        repairs.insert(key, now);
        true
    }

    /// Server-allocated, process-unique IDs for sockets, park holders and clients.
    pub fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }

    pub fn router_lease_counts(&self) -> crate::router_lease::Counts {
        let mut users = self.user_drains.users_with_sockets();
        users.extend(
            self.accounts
                .lock()
                .unwrap()
                .values()
                .filter(|account| !account.is_public())
                .map(|account| account.user.clone()),
        );
        self.router_leases.counts(&users)
    }

    pub fn account_count(&self) -> usize {
        self.accounts.lock().unwrap().len()
    }

    pub fn threads(&self) -> &EngineThreads {
        &self.threads
    }

    /// Private settings changes require an explicit admitted-client action.
    /// Live/parked sockets and resident copies keep their existing settings.
    pub fn rebuild_refusal(
        &self,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
    ) -> Option<String> {
        self.rebuild_refusal_for_mode(user, init, public_only, holder, false)
    }

    /// Admission check shared by socket fast paths and trusted construction.
    pub fn signing_mode_refusal(
        &self,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        delegated: bool,
    ) -> Option<String> {
        let accounts = self.accounts.lock().unwrap();
        let name = match if public_only {
            Ok(init.account_name.clone())
        } else {
            identity::registry_name(user, init)
        } {
            Ok(name) => name,
            Err(error) => return Some(error),
        };
        let keyed = accounts.get(&(user.to_owned(), name));
        if keyed.is_some_and(|account| {
            account.identity.is_some() != (!public_only && identity::enabled(init))
        }) {
            return Some(
                "An existing account has a conflicting label. Rename it and try again.".into(),
            );
        }
        let account = keyed.or_else(|| {
            if !public_only {
                return None;
            }
            accounts.values().find(|account| {
                account.user == user
                    && account.delegated.is_some()
                    && account.name == init.account_name
            })
        })?;
        if public_only && account.delegated.is_none() {
            return None;
        }
        (!account.is_public() && account.delegated.is_some() != delegated)
            .then(|| "Account signing mode differs; account not attached".to_owned())
    }

    fn rebuild_refusal_for_mode(
        &self,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
        delegated: bool,
    ) -> Option<String> {
        if let Some(refusal) = self.signing_mode_refusal(user, init, public_only, delegated) {
            return Some(refusal);
        }
        let name = if public_only {
            init.account_name.clone()
        } else {
            match identity::registry_name(user, init) {
                Ok(name) => name,
                Err(error) => return Some(error),
            }
        };
        let key = (user.to_owned(), name);
        let account = {
            let accounts = self.accounts.lock().unwrap();
            let identity_account = if public_only {
                accounts
                    .values()
                    .find(|account| {
                        account.user == user
                            && account.identity.is_some()
                            && (account.name == init.account_name
                                || account
                                    .holders
                                    .lock()
                                    .unwrap()
                                    .labels
                                    .values()
                                    .any(|labels| labels.contains_key(&init.account_name)))
                    })
                    .cloned()
            } else {
                None
            };
            identity_account.or_else(|| accounts.get(&key).cloned())
        }?;
        // Shared public data carries no signer or algos; it rebuilds as before.
        if account.is_public() {
            return None;
        }
        let fingerprint = if delegated {
            crate::delegated::fingerprint(init)
        } else {
            init.fingerprint()
        };
        if account.fingerprint == fingerprint && !public_only {
            return None;
        }
        let differs = differing_fields(&account.fingerprint, &fingerprint);
        Log::warn(&format!(
            "init {} from socket {holder} refused: explicit settings action required ({})",
            init.account_name,
            differs.join(", ")
        ));
        Some(format!(
            "{} is already running with different settings ({}). The change was not applied. Join with the current settings and use apply-settings before reinitializing.",
            init.account_name,
            differs.join(", ")
        ))
    }

    fn begin_init(
        &self,
        user: &str,
        name: &str,
        public_only: bool,
        fingerprint: &str,
        expires_at_ms: Option<u64>,
    ) -> Result<InitAttempt<'_>, String> {
        crate::auth::Verifier::check_expiry(expires_at_ms, now_ms())?;
        let lease = if public_only {
            None
        } else {
            match self.router_leases.get(user) {
                Some(lease) => Some(lease.permit()),
                None if !self.router_leases.enforced() => None,
                None => return Err("Router lease unavailable; no account work was sent".into()),
            }
        };
        let authority = InitAuthority {
            current: Arc::new(AtomicBool::new(true)),
            lease,
            probe_lease: None,
            admission_expires_at: None,
            probe_current: None,
        };
        authority.check_admission()?;
        let key = (user.to_owned(), name.to_owned());
        {
            let mut pending = self.pending_inits.lock().unwrap();
            let incarnation = pending
                .entry(key.clone())
                .or_insert_with(|| InitIncarnation {
                    fingerprint: fingerprint.to_owned(),
                    public_only,
                    attempts: vec![],
                    probes: vec![],
                });
            if incarnation.fingerprint != fingerprint || incarnation.public_only != public_only {
                for previous in &incarnation.attempts {
                    previous.store(false, Ordering::Release);
                }
                incarnation.attempts.clear();
                incarnation.fingerprint = fingerprint.to_owned();
                incarnation.public_only = public_only;
            }
            incarnation.attempts.push(authority.current.clone());
        }
        Ok(InitAttempt {
            pending: &self.pending_inits,
            key,
            authority,
            expires_at_ms,
            published: false,
            _probe: None,
        })
    }

    /// The account this user's `init` names, built or reused. Reuse requires an
    /// identical fingerprint; private settings changes require `apply_settings`.
    pub async fn obtain(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
    ) -> Result<Arc<Account>, String> {
        self.obtain_with_pair(user, init, public_only, holder, token, outbox, None, None)
            .await
    }

    /// Host-only entry point after handover verification. Never opens socket data.
    #[allow(clippy::too_many_arguments)]
    pub async fn obtain_delegated(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        pair: DelegatedPair,
    ) -> Result<Arc<Account>, String> {
        pair.validate(init)?;
        self.obtain_with_pair(user, init, false, holder, token, outbox, Some(pair), None)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn apply_settings(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        account: Arc<Account>,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
    ) -> Result<Arc<Account>, String> {
        if account.identity.is_none()
            || !account.matches_init(user, init)
            || !self.holds(&account, holder)
            || init.sealed.is_some()
            || account.delegated.is_some() != (init.signing_mode.as_deref() == Some("delegated"))
        {
            return Err("Settings can only be applied to the joined account identity".into());
        }
        if token
            .as_ref()
            .is_none_or(|(_, expires)| *expires <= now_ms())
        {
            return Err("Tealstreet proxy token expired; settings were not applied".into());
        }
        let setup = if let Some(resident) = &account.delegated {
            resident.reconfigure(init)?;
            Some(DelegatedSetup::Resident(Box::new(resident.clone())))
        } else {
            prepare(init, false, Some(SignerLink::new(holder, outbox.clone())))?;
            None
        };
        self.obtain_with_setup(
            user,
            init,
            false,
            holder,
            token,
            outbox,
            setup,
            None,
            Some(account),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn obtain_with_pair(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        pair: Option<DelegatedPair>,
        proof: Option<ProbePermit>,
    ) -> Result<Arc<Account>, String> {
        self.obtain_with_setup(
            user,
            init,
            public_only,
            holder,
            token,
            outbox,
            pair.map(DelegatedSetup::Pair),
            proof,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn obtain_with_setup(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
        setup: Option<DelegatedSetup>,
        proof: Option<ProbePermit>,
        apply: Option<Arc<Account>>,
    ) -> Result<Arc<Account>, String> {
        let expires_at_ms = token.as_ref().map(|(_, expires)| *expires);
        crate::auth::Verifier::check_expiry(expires_at_ms, now_ms())?;
        if self.stopping.load(Ordering::Relaxed) {
            return Err("The server is restarting; reconnecting".into());
        }
        let delegated = setup.is_some();
        if let Some(refusal) = self.signing_mode_refusal(user, init, public_only, delegated) {
            return Err(refusal);
        }
        if public_only
            && let Some(refusal) =
                self.rebuild_refusal_for_mode(user, init, public_only, holder, delegated)
        {
            return Err(refusal);
        }
        let fingerprint = if delegated {
            crate::delegated::fingerprint(init)
        } else {
            init.fingerprint()
        };
        let registry_name = if public_only {
            init.account_name.clone()
        } else {
            identity::registry_name(user, init)?
        };
        let mut attempt = self.begin_init(
            user,
            &registry_name,
            public_only,
            &fingerprint,
            expires_at_ms,
        )?;
        if apply.is_some() {
            attempt.authority.admission_expires_at = token.as_ref().map(|(_, expiry)| *expiry);
        }
        let probed = proof.is_some();
        if let Some(proof) = proof {
            proof
                .check(user, init)
                .map_err(|_| "Account verification expired".to_owned())?;
            attempt.authority.probe_lease = proof.lease.clone();
            attempt.authority.admission_expires_at = proof.token_expires_at;
            attempt.authority.probe_current = Some(proof.current.clone());
            attempt._probe = Some(proof);
        }
        let key = attempt.key.clone();
        let existing = self.accounts.lock().unwrap().get(&key).cloned();
        if let Some(expected) = &apply
            && (expected.identity.is_none()
                || !self.holds(expected, holder)
                || existing
                    .as_ref()
                    .is_none_or(|current| !Arc::ptr_eq(current, expected)))
        {
            return Err("The account registration changed; settings were not applied".into());
        }
        if let Some(account) = existing {
            if probed && (account.fingerprint != fingerprint || account.is_public()) {
                return Err("Account settings differ; account not attached".into());
            }
            if probed {
                attempt.authority.check_admission()?;
            }
            if !account.is_public() && account.delegated.is_some() != delegated {
                return Err("Account signing mode differs; account not attached".into());
            }
            if account.fingerprint == fingerprint && account.is_public() == public_only {
                crate::auth::Verifier::check_expiry(expires_at_ms, now_ms())?;
                account.hold_label(holder, Live { outbox, token }, &init.account_name);
                return Ok(account);
            }
            if apply.is_none()
                && let Some(refusal) =
                    self.rebuild_refusal_for_mode(user, init, public_only, holder, delegated)
            {
                return Err(refusal);
            }
            if apply.is_none() {
                self.dispose(
                    &account,
                    Disposal::rebuild(account.is_public(), public_only),
                );
            }
        }
        let live = Live {
            outbox: outbox.clone(),
            token: token.clone(),
        };
        let (account, remote) = self
            .build(
                user,
                init,
                public_only,
                fingerprint,
                token,
                holder,
                outbox.clone(),
                &attempt.authority,
                setup,
            )
            .await?;
        let account = Arc::new(account);
        if let Err(error) = attempt.authority.check_admission().and_then(|()| {
            crate::auth::Verifier::check_expiry(expires_at_ms, now_ms()).map_err(str::to_owned)
        }) {
            account.handle.dispose();
            if let Some(link) = &account.link {
                link.mark_unavailable();
            }
            return Err(error);
        }
        self.user_drains.track(
            user,
            account.registry_name(),
            &account.running,
            Arc::downgrade(&account),
        );
        if self.user_drains.get(user).latched() {
            Self::stop(
                &account,
                "the user is draining",
                None,
                DisposalCause::Moving,
            );
            return Err("The user is draining; reconnecting".into());
        }
        *account.algo_owner.lock().unwrap() = Some(Arc::downgrade(&account));
        account.hold_label(holder, live, &init.account_name);
        if let Some(remote) = remote {
            let registry = Arc::downgrade(self);
            let weak = Arc::downgrade(&account);
            remote.on_fatal(move || {
                if let (Some(registry), Some(account)) = (registry.upgrade(), weak.upgrade()) {
                    registry.signer_failed(&account);
                }
            });
        }
        let replaced = match attempt.publish(&self.accounts, &account) {
            Ok(replaced) => replaced,
            Err(error) => {
                account.handle.dispose();
                if let Some(link) = &account.link {
                    link.mark_unavailable();
                }
                return Err(error);
            }
        };
        if let Some(replaced) = replaced {
            // Only current settings can publish; identical inits may still race.
            // Its holders go too, so the losing socket stops calling a stopped account.
            if apply.is_some() {
                self.stop_for_new_settings(&replaced, outbox, Disposal::SettingsChanged);
            } else {
                self.dispose_for(
                    &replaced,
                    Disposal::Replaced,
                    "account replaced by a newer init",
                    Some(tell_through(&replaced, replaced.live_outboxes())),
                );
            }
        }
        Ok(account)
    }

    #[allow(clippy::too_many_arguments)]
    async fn build(
        &self,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        fingerprint: String,
        token: Option<(String, u64)>,
        socket: u64,
        outbox: Outbox,
        authority: &InitAuthority,
        setup: Option<DelegatedSetup>,
    ) -> Result<(Account, Option<safe_cex_remote_signer::RemoteAccount>), String> {
        self.pacer.admit().await;
        authority.check_admission()?;
        crate::auth::Verifier::check_expiry(token.as_ref().map(|(_, expires)| *expires), now_ms())?;
        let venue = init.exchange_name.clone();
        let delegated_build = setup.is_some();
        let (link, plan, options, remote, signer, delegated) = if let Some(setup) = setup {
            let PreparedDelegated {
                plan,
                options,
                resident,
            } = match setup {
                DelegatedSetup::Pair(pair) => prepare_delegated(init, pair)?,
                DelegatedSetup::Resident(resident) => resident.reconfigure(init)?,
            };
            let signer = Some(resident.provider());
            (None, plan, options, None, signer, Some(resident))
        } else {
            let link = (!public_only).then(|| SignerLink::new(socket, outbox));
            let Prepared {
                plan,
                options,
                remote,
            } = prepare(init, public_only, link.clone())?;
            let signer = remote.as_ref().map(|remote| remote.provider.clone());
            (link, plan, options, remote, signer, None)
        };
        let proxy_token = ProxyToken::new();
        // Preserve COSIGNED keyed construction; DELEGATED supplies its own provider.
        if !delegated_build {
            assert_eq!(
                signer.is_none(),
                public_only,
                "private accounts sign remotely"
            );
        } else {
            assert!(!public_only && signer.is_some() && link.is_none());
        }
        // Public-only reads on a proxied venue need the proxy too.
        proxy_token.set(token);
        let tokens = proxy_token.provider();
        let mut config = self.engine_config.clone();
        config.trusted_signer_state = None;
        if let Some(resident) = &delegated {
            resident.configure(&mut config);
        }
        config.log_label = Some(self.account_log_label(user, &init.account_name, &venue));
        // Public and COSIGNED diagnostic modes retain their original values.
        config.signing_mode = Some(if public_only {
            "public"
        } else if delegated_build {
            "hosted_delegated"
        } else {
            "hosted_cosign"
        });
        let algo_owner: Arc<Mutex<Option<std::sync::Weak<Account>>>> = Arc::default();
        let placed_symbols = PlacedSymbols::default();
        let algo_placed = placed_symbols.clone();
        let held = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let algo_held = held.clone();
        let running = Arc::new(crate::algos::Running::new(self.algos_ended.clone()));
        let identity = if public_only {
            None
        } else {
            identity::digest(user, init)?
        };
        let registry_name = identity.as_deref().unwrap_or(&init.account_name);
        self.user_drains
            .track(user, registry_name, &running, std::sync::Weak::new());
        if let Some(lease) = &authority.lease {
            running.attach_lease(lease.clone());
        }
        running.attach_restores(self.interruptions.account(user, registry_name));
        let algo_running = running.clone();
        let algo_identity = identity.clone();
        let (algos, algo_inbox) = if public_only {
            (None, None)
        } else {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        };
        let algo_client = self.next_id();
        let twap_owner = Arc::new(crate::twap_owner::Binding::new(
            identity.as_deref().unwrap_or(user),
        ));
        let algo_twap_owner = twap_owner.clone();
        let algo_signer = link.clone();
        let owner = algo_owner.clone();
        let (emit, broadcast) = algo_reach(owner);
        let algo_who = crate::audit::Who {
            user: user.to_owned(),
            account: init.account_name.clone(),
            venue: venue.clone(),
        };
        if public_only {
            config.interest_observer = Some(self.public_topics.observer(&venue));
        }
        let (spawned, receive) = tokio::sync::oneshot::channel();
        #[cfg(test)]
        let plan = self
            .construction_test_plans
            .get(user)
            .cloned()
            .unwrap_or(plan);
        let authority = authority.clone();
        #[cfg(test)]
        let e2e_venue = self.e2e_venue.clone();
        let placed = self.threads.place(move |placement| {
            if let Err(error) = authority.check_admission() {
                let _ = spawned.send(Err(error));
                return;
            }
            let mut transport = NetworkTransport::with_routing(plan, tokens).with_venue(&venue);
            transport = transport.with_io_guard(Arc::new(move || authority.check().is_ok()));
            #[cfg(test)]
            let transport = crate::delegated_e2e_io::Io::new(transport, e2e_venue);
            let result =
                safe_cex_native::spawn_venue_account(&venue, &options, transport, signer, config)
                    .map(|(handle, task)| {
                        tokio::task::spawn_local(async move {
                            let _ = task.await;
                            drop(placement);
                        });
                        if let Some(inbox) = algo_inbox {
                            tokio::task::spawn_local(crate::algos::run_with_identity(
                                handle.clone(),
                                algo_client,
                                algo_who,
                                emit,
                                broadcast,
                                algo_signer,
                                algo_placed,
                                algo_held,
                                algo_running,
                                algo_twap_owner,
                                algo_identity,
                                inbox,
                            ));
                        }
                        handle
                    });
            let _ = spawned.send(result.map_err(|error| error.to_string()));
        });
        if !placed {
            return Err("engine threads stopped".into());
        }
        let handle = receive
            .await
            .map_err(|_| "engine thread failed".to_owned())??;
        let account = Account {
            user: user.to_owned(),
            name: init.account_name.clone(),
            identity,
            venue: init.exchange_name.clone(),
            handle,
            link,
            public_only,
            delegated,
            token: proxy_token,
            fingerprint,
            holders: Mutex::default(),
            algos,
            algo_owner,
            placed: placed_symbols,
            held,
            frame_reads: Arc::new(tokio::sync::Semaphore::new(MAX_FRAME_READS)),
            reads: Arc::new(tokio::sync::Semaphore::new(MAX_READS)),
            metadata_refreshes: Arc::new(tokio::sync::Semaphore::new(MAX_METADATA_REFRESHES)),
            running,
            twap_owner,
        };
        Ok((account, remote))
    }

    /// The account `user` holds under `name` through `holder`, never another user's.
    pub fn held(&self, user: &str, name: &str, holder: u64) -> Option<Arc<Account>> {
        let accounts = self.accounts.lock().unwrap();
        let account = accounts
            .get(&(user.to_owned(), name.to_owned()))
            .filter(|account| account.identity.is_none() && self.holds(account, holder))
            .cloned()
            .or_else(|| {
                accounts
                    .values()
                    .find(|account| {
                        account.user == user
                            && account.labels(holder).iter().any(|label| label == name)
                    })
                    .cloned()
            })?;
        let holds = account.holders.lock().unwrap().by_id.contains_key(&holder);
        holds.then_some(account)
    }

    /// Lets go of one holder; the last one disposes the account, for `disposal`.
    pub fn release(&self, account: &Arc<Account>, holder: u64, disposal: Disposal) {
        let empty = {
            let mut holders = account.holders.lock().unwrap();
            holders.remove(holder);
            holders.labels.remove(&holder);
            account.hand_over(&mut holders, holder);
            holders.by_id.is_empty()
        };
        if empty {
            self.dispose_for(account, disposal, "account disposed", None);
        }
    }

    pub(crate) fn release_label(
        &self,
        account: &Arc<Account>,
        holder: u64,
        label: &str,
        disposal: Disposal,
    ) {
        let still_held = {
            let mut holders = account.holders.lock().unwrap();
            let remaining = if let Some(labels) = holders.labels.get_mut(&holder) {
                labels.remove(label);
                labels.values().find_map(Clone::clone)
            } else {
                None
            };
            if let Some(live) = remaining {
                holders.by_id.insert(holder, Some(live.clone()));
                if holders.signing == Some(holder) {
                    account.sign_through(&mut holders, holder, &live);
                }
                true
            } else {
                false
            }
        };
        if !still_held {
            self.release(account, holder, disposal);
        }
    }

    pub(crate) fn account_log_label(&self, user: &str, name: &str, venue: &str) -> String {
        let presence = self.presence.lock().unwrap();
        let handle = presence
            .get(user)
            .and_then(|seen| seen.chat_handle.as_ref());
        format!("{} {name} {venue}", user_log_label(user, handle))
    }

    /// A socket of `user` opened: retain only its validated public handle for logs.
    pub fn user_connected(&self, user: &str, chat_handle: Option<&crate::auth::ChatHandle>) {
        Log::info(&format!(
            "hosted user connected: {}",
            user_log_label(user, chat_handle)
        ));
        let now = std::time::Instant::now();
        let mut presence = self.presence.lock().unwrap();
        presence
            .entry(user.to_owned())
            .and_modify(|seen| {
                seen.socket_at = now;
                seen.chat_handle = chat_handle.cloned();
            })
            .or_insert(Presence {
                socket_at: now,
                told_at: None,
                chat_handle: chat_handle.cloned(),
            });
    }

    /// The line for a user whose session was disposed for no browser at `disposed_at`
    /// and who has opened no socket since, at most once per user per RETURN_LOG_EVERY.
    fn did_not_return(
        &self,
        user: &str,
        disposed_at: std::time::Instant,
        now: std::time::Instant,
    ) -> Option<String> {
        let mut presence = self.presence.lock().unwrap();
        let seen = presence.entry(user.to_owned()).or_insert(Presence {
            socket_at: disposed_at,
            told_at: None,
            chat_handle: None,
        });
        if seen.socket_at > disposed_at
            || seen
                .told_at
                .is_some_and(|told| now.duration_since(told) < RETURN_LOG_EVERY)
        {
            return None;
        }
        seen.told_at = Some(now);
        Some(format!(
            "hosted user did not return: {} after {} min",
            user_log_label(user, seen.chat_handle.as_ref()),
            now.duration_since(disposed_at).as_secs() / 60
        ))
    }

    /// The stop notice for the same fingerprint and session resumed `from`, if still owed.
    /// Another session's init must not cost the owner it, and a session that can't
    /// be the owner (a reopened tab, a second device) is told once between them.
    pub fn take_notice(
        &self,
        user: &str,
        name: &str,
        fingerprint: &str,
        from: Option<&str>,
    ) -> Option<crate::algos::Kept> {
        let mut notices = self.notices.lock().unwrap();
        let key = (user.to_owned(), name.to_owned(), fingerprint.to_owned());
        let record = notices.get_mut(&key)?;
        if record.at.elapsed() >= NOTICE_TTL {
            notices.remove(&key);
            return None;
        }
        if from == Some(record.owner.as_str()) {
            return notices.remove(&key).map(|record| record.kept);
        }
        if record.told_other {
            return None;
        }
        record.told_other = true;
        Some(record.kept.clone())
    }

    /// Where the algo host leaves a notice for a browser that is gone.
    pub(crate) fn keep_notice(&self, account: &Account, owner: &str) -> crate::algos::StopNotice {
        let notices = self.notices.clone();
        let accounts = Arc::downgrade(&self.accounts);
        let key = (account.user.clone(), account.registry_name().to_owned());
        let fingerprint = account.fingerprint.clone();
        let notice_key = (key.0.clone(), key.1.clone(), fingerprint.clone());
        let owner = owner.to_owned();
        Box::new(move |kept| {
            let Some(accounts) = accounts.upgrade() else {
                return;
            };
            // Keep the ownership comparison and retained insertion atomic with replacement.
            let accounts = accounts.lock().unwrap();
            if accounts
                .get(&key)
                .is_some_and(|current| current.fingerprint != fingerprint)
            {
                return;
            }
            let mut notices = notices.lock().unwrap();
            notices.retain(|_, record| record.at.elapsed() < NOTICE_TTL);
            notices.insert(
                notice_key.clone(),
                StopRecord {
                    kept,
                    at: tokio::time::Instant::now(),
                    owner: owner.clone(),
                    told_other: false,
                },
            );
        })
    }

    /// With algos, their host settles chases first, then disposes the engine and signer.
    fn stop(
        account: &Account,
        reason: &str,
        stopped: Option<crate::algos::StopNotice>,
        cause: DisposalCause,
    ) {
        account.handle.begin_disposal(cause);
        if !account.running.io_allowed() {
            let stopped = stopped.unwrap_or_else(|| Box::new(|_| {}));
            if !account.algos.as_ref().is_some_and(|sender| {
                sender
                    .send(crate::algos::AlgoInput::RouterFence { stopped })
                    .is_ok()
            }) {
                account.handle.dispose();
                if let Some(link) = &account.link {
                    link.mark_unavailable();
                }
            }
            return;
        }
        let handed = account.algos.as_ref().is_some_and(|algos| {
            algos
                .send(crate::algos::AlgoInput::Dispose {
                    reason: reason.to_owned(),
                    stopped,
                })
                .is_ok()
        });
        if !handed {
            if let Some(link) = &account.link {
                link.mark_unavailable();
            }
            account.handle.dispose();
        }
    }

    /// A signer that keeps answering with failures stops the account. The page
    /// re-inits on its next pong; algos do not restart, so whoever holds it is told.
    fn signer_failed(&self, account: &Arc<Account>) {
        Log::error(
            &format!("signer for {} failing; disposing", account.venue),
            &crate::telemetry::Report {
                kind: "signer.failing",
                summary: "signer keeps failing; account disposed",
                user_tag: Some(user_tag(&account.user)),
                venue: Some(&account.venue),
                ..Default::default()
            },
        );
        let name = account.name.clone();
        let live = account.live_outboxes();
        for outbox in &live {
            outbox(crate::envelope::algo_error(
                &name,
                Value::String(
                    "Remote signing failed repeatedly: this account stopped and is restarting. \
                     Orders and Nuke need it back; reload if it does not return."
                        .to_owned(),
                ),
            ));
        }
        let stopped: crate::algos::StopNotice = Box::new(move |kept| {
            let frames = kept.frames(&name);
            for outbox in &live {
                for frame in &frames {
                    outbox(frame.clone());
                }
            }
        });
        self.dispose_for(
            account,
            Disposal::SignerFailures,
            "remote signing failed repeatedly",
            Some(stopped),
        );
    }

    /// Whether `holder` still holds `account`; disposing drops every holder.
    pub fn holds(&self, account: &Account, holder: u64) -> bool {
        account.holders.lock().unwrap().by_id.contains_key(&holder)
    }

    /// Disposes regardless of other holders (a rebuilt init or a fatal signer).
    pub fn dispose(&self, account: &Arc<Account>, disposal: Disposal) {
        self.dispose_for(account, disposal, "account disposed", None);
    }

    /// New settings rebuild `account`: its algos stop now, their finals told through
    /// `outbox`, rather than left running for a page told they stopped.
    pub fn stop_for_new_settings(
        &self,
        account: &Arc<Account>,
        outbox: Outbox,
        disposal: Disposal,
    ) {
        self.dispose_for(
            account,
            disposal,
            "its key or connection settings changed",
            Some(tell_through(account, vec![outbox])),
        );
    }

    /// Reset only the live admitted holder's current account; retire older joins.
    pub(crate) fn reset_settings(
        &self,
        account: &Arc<Account>,
        holder: u64,
        outbox: Outbox,
    ) -> bool {
        let key = (account.user.clone(), account.registry_name().to_owned());
        let mut pending = self.pending_inits.lock().unwrap();
        let mut accounts = self.accounts.lock().unwrap();
        if account.is_public()
            || !account.running.io_allowed()
            || account.handle.disposal_cause().is_some()
            || !accounts
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            return false;
        }
        let mut holders = account.holders.lock().unwrap();
        if !holders.by_id.get(&holder).is_some_and(Option::is_some) {
            return false;
        }
        accounts.remove(&key);
        *holders = Holders::default();
        if let Some(incarnation) = pending.remove(&key) {
            for current in incarnation.attempts.iter().chain(&incarnation.probes) {
                current.store(false, Ordering::Release);
            }
        }
        drop(holders);
        drop(accounts);
        drop(pending);
        self.stop_for_new_settings(account, outbox, Disposal::SettingsChanged);
        true
    }

    fn dispose_for(
        &self,
        account: &Arc<Account>,
        disposal: Disposal,
        reason: &str,
        stopped: Option<crate::algos::StopNotice>,
    ) {
        let holders: Vec<u64> = account
            .holders
            .lock()
            .unwrap()
            .by_id
            .keys()
            .copied()
            .collect();
        Log::info(&format!(
            "dispose {}: {reason} (disposal={}, holders {holders:?})",
            account.name,
            disposal.as_str()
        ));
        let mut accounts = self.accounts.lock().unwrap();
        let key = (account.user.clone(), account.registry_name().to_owned());
        if accounts
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            accounts.remove(&key);
        }
        drop(accounts);
        *account.holders.lock().unwrap() = Holders::default();
        let stopped = if stopped.is_none() && !account.running.io_allowed() {
            Some(self.keep_notice(account, "router-lease"))
        } else {
            stopped
        };
        let cause = if disposal == Disposal::Moving && self.planned_move(&account.user) {
            DisposalCause::Moving
        } else {
            DisposalCause::Stopped
        };
        Self::stop(account, reason, stopped, cause);
    }

    /// The process is stopping: every account's algos settle and its engine ends,
    /// within `deadline`. Holders stay, so browsers still sign those cancels and
    /// are told. False if some account had not ended by then.
    pub async fn drain(&self, deadline: Duration) -> bool {
        self.stopping.store(true, Ordering::Relaxed);
        self.user_drains.box_draining(true);
        let accounts: Vec<Arc<Account>> = self.accounts.lock().unwrap().values().cloned().collect();
        for account in &accounts {
            Self::stop(
                account,
                "the server is restarting",
                None,
                DisposalCause::Stopped,
            );
        }
        let until = tokio::time::Instant::now() + deadline;
        while self.threads.accounts() > 0 {
            if tokio::time::Instant::now() >= until {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        true
    }

    /// The operator drains the box: sockets move as their algos end, and only
    /// users already live here may init. Running algos are never stopped for it.
    pub fn start_drain(&self) {
        self.user_drains.box_draining(true);
        self.drain.send_if_modified(|drain| {
            let starts = *drain == Drain::Off;
            if starts {
                *drain = Drain::Waiting;
            }
            starts
        });
    }

    /// Break-glass: stop every chase and TWAP through the SIGTERM path, which tells
    /// each live page what stopped, and only then move every socket. False if some
    /// account had not ended within `deadline`.
    pub async fn break_glass(&self, deadline: Duration) -> bool {
        self.box_forced.store(true, Ordering::Release);
        self.start_drain();
        let until = tokio::time::Instant::now() + deadline;
        let mut settled = self.drain(deadline).await;
        // A host zeroes its counts only after telling its pages what stopped.
        while settled && self.running_algos() != (0, 0) {
            if tokio::time::Instant::now() >= until {
                settled = false;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.drain.send_replace(Drain::Now);
        settled
    }

    /// Ends a drain that did not end in a restart.
    pub fn undrain(&self) {
        self.stopping.store(false, Ordering::Relaxed);
        self.drain.send_replace(Drain::Off);
        self.user_drains.box_draining(false);
        self.box_forced.store(false, Ordering::Release);
    }

    pub(crate) fn planned_move(&self, user: &str) -> bool {
        let entry = self.user_drains.get(user);
        if self.box_forced.load(Ordering::Acquire)
            || entry.forced()
            || self.drain_state() == Drain::Now
        {
            return false;
        }
        self.drain_state() == Drain::Waiting || entry.planned_move()
    }

    pub fn start_user_drain(&self, user: &str, cap_ms: u64) -> Value {
        let status = self.user_drains.start(user, cap_ms);
        self.algos_ended
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        status
    }

    /// Detached from the HTTP request: disconnecting an operator cannot abandon cleanup.
    pub fn force_user_drain(
        self: &Arc<Self>,
        user: &str,
        id: &str,
    ) -> Option<tokio::task::JoinHandle<bool>> {
        let accounts = self.user_drains.begin_force(user, id)?;
        let registry = self.clone();
        let (user, id) = (user.to_owned(), id.to_owned());
        Some(tokio::spawn(async move {
            for account in accounts {
                let keep = registry.keep_notice(&account, &id);
                let weak = Arc::downgrade(&account);
                let stopped = Box::new(move |kept: crate::algos::Kept| {
                    if let Some(account) = weak.upgrade() {
                        for frame in kept.frames(&account.name) {
                            account.reach_live(Some(frame));
                        }
                    }
                    keep(kept);
                });
                Self::stop(
                    &account,
                    "the operator forced a user drain",
                    Some(stopped),
                    DisposalCause::Stopped,
                );
            }
            let until = tokio::time::Instant::now() + crate::user_drain::FORCE_DEADLINE;
            let settled = loop {
                if registry.user_drains.status(&user)["algosDone"] == true {
                    break true;
                }
                if tokio::time::Instant::now() >= until {
                    break false;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            };
            registry.user_drains.end_force(&user, &id);
            registry
                .algos_ended
                .send_modify(|revision| *revision = revision.wrapping_add(1));
            settled
        }))
    }

    pub fn drain_state(&self) -> Drain {
        *self.drain.borrow()
    }

    /// Changes as a drain starts, goes break-glass or ends.
    pub fn watch_drain(&self) -> tokio::sync::watch::Receiver<Drain> {
        self.drain.subscribe()
    }

    /// Changes as any account's chases or TWAPs end.
    pub fn watch_algos_ended(&self) -> tokio::sync::watch::Receiver<u64> {
        self.algos_ended.subscribe()
    }

    /// `(chases, twaps)` running on this box, parked accounts included.
    pub fn running_algos(&self) -> (usize, usize) {
        self.accounts
            .lock()
            .unwrap()
            .values()
            .map(|account| account.running.counts())
            .fold((0, 0), |(chases, twaps), (more_chases, more_twaps)| {
                (chases + more_chases, twaps + more_twaps)
            })
    }

    /// Whether `user` holds any private account here, live or parked.
    pub fn live_here(&self, user: &str) -> bool {
        self.accounts
            .lock()
            .unwrap()
            .keys()
            .any(|(holder, _)| holder == user)
    }

    /// Moves a closing socket's private accounts under `resume_id` for the grace
    /// window. Their signers suspend: signs already sent wait for a resume.
    pub fn park(
        self: &Arc<Self>,
        resume_id: String,
        user: &str,
        accounts: Vec<(Arc<Account>, u64)>,
    ) {
        let mut parked = Vec::with_capacity(accounts.len());
        for (account, holder) in accounts {
            if parked
                .iter()
                .any(|(previous, _)| Arc::ptr_eq(previous, &account))
            {
                continue;
            }
            let park_holder = self.next_id();
            {
                let mut holders = account.holders.lock().unwrap();
                holders.remove(holder);
                if let Some(mut labels) = holders.labels.remove(&holder) {
                    for live in labels.values_mut() {
                        *live = None;
                    }
                    holders.labels.insert(park_holder, labels);
                }
                holders.insert(park_holder, None);
                account.hand_over(&mut holders, holder);
            }
            parked.push((account, park_holder));
        }
        if parked.is_empty() {
            return;
        }
        self.parked.lock().unwrap().insert(
            resume_id.clone(),
            Parked {
                user: user.to_owned(),
                accounts: parked,
                alias_until: None,
            },
        );
        let registry = self.clone();
        let grace = self.resume_grace;
        tokio::spawn(async move {
            tokio::time::sleep(grace).await;
            let expired = registry.parked.lock().unwrap().remove(&resume_id);
            if let Some(expired) = expired {
                let reason = format!("no browser for {}s", grace.as_secs());
                let disposed_at = std::time::Instant::now();
                let user = expired.user.clone();
                let watcher = registry.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(RETURN_WAIT).await;
                    if let Some(line) =
                        watcher.did_not_return(&user, disposed_at, std::time::Instant::now())
                    {
                        Log::warn(&line);
                    }
                });
                for (account, park_holder) in expired.accounts {
                    // No browser to tell: the next init of the account is told.
                    let last = {
                        let mut holders = account.holders.lock().unwrap();
                        holders.remove(park_holder);
                        holders.labels.remove(&park_holder);
                        account.hand_over(&mut holders, park_holder);
                        holders.by_id.is_empty()
                    };
                    if last {
                        let stopped = registry.keep_notice(&account, &resume_id);
                        registry.dispose_for(&account, Disposal::NoBrowser, &reason, Some(stopped));
                    }
                }
            }
            // A resumed id stays an alias for one more grace window.
            tokio::time::sleep(grace).await;
            let mut parked = registry.parked.lock().unwrap();
            if parked
                .get(&resume_id)
                .is_some_and(|alias| alias.alias_until.is_some())
            {
                parked.remove(&resume_id);
            }
        });
    }

    /// A resumable session opened under `resume_id`; it closes when told.
    pub fn open_session(&self, resume_id: &str, user: &str) -> Arc<tokio::sync::Notify> {
        let close = Arc::new(tokio::sync::Notify::new());
        self.open.lock().unwrap().insert(
            resume_id.to_owned(),
            OpenSession {
                user: user.to_owned(),
                close: close.clone(),
                parked: Vec::new(),
            },
        );
        close
    }

    /// The session under `resume_id` has closed and parked its accounts.
    pub fn session_closed(&self, resume_id: &str) {
        if let Some(open) = self.open.lock().unwrap().remove(resume_id) {
            for parked in open.parked {
                let _ = parked.send(());
            }
        }
    }

    /// A resume naming a session still open for the same user: the old socket
    /// dropped without a close frame. Tells it to close; resolves once parked.
    pub fn take_over(
        &self,
        resume_id: &str,
        user: &str,
    ) -> Option<tokio::sync::oneshot::Receiver<()>> {
        let mut open = self.open.lock().unwrap();
        let session = open
            .get_mut(resume_id)
            .filter(|session| session.user == user)?;
        let (parked, wait) = tokio::sync::oneshot::channel();
        session.parked.push(parked);
        session.close.notify_one();
        Some(wait)
    }

    /// Takes back parked COSIGNED accounts for the same user. DELEGATED
    /// accounts retain their parked holder until their original expiry and use
    /// fresh admission. Unknown, expired and foreign ids return no accounts.
    pub fn resume(
        &self,
        resume_id: &str,
        user: &str,
        holder: u64,
        token: (String, u64),
        outbox: &dyn Fn(&str) -> Outbox,
    ) -> Vec<Arc<Account>> {
        let mut parked = self.parked.lock().unwrap();
        let Some(entry) = parked.get_mut(resume_id) else {
            return Vec::new();
        };
        if entry.user != user
            || entry.alias_until.is_some()
            || crate::auth::Verifier::check_expiry(Some(token.1), now_ms()).is_err()
        {
            return Vec::new();
        }
        let accounts = entry
            .accounts
            .extract_if(.., |(account, _)| account.delegated.is_none())
            .collect::<Vec<_>>();
        entry.alias_until = Some(now_ms() + 2 * self.resume_grace.as_millis() as u64);
        drop(parked);
        accounts
            .into_iter()
            .map(|(account, park_holder)| {
                let labels = {
                    let mut holders = account.holders.lock().unwrap();
                    holders.remove(park_holder);
                    holders.labels.remove(&park_holder).unwrap_or_default()
                };
                let labels = if labels.is_empty() {
                    vec![account.name.clone()]
                } else {
                    labels.into_keys().collect()
                };
                for label in labels {
                    let outbox = outbox(&label);
                    let routed = if account.identity.is_some() {
                        let (outbox, label) = (outbox.clone(), label.clone());
                        Arc::new(move |text| {
                            outbox(crate::envelope::with_account_label(text, &label))
                        }) as Outbox
                    } else {
                        outbox.clone()
                    };
                    account.hold_label(
                        holder,
                        Live {
                            outbox: routed,
                            token: Some(token.clone()),
                        },
                        &label,
                    );
                }
                account
            })
            .collect()
    }
}

/// The top-level init fields (names only) on which two fingerprints differ.
fn differing_fields(old: &str, new: &str) -> Vec<String> {
    let parse = |text: &str| {
        serde_json::from_str::<serde_json::Map<String, Value>>(text).unwrap_or_default()
    };
    let (old, new) = (parse(old), parse(new));
    let mut fields: Vec<String> = old
        .keys()
        .chain(new.keys())
        .filter(|field| old.get(*field) != new.get(*field))
        .cloned()
        .collect();
    fields.sort();
    fields.dedup();
    fields
}

/// 128 bits from the OS CSPRNG, hex; never derived from time, counters or users.
pub fn resume_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS entropy");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Test accounts on a test venue, for this crate's tests and the two-box harness
/// (`tests/boxes.rs`); built only for tests or the `test-support` feature.
#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    /// Synthetic venue fixtures have no real private-startup transport. The
    /// production proof requirement remains the default; legacy fixtures opt out explicitly.
    pub fn set_join_admission(registry: &mut super::Registry, enabled: bool) {
        registry.join_admission_override = Some(enabled);
    }

    use super::*;

    pub fn bind_identity(account: &mut Arc<Account>, init: &InitPayload) {
        let identity = identity::digest(&account.user, init).unwrap();
        let account =
            Arc::get_mut(account).expect("identity is bound before sharing the test account");
        account.twap_owner = Arc::new(crate::twap_owner::Binding::new(
            identity.as_deref().unwrap_or(&account.user),
        ));
        account.identity = identity;
    }

    #[cfg(test)]
    pub(crate) fn e2e_registry(
        user: &str,
        venue: crate::delegated_e2e_io::MockVenue,
    ) -> Arc<Registry> {
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        registry.e2e_venue = Some(venue);
        // A missing override must fail locally rather than reach a venue.
        let http = safe_cex_native::HttpRoute {
            prefix: "in-memory://delegated-e2e".into(),
            ..Default::default()
        };
        let ws = safe_cex_native::WsRoute {
            host: Some("in-memory://delegated-e2e".into()),
            ..Default::default()
        };
        registry.construction_test_plans.insert(
            user.into(),
            RoutingPlan {
                http: http.clone(),
                public_http: Some(http.clone()),
                read_http: Some(http.clone()),
                read_public_http: Some(http.clone()),
                public_read_http: Some(http),
                ws: safe_cex_native::WsRoutes {
                    public: ws.clone(),
                    private: ws.clone(),
                    business: ws,
                },
                ..Default::default()
            },
        );
        Arc::new(registry)
    }

    #[cfg(test)]
    pub(crate) fn engine_count(registry: &Registry) -> usize {
        registry.threads.accounts()
    }

    #[cfg(test)]
    pub(crate) fn probe_registry(plan: RoutingPlan) -> Arc<Registry> {
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        for user in ["alice", "bob"] {
            registry
                .construction_test_plans
                .insert(user.into(), plan.clone());
        }
        Arc::new(registry)
    }

    /// A private account of `user` named `name` on `venue`, held by no socket yet,
    /// and the inbox its algo host would read.
    pub fn account_on<V: safe_cex_engine::Venue + 'static>(
        venue: V,
        (user, name, venue_name): (&str, &str, &str),
        link: Option<Arc<SignerLink>>,
        fingerprint: String,
        running: Arc<crate::algos::Running>,
    ) -> (
        Arc<Account>,
        mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
    ) {
        let engine = safe_cex_engine::test_support::seeded_engine(venue);
        let (handle, _task) = safe_cex_native::spawn_account(
            safe_cex_native::Host::new(engine, NoIo),
            ServerConfig::default(),
        )
        .unwrap();
        let (algos, inbox) = mpsc::unbounded_channel();
        let account = Arc::new(Account {
            user: user.into(),
            name: name.into(),
            identity: None,
            venue: venue_name.into(),
            handle,
            public_only: link.is_none(),
            delegated: None,
            link,
            token: ProxyToken::new(),
            fingerprint,
            holders: Mutex::default(),
            algos: Some(algos),
            algo_owner: Arc::default(),
            placed: PlacedSymbols::default(),
            held: Arc::default(),
            frame_reads: Arc::new(tokio::sync::Semaphore::new(MAX_FRAME_READS)),
            reads: Arc::new(tokio::sync::Semaphore::new(MAX_READS)),
            metadata_refreshes: Arc::new(tokio::sync::Semaphore::new(MAX_METADATA_REFRESHES)),
            running,
            twap_owner: Arc::new(crate::twap_owner::Binding::new(user)),
        });
        (account, inbox)
    }

    /// `user`'s loaded BTCUSDT account under `registry`, as `build` would leave it
    /// for `init`: no socket holds it until one sends that init, its algo host runs
    /// and is counted, and every command is answered at once. Call in a `LocalSet`.
    pub fn trading_account(registry: &Registry, user: &str, init: &InitPayload) -> Arc<Account> {
        let link = SignerLink::new(0, Arc::new(|_| false));
        let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
        let venue = HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        };
        let names = (
            user,
            init.account_name.as_str(),
            init.exchange_name.as_str(),
        );
        let (mut account, inbox) = account_on(
            venue,
            names,
            Some(link.clone()),
            init.fingerprint(),
            running,
        );
        bind_identity(&mut account, init);
        let account = run_algos(account, link, inbox);
        register(registry, &account);
        account
    }

    /// Sends the web's algo `op` to `account`'s algo host.
    pub fn algo_op(account: &Account, op: &str, data: &str) {
        let input = crate::algos::AlgoInput::Op {
            op: op.into(),
            data: data.into(),
            origin: "test".into(),
            twap_owner: account.twap_owner.snapshot(),
            action: None,
        };
        let sent = account.algos.as_ref().map(|algos| algos.send(input));
        assert!(
            matches!(sent, Some(Ok(()))),
            "{} runs no algo host",
            account.name
        );
    }

    /// Who holds one account on this box, and whether it can sign.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct Held {
        pub live: Vec<u64>,
        pub parked: bool,
        /// The live socket signing goes through.
        pub signing: Option<u64>,
        /// Its signer link would send a sign-req now (no suspended or ended session).
        pub can_sign: bool,
        pub chases: usize,
        pub twaps: usize,
    }

    /// Every account `registry` holds.
    pub fn accounts(registry: &Registry) -> Vec<Arc<Account>> {
        registry
            .accounts
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect()
    }

    /// Every account `registry` holds, by (user, account), live or parked.
    pub fn holdings(registry: &Registry) -> std::collections::BTreeMap<(String, String), Held> {
        let accounts = registry.accounts.lock().unwrap();
        accounts
            .iter()
            .map(|(key, account)| {
                let holders = account.holders.lock().unwrap();
                let mut live: Vec<u64> = holders
                    .by_id
                    .iter()
                    .filter(|(_, holder)| holder.is_some())
                    .map(|(id, _)| *id)
                    .collect();
                live.sort_unstable();
                let (chases, twaps) = account.running.counts();
                let held = Held {
                    live,
                    parked: holders.by_id.values().any(Option::is_none),
                    signing: holders.signing,
                    can_sign: account
                        .link
                        .as_ref()
                        .is_some_and(|link| link.is_available()),
                    chases,
                    twaps,
                };
                (key.clone(), held)
            })
            .collect()
    }

    #[derive(Clone)]
    pub struct NoIo;
    impl safe_cex_native::Transport for NoIo {
        fn connect(
            &self,
            _: safe_cex_engine::SocketId,
            _: String,
            _: safe_cex_native::Events,
        ) -> mpsc::UnboundedSender<String> {
            mpsc::unbounded_channel().0
        }
        fn request(&self, _: safe_cex_engine::HttpRequest, _: safe_cex_native::Events) {}
    }

    /// A BTCUSDT venue that loads after `load_after_ms`, if ever.
    pub struct HeldLoad {
        /// Placements seen while `ticker` is off; they never settle.
        pub placed: Arc<std::sync::atomic::AtomicUsize>,
        pub load_after_ms: Option<f64>,
        /// A BTCUSDT ticker, and every command answered: chases run as on a trading venue.
        pub ticker: bool,
    }
    impl safe_cex_engine::Venue for HeldLoad {
        type Pending = ();
        fn start(&mut self, cx: &mut safe_cex_engine::Cx<'_, ()>) {
            if let Some(delay) = self.load_after_ms {
                cx.set_timer(delay);
            }
        }
        fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
        fn on_message(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: safe_cex_engine::SocketId,
            _: &str,
        ) {
        }
        fn on_close(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: safe_cex_engine::SocketId,
            _: u16,
        ) {
        }
        fn on_response(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: (),
            _: safe_cex_engine::Response<'_>,
        ) {
        }
        fn on_request_failed(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: (), _: &str) {}
        fn on_command(
            &mut self,
            cx: &mut safe_cex_engine::Cx<'_, ()>,
            command: safe_cex_engine::Command,
        ) {
            if self.ticker {
                if let Some(id) = command.completion_id() {
                    let result = match &command {
                        safe_cex_engine::Command::PlaceOrders { orders, .. } => {
                            let ids: Vec<String> =
                                (0..orders.len()).map(|i| format!("o-{i}")).collect();
                            serde_json::to_string(&ids).unwrap()
                        }
                        _ => "null".into(),
                    };
                    cx.done(id, result);
                }
                return;
            }
            match command {
                safe_cex_engine::Command::PlaceOrders { .. } => {
                    self.placed.fetch_add(1, Ordering::SeqCst);
                }
                // The nuke's cancels precede its close.
                safe_cex_engine::Command::CancelSymbolOrders { command, .. } => {
                    cx.done(command, "[]".to_owned());
                }
                _ => {}
            }
        }
        fn on_timer(&mut self, cx: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::TimerId) {
            use safe_cex_engine::model::{
                Limits, Market, MinMax, Position, PositionSide, Precision,
            };
            cx.store.set_markets(vec![Market {
                id: "BTCUSDT".into(),
                symbol: "BTCUSDT".into(),
                pretty_symbol: None,
                base: "BTC".into(),
                quote: "USDT".into(),
                active: true,
                precision: Precision {
                    amount: 0.001,
                    price: 0.1,
                },
                limits: Limits {
                    amount: MinMax {
                        min: 0.001,
                        max: 1000.,
                    },
                    leverage: MinMax { min: 1., max: 100. },
                    price: None,
                },
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: Some(true),
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
            }]);
            if self.ticker {
                cx.store.set_tickers(vec![safe_cex_engine::model::Ticker {
                    id: "BTCUSDT".into(),
                    symbol: "BTCUSDT".into(),
                    bid: 77_000.,
                    ask: 77_000.1,
                    last: 77_000.,
                    mark: 77_000.,
                    index: 77_000.,
                    mid_price: None,
                    percentage: 0.,
                    open_interest: 0.,
                    funding_rate: 0.,
                    funding_time: 0.,
                    volume: 0.,
                    quote_volume: 0.,
                }]);
            }
            cx.store.set_orders(Vec::new());
            cx.store.set_positions(vec![Position {
                id: "BTCUSDT-long".into(),
                symbol: "BTCUSDT".into(),
                side: PositionSide::Long,
                entry_price: 77_000.,
                notional: 77.,
                leverage: 10.,
                margin_type: None,
                position_mode: None,
                unrealized_pnl: 0.,
                realized_pnl: None,
                contracts: 0.001,
                liquidation_price: 0.,
                margin_size: None,
                last_update: None,
            }]);
        }
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    /// Runs `account`'s algo host on this `LocalSet`, wired as `build` wires it.
    pub fn run_algos(
        account: Arc<Account>,
        link: Arc<SignerLink>,
        inbox: mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
    ) -> Arc<Account> {
        *account.algo_owner.lock().unwrap() = Some(Arc::downgrade(&account));
        let owner = account.algo_owner.clone();
        let (emit, broadcast) = algo_reach(owner);
        tokio::task::spawn_local(crate::algos::run_with_identity(
            account.handle.clone(),
            1_000_000,
            crate::audit::Who {
                user: account.user.clone(),
                account: account.name.clone(),
                venue: account.venue.clone(),
            },
            emit,
            broadcast,
            Some(link),
            account.placed.clone(),
            account.held.clone(),
            account.running.clone(),
            account.twap_owner.clone(),
            account.identity.clone(),
            inbox,
        ));
        account
    }

    /// `account` as the registry's account for its (user, name).
    pub fn register(registry: &Registry, account: &Arc<Account>) {
        account.running.attach_restores(
            registry
                .interruptions
                .account(&account.user, account.registry_name()),
        );
        if let Some(user) = registry.router_leases.get(&account.user) {
            account.running.attach_lease(user.permit());
        }
        registry.user_drains.track(
            &account.user,
            account.registry_name(),
            &account.running,
            Arc::downgrade(account),
        );
        registry.accounts.lock().unwrap().insert(
            (account.user.clone(), account.registry_name().to_owned()),
            account.clone(),
        );
    }
}

#[cfg(test)]
pub(crate) mod tests {
    fn grant_init_owner(registry: &Registry, epoch: u64) {
        let now = now_ms();
        registry
            .router_leases
            .accept(&crate::router_lease::Heartbeat {
                r#box: "01".into(),
                issued_at: now,
                users: vec![crate::router_lease::Assertion {
                    user_id: "u".into(),
                    owner_epoch: epoch,
                    sequence: 1,
                    not_after: now + 45_000,
                }],
            })
            .unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn paced_private_init_drops_its_retired_router_generation() {
        use std::future::Future;
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        )
        .with_router_lease(crate::router_lease::Config {
            box_id: Some("01".into()),
            enforce: true,
            ..Default::default()
        });
        registry.pacer = crate::pacing::Pacer::new(1, Duration::ZERO);
        let registry = Arc::new(registry);
        grant_init_owner(&registry, 1);
        let old_permit = registry.router_leases.get("u").unwrap().permit();
        assert!(old_permit.allowed());
        registry.pacer.admit().await;
        let init = InitPayload {
            account_name: "acct".into(),
            exchange_name: "hyrotrader".into(),
            ..Default::default()
        };
        let expected_control = prepare(&init, false, Some(SignerLink::new(8, Arc::new(|_| true))))
            .err()
            .unwrap();
        let stale = registry.obtain("u", &init, false, 7, None, Arc::new(|_| true));
        tokio::pin!(stale);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(stale.as_mut().poll(cx).is_pending()))
                .await
        );
        tokio::time::advance(Duration::from_secs(46)).await;
        assert!(
            !old_permit.allowed(),
            "original authority is retired during pacing"
        );
        grant_init_owner(&registry, 2);
        assert!(registry.router_leases.get("u").unwrap().permit().allowed());
        assert!(
            !old_permit.allowed(),
            "fresh grant cannot revive original authority"
        );
        let error = stale.await.err().expect("stale init must be refused");
        assert!(
            error.contains("Router lease expired"),
            "stale init adopted fresh authority and reached preparation: {error}"
        );
        let fresh = registry
            .obtain("u", &init, false, 8, None, Arc::new(|_| true))
            .await;
        assert_eq!(
            fresh.err().unwrap(),
            expected_control,
            "fresh-owner control reaches ordinary preparation"
        );
        assert_eq!(registry.account_count(), 0);
        assert_eq!(
            registry.threads.accounts(),
            0,
            "no account/private transport was created"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn paced_init_drops_after_newer_same_name_settings() {
        use std::future::Future;
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        registry.pacer = crate::pacing::Pacer::new(1, Duration::ZERO);
        let registry = Arc::new(registry);
        registry.pacer.admit().await;
        let old = InitPayload {
            account_name: "acct".into(),
            exchange_name: "hyrotrader".into(),
            key: "synthetic-A".into(),
            ..Default::default()
        };
        let new = InitPayload {
            key: "synthetic-B".into(),
            ..old.clone()
        };
        let stale = registry.obtain("u", &old, false, 7, None, Arc::new(|_| true));
        tokio::pin!(stale);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(stale.as_mut().poll(cx).is_pending()))
                .await
        );
        let fresh = registry
            .obtain("u", &new, false, 8, None, Arc::new(|_| true))
            .await;
        let expected = prepare(&new, false, Some(SignerLink::new(8, Arc::new(|_| true))))
            .err()
            .unwrap();
        assert_eq!(
            fresh.err().unwrap(),
            expected,
            "new settings reach ordinary preparation"
        );
        let error = stale.await.err().expect("superseded init must be refused");
        assert!(
            error.contains("superseded"),
            "older settings resumed after a newer init: {error}"
        );
        assert_eq!(registry.account_count(), 0);
        assert_eq!(registry.threads.accounts(), 0);
    }

    #[tokio::test]
    async fn completed_init_cannot_publish_under_a_fresh_router_generation() {
        use std::future::Future;
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = Arc::new(
                    Registry::new(
                        EngineThreads::start(1).unwrap(),
                        Duration::from_millis(1),
                        Duration::from_secs(60),
                    )
                    .with_router_lease(crate::router_lease::Config {
                        box_id: Some("01".into()),
                        enforce: true,
                        ..Default::default()
                    }),
                );
                grant_init_owner(&registry, 1);
                let mut old = registry
                    .begin_init("u", "acct", false, "synthetic-A", None)
                    .unwrap();
                let (old_account, _old_inbox) = held_account_with(
                    Some(SignerLink::new(7, Arc::new(|_| true))),
                    "synthetic-A".into(),
                );
                let (release, receive) = tokio::sync::oneshot::channel();
                let completion = async {
                    receive.await.unwrap();
                    old.publish(&registry.accounts, &old_account)
                };
                tokio::pin!(completion);
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(
                        completion.as_mut().poll(cx).is_pending()
                    ))
                    .await
                );
                grant_init_owner(&registry, 2);
                let (fresh, _fresh_inbox) = held_account_with(
                    Some(SignerLink::new(8, Arc::new(|_| true))),
                    "synthetic-B".into(),
                );
                register(&registry, &fresh);
                assert!(fresh.running.io_allowed(), "fresh B control is authorized");
                release.send(()).unwrap();
                let error = completion
                    .await
                    .err()
                    .expect("old completion must be dropped");
                assert!(error.contains("Router lease expired"));
                assert!(Arc::ptr_eq(
                    registry
                        .accounts
                        .lock()
                        .unwrap()
                        .get(&("u".into(), "acct".into()))
                        .unwrap(),
                    &fresh
                ));
                old_account.handle.dispose();
                fresh.handle.dispose();
            })
            .await;
    }

    #[tokio::test]
    async fn completed_init_cannot_overwrite_newer_settings_or_fence_a_published_control() {
        use std::future::Future;
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let mut old = registry
                    .begin_init("u", "acct", false, "synthetic-A", None)
                    .unwrap();
                let (old_account, _old_inbox) = held_account_with(
                    Some(SignerLink::new(7, Arc::new(|_| true))),
                    "synthetic-A".into(),
                );
                let (release, receive) = tokio::sync::oneshot::channel();
                let completion = async {
                    receive.await.unwrap();
                    old.publish(&registry.accounts, &old_account)
                };
                tokio::pin!(completion);
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(
                        completion.as_mut().poll(cx).is_pending()
                    ))
                    .await
                );
                let mut new = registry
                    .begin_init("u", "acct", false, "synthetic-B", None)
                    .unwrap();
                let (fresh, _fresh_inbox) = held_account_with(
                    Some(SignerLink::new(8, Arc::new(|_| true))),
                    "synthetic-B".into(),
                );
                assert!(new.publish(&registry.accounts, &fresh).unwrap().is_none());
                let next = registry
                    .begin_init("u", "acct", false, "synthetic-B", None)
                    .unwrap();
                assert!(
                    new.authority.check().is_ok(),
                    "publishing detaches B from pending-init cancellation before returning"
                );
                drop(next);
                release.send(()).unwrap();
                let error = completion
                    .await
                    .err()
                    .expect("old completion must be dropped");
                assert!(error.contains("superseded"));
                assert!(Arc::ptr_eq(
                    registry
                        .accounts
                        .lock()
                        .unwrap()
                        .get(&("u".into(), "acct".into()))
                        .unwrap(),
                    &fresh
                ));
                old_account.handle.dispose();
                fresh.handle.dispose();
            })
            .await;
    }

    #[tokio::test]
    async fn queued_private_init_drops_its_original_router_generation_before_engine_start() {
        use std::future::Future;
        let registry = Arc::new(
            Registry::new(
                EngineThreads::start(1).unwrap(),
                Duration::from_millis(1),
                Duration::from_secs(60),
            )
            .with_router_lease(crate::router_lease::Config {
                box_id: Some("01".into()),
                enforce: true,
                ..Default::default()
            }),
        );
        grant_init_owner(&registry, 1);
        let (release, blocker) = std::sync::mpsc::channel::<()>();
        let (entered, receive) = tokio::sync::oneshot::channel();
        assert!(registry.threads.place(move |_| {
            let _ = entered.send(());
            let _ = blocker.recv();
        }));
        receive.await.unwrap();
        let init = InitPayload {
            account_name: "acct".into(),
            exchange_name: "bybit".into(),
            key: "synthetic-A".into(),
            ..Default::default()
        };
        let stale = registry.obtain("u", &init, false, 7, None, Arc::new(|_| true));
        tokio::pin!(stale);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                assert!(
                    std::future::poll_fn(|cx| std::task::Poll::Ready(
                        stale.as_mut().poll(cx).is_pending()
                    ))
                    .await
                );
                if registry.threads.accounts() == 2 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("init really queues behind the held engine lane");
        grant_init_owner(&registry, 2);
        assert!(
            registry.router_leases.get("u").unwrap().permit().allowed(),
            "fresh B control is authorized"
        );
        release.send(()).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(5), stale)
            .await
            .unwrap()
            .err()
            .unwrap();
        assert!(error.contains("Router lease expired"));
        assert_eq!(registry.account_count(), 0);
    }

    #[tokio::test]
    async fn review_3086_box_force_is_unquieted_during_cleanup() {
        use std::future::Future;
        let registry = registry();
        let (send, receive) = tokio::sync::oneshot::channel();
        assert!(registry.threads.place(move |placement| {
            assert!(send.send(placement).is_ok());
        }));
        let _held_actor_cleanup = receive.await.unwrap();
        registry.start_drain();
        assert!(
            registry.planned_move("u"),
            "ordinary box drain is a working control"
        );
        let force = registry.break_glass(Duration::from_secs(25));
        tokio::pin!(force);
        let pending =
            std::future::poll_fn(|cx| std::task::Poll::Ready(force.as_mut().poll(cx).is_pending()))
                .await;
        assert!(pending, "forced box cleanup is really in flight");
        assert!(registry.stopping.load(Ordering::Relaxed));
        assert_eq!(registry.drain_state(), Drain::Waiting);
        assert!(
            !registry.planned_move("u"),
            "break-glass cannot be quieted while forced actor cleanup is still pending"
        );
    }

    #[tokio::test]
    async fn box_force_intent_survives_cancelled_cleanup_until_explicit_undrain() {
        use std::future::Future;
        let registry = registry();
        let (send, receive) = tokio::sync::oneshot::channel();
        assert!(registry.threads.place(move |placement| {
            assert!(send.send(placement).is_ok());
        }));
        let held_actor_cleanup = receive.await.unwrap();
        {
            let force = registry.break_glass(Duration::from_secs(25));
            tokio::pin!(force);
            assert!(
                std::future::poll_fn(|cx| std::task::Poll::Ready(
                    force.as_mut().poll(cx).is_pending()
                ))
                .await
            );
        }
        registry.start_drain();
        assert!(
            !registry.planned_move("u"),
            "dropping force must not clear its intent"
        );
        drop(held_actor_cleanup);
        registry.undrain();
        registry.start_drain();
        assert!(registry.planned_move("u"));
    }

    #[test]
    fn a_forced_drain_never_becomes_a_quiet_move_even_after_cleanup() {
        let registry = registry();
        let status = registry.start_user_drain("u", crate::user_drain::DEFAULT_CAP_MS);
        let id = status["drainId"].as_str().unwrap();
        assert!(registry.planned_move("u"));
        assert!(registry.user_drains.begin_force("u", id).is_some());
        registry.user_drains.end_force("u", id);
        assert!(!registry.planned_move("u"));
        registry.start_drain();
        assert!(
            !registry.planned_move("u"),
            "a box drain cannot hide a forced user stop"
        );
        registry.start_user_drain("other", crate::user_drain::DEFAULT_CAP_MS);
        registry.drain.send_replace(Drain::Now);
        assert!(
            !registry.planned_move("other"),
            "break-glass box drain wins over a user latch"
        );
    }

    #[test]
    fn a_rebuild_onto_public_data_is_told_apart_from_new_settings() {
        assert_eq!(Disposal::rebuild(false, true), Disposal::PublicData);
        assert_eq!(Disposal::rebuild(false, false), Disposal::SettingsChanged);
        assert_eq!(Disposal::rebuild(true, true), Disposal::SettingsChanged);
        assert_eq!(Disposal::rebuild(true, false), Disposal::SettingsChanged);
        assert_eq!(Disposal::PublicData.as_str(), "public_data");
        assert_eq!(Disposal::SignerFailures.as_str(), "signer_failures");
    }

    pub(crate) use super::testing::register;
    use super::testing::{HeldLoad, NoIo, account_on, run_algos};
    use super::*;

    /// A venue that does nothing: the registry tests need only its handle.
    struct Idle;
    impl safe_cex_engine::Venue for Idle {
        type Pending = ();
        fn start(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>) {}
        fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
        fn on_message(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: safe_cex_engine::SocketId,
            _: &str,
        ) {
        }
        fn on_close(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: safe_cex_engine::SocketId,
            _: u16,
        ) {
        }
        fn on_response(
            &mut self,
            _: &mut safe_cex_engine::Cx<'_, ()>,
            _: (),
            _: safe_cex_engine::Response<'_>,
        ) {
        }
        fn on_request_failed(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
        fn on_command(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::Command) {
        }
        fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    /// A private account held by socket 7, and the inbox its algo host would read.
    fn held_account() -> (
        Arc<Account>,
        mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
    ) {
        held_account_with(None, String::new())
    }

    /// `held_account` with a signer link and the fingerprint it was built from.
    pub(crate) fn held_account_with(
        link: Option<Arc<SignerLink>>,
        fingerprint: String,
    ) -> (
        Arc<Account>,
        mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
    ) {
        held_account_on(Idle, link, fingerprint, Arc::default())
    }

    fn held_account_on<V: safe_cex_engine::Venue + 'static>(
        venue: V,
        link: Option<Arc<SignerLink>>,
        fingerprint: String,
        running: Arc<crate::algos::Running>,
    ) -> (
        Arc<Account>,
        mpsc::UnboundedReceiver<crate::algos::AlgoInput>,
    ) {
        let (account, inbox) =
            account_on(venue, ("u", "acct", "bitget"), link, fingerprint, running);
        account.hold(
            7,
            Live {
                outbox: Arc::new(|_| true),
                token: None,
            },
        );
        (account, inbox)
    }

    /// `held_account_with` plus its real algo host, emitting as `build` wires it.
    pub(crate) fn held_account_running_algos(
        link: Arc<SignerLink>,
        fingerprint: String,
    ) -> Arc<Account> {
        let (account, inbox) = held_account_with(Some(link.clone()), fingerprint);
        run_algos(account, link, inbox)
    }

    /// A private account whose startup is held until `HeldLoad` releases it, as a
    /// silent signer holds it, with one long open on BTCUSDT.
    pub(crate) fn held_load_account(
        link: Arc<SignerLink>,
        load_after_ms: Option<f64>,
    ) -> (Arc<Account>, Arc<std::sync::atomic::AtomicUsize>) {
        let placed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let venue = HeldLoad {
            placed: placed.clone(),
            load_after_ms,
            ticker: false,
        };
        let (account, inbox) =
            held_account_on(venue, Some(link.clone()), String::new(), Arc::default());
        (run_algos(account, link, inbox), placed)
    }

    /// An unloaded account's actor on its own thread, so a test can keep the engine
    /// busy without blocking the test's own tasks.
    pub(crate) fn idle_handle_on_own_thread() -> safe_cex_native::AccountHandle {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            tokio::task::LocalSet::new().block_on(&runtime, async move {
                let engine = safe_cex_engine::test_support::seeded_engine(Idle);
                let (handle, task) = safe_cex_native::spawn_account(
                    safe_cex_native::Host::new(engine, NoIo),
                    ServerConfig::default(),
                )
                .unwrap();
                tx.send(handle).unwrap();
                let _ = task.await;
            });
        });
        rx.recv().unwrap()
    }

    /// A loaded BTCUSDT account under `registry` running its algo host and
    /// counting its algos there, as `build` wires it. Placements never settle.
    pub(crate) fn held_account_counted(
        registry: &Registry,
        link: Arc<SignerLink>,
        fingerprint: String,
    ) -> Arc<Account> {
        let running = Arc::new(crate::algos::Running::new(registry.algos_ended.clone()));
        let venue = HeldLoad {
            placed: Arc::default(),
            load_after_ms: Some(0.),
            ticker: true,
        };
        let (account, inbox) = held_account_on(venue, Some(link.clone()), fingerprint, running);
        let account = run_algos(account, link, inbox);
        register(registry, &account);
        account
    }

    /// Another live socket holding `account`, which then signs through it.
    pub(crate) fn hold_live(account: &Account, id: u64, outbox: Outbox) {
        account.hold(
            id,
            Live {
                outbox,
                token: None,
            },
        );
    }

    /// The socket `account` signs through.
    pub(crate) fn signing(account: &Account) -> Option<u64> {
        account.holders.lock().unwrap().signing
    }

    #[tokio::test]
    async fn a_drain_stops_every_account_keeps_its_holders_and_refuses_new_ones() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = Arc::new(Registry::new(
                    EngineThreads::start(1).unwrap(),
                    Duration::from_millis(1),
                    Duration::from_secs(60),
                ));
                let (account, mut inbox) = held_account();
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .insert(("u".into(), "acct".into()), account.clone());
                assert!(registry.drain(Duration::from_millis(100)).await);
                assert!(matches!(
                    inbox.try_recv(),
                    Ok(crate::algos::AlgoInput::Dispose { .. })
                ));
                assert_eq!(account.holders.lock().unwrap().by_id.len(), 1);
                let refused = registry
                    .obtain("u", &init_frame(""), true, 2, None, Arc::new(|_| true))
                    .await;
                assert!(refused.is_err_and(|error| error.contains("restarting")));
            })
            .await;
    }

    #[tokio::test]
    async fn a_failing_signer_tells_live_sockets_what_stopped_with_the_account() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, mut algos) = held_account();
                let sent: Arc<Mutex<Vec<String>>> = Arc::default();
                let seen = sent.clone();
                hold_live(
                    &account,
                    8,
                    Arc::new(move |text| {
                        seen.lock().unwrap().push(text);
                        true
                    }),
                );
                registry.signer_failed(&account);
                let frames = sent.lock().unwrap().clone();
                assert!(
                    frames.iter().any(|frame| frame.contains("restarting")
                        && frame.contains("Nuke")
                        && frame.contains("forceToast")),
                    "says what stopped, even to a hidden tab: {frames:?}"
                );
                let Ok(crate::algos::AlgoInput::Dispose { stopped, .. }) = algos.try_recv() else {
                    panic!("the algo host is told to stop");
                };
                // The holders are gone by the time the host reports what it stopped.
                stopped.expect("the live socket stays reachable")(
                    "Chase on BTCUSDT stopped".into(),
                );
                assert!(
                    sent.lock()
                        .unwrap()
                        .last()
                        .is_some_and(|frame| frame.contains("Chase on BTCUSDT stopped")
                            && frame.contains("forceToast"))
                );
            })
            .await;
    }

    /// The page starts a chase on what its frames show; the algo host's store may
    /// not have caught up with that frame yet, and the chase must not fail on it.
    #[tokio::test(start_paused = true)]
    async fn a_chase_started_before_the_algo_host_read_the_loaded_market_runs() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for _ in 0..16 {
                    let link = SignerLink::new(7, Arc::new(|_| true));
                    let running = Arc::new(crate::algos::Running::default());
                    let venue = HeldLoad {
                        placed: Arc::default(),
                        load_after_ms: Some(0.),
                        ticker: true,
                    };
                    let (account, inbox) =
                        held_account_on(venue, Some(link.clone()), String::new(), running.clone());
                    // The engine loads; the op is queued before the algo host has read a frame.
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    super::testing::algo_op(
                        &account,
                        "chaser:start",
                        r#"{"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"}"#,
                    );
                    let _account = run_algos(account, link, inbox);
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    assert_eq!(running.counts(), (1, 0), "the chase runs");
                }
            })
            .await;
    }

    pub(crate) fn registry() -> Arc<Registry> {
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        testing::set_join_admission(&mut registry, false);
        Arc::new(registry)
    }

    #[tokio::test]
    async fn token_expiry_is_rechecked_after_init_pacing_before_preparation() {
        use std::future::Future;
        let mut registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        registry.pacer = crate::pacing::Pacer::new(1, Duration::ZERO);
        let registry = Arc::new(registry);
        registry.pacer.admit().await;
        // This venue's ordinary preparation refuses locally, with no venue IO.
        let init = InitPayload {
            account_name: "acct".into(),
            exchange_name: "hyrotrader".into(),
            key: "synthetic".into(),
            ..Default::default()
        };
        let token = Some(("synthetic".into(), now_ms() + 50));
        let work = registry.obtain("alice", &init, false, 9, token, Arc::new(|_| true));
        tokio::pin!(work);
        assert!(
            std::future::poll_fn(|cx| std::task::Poll::Ready(work.as_mut().poll(cx).is_pending()))
                .await
        );
        let error = work.await.err().expect("expired init refuses");
        assert!(error.contains("Tealstreet proxy token expired"), "{error}");
        assert_eq!(registry.account_count(), 0);
    }

    #[tokio::test]
    async fn summary_counts_account_and_socket_users_once_without_creating_lease_records() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let link = SignerLink::new(0, Arc::new(|_| false));
                let (first, _inbox) = account_on(
                    Idle,
                    ("parked-private", "first", "bitget"),
                    Some(link.clone()),
                    String::new(),
                    Arc::default(),
                );
                let (second, _inbox) = account_on(
                    Idle,
                    ("parked-private", "second", "bitget"),
                    Some(link),
                    String::new(),
                    Arc::default(),
                );
                let (public, _inbox) = account_on(
                    Idle,
                    ("", PUBLIC_ACCOUNT, PUBLIC_EXCHANGE),
                    None,
                    String::new(),
                    Arc::default(),
                );
                for account in [&first, &second, &public] {
                    registry.accounts.lock().unwrap().insert(
                        (account.user.clone(), account.name.clone()),
                        (*account).clone(),
                    );
                }
                let socket = registry.user_drains.socket("socket-only");
                let duplicate = registry.user_drains.socket("parked-private");
                registry.user_drains.start("latch-only", 1000);
                let counts = registry.router_lease_counts();
                assert_eq!(counts.live_users_without_lease, 2);
                assert_eq!(counts.tracked_users, 0);
                registry.router_leases.get("socket-only").unwrap();
                let counts = registry.router_lease_counts();
                assert_eq!(counts.live_users_without_lease, 1);
                assert_eq!(counts.tracked_users, 1);
                assert_eq!(counts.would_fence_users, 1);
                assert_eq!(counts.live_would_fence_users, 1);
                drop(socket);
                drop(duplicate);
                let counts = registry.router_lease_counts();
                assert_eq!(counts.would_fence_users, 1);
                assert_eq!(
                    counts.live_would_fence_users, 0,
                    "a user whose socket closed keeps its record but is not live"
                );
                assert_eq!(
                    registry.router_lease_counts().live_users_without_lease,
                    1,
                    "parked account remains live without any socket"
                );
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .remove(&(first.user.clone(), first.name.clone()));
                assert_eq!(
                    registry.router_lease_counts().live_users_without_lease,
                    1,
                    "second private account keeps the same user live"
                );
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .remove(&(second.user.clone(), second.name.clone()));
                assert_eq!(
                    registry.router_lease_counts().live_users_without_lease,
                    0,
                    "public account and operator latch do not count as hosted activity"
                );
            })
            .await;
    }
    #[tokio::test(start_paused = true)]
    async fn a_stop_notice_reaches_its_owner_past_another_session_and_expires_after_a_day() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, _algos) = held_account();
                (registry.keep_notice(&account, "owner"))("stopped".into());
                assert_eq!(
                    registry.take_notice("other", "acct", &account.fingerprint, Some("owner")),
                    None,
                    "never another user's"
                );
                assert_eq!(
                    registry
                        .take_notice("u", "acct", &account.fingerprint, None)
                        .map(|kept| kept.summary())
                        .as_deref(),
                    Some("stopped"),
                    "a second device, or a reopened tab, is told"
                );
                assert_eq!(
                    registry.take_notice("u", "acct", &account.fingerprint, Some("elsewhere")),
                    None,
                    "and a page load after it is not told again"
                );
                assert_eq!(
                    registry
                        .take_notice("u", "acct", &account.fingerprint, Some("owner"))
                        .map(|kept| kept.summary())
                        .as_deref(),
                    Some("stopped"),
                    "the device that owned the chase still is"
                );
                assert_eq!(
                    registry.take_notice("u", "acct", &account.fingerprint, Some("owner")),
                    None,
                    "once"
                );
                (registry.keep_notice(&account, "owner"))("first".into());
                (registry.keep_notice(&account, "owner"))("second".into());
                assert_eq!(
                    registry
                        .take_notice("u", "acct", &account.fingerprint, Some("owner"))
                        .map(|kept| kept.summary())
                        .as_deref(),
                    Some("second"),
                    "one per account"
                );
                (registry.keep_notice(&account, "owner"))("stale".into());
                tokio::time::sleep(NOTICE_TTL).await;
                assert_eq!(
                    registry.take_notice("u", "acct", &account.fingerprint, Some("owner")),
                    None,
                    "dropped after 24h"
                );
            })
            .await;
    }

    #[test]
    fn public_chat_handle_labels_keep_the_tag_and_do_not_cross_users() {
        let registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        let user = "auth0|opaque-user";
        let handle = crate::auth::ChatHandle::parse("Trader-063249").unwrap();
        registry.user_connected(user, Some(&handle));
        assert_eq!(
            registry.account_log_label(user, "existing-account", "bingx"),
            format!(
                "{} chat=Trader-063249 existing-account bingx",
                user_tag(user)
            )
        );
        assert_eq!(
            registry.account_log_label("other-user", "existing-account", "bingx"),
            format!("{} existing-account bingx", user_tag("other-user"))
        );
        let disposed = std::time::Instant::now();
        let line = registry
            .did_not_return(user, disposed, disposed + RETURN_WAIT)
            .unwrap();
        assert_eq!(
            line,
            format!(
                "hosted user did not return: {} chat=Trader-063249 after 5 min",
                user_tag(user)
            )
        );
        assert!(!line.contains(user) && !line.contains("existing-account"));
        registry.user_connected(user, None);
        assert_eq!(
            registry.account_log_label(user, "existing-account", "bingx"),
            format!("{} existing-account bingx", user_tag(user))
        );
    }

    #[test]
    fn a_user_who_does_not_return_is_logged_by_tag_once_an_hour() {
        let registry = Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        );
        let user = "auth0|5f1d00c0ffee";
        let disposed = std::time::Instant::now();
        let five = disposed + Duration::from_secs(5 * 60);
        let line = registry.did_not_return(user, disposed, five).unwrap();
        assert_eq!(
            line,
            format!("hosted user did not return: {} after 5 min", user_tag(user))
        );
        assert!(
            !line.contains("auth0") && user_tag(user).len() == 12,
            "{line}"
        );
        assert_eq!(
            registry.did_not_return(user, disposed, five + Duration::from_secs(60)),
            None
        );
        let later = five + Duration::from_secs(60 * 60);
        assert!(
            registry.did_not_return(user, disposed, later).is_some(),
            "again after an hour"
        );
        // One who came back is not logged.
        let back = "auth0|returning";
        let gone = std::time::Instant::now();
        std::thread::sleep(Duration::from_millis(2));
        registry.user_connected(back, None);
        assert_eq!(
            registry.did_not_return(back, gone, gone + Duration::from_secs(300)),
            None
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_parked_account_keeps_its_algos_then_stops_them_for_no_browser() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, mut algos) = held_account();
                registry.park("r1".into(), "u", vec![(account, 7)]);
                tokio::time::sleep(Duration::from_secs(59)).await;
                assert!(
                    algos.try_recv().is_err(),
                    "chases and TWAPs run on through the window"
                );
                tokio::time::sleep(Duration::from_secs(2)).await;
                match algos.try_recv() {
                    Ok(crate::algos::AlgoInput::Dispose { reason, stopped }) => {
                        assert!(stopped.is_some(), "expiry keeps a notice for the next init");
                        assert_eq!(reason, "no browser for 60s")
                    }
                    _ => panic!("the window expired without disposing the algos"),
                }
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_resume_within_the_window_keeps_the_account_and_its_algos() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, mut algos) = held_account();
                registry.park("r1".into(), "u", vec![(account, 7)]);
                tokio::time::sleep(Duration::from_secs(30)).await;
                let outbox: Outbox = Arc::new(|_| true);
                let resumed =
                    registry.resume("r1", "u", 8, ("t".into(), u64::MAX), &|_| outbox.clone());
                assert_eq!(resumed.len(), 1);
                tokio::time::sleep(Duration::from_secs(150)).await;
                assert!(
                    algos.try_recv().is_err(),
                    "a resumed account is never disposed by the old window"
                );
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_abrupt_drop_resumed_at_8s_takes_over_the_open_session() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, mut algos) = held_account();
                let taken_over = registry.open_session("r1", "u");
                // The old socket loop as http.rs runs it: it never saw the drop,
                // and closes and parks only when the resume tells it to.
                let old = {
                    let (registry, account) = (registry.clone(), account.clone());
                    tokio::task::spawn_local(async move {
                        taken_over.notified().await;
                        registry.park("r1".into(), "u", vec![(account, 7)]);
                        registry.session_closed("r1");
                    })
                };
                tokio::time::sleep(Duration::from_secs(8)).await;
                let outbox: Outbox = Arc::new(|_| true);
                let token = ("t".to_owned(), u64::MAX);
                assert!(
                    registry
                        .resume("r1", "u", 8, token.clone(), &|_| outbox.clone())
                        .is_empty(),
                    "nothing is parked while the old socket looks open"
                );
                assert!(
                    registry.take_over("r1", "other").is_none(),
                    "never another user's"
                );
                registry
                    .take_over("r1", "u")
                    .expect("the open session")
                    .await
                    .unwrap();
                old.await.unwrap();
                let resumed = registry.resume("r1", "u", 8, token, &|_| outbox.clone());
                assert_eq!(resumed.len(), 1, "resumed, not re-inited");
                assert!(Arc::ptr_eq(&resumed[0], &account));
                assert_eq!(account.holders.lock().unwrap().signing, Some(8));
                tokio::time::sleep(Duration::from_secs(200)).await;
                assert!(algos.try_recv().is_err(), "algos alive: never disposed");
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn an_identical_reinit_on_a_new_socket_never_disposes_or_stops_algos() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bitget".into(),
                    ..InitPayload::default()
                };
                let link = SignerLink::new(7, Arc::new(|_| true));
                let (account, mut algos) = held_account_with(Some(link), init.fingerprint());
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .insert(("u".into(), "acct".into()), account.clone());
                let again = registry
                    .obtain("u", &init, false, 8, None, Arc::new(|_| true))
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&again, &account), "reused, not rebuilt");
                // The old socket's drop is seen later: it parks, and its window expires.
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                tokio::time::sleep(Duration::from_secs(200)).await;
                assert!(algos.try_recv().is_err(), "algos alive: never disposed");
                assert_eq!(registry.account_count(), 1);
                assert_eq!(account.holders.lock().unwrap().signing, Some(8));
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_client_with_other_settings_never_disposes_a_live_account() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bitget".into(),
                    ..InitPayload::default()
                };
                let link = SignerLink::new(7, Arc::new(|_| true));
                let (account, mut algos) = held_account_with(Some(link), init.fingerprint());
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .insert(("u".into(), "acct".into()), account.clone());
                let other = InitPayload {
                    enable_spot_markets: Some(true),
                    ..init.clone()
                };
                for _ in 0..3 {
                    let refused = registry
                        .obtain("u", &other, false, 8, None, Arc::new(|_| true))
                        .await
                        .map(|_| ())
                        .unwrap_err();
                    assert!(refused.contains("enableSpotMarkets"), "{refused}");
                    assert!(
                        refused.contains("use apply-settings before reinitializing"),
                        "{refused}"
                    );
                }
                assert!(registry.rebuild_refusal("u", &other, false, 8).is_some());
                assert!(
                    registry.rebuild_refusal("u", &init, false, 8).is_none(),
                    "same settings join"
                );
                assert!(
                    registry.rebuild_refusal("u", &other, false, 7).is_some(),
                    "the holder itself must explicitly apply new settings"
                );
                assert!(algos.try_recv().is_err(), "the live holder's algos run on");
                assert_eq!(account.holders.lock().unwrap().signing, Some(7));
                let current = registry
                    .accounts
                    .lock()
                    .unwrap()
                    .get(&("u".into(), "acct".into()))
                    .cloned();
                assert!(current.is_some_and(|current| Arc::ptr_eq(&current, &account)));

                // A parked client still holds its settings.
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                assert!(registry.rebuild_refusal("u", &other, false, 8).is_some());
            })
            .await;
    }

    #[test]
    fn a_fingerprint_compares_credentials_without_keeping_them() {
        let init = InitPayload {
            account_name: "a".into(),
            exchange_name: "okx".into(),
            key: "account-key-text".into(),
            passphrase: Some("account-passphrase-text".into()),
            ..InitPayload::default()
        };
        let fingerprint = init.fingerprint();
        assert!(!fingerprint.contains("account-key-text"));
        assert!(!fingerprint.contains("account-passphrase-text"));
        assert_eq!(fingerprint, init.clone().fingerprint());
        let rotated = InitPayload {
            passphrase: Some("other".into()),
            ..init.clone()
        };
        assert_eq!(
            differing_fields(&fingerprint, &rotated.fingerprint()),
            ["passphrase"]
        );
        let rekeyed = InitPayload {
            key: "other".into(),
            ..init
        };
        assert_eq!(
            differing_fields(&fingerprint, &rekeyed.fingerprint()),
            ["key"]
        );
    }

    #[test]
    fn differing_fields_name_only_the_fields_that_changed() {
        let a = InitPayload {
            account_name: "a".into(),
            exchange_name: "bybit".into(),
            ..InitPayload::default()
        };
        let b = InitPayload {
            enable_spot_markets: Some(true),
            ..a.clone()
        };
        assert_eq!(
            differing_fields(&a.fingerprint(), &b.fingerprint()),
            ["enableSpotMarkets"]
        );
        assert!(differing_fields(&a.fingerprint(), &a.fingerprint()).is_empty());
    }

    pub(crate) fn signing_holder(account: &Account) -> Option<u64> {
        account.holders.lock().unwrap().signing
    }

    pub(crate) fn fail_signer(registry: &Registry, account: &Arc<Account>) {
        registry.signer_failed(account);
    }

    #[tokio::test(start_paused = true)]
    async fn algo_replies_reach_every_live_holder() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let link = SignerLink::new(7, Arc::new(|_| true));
                let account = held_account_running_algos(link, String::new());
                tokio::time::sleep(Duration::from_millis(200)).await;
                let seen = Arc::new(Mutex::new(Vec::new()));
                for id in [8, 9] {
                    let seen = seen.clone();
                    hold_live(
                        &account,
                        id,
                        Arc::new(move |frame| {
                            seen.lock().unwrap().push((id, frame));
                            true
                        }),
                    );
                }
                account
                    .algos
                    .as_ref()
                    .unwrap()
                    .send(crate::algos::AlgoInput::Joined)
                    .unwrap();
                tokio::time::sleep(Duration::from_millis(200)).await;
                let seen = seen.lock().unwrap();
                for id in [8, 9] {
                    assert!(
                        seen.iter()
                            .any(|(holder, frame)| *holder == id
                                && frame.contains(r#""op":"store""#)),
                        "holder {id}: {seen:?}"
                    );
                }
            })
            .await;
    }

    #[tokio::test]
    async fn the_loser_of_an_init_race_no_longer_holds_the_stopped_account() {
        let registry = registry();
        let init = init_frame("");
        let out = || -> Outbox { Arc::new(|_| true) };
        let (first, second) = tokio::join!(
            registry.obtain("u", &init, true, 1, None, out()),
            registry.obtain("u", &init, true, 2, None, out()),
        );
        let (first, second) = (first.unwrap(), second.unwrap());
        assert!(!Arc::ptr_eq(&first, &second), "both built");
        assert_eq!(
            [registry.holds(&first, 1), registry.holds(&second, 2)]
                .iter()
                .filter(|held| **held)
                .count(),
            1
        );
        assert_eq!(registry.account_count(), 1);
    }

    #[test]
    fn signing_hands_over_to_the_newest_live_holder_then_suspends() {
        let live = |token: &str| {
            Some(Live {
                outbox: Arc::new(|_| true),
                token: Some((token.to_owned(), u64::MAX)),
            })
        };
        let mut holders = Holders::default();
        holders.insert(1, live("first tab"));
        holders.insert(2, live("second tab"));
        holders.insert(3, None);
        holders.signing = Some(2);
        assert!(
            holders.successor(1).is_none(),
            "a non-signing tab leaving changes nothing"
        );
        holders.remove(2);
        let (id, next) = holders.successor(2).unwrap().unwrap();
        assert_eq!((id, next.token.unwrap().0.as_str()), (1, "first tab"));
        holders.signing = Some(1);
        holders.remove(1);
        assert!(
            holders.successor(1).unwrap().is_none(),
            "only a parked holder: suspend"
        );
    }

    #[test]
    fn unusable_builders_fall_back_to_the_default() {
        let init = |address: &str, fee: f64| InitPayload {
            account_name: "a".into(),
            exchange_name: "hyperliquid".into(),
            hyperliquid_builder: Some(crate::envelope::HyperliquidBuilder {
                address: address.into(),
                fee_tenths_bps: fee,
            }),
            ..InitPayload::default()
        };
        let good = format!("0x{}", "a".repeat(40));
        // The browser's builder fee arrives as an f64; the registry parses it as u32.
        let options = engine_options(&init(&good, 10.0), false).to_string();
        assert!(
            options.contains(r#""feeTenthsBps":10}"#),
            "the fee must serialize as an integer: {options}"
        );
        for (address, fee) in [
            (good.as_str(), 0.0),
            (good.as_str(), f64::NAN),
            (good.as_str(), f64::INFINITY),
            (good.as_str(), 10.5),
            (good.as_str(), 5e9),
            ("0x12", 10.0),
        ] {
            assert!(
                engine_options(&init(address, fee), false)
                    .get("hyperliquidBuilder")
                    .is_none()
            );
        }
    }

    #[test]
    fn resume_ids_are_128_bit_random_hex() {
        let (a, b) = (resume_id(), resume_id());
        assert_eq!(a.len(), 32);
        assert!(a.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn engine_options_carry_no_secret_and_public_accounts_carry_no_key() {
        let init = InitPayload {
            account_name: "a".into(),
            exchange_name: "bybit".into(),
            key: "pub-key".into(),
            passphrase: Some("pp".into()),
            testnet: Some(true),
            ..InitPayload::default()
        };
        let private = engine_options(&init, false);
        assert_eq!(private["key"], "pub-key");
        assert!(private.get("secret").is_none());
        let public = engine_options(&init, true);
        assert!(public.get("key").is_none() && public.get("passphrase").is_none());
        assert_eq!(public["publicDataOnly"], true);
        assert_eq!(private["brokerAttribution"], true);
    }

    /// The browser's init frame, as ExchangeServerWorker sends it.
    fn init_frame(fields: &str) -> InitPayload {
        let frame = format!(
            r#"{{"t":"init","d":{{"accountName":"a","exchangeName":"hyperliquid","key":"0xabc"{fields}}}}}"#
        );
        match serde_json::from_str::<crate::envelope::Inbound>(&frame).unwrap() {
            crate::envelope::Inbound::Init(init) => *init,
            _ => panic!("not an init"),
        }
    }

    #[test]
    fn venue_behaviour_fields_reach_the_engine() {
        let init = init_frame(
            r#","vault":"0x3333333333333333333333333333333333333333","bybitSiteId":"BRA_BTL","enableSpotMarkets":true,"enableInverseMarkets":true,"enableOptionMarkets":true,"hyperliquidBuilder":{"address":"0x4444444444444444444444444444444444444444","feeTenthsBps":10}"#,
        );
        let private = engine_options(&init, false);
        assert_eq!(
            private["vault"],
            "0x3333333333333333333333333333333333333333"
        );
        assert_eq!(private["bybitSiteId"], "BRA_BTL");
        for field in [
            "enableSpotMarkets",
            "enableInverseMarkets",
            "enableOptionMarkets",
        ] {
            assert_eq!(private[field], true, "{field}");
        }
        // The registry parses the fee as u32: an integer on the wire, not 10.0.
        assert_eq!(
            private["hyperliquidBuilder"]["feeTenthsBps"].to_string(),
            "10"
        );
        // A public account trades nothing, so it names no vault; the site still picks its host.
        let public = engine_options(&init, true);
        assert!(public.get("vault").is_none());
        assert_eq!(public["bybitSiteId"], "BRA_BTL");
    }

    #[test]
    fn absent_or_off_venue_fields_are_omitted() {
        let init = init_frame(r#","vault":"","bybitSiteId":"","enableSpotMarkets":false"#);
        let options = engine_options(&init, false);
        for field in [
            "vault",
            "bybitSiteId",
            "enableSpotMarkets",
            "enableInverseMarkets",
            "enableOptionMarkets",
        ] {
            assert!(options.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn the_venue_fields_change_the_init_fingerprint() {
        let main = init_frame("");
        let vault = init_frame(r#","vault":"0x3333333333333333333333333333333333333333""#);
        assert_ne!(main.fingerprint(), vault.fingerprint());
    }

    #[test]
    fn only_what_builds_the_engine_makes_two_inits_differ() {
        let bybit = InitPayload {
            account_name: "bear5".into(),
            exchange_name: "bybitinverse".into(),
            key: "k".into(),
            ..InitPayload::default()
        };
        let builder = || {
            Some(crate::envelope::HyperliquidBuilder {
                address: format!("0x{}", "4".repeat(40)),
                fee_tenths_bps: 10.0,
            })
        };
        // A Hyperliquid-only setting, a default sent explicitly, diagnostics: same engine.
        for same in [
            InitPayload {
                hyperliquid_builder: builder(),
                ..bybit.clone()
            },
            InitPayload {
                enable_spot_markets: Some(false),
                testnet: Some(false),
                ..bybit.clone()
            },
            InitPayload {
                vault: Some("0x33".into()),
                ..bybit.clone()
            },
            InitPayload {
                verbose: Some(true),
                ..bybit.clone()
            },
        ] {
            assert_eq!(same.fingerprint(), bybit.fingerprint(), "{same:?}");
        }
        // What the Bybit engine reads still tells them apart.
        for other in [
            InitPayload {
                key: "k2".into(),
                ..bybit.clone()
            },
            InitPayload {
                enable_spot_markets: Some(true),
                ..bybit.clone()
            },
            InitPayload {
                bybit_site_id: Some("BRA_BTL".into()),
                ..bybit.clone()
            },
        ] {
            assert_ne!(other.fingerprint(), bybit.fingerprint(), "{other:?}");
        }
        // On Hyperliquid the builder is part of the engine.
        let hyperliquid = InitPayload {
            exchange_name: "hyperliquid".into(),
            key: "0xabc".into(),
            ..bybit.clone()
        };
        assert_ne!(
            InitPayload {
                hyperliquid_builder: builder(),
                ..hyperliquid.clone()
            }
            .fingerprint(),
            hyperliquid.fingerprint()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_device_with_a_changed_builder_requires_apply_settings() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let builder = |fee: f64| {
                    Some(crate::envelope::HyperliquidBuilder {
                        address: format!("0x{}", "4".repeat(40)),
                        fee_tenths_bps: fee,
                    })
                };
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "hyperliquid".into(),
                    key: "0xabc".into(),
                    hyperliquid_builder: builder(10.0),
                    ..InitPayload::default()
                };
                let link = SignerLink::new(7, Arc::new(|_| true));
                let (account, mut algos) = held_account_with(Some(link), init.fingerprint());
                register(&registry, &account);
                // A different effective builder is a settings change too.
                let default_builder = InitPayload {
                    hyperliquid_builder: None,
                    ..init.clone()
                };
                assert_ne!(default_builder.fingerprint(), init.fingerprint());
                assert!(
                    registry
                        .rebuild_refusal("u", &default_builder, false, 8)
                        .is_some()
                );
                assert!(
                    registry
                        .obtain("u", &default_builder, false, 8, None, Arc::new(|_| true))
                        .await
                        .is_err()
                );
                assert!(algos.try_recv().is_err(), "never disposed");
                assert_eq!(signing(&account), Some(7));
                assert!(!registry.holds(&account, 8));
                // Anything else that differs is still refused while a live holder exists.
                let spot = InitPayload {
                    enable_spot_markets: Some(true),
                    ..default_builder.clone()
                };
                assert!(registry.rebuild_refusal("u", &spot, false, 9).is_some());
                assert!(
                    registry
                        .rebuild_refusal(
                            "u",
                            &InitPayload {
                                enable_spot_markets: Some(true),
                                ..init.clone()
                            },
                            false,
                            9
                        )
                        .is_some()
                );
                let same = registry
                    .obtain("u", &init, false, 8, None, Arc::new(|_| true))
                    .await
                    .unwrap();
                assert!(
                    Arc::ptr_eq(&same, &account),
                    "unchanged settings still share the live account"
                );
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_sole_holder_changing_its_builder_requires_an_explicit_action() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "hyperliquid".into(),
                    key: "0xabc".into(),
                    ..InitPayload::default()
                };
                let link = SignerLink::new(7, Arc::new(|_| true));
                let (account, _algos) = held_account_with(Some(link), init.fingerprint());
                register(&registry, &account);
                let assigned = InitPayload {
                    hyperliquid_builder: Some(crate::envelope::HyperliquidBuilder {
                        address: format!("0x{}", "4".repeat(40)),
                        fee_tenths_bps: 10.0,
                    }),
                    ..init.clone()
                };
                // Holder 7 is the only live holder: nothing to join, so its own change is no join.
                assert!(account.live_holders_besides(7).is_empty());
                assert!(registry.rebuild_refusal("u", &assigned, false, 7).is_some());
                assert_eq!(
                    differing_fields(&account.fingerprint, &assigned.fingerprint()),
                    vec!["hyperliquidBuilder".to_owned()]
                );
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn two_clients_differing_only_in_the_hyperliquid_builder_share_a_bybit_account() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybit".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = SignerLink::new(1, Arc::new(|_| true));
                let (mut account, mut algos) = held_account_with(Some(link), init.fingerprint());
                testing::bind_identity(&mut account, &init);
                register(&registry, &account);
                let other_device = InitPayload {
                    hyperliquid_builder: Some(crate::envelope::HyperliquidBuilder {
                        address: format!("0x{}", "4".repeat(40)),
                        fee_tenths_bps: 10.0,
                    }),
                    ..init.clone()
                };
                assert!(
                    registry
                        .rebuild_refusal("u", &other_device, false, 5)
                        .is_none()
                );
                let shared = registry
                    .obtain("u", &other_device, false, 5, None, Arc::new(|_| true))
                    .await
                    .unwrap();
                assert!(
                    Arc::ptr_eq(&shared, &account),
                    "one account for both clients"
                );
                assert!(algos.try_recv().is_err(), "never disposed");
            })
            .await;
    }

    #[test]
    fn woo_hosted_routing_resolves_before_loading_on_both_environments() {
        for testnet in [false, true] {
            for public_only in [false, true] {
                for (proxy, tunnel) in [(false, false), (true, false), (false, true)] {
                    let init = InitPayload {
                        account_name: "fixture-woo".into(),
                        exchange_name: "woo".into(),
                        key: "fixture-key".into(),
                        application_id: Some("fixture-app".into()),
                        testnet: Some(testnet),
                        tealstreet_proxy: Some(proxy),
                        tealstreet_tunnel: Some(tunnel),
                        ..InitPayload::default()
                    };
                    let plan = routing_plan(&init, public_only).unwrap();
                    assert!(plan.ws.public.host.is_none());
                    if testnet || !(proxy || tunnel) {
                        assert!(plan.ws.private.host.is_none());
                    } else {
                        let relay =
                            reqwest::Url::parse(plan.ws.private.host.as_ref().unwrap()).unwrap();
                        assert_eq!(
                            relay
                                .query_pairs()
                                .find(|(key, _)| key == "ts-relay")
                                .unwrap()
                                .1,
                            "wss://wss.woox.io/v2/ws/private/stream/fixture-app"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn woo_hosted_relay_suffix_matches_the_engine_application_id() {
        for application_id in [None, Some(""), Some("fixture-app")] {
            let init = InitPayload {
                account_name: "fixture-woo".into(),
                exchange_name: "woo".into(),
                key: "fixture-key".into(),
                application_id: application_id.map(str::to_owned),
                tealstreet_proxy: Some(true),
                ..InitPayload::default()
            };
            let options = engine_options(&init, false);
            let suffix = options["applicationId"].as_str().unwrap_or("undefined");
            let plan = routing_plan(&init, false).unwrap();
            let relay = reqwest::Url::parse(plan.ws.private.host.as_ref().unwrap()).unwrap();
            assert_eq!(
                relay
                    .query_pairs()
                    .find(|(key, _)| key == "ts-relay")
                    .unwrap()
                    .1,
                format!("wss://wss.woox.io/v2/ws/private/stream/{suffix}")
            );
        }
    }

    #[test]
    fn a_venue_with_no_hosted_origin_is_refused_in_plain_words() {
        let init = InitPayload {
            account_name: "a".into(),
            exchange_name: "asterdex".into(),
            ..InitPayload::default()
        };
        let refusal = routing_plan(&init, true).unwrap_err();
        assert_eq!(
            refusal,
            "asterdex cannot run on the Hosted server. Turn Hosted off to use this account."
        );
    }

    #[test]
    fn hosted_hyrotrader_is_refused_visibly() {
        assert_eq!(
            refuse_unhosted("hyrotrader"),
            Err(HYROTRADER_UNHOSTED.to_owned())
        );
        assert_eq!(refuse_unhosted("bybit"), Ok(()));
    }

    #[test]
    fn a_bybit_site_is_refused_outside_the_bybit_family() {
        let site = |venue: &str, site: &str| InitPayload {
            account_name: "a".into(),
            exchange_name: venue.into(),
            key: "k".into(),
            bybit_site_id: Some(site.into()),
            ..InitPayload::default()
        };
        for venue in BYBIT_SITE_VENUES {
            assert_eq!(refuse_misplaced_site(&site(venue, "BRA_BTL")), Ok(()));
        }
        for venue in ["binance", "hyperliquid", "okx"] {
            let refused = prepare(&site(venue, "BRA_BTL"), true, None).err();
            assert_eq!(
                refused.as_deref(),
                Some(format!("bybitSiteId applies only to Bybit accounts, not {venue}").as_str())
            );
            assert_eq!(refuse_misplaced_site(&site(venue, "")), Ok(()));
        }
    }

    #[test]
    fn hosted_lighter_tells_the_engine_its_sockets_are_routed() {
        for (venue, route, expected) in [
            ("lighter", Some(true), Some(true)),
            ("lighter-robinhood", Some(true), Some(true)),
            ("lighter", None, None),
            ("okx", Some(true), None),
        ] {
            let init = InitPayload {
                account_name: "a".into(),
                exchange_name: venue.into(),
                key: "k".into(),
                tealstreet_ws_route: route,
                ..InitPayload::default()
            };
            let options = engine_options(&init, false);
            assert_eq!(
                options["lighterWsRoute"].as_bool(),
                expected,
                "{venue} {route:?}"
            );
        }
    }

    /// Legacy sent Lighter REST through the public CORS proxy; Hosted keeps that, unauthorized.
    #[test]
    fn hosted_lighter_rest_goes_through_the_public_cors_proxy() {
        for (venue, testnet, proxy, expected) in [
            ("lighter", None, None, true),
            ("lighter", None, Some(false), true),
            ("lighter", Some(true), None, true),
            ("lighter-robinhood", None, None, true),
            ("lighter-robinhood", Some(true), None, false),
        ] {
            let init = InitPayload {
                account_name: "a".into(),
                exchange_name: venue.into(),
                key: "k".into(),
                tealstreet_proxy: proxy,
                testnet,
                ..InitPayload::default()
            };
            let plan = routing_plan(&init, false).unwrap();
            let http = serde_json::to_value(&plan).unwrap()["http"].clone();
            let prefix = http["prefix"].as_str().unwrap();
            let label = format!("{venue} {testnet:?} {proxy:?}: {prefix}");
            if expected {
                assert!(prefix.ends_with("/api/public_proxy"), "{label}");
            } else {
                assert_eq!(prefix, "", "{label}");
            }
            assert_eq!(http["authorize"], false, "{venue} {testnet:?} {proxy:?}");
        }
    }

    #[test]
    fn the_brazil_site_routes_to_its_host_and_header() {
        let venue = "bybit";
        let mut inputs = resolve::account_transport(venue, &AccountTransport::default()).unwrap();
        inputs.bybit_site_id = Some("BRA_BTL".into());
        let base = resolve::rest_base_url(venue, false).unwrap();
        let plan =
            resolve::resolve_routing_plan(venue, &inputs, base, Provider::Tealstreet).unwrap();
        let text = serde_json::to_string(&plan).unwrap();
        assert!(text.contains("BRA_BTL"), "{text}");
    }

    #[test]
    fn placed_symbols_keep_only_the_accounts_markets_and_are_capped() {
        let placed = PlacedSymbols::default();
        placed.record(["BTC-USDT-SWAP"]);
        assert!(
            placed.symbols().is_empty(),
            "nothing before markets are known"
        );
        let markets: Vec<String> = (0..MAX_PLACED_SYMBOLS + 10)
            .map(|index| format!("M{index}"))
            .chain(["BTC-USDT-SWAP".into(), "ETH-USDT-SWAP".into()])
            .collect();
        placed.sync_markets(markets.iter());
        placed.record_command(&json!({"op":"placeOrders","orders":[
            {"symbol":"BTC-USDT-SWAP","side":"buy"},
            {"symbol":"JUNK-1","side":"buy"},
            {"symbol":"ETH-USDT-SWAP","side":"sell"},
        ]}));
        placed.record_command(&json!({"op":"cancelSymbolOrders","symbol":"M1"}));
        assert_eq!(placed.symbols(), ["BTC-USDT-SWAP", "ETH-USDT-SWAP"]);
        placed.record(markets.iter().map(String::as_str));
        assert_eq!(placed.symbols().len(), MAX_PLACED_SYMBOLS);
        placed.sync_markets(markets[..3].iter());
        assert!(
            placed
                .symbols()
                .iter()
                .all(|symbol| markets[..3].contains(symbol))
        );
        // A same-size swap: one delisted, one listed.
        let swapped: Vec<String> = vec!["M0".into(), "M1".into(), "NEW-USDT-SWAP".into()];
        placed.record(["M2"]);
        assert!(placed.symbols().contains(&"M2".to_owned()));
        placed.sync_markets(swapped.iter());
        assert!(
            !placed.symbols().contains(&"M2".to_owned()),
            "the delisted symbol is pruned"
        );
        placed.record(["NEW-USDT-SWAP", "M2"]);
        assert!(placed.symbols().contains(&"NEW-USDT-SWAP".to_owned()));
        assert!(!placed.symbols().contains(&"M2".to_owned()));
    }

    #[tokio::test]
    async fn a_public_only_account_carries_and_follows_its_socket_token() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let token = |name: &str| Some((name.to_owned(), now_ms() + 60_000));
                let account = registry
                    .obtain(
                        "u",
                        &init_frame(""),
                        true,
                        1,
                        token("first"),
                        Arc::new(|_| true),
                    )
                    .await
                    .unwrap();
                assert_eq!(account.token.provider()().await.unwrap(), "first");
                let first = account.twap_owner.snapshot();
                account.sign_through_socket(2, Arc::new(|_| true), token("second"));
                assert_eq!(account.token.provider()().await.unwrap(), "second");
                let second = account.twap_owner.snapshot();
                assert_eq!(first.owner_id, "u");
                assert_eq!(second.owner_id, first.owner_id);
                // Same-user handovers stay inside the COSIGNED resume window's generation.
                assert_eq!(second.generation, first.generation);
                account.sign_through_socket(2, Arc::new(|_| true), token("refreshed"));
                assert_eq!(account.twap_owner.snapshot(), first);
                account.sign_through_socket(1, Arc::new(|_| true), token("returned"));
                assert_eq!(account.twap_owner.snapshot(), first);
                registry.dispose(&account, Disposal::SettingsChanged);
            })
            .await;
    }

    #[tokio::test]
    async fn the_proxy_token_refuses_once_expired() {
        let token = ProxyToken::new();
        let provider = token.provider();
        assert!(provider().await.is_err());
        token.set(Some(("t".into(), now_ms() + 60_000)));
        assert_eq!(token.provider()().await.unwrap(), "t");
        token.set(Some(("t".into(), now_ms().saturating_sub(1))));
        assert!(token.provider()().await.is_err());
    }
}

#[cfg(test)]
#[path = "delegated_accounts_tests.rs"]
mod delegated_accounts_tests;

#[cfg(test)]
#[path = "delegated_construction_tests.rs"]
mod delegated_construction_tests;

#[cfg(test)]
mod apply_settings_tests {
    use super::*;

    #[tokio::test]
    async fn apply_settings_retires_even_preconstruction_proofs_without_crossing_users() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybit".into(),
                    key: "synthetic-settings-key".into(),
                    ..Default::default()
                };
                let proof = registry.probe_permit("u", &init).unwrap();
                let other_user = registry.probe_permit("other", &init).unwrap();
                let attempt = registry
                    .begin_init(
                        "u",
                        &identity::registry_name("u", &init).unwrap(),
                        false,
                        &init.fingerprint(),
                        None,
                    )
                    .unwrap();
                let (mut account, _algos) = tests::held_account_with(
                    Some(SignerLink::new(7, Arc::new(|_| true))),
                    init.fingerprint(),
                );
                Arc::get_mut(&mut account).unwrap().venue = "bybit".into();
                testing::bind_identity(&mut account, &init);
                testing::register(&registry, &account);
                assert!(proof.check("u", &init).is_ok());
                assert!(registry.reset_settings(&account, 7, Arc::new(|_| true)));
                assert!(proof.check("u", &init).is_err());
                assert!(attempt.authority.check().is_err());
                assert!(other_user.check("other", &init).is_ok());
                let replacement = registry.probe_permit("u", &init).unwrap();
                drop(proof);
                drop(attempt);
                assert!(
                    replacement.check("u", &init).is_ok(),
                    "old cleanup cannot retire the new generation"
                );
                drop(replacement);
                drop(other_user);
                assert!(
                    registry.pending_inits.lock().unwrap().is_empty(),
                    "registrations must be reclaimed"
                );
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn apply_settings_is_required_for_live_and_parked_holders() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for parked in [false, true] {
                    let registry = tests::registry();
                    let init = InitPayload {
                        account_name: "acct".into(),
                        exchange_name: "bitget".into(),
                        ..Default::default()
                    };
                    let (account, mut algos) = tests::held_account_with(
                        Some(SignerLink::new(7, Arc::new(|_| true))),
                        init.fingerprint(),
                    );
                    testing::register(&registry, &account);
                    if parked {
                        registry.park("resume".into(), "u", vec![(account.clone(), 7)]);
                    }
                    let changed = InitPayload {
                        enable_spot_markets: Some(true),
                        ..init.clone()
                    };
                    for holder in [7, 8] {
                        assert!(
                            registry
                                .rebuild_refusal("u", &changed, false, holder)
                                .is_some(),
                            "parked={parked}, holder={holder}"
                        );
                        assert!(
                            registry
                                .obtain("u", &changed, false, holder, None, Arc::new(|_| true))
                                .await
                                .is_err()
                        );
                        let public = InitPayload {
                            public_only: Some(true),
                            ..changed.clone()
                        };
                        assert!(
                            registry
                                .rebuild_refusal("u", &public, true, holder)
                                .is_some(),
                            "public fallback must not bypass an explicit settings action"
                        );
                    }
                    assert!(algos.try_recv().is_err());
                    assert!(account.handle.disposal_cause().is_none());
                    assert!(
                        registry.rebuild_refusal("u", &init, false, 8).is_none(),
                        "identical settings still join"
                    );
                    registry.dispose(&account, Disposal::PageDisposed);
                }
            })
            .await;
    }
}

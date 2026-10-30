//! Accounts keyed by `(userId, accountName)`: built once, shared by that user's
//! sockets, disposed when the last holder (live socket or parked resume) lets go.
use crate::envelope::InitPayload;
use crate::state::Log;
use crate::threads::EngineThreads;
use safe_cex_native::resolve::{self, AccountTransport, Provider};
use safe_cex_native::{
    AccountHandle, NetworkTransport, ProxyTokenProvider, RoutingPlan, ServerConfig,
};
use safe_cex_remote_signer::{Outbox, SignerLink, remote_account};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

/// The unauthenticated public pseudo-account, as in NX `server.ts`.
pub const PUBLIC_ACCOUNT: &str = "__pub_bybit";
pub const PUBLIC_EXCHANGE: &str = "bybit";

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
    pub venue: String,
    pub handle: AccountHandle,
    /// Present for every private account; public-only accounts have no signer.
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
}

/// Concurrent waiting store reads per account; more are refused, not queued.
pub const MAX_READS: usize = 4;

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

/// A live socket holding an account: where its signing and proxy token go.
#[derive(Clone)]
pub struct Live {
    pub outbox: Outbox,
    pub token: Option<(String, u64)>,
}

#[derive(Default)]
struct Holders {
    by_id: HashMap<u64, Option<Live>>,
    /// The socket the signer and proxy token follow.
    signing: Option<u64>,
}

impl Holders {
    /// `None` when `leaving` was not signing; otherwise the newest live holder
    /// to sign through next, or `Some(None)` when none is live.
    fn successor(&self, leaving: u64) -> Option<Option<(u64, Live)>> {
        if self.signing != Some(leaving) {
            return None;
        }
        Some(
            self.by_id
                .iter()
                .filter(|(id, _)| **id != leaving)
                .filter_map(|(id, live)| live.clone().map(|live| (*id, live)))
                .max_by_key(|(id, _)| *id),
        )
    }
}

impl Account {
    pub fn is_public(&self) -> bool {
        self.link.is_none()
    }

    /// Live (not parked) sockets holding this account, other than `holder`.
    fn live_holders_besides(&self, holder: u64) -> Vec<u64> {
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
        holders.signing = Some(id);
        self.held.store(true, Ordering::Relaxed);
        if let Some(link) = &self.link {
            link.attach(id, live.outbox.clone());
            self.token.set(live.token.clone());
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
        let mut holders = self.holders.lock().unwrap();
        holders.by_id.insert(id, Some(live.clone()));
        self.sign_through(&mut holders, id, &live);
    }

    /// The current signing socket's outbox, for notices about the account itself.
    pub fn notify(&self, text: String) {
        let holders = self.holders.lock().unwrap();
        if let Some(Some(live)) = holders.signing.and_then(|id| holders.by_id.get(&id)) {
            (live.outbox)(text);
        }
    }
}

struct Parked {
    user: String,
    accounts: Vec<(Arc<Account>, u64)>,
    alias_until: Option<u64>,
}

pub struct Registry {
    accounts: Mutex<HashMap<(String, String), Arc<Account>>>,
    parked: Mutex<HashMap<String, Parked>>,
    /// Resumable sessions still open, by resume id: a resume of one whose drop
    /// the server has not seen yet (no close frame) takes it over.
    open: Mutex<HashMap<String, OpenSession>>,
    threads: EngineThreads,
    ids: AtomicU64,
    engine_config: ServerConfig,
    pub resume_grace: Duration,
    /// 5-10 account starts per second per egress IP, with jitter (DESIGN sizing).
    pacer: crate::pacing::Pacer,
    /// Why algos stopped while no browser was there to see it, one per
    /// (user, account), for that account's next init.
    notices: Notices,
}

/// A still-open resumable session, as a resume of its id reaches it.
struct OpenSession {
    user: String,
    /// Tells its socket loop to close now, as though the drop had been seen.
    close: Arc<tokio::sync::Notify>,
    /// Told once its accounts are parked.
    parked: Vec<tokio::sync::oneshot::Sender<()>>,
}

/// Stop notices by (user, account), each with when it was kept.
type Notices = Arc<Mutex<HashMap<(String, String), (String, tokio::time::Instant)>>>;

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
    if init.testnet == Some(true) {
        options.insert("testnet".into(), true.into());
    }
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
fn routing_plan(init: &InitPayload, public_only: bool) -> Result<RoutingPlan, String> {
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
    if public_only {
        inputs.public_data_only = Some(true);
        inputs.has_key = false;
    }
    let base_url = resolve::rest_base_url(venue, init.testnet.unwrap_or(false))
        .map_err(|error| error.to_string())?;
    resolve::resolve_routing_plan(venue, &inputs, base_url, Provider::Tealstreet)
        .map_err(|error| error.to_string())
}

/// What an account's engine is built from: its venue, its venue-scoped engine
/// options and its resolved routing, as one flat JSON object (so the names of
/// differing fields stay readable). Two inits that would build the same engine
/// share it, whatever else differs between the clients that sent them: a field
/// another venue reads, a default sent explicitly, flags resolving alike.
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
    Value::Object(fields).to_string()
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
            accounts: Mutex::default(),
            parked: Mutex::default(),
            open: Mutex::default(),
            threads,
            ids: AtomicU64::new(1),
            engine_config: ServerConfig {
                flush_every: account_flush,
                ..ServerConfig::default()
            },
            resume_grace,
            pacer: crate::pacing::Pacer::new(7, Duration::from_millis(60)),
            notices: Arc::default(),
        }
    }

    /// Server-allocated, process-unique IDs for sockets, park holders and clients.
    pub fn next_id(&self) -> u64 {
        self.ids.fetch_add(1, Ordering::Relaxed)
    }

    pub fn account_count(&self) -> usize {
        self.accounts.lock().unwrap().len()
    }

    pub fn threads(&self) -> &EngineThreads {
        &self.threads
    }

    /// Why `holder`'s init of an account must not rebuild it: another live
    /// socket holds that account under a different fingerprint, and disposing
    /// it would stop that client's account and algos (two clients would flap).
    pub fn rebuild_refusal(
        &self,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
    ) -> Option<String> {
        // Shared public data carries no signer or algos; it rebuilds as before.
        if public_only {
            return None;
        }
        let key = (user.to_owned(), init.account_name.clone());
        let account = self.accounts.lock().unwrap().get(&key).cloned()?;
        let fingerprint = init.fingerprint();
        if account.fingerprint == fingerprint && !account.is_public() {
            return None;
        }
        let others = account.live_holders_besides(holder);
        // Joins the live account instead (see `obtain`).
        if others.is_empty() || same_but_builder(&account.fingerprint, &fingerprint) {
            return None;
        }
        let differs = differing_fields(&account.fingerprint, &fingerprint);
        Log::warn(&format!(
            "init {} from socket {holder} refused: socket(s) {others:?} hold it with other settings ({})",
            init.account_name,
            differs.join(", ")
        ));
        Some(format!(
            "{} is open in another window or on another device with different settings ({}). The change was not applied and that copy keeps running: close it there, or reload both, and this one will load with the new settings.",
            init.account_name,
            differs.join(", ")
        ))
    }

    /// The account this user's `init` names, built or reused. Reuse requires an
    /// identical fingerprint; otherwise the old account is disposed first, but
    /// never while another live socket holds it (see `rebuild_refusal`).
    pub async fn obtain(
        self: &Arc<Self>,
        user: &str,
        init: &InitPayload,
        public_only: bool,
        holder: u64,
        token: Option<(String, u64)>,
        outbox: Outbox,
    ) -> Result<Arc<Account>, String> {
        let key = (user.to_owned(), init.account_name.clone());
        let fingerprint = init.fingerprint();
        let existing = self.accounts.lock().unwrap().get(&key).cloned();
        if let Some(account) = existing {
            if account.fingerprint == fingerprint && account.is_public() == public_only {
                account.hold(holder, Live { outbox, token });
                return Ok(account);
            }
            if let Some(refusal) = self.rebuild_refusal(user, init, public_only, holder) {
                return Err(refusal);
            }
            // The Hyperliquid builder is per device (browser storage): a second
            // client differing only in it trades with the live holder's builder.
            if !public_only
                && !account.live_holders_besides(holder).is_empty()
                && same_but_builder(&account.fingerprint, &fingerprint)
            {
                Log::info(&format!(
                    "init {} from socket {holder} joins the live account; only its hyperliquidBuilder differs",
                    init.account_name
                ));
                account.hold(holder, Live { outbox, token });
                return Ok(account);
            }
            self.dispose(&account);
        }
        let live = Live {
            outbox: outbox.clone(),
            token: token.clone(),
        };
        let (account, remote) = self
            .build(user, init, public_only, fingerprint, token, holder, outbox)
            .await?;
        let account = Arc::new(account);
        *account.algo_owner.lock().unwrap() = Some(Arc::downgrade(&account));
        account.hold(holder, live);
        if let Some(remote) = remote {
            // As TS `setFatalFailureCallback`: a signer failing repeatedly disposes the account.
            let registry = Arc::downgrade(self);
            let weak = Arc::downgrade(&account);
            remote.on_fatal(move || {
                if let (Some(registry), Some(account)) = (registry.upgrade(), weak.upgrade()) {
                    Log::warn(&format!("signer for {} failing; disposing", account.venue));
                    account.notify(crate::envelope::error(
                        Some(&account.name),
                        "Remote signing failed repeatedly; the account was disposed",
                    ));
                    registry.dispose(&account);
                }
            });
        }
        let replaced = self.accounts.lock().unwrap().insert(key, account.clone());
        if let Some(replaced) = replaced {
            // A concurrent init of the same name won the race; the later build stands.
            Self::stop(&replaced, "account replaced by a newer init", None);
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
    ) -> Result<(Account, Option<safe_cex_remote_signer::RemoteAccount>), String> {
        self.pacer.admit().await;
        let venue = init.exchange_name.clone();
        let link = (!public_only).then(|| SignerLink::new(socket, outbox));
        let Prepared {
            plan,
            options,
            remote,
        } = prepare(init, public_only, link.clone())?;
        let signer = remote.as_ref().map(|remote| remote.provider.clone());
        let proxy_token = ProxyToken::new();
        // Signer and public-only are one decision: a private account never builds
        // from options without the remote provider (keyed construction).
        assert_eq!(
            signer.is_none(),
            public_only,
            "private accounts sign remotely"
        );
        proxy_token.set(if public_only { None } else { token });
        let tokens = proxy_token.provider();
        let config = self.engine_config.clone();
        let algo_owner: Arc<Mutex<Option<std::sync::Weak<Account>>>> = Arc::default();
        let placed_symbols = PlacedSymbols::default();
        let algo_placed = placed_symbols.clone();
        let held = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let algo_held = held.clone();
        let (algos, algo_inbox) = if public_only {
            (None, None)
        } else {
            let (tx, rx) = mpsc::unbounded_channel();
            (Some(tx), Some(rx))
        };
        let algo_client = self.next_id();
        let algo_signer = link.clone();
        let owner = algo_owner.clone();
        let emit: crate::algos::Emit = Box::new(move |text| {
            let account = owner
                .lock()
                .unwrap()
                .as_ref()
                .and_then(std::sync::Weak::upgrade);
            if let Some(account) = account {
                account.notify(text);
            }
        });
        let algo_who = crate::audit::Who {
            user: user.to_owned(),
            account: init.account_name.clone(),
            venue: venue.clone(),
        };
        let (spawned, receive) = tokio::sync::oneshot::channel();
        let placed = self.threads.place(move |placement| {
            let transport = NetworkTransport::with_routing(plan, tokens);
            let result =
                safe_cex_native::spawn_venue_account(&venue, &options, transport, signer, config)
                    .map(|(handle, task)| {
                        tokio::task::spawn_local(async move {
                            let _ = task.await;
                            drop(placement);
                        });
                        if let Some(inbox) = algo_inbox {
                            tokio::task::spawn_local(crate::algos::run(
                                handle.clone(),
                                algo_client,
                                algo_who,
                                emit,
                                algo_signer,
                                algo_placed,
                                algo_held,
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
            venue: init.exchange_name.clone(),
            handle,
            link,
            token: proxy_token,
            fingerprint,
            holders: Mutex::default(),
            algos,
            algo_owner,
            placed: placed_symbols,
            held,
            frame_reads: Arc::new(tokio::sync::Semaphore::new(MAX_FRAME_READS)),
            reads: Arc::new(tokio::sync::Semaphore::new(MAX_READS)),
        };
        Ok((account, remote))
    }

    /// The account `user` holds under `name` through `holder`, never another user's.
    pub fn held(&self, user: &str, name: &str, holder: u64) -> Option<Arc<Account>> {
        let account = self
            .accounts
            .lock()
            .unwrap()
            .get(&(user.to_owned(), name.to_owned()))
            .cloned()?;
        let holds = account.holders.lock().unwrap().by_id.contains_key(&holder);
        holds.then_some(account)
    }

    /// Lets go of one holder; the last one disposes the account.
    pub fn release(&self, account: &Arc<Account>, holder: u64) {
        let empty = {
            let mut holders = account.holders.lock().unwrap();
            holders.by_id.remove(&holder);
            account.hand_over(&mut holders, holder);
            holders.by_id.is_empty()
        };
        if empty {
            self.dispose_for(account, "account disposed", None);
        }
    }

    /// The stop notice kept for `user`'s account `name`, once, if still fresh.
    pub fn take_notice(&self, user: &str, name: &str) -> Option<String> {
        let (notice, at) = self
            .notices
            .lock()
            .unwrap()
            .remove(&(user.to_owned(), name.to_owned()))?;
        (at.elapsed() < NOTICE_TTL).then_some(notice)
    }

    /// Where the algo host leaves a notice for a browser that is gone.
    fn keep_notice(&self, account: &Account) -> crate::algos::StopNotice {
        let notices = self.notices.clone();
        let key = (account.user.clone(), account.name.clone());
        Box::new(move |notice| {
            let mut notices = notices.lock().unwrap();
            notices.retain(|_, (_, at)| at.elapsed() < NOTICE_TTL);
            notices.insert(key, (notice, tokio::time::Instant::now()));
        })
    }

    /// With algos, their host settles chases first, then disposes the engine and signer.
    fn stop(account: &Account, reason: &str, stopped: Option<crate::algos::StopNotice>) {
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

    /// Disposes regardless of other holders (a rebuilt init or a fatal signer).
    pub fn dispose(&self, account: &Arc<Account>) {
        self.dispose_for(account, "account disposed", None);
    }

    fn dispose_for(
        &self,
        account: &Arc<Account>,
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
            "dispose {}: {reason} (holders {holders:?})",
            account.name
        ));
        let mut accounts = self.accounts.lock().unwrap();
        let key = (account.user.clone(), account.name.clone());
        if accounts
            .get(&key)
            .is_some_and(|current| Arc::ptr_eq(current, account))
        {
            accounts.remove(&key);
        }
        drop(accounts);
        *account.holders.lock().unwrap() = Holders::default();
        Self::stop(account, reason, stopped);
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
            let park_holder = self.next_id();
            {
                let mut holders = account.holders.lock().unwrap();
                holders.by_id.remove(&holder);
                holders.by_id.insert(park_holder, None);
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
                for (account, park_holder) in expired.accounts {
                    // No browser to tell: the next init of the account is told.
                    let last = {
                        let mut holders = account.holders.lock().unwrap();
                        holders.by_id.remove(&park_holder);
                        account.hand_over(&mut holders, park_holder);
                        holders.by_id.is_empty()
                    };
                    if last {
                        let stopped = registry.keep_notice(&account);
                        registry.dispose_for(&account, &reason, Some(stopped));
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

    /// Takes back a parked session for the same user; the new socket becomes
    /// the holder and signer owner. False for unknown, expired or foreign ids.
    pub fn resume(
        &self,
        resume_id: &str,
        user: &str,
        holder: u64,
        token: (String, u64),
        outbox: &Outbox,
    ) -> Vec<Arc<Account>> {
        let mut parked = self.parked.lock().unwrap();
        let Some(entry) = parked.get_mut(resume_id) else {
            return Vec::new();
        };
        if entry.user != user || entry.alias_until.is_some() {
            return Vec::new();
        }
        let accounts = std::mem::take(&mut entry.accounts);
        entry.alias_until = Some(now_ms() + 2 * self.resume_grace.as_millis() as u64);
        drop(parked);
        accounts
            .into_iter()
            .map(|(account, park_holder)| {
                account.holders.lock().unwrap().by_id.remove(&park_holder);
                account.hold(
                    holder,
                    Live {
                        outbox: outbox.clone(),
                        token: Some(token.clone()),
                    },
                );
                account
            })
            .collect()
    }
}

/// Two fingerprints that differ in nothing but the Hyperliquid builder.
fn same_but_builder(old: &str, new: &str) -> bool {
    let fields = differing_fields(old, new);
    !fields.is_empty() && fields.iter().all(|field| field == "hyperliquidBuilder")
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

#[cfg(test)]
pub(crate) mod tests {
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

    #[derive(Clone)]
    struct NoIo;
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
        let engine = safe_cex_engine::test_support::seeded_engine(Idle);
        let (handle, _task) = safe_cex_native::spawn_account(
            safe_cex_native::Host::new(engine, NoIo),
            ServerConfig::default(),
        )
        .unwrap();
        let (algos, inbox) = mpsc::unbounded_channel();
        let account = Arc::new(Account {
            user: "u".into(),
            name: "acct".into(),
            venue: "bitget".into(),
            handle,
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
        });
        account.hold(
            7,
            Live {
                outbox: Arc::new(|_| true),
                token: None,
            },
        );
        (account, inbox)
    }

    /// The socket `account` signs through.
    pub(crate) fn signing(account: &Account) -> Option<u64> {
        account.holders.lock().unwrap().signing
    }

    /// `account` as the registry's account for its (user, name).
    pub(crate) fn register(registry: &Registry, account: &Arc<Account>) {
        registry.accounts.lock().unwrap().insert(
            (account.user.clone(), account.name.clone()),
            account.clone(),
        );
    }

    pub(crate) fn registry() -> Arc<Registry> {
        Arc::new(Registry::new(
            EngineThreads::start(1).unwrap(),
            Duration::from_millis(1),
            Duration::from_secs(60),
        ))
    }

    #[tokio::test(start_paused = true)]
    async fn a_stop_notice_reaches_the_next_init_once_and_expires_after_a_day() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = registry();
                let (account, _algos) = held_account();
                (registry.keep_notice(&account))(
                    "Chases and TWAPs stopped: no browser for 60s".into(),
                );
                assert_eq!(
                    registry.take_notice("other", "acct"),
                    None,
                    "never another user's"
                );
                assert_eq!(
                    registry.take_notice("u", "acct").as_deref(),
                    Some("Chases and TWAPs stopped: no browser for 60s")
                );
                assert_eq!(registry.take_notice("u", "acct"), None, "delivered once");
                (registry.keep_notice(&account))("first".into());
                (registry.keep_notice(&account))("second".into());
                assert_eq!(
                    registry.take_notice("u", "acct").as_deref(),
                    Some("second"),
                    "one per account"
                );
                (registry.keep_notice(&account))("stale".into());
                tokio::time::sleep(NOTICE_TTL).await;
                assert_eq!(registry.take_notice("u", "acct"), None, "dropped after 24h");
            })
            .await;
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
                let resumed = registry.resume("r1", "u", 8, ("t".into(), u64::MAX), &outbox);
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
                        .resume("r1", "u", 8, token.clone(), &outbox)
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
                let resumed = registry.resume("r1", "u", 8, token, &outbox);
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
                        refused.contains("close it there, or reload both"),
                        "{refused}"
                    );
                }
                assert!(registry.rebuild_refusal("u", &other, false, 8).is_some());
                assert!(
                    registry.rebuild_refusal("u", &init, false, 8).is_none(),
                    "same settings join"
                );
                assert!(
                    registry.rebuild_refusal("u", &other, false, 7).is_none(),
                    "the holder itself changing its settings rebuilds"
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

                // Once that client is gone (parked only), the new settings rebuild as before.
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                assert!(registry.rebuild_refusal("u", &other, false, 8).is_none());
            })
            .await;
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

    #[test]
    fn signing_hands_over_to_the_newest_live_holder_then_suspends() {
        let live = |token: &str| {
            Some(Live {
                outbox: Arc::new(|_| true),
                token: Some((token.to_owned(), u64::MAX)),
            })
        };
        let mut holders = Holders::default();
        holders.by_id.insert(1, live("first tab"));
        holders.by_id.insert(2, live("second tab"));
        holders.by_id.insert(3, None);
        holders.signing = Some(2);
        assert!(
            holders.successor(1).is_none(),
            "a non-signing tab leaving changes nothing"
        );
        holders.by_id.remove(&2);
        let (id, next) = holders.successor(2).unwrap().unwrap();
        assert_eq!((id, next.token.unwrap().0.as_str()), (1, "first tab"));
        holders.signing = Some(1);
        holders.by_id.remove(&1);
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
            exchange_name: "bybit".into(),
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
    async fn a_second_device_differing_only_in_the_builder_joins_a_live_hyperliquid_account() {
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
                // Another device still on the default builder: joins, never refused or rebuilt.
                let default_builder = InitPayload {
                    hyperliquid_builder: None,
                    ..init.clone()
                };
                assert_ne!(default_builder.fingerprint(), init.fingerprint());
                assert!(
                    registry
                        .rebuild_refusal("u", &default_builder, false, 8)
                        .is_none()
                );
                let joined = registry
                    .obtain("u", &default_builder, false, 8, None, Arc::new(|_| true))
                    .await
                    .unwrap();
                assert!(Arc::ptr_eq(&joined, &account), "the live account, shared");
                assert!(algos.try_recv().is_err(), "never disposed");
                assert_eq!(signing(&account), Some(8));
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
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn the_sole_holder_changing_its_builder_rebuilds() {
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
                assert!(registry.rebuild_refusal("u", &assigned, false, 7).is_none());
                assert!(same_but_builder(
                    &account.fingerprint,
                    &assigned.fingerprint()
                ));
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
                let (account, mut algos) = held_account_with(Some(link), init.fingerprint());
                registry
                    .accounts
                    .lock()
                    .unwrap()
                    .insert(("u".into(), "acct".into()), account.clone());
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

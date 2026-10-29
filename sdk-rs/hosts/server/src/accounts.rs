//! Accounts keyed by `(userId, accountName)`: built once, shared by that user's
//! sockets, disposed when the last holder (live socket or parked resume) lets go.
use crate::envelope::InitPayload;
use crate::state::Log;
use crate::threads::EngineThreads;
use safe_cex_native::resolve::{self, AccountTransport, Provider};
use safe_cex_native::{AccountHandle, NetworkTransport, ProxyTokenProvider, ServerConfig};
use safe_cex_remote_signer::{Outbox, SignerLink, remote_account};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

    /// Signing and the proxy token follow `id` (the newest init or resume).
    fn sign_through(&self, holders: &mut Holders, id: u64, live: &Live) {
        holders.signing = Some(id);
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
    threads: EngineThreads,
    ids: AtomicU64,
    engine_config: ServerConfig,
    pub resume_grace: Duration,
}

/// Engine options for a hosted account: non-secret init fields only.
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
    // As `isUsableBuilder`: an unusable builder falls back to the default.
    if let Some(builder) = init
        .hyperliquid_builder
        .as_ref()
        .filter(|builder| usable_builder(builder))
    {
        options.insert("hyperliquidBuilder".into(), json!(builder));
    }
    if public_only {
        options.insert("publicDataOnly".into(), true.into());
    }
    Value::Object(options)
}

fn usable_builder(builder: &crate::envelope::HyperliquidBuilder) -> bool {
    let address = builder.address.as_bytes();
    address.len() == 42
        && address.starts_with(b"0x")
        && address[2..].iter().all(u8::is_ascii_hexdigit)
        && builder.fee_tenths_bps.is_finite()
        && builder.fee_tenths_bps > 0.0
}

impl Registry {
    pub fn new(threads: EngineThreads, account_flush: Duration, resume_grace: Duration) -> Self {
        Self {
            accounts: Mutex::default(),
            parked: Mutex::default(),
            threads,
            ids: AtomicU64::new(1),
            engine_config: ServerConfig {
                flush_every: account_flush,
                ..ServerConfig::default()
            },
            resume_grace,
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

    /// The account this user's `init` names, built or reused. Reuse requires an
    /// identical fingerprint; otherwise the old account is disposed first.
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
            Self::stop(&replaced);
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
        let venue = init.exchange_name.clone();
        let transport_flags = AccountTransport {
            tealstreet_proxy: init.tealstreet_proxy,
            tealstreet_tunnel: init.tealstreet_tunnel,
            tealstreet_ws_route: init.tealstreet_ws_route,
            proxy_region: init.proxy_region.clone(),
            testnet: init.testnet,
        };
        let mut inputs = resolve::account_transport(&venue, &transport_flags)
            .map_err(|error| error.to_string())?;
        if public_only {
            inputs.public_data_only = Some(true);
            inputs.has_key = false;
        }
        let base_url = resolve::rest_base_url(&venue, init.testnet.unwrap_or(false))
            .map_err(|error| error.to_string())?;
        let plan = resolve::resolve_routing_plan(&venue, &inputs, base_url, Provider::Tealstreet)
            .map_err(|error| error.to_string())?;
        let options = engine_options(init, public_only).to_string();
        let proxy_token = ProxyToken::new();
        let (link, signer, options, remote) = if public_only {
            (None, None, options, None)
        } else {
            let link = SignerLink::new(socket, outbox);
            let remote = remote_account(
                link.clone(),
                &venue,
                Some(init.account_name.clone()),
                &options,
            )
            .map_err(|error| error.to_string())?;
            let options = remote.options_json.clone();
            (
                Some(link),
                Some(remote.provider.clone()),
                options,
                Some(remote),
            )
        };
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
            self.dispose(account);
        }
    }

    fn stop(account: &Account) {
        if let Some(link) = &account.link {
            link.mark_unavailable();
        }
        account.handle.dispose();
    }

    /// Disposes regardless of other holders (a rebuilt init or a fatal signer).
    pub fn dispose(&self, account: &Arc<Account>) {
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
        Self::stop(account);
    }

    /// Moves a closing socket's private accounts under `resume_id` for the grace
    /// window. Their signers suspend: what waited on the old socket fails.
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
                for (account, park_holder) in expired.accounts {
                    registry.release(&account, park_holder);
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

/// 128 bits from the OS CSPRNG, hex; never derived from time, counters or users.
pub fn resume_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("OS entropy");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(
            engine_options(&init(&good, 10.0), false)
                .get("hyperliquidBuilder")
                .is_some()
        );
        for (address, fee) in [
            (good.as_str(), 0.0),
            (good.as_str(), f64::NAN),
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

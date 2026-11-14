//! One `/account` socket: the JSON envelope over this user's accounts.
//! Every account op resolves through (this socket's user, accounts it joined).
use crate::accounts::now_ms;
use crate::accounts::{Account, Disposal, PUBLIC_ACCOUNT, PUBLIC_EXCHANGE, Registry};
use crate::envelope::{self, AccountMsg, Inbound, InitPayload};
use crate::methods::{self, Plan};
use crate::out::Out;
use crate::state::{Log, Metrics};
use safe_cex_algo_twap::{PersistedTwap, TwapStatus};
use safe_cex_engine::capabilities::CancelAll;
use safe_cex_native::{AccountHandle, ClientLink, Outgoing, SocketSummary};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

mod order_actions;
mod order_frames;
mod settings;

#[derive(Clone, Copy)]
pub(super) enum OrderReceipt {
    Placement(u64),
    Action {
        seq: u64,
        kind: &'static str,
        deadline: crate::order_actions::ActionDeadline,
    },
}

/// Who connected, as verified before the upgrade.
#[derive(Clone)]
pub struct Identity {
    /// `None` for an unauthenticated public-data socket.
    pub user: Option<String>,
    pub chat_handle: Option<crate::auth::ChatHandle>,
    pub token: Option<(String, u64)>,
    pub resume_from: Option<String>,
    pub resumable: bool,
}

/// One engine command waiting on its done notice.
struct Pending {
    request_id: Value,
    method: String,
    reply: methods::Reply,
    /// A cancel-all fanned out over symbols: one shared outcome for the group.
    group: Option<Arc<Mutex<Group>>>,
    /// The engine said this history read returned only part of the window.
    incomplete: bool,
    /// Order-affecting commands are logged with their result when they settle.
    audit: Option<Audit>,
}

struct Audit {
    who: crate::audit::Who,
    origin: String,
    fields: serde_json::Map<String, Value>,
}

struct Group {
    remaining: usize,
    failures: Vec<String>,
}

/// One joined client of an account. Dropping its `ClientLink` (by aborting the
/// pump) is how a leave is guaranteed even when the account's inbox is full.
struct Client {
    id: u64,
    pump: JoinHandle<()>,
    pending: Arc<Mutex<HashMap<u32, Pending>>>,
    /// After a re-snapshot the old client only finishes its pending replies.
    draining: Arc<AtomicBool>,
}

/// Subscribers one socket can stack on a book or candle key before extras stop counting.
const MAX_SUBSCRIBERS_PER_KEY: u32 = 64;

struct Joined {
    account: Arc<Account>,
    out: Out,
    client: Client,
    old: Vec<Client>,
    next_command: u32,
    /// Book and candle subscribe commands this socket holds, by command text and count.
    /// The engine keeps one demand per client, so a re-snapshot replays each key once.
    subscriptions: HashMap<String, u32>,
}

#[derive(Default)]
struct RefusalNotifications {
    legacy_reason: Option<String>,
    registered: HashSet<(String, String)>,
}

pub struct Session {
    user_socket: Option<crate::user_drain::Socket>,
    registry: Arc<Registry>,
    metrics: Arc<Metrics>,
    identity: Identity,
    pub socket: u64,
    pub resume_id: String,
    out: Out,
    joined: HashMap<String, Joined>,
    /// Current fallback reply registration, independent of live account authority.
    registration_outputs: HashMap<String, Out>,
    /// A held client's account has loaded: `(account name, client id)`.
    loaded: tokio::sync::mpsc::UnboundedSender<(String, u64)>,
    loaded_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, u64)>>,
    /// A client the engine removed (its queue full, or a frame too large): `(account, client)`.
    dropped: tokio::sync::mpsc::UnboundedSender<(String, u64)>,
    dropped_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, u64)>>,
    /// When each account was last re-snapshotted after such a removal.
    rejoined_at: HashMap<String, std::time::Instant>,
    /// Told when a resume of this session's id takes it over.
    taken_over: Arc<tokio::sync::Notify>,
    /// Settings refusals already sent per account and registration, so backoff
    /// retries are quiet; cleared before an init proceeds to account obtain.
    refused: HashMap<String, RefusalNotifications>,
    /// Chart and book subscriptions that arrived before their account joined,
    /// applied at join: the page sends them whenever its chart asks.
    early: HashMap<String, Vec<Topic>>,
    /// Moved off a draining box: its accounts are released, never parked here.
    moved: bool,
    /// Captured at connection creation; deployments drain/restart to change it.
    order_frames_enabled: bool,
    handovers: crate::delegated::Handovers,
    pending_verifications: HashMap<String, PendingVerification>,
    next_verification: u64,
    verified: tokio::sync::mpsc::UnboundedSender<JoinCompletion>,
    verified_rx: Option<tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>>,
}

struct PendingVerification {
    generation: u64,
    task: tokio::task::JoinHandle<()>,
    out: Out,
    cosigned_link: Option<Arc<safe_cex_remote_signer::SignerLink>>,
    _signer_output: Option<crate::out::OutputLifetime>,
}

impl Drop for PendingVerification {
    fn drop(&mut self) {
        self.task.abort();
        if let Some(link) = &self.cosigned_link {
            link.mark_unavailable();
        }
    }
}

/// An off-loop result still belongs to the exact connection and init that
/// created it. The socket loop alone decides whether it may be applied.
pub struct JoinCompletion {
    user: String,
    socket: u64,
    generation: u64,
    account_name: String,
    candidate: Result<crate::delegated::Credentials, crate::delegated::Refused>,
    probe: Option<ProbedInit>,
}

struct ProbedInit {
    init: InitPayload,
    permit: Result<crate::accounts::ProbePermit, crate::delegated::probe::ProbeRefusal>,
}

/// A chart or book subscription, by its page op.
#[derive(Clone, Debug)]
struct Topic {
    op: String,
    symbol: String,
    interval: Option<String>,
    /// A chart's forced reset: the upstream stream may be dead, so it is repaired too.
    reset: bool,
}

/// Per account; a page re-sends its live topics on every init, so this only
/// needs to outlast one join.
const EARLY_TOPICS: usize = 64;

/// A page that cannot keep up is re-snapshotted at most this often.
const REJOIN_AFTER_DROP_MIN: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a resume waits for the session it takes over to park, as long as
/// that socket loop can be stuck in one send.
const TAKEOVER_WAIT: std::time::Duration = std::time::Duration::from_secs(12);

/// A private account's first snapshot waits for its orders and positions, so a
/// fresh build never shows the client an empty account; past this it goes anyway.
/// Forced past a reconnect or a hidden tab: the user pressed an emergency control.
const NUKE_WAITING: &str = "Nuke is waiting for this account's positions and orders to load. It runs once they are known, and fails if they have not loaded within 30s.";

pub const SNAPSHOT_HOLD_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);
const SNAPSHOT_HOLD_CHECK: std::time::Duration = std::time::Duration::from_millis(250);
/// How long a hold's load check may wait on the engine; a miss is retried next hold tick.
const HOLD_LOAD_CHECK: std::time::Duration = std::time::Duration::from_millis(20);

const PUBLIC_OPS: [&str; 5] = [
    "sendSnapshot",
    "subscribeOrderBook",
    "subscribeOHLCV",
    "unsubscribeOrderBook",
    "unsubscribeOHLCV",
];

/// The close a drained box sends: this box is going away, reconnect elsewhere.
pub const MOVED_CODE: u16 = 4100;
pub const MOVED_REASON: &str = "draining: reconnect";

/// Why the socket must close now.
#[derive(Debug, PartialEq, Eq)]
pub enum Close {
    /// 1008, with the reason.
    Policy(&'static str),
    /// The box is draining: [`MOVED_CODE`].
    Moved,
}

impl Close {
    pub fn code(&self) -> u16 {
        match self {
            Self::Policy(_) => 1008,
            Self::Moved => MOVED_CODE,
        }
    }
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Policy(reason) => reason,
            Self::Moved => MOVED_REASON,
        }
    }
}

impl Session {
    fn token_expires_at_ms(&self) -> Option<u64> {
        self.identity.token.as_ref().map(|(_, expires)| *expires)
    }

    fn token_expired(&self) -> bool {
        crate::auth::Verifier::check_expiry(self.token_expires_at_ms(), now_ms()).is_err()
    }

    pub fn new(
        registry: Arc<Registry>,
        metrics: Arc<Metrics>,
        identity: Identity,
        out: Out,
    ) -> Self {
        if let Some(user) = identity.user.as_deref() {
            registry.user_connected(user, identity.chat_handle.as_ref());
        }
        let socket = registry.next_id();
        let (loaded, loaded_rx) = tokio::sync::mpsc::unbounded_channel();
        let (dropped, dropped_rx) = tokio::sync::mpsc::unbounded_channel();
        let (verified, verified_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            user_socket: identity
                .user
                .as_deref()
                .map(|user| registry.user_drains.socket(user)),
            registry,
            metrics,
            identity,
            socket,
            resume_id: crate::accounts::resume_id(),
            out,
            joined: HashMap::new(),
            registration_outputs: HashMap::new(),
            loaded,
            loaded_rx: Some(loaded_rx),
            dropped,
            dropped_rx: Some(dropped_rx),
            rejoined_at: HashMap::new(),
            taken_over: Arc::new(tokio::sync::Notify::new()),
            refused: HashMap::new(),
            early: HashMap::new(),
            moved: false,
            order_frames_enabled: crate::order_contract::runtime_enabled(),
            handovers: crate::delegated::Handovers::default(),
            pending_verifications: HashMap::new(),
            next_verification: 0,
            verified,
            verified_rx: Some(verified_rx),
        }
    }

    fn router_fenced(&self) -> bool {
        self.identity
            .user
            .as_deref()
            .is_some_and(|user| self.registry.user_lease_fenced(user))
            || self.joined.values().any(|joined| {
                !joined.account.is_public()
                    && (!joined.account.running.io_allowed()
                        || joined.account.running.parked_by_lease())
            })
    }

    pub fn user_draining(&self) -> bool {
        self.router_fenced()
            || self
                .user_socket
                .as_ref()
                .is_some_and(crate::user_drain::Socket::latched)
    }

    /// Whether a drain moves this socket now: break-glass moves every socket,
    /// a waiting drain only one whose accounts run no chase or TWAP.
    pub fn drain_close(&mut self) -> Option<Close> {
        let user_moves = self.router_fenced()
            || self
                .user_socket
                .as_ref()
                .is_some_and(crate::user_drain::Socket::should_close);
        let moves = user_moves
            || match self.registry.drain_state() {
                crate::accounts::Drain::Off => false,
                crate::accounts::Drain::Now => true,
                crate::accounts::Drain::Waiting => !self
                    .joined
                    .values()
                    .any(|joined| joined.account.running.any()),
            };
        self.moved |= moves;
        moves.then_some(Close::Moved)
    }

    /// A draining box admits inits only from users already live on it.
    fn moved_by_drain(&self) -> bool {
        if self.router_fenced() {
            return true;
        }
        if self
            .user_socket
            .as_ref()
            .is_some_and(crate::user_drain::Socket::should_close)
        {
            return true;
        }
        match self.registry.drain_state() {
            crate::accounts::Drain::Off => false,
            crate::accounts::Drain::Now => true,
            crate::accounts::Drain::Waiting => !self
                .identity
                .user
                .as_deref()
                .is_some_and(|user| self.registry.live_here(user)),
        }
    }

    /// Enough of the resume id to tell sessions apart in logs, never enough to resume one.
    fn resume_id_prefix(&self) -> &str {
        &self.resume_id[..self.resume_id.len().min(6)]
    }

    /// Fires when a resume of this session's id takes it over: close and park now.
    pub fn taken_over(&self) -> Arc<tokio::sync::Notify> {
        self.taken_over.clone()
    }

    /// Any frame from the browser: its sockets' signs are still being heard.
    pub fn heard(&self) {
        for joined in self.joined.values() {
            if let Some(link) = &joined.account.link {
                link.heard(self.socket);
            }
        }
    }

    /// The socket loop's receiver of held clients whose accounts have loaded;
    /// each goes to [`Session::account_loaded`].
    pub fn take_loaded(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<(String, u64)> {
        self.loaded_rx.take().expect("taken once")
    }

    /// The socket loop's receiver of clients the engine removed; each goes to
    /// [`Session::client_dropped`].
    pub fn take_dropped(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<(String, u64)> {
        self.dropped_rx.take().expect("taken once")
    }

    pub fn take_verified(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<JoinCompletion> {
        self.verified_rx.take().expect("taken once")
    }

    pub fn expire_handovers(&mut self) {
        self.handovers.expire(crate::accounts::now_ms());
    }

    fn cancel_verification(&mut self, name: &str) {
        self.pending_verifications.remove(name);
    }

    #[cfg(test)]
    fn start_verification<F>(&mut self, name: String, verify: F)
    where
        F: std::future::Future<
                Output = Result<crate::delegated::Credentials, crate::delegated::Refused>,
            > + Send
            + 'static,
    {
        self.start_probe_verification(name, None, async move { (verify.await, None) });
    }

    fn start_probe_verification<F>(
        &mut self,
        name: String,
        signer: Option<(
            Arc<safe_cex_remote_signer::SignerLink>,
            crate::out::OutputLifetime,
        )>,
        verify: F,
    ) where
        F: std::future::Future<
                Output = (
                    Result<crate::delegated::Credentials, crate::delegated::Refused>,
                    Option<ProbedInit>,
                ),
            > + Send
            + 'static,
    {
        self.cancel_verification(&name);
        let (cosigned_link, signer_output) = signer.unzip();
        self.next_verification += 1;
        let generation = self.next_verification;
        let user = self.user().to_owned();
        let socket = self.socket;
        let account_name = name.clone();
        let completed = self.verified.clone();
        let out = self.handover_out(&name);
        let task = tokio::spawn(async move {
            let (candidate, probe) = verify.await;
            // A closed receiver drops the candidate and erases its fields.
            let _ = completed.send(JoinCompletion {
                user,
                socket,
                generation,
                account_name,
                candidate,
                probe,
            });
        });
        self.pending_verifications.insert(
            name,
            PendingVerification {
                generation,
                task,
                out,
                cosigned_link,
                _signer_output: signer_output,
            },
        );
    }

    pub async fn verification_completed(&mut self, completed: JoinCompletion) -> Result<(), Close> {
        if completed.user != self.user()
            || completed.socket != self.socket
            || self
                .pending_verifications
                .get(&completed.account_name)
                .is_none_or(|pending| pending.generation != completed.generation)
        {
            return Ok(());
        }
        let Some(mut out) = self
            .pending_verifications
            .get(&completed.account_name)
            .map(|pending| pending.out.clone())
        else {
            return Ok(());
        };
        self.cancel_verification(&completed.account_name);
        if self.token_expired() {
            self.drop_early(&completed.account_name, "admission expired");
            out.send(envelope::error(
                Some(&completed.account_name),
                crate::auth::TOKEN_EXPIRED,
            ));
            return Ok(());
        }
        if self.moved_by_drain() {
            self.moved = true;
            return Err(Close::Moved);
        }
        if !self.admission_token_valid() {
            self.drop_early(&completed.account_name, "verification refused");
            out.send(envelope::init_unverified(&completed.account_name, false));
            return Ok(());
        }
        if let Some(probe) = completed.probe {
            let candidate = completed.candidate.ok();
            out = out.account(&completed.account_name);
            let account = match probe.permit {
                Ok(permit) => {
                    self.registry
                        .obtain_probed_join(
                            self.user(),
                            &probe.init,
                            self.socket,
                            self.identity.token.clone(),
                            out.outbox(),
                            candidate.as_ref().map(crate::delegated::Credentials::pair),
                            permit,
                        )
                        .await
                }
                Err(refusal) => Err(refusal),
            };
            drop(candidate);
            match account {
                Ok(account) => {
                    self.metrics.note_init();
                    if self
                        .joined
                        .get(&completed.account_name)
                        .is_none_or(|joined| !Arc::ptr_eq(&joined.account, &account))
                    {
                        self.out.retire_account(&completed.account_name);
                    }
                    out = out.account(&completed.account_name);
                    account.hold_registration(
                        self.socket,
                        out.outbox(),
                        self.identity.token.clone(),
                        &completed.account_name,
                    );
                    if let Some(joined) = self.joined.get_mut(&completed.account_name)
                        && Arc::ptr_eq(&joined.account, &account)
                    {
                        let previous = std::mem::replace(&mut joined.out, out.clone());
                        previous.retire_order_receipts();
                        previous.finish_registration(&out);
                        self.resnapshot(&completed.account_name).await;
                    } else {
                        if let Some(previous) = self.joined.remove(&completed.account_name) {
                            previous.out.retire_registration();
                            let previous_account = previous.account.clone();
                            Self::leave_engine(previous);
                            self.registry.release_label(
                                &previous_account,
                                self.socket,
                                &completed.account_name,
                                Disposal::SettingsChanged,
                            );
                        }
                        self.join(account, out.clone()).await;
                    }
                    if self.joined.contains_key(&completed.account_name) {
                        if self.joined[&completed.account_name]
                            .account
                            .delegated
                            .is_some()
                        {
                            self.registry.interruptions.delegated_status(
                                self.user(),
                                &completed.account_name,
                                &out,
                                crate::interruption::DelegatedKeyStatus::KeyAvailable,
                            );
                        }
                        self.registry.interruptions.deliver(
                            self.user(),
                            &completed.account_name,
                            &out,
                        );
                        if let Some(kept) = self.registry.take_notice(
                            self.user(),
                            self.joined[&completed.account_name].account.registry_name(),
                            &self.joined[&completed.account_name].account.fingerprint,
                            self.identity.resume_from.as_deref(),
                        ) {
                            for frame in kept.frames(&completed.account_name) {
                                out.send(frame);
                            }
                        }
                    }
                }
                Err(refusal) => {
                    self.drop_early(&completed.account_name, "verification refused");
                    out.send(envelope::init_unverified(
                        &completed.account_name,
                        refusal.transient,
                    ));
                }
            }
            return Ok(());
        }
        // Opening-only test completions do not carry venue verification.
        let transient = completed.candidate.is_ok();
        self.drop_early(&completed.account_name, "verification refused");
        out.send(envelope::init_unverified(
            &completed.account_name,
            transient,
        ));
        Ok(())
    }

    fn admission_token_valid(&self) -> bool {
        self.identity.user.is_some()
            && self
                .identity
                .token
                .as_ref()
                .is_some_and(|(_, expiry)| *expiry > now_ms())
    }

    fn admission_init(&mut self, init: InitPayload) {
        self.cancel_verification(&init.account_name);
        let delegated = init.signing_mode.as_deref() == Some("delegated");
        if !self.admission_token_valid()
            || delegated && !crate::delegated::enabled(&init.exchange_name)
            || init.public_only == Some(true)
            || init.account_name.starts_with("__pub_")
            || self
                .registry
                .signing_mode_refusal(
                    self.user(),
                    &init,
                    false,
                    init.signing_mode.as_deref() == Some("delegated"),
                )
                .is_some()
        {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "delegated init refused");
            self.refuse_handover(&init.account_name, false);
            return;
        }
        if self.pending_verifications.len() >= 64 {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "verification capacity reached");
            self.refuse_handover(&init.account_name, true);
            return;
        }
        if !delegated && !crate::delegated::probe::cosigned_ready(&init.exchange_name) {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "proof unavailable");
            self.handover_out(&init.account_name)
                .send(envelope::init_proof_unavailable(&init.account_name));
            return;
        }
        if delegated {
            self.open_probed_handover(init);
        } else if init.sealed.is_some() {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "verification refused");
            self.refuse_handover(&init.account_name, false);
        } else {
            self.handovers.forget(&init.account_name);
            self.open_probed_cosigned(init);
        }
    }

    fn open_probed_cosigned(&mut self, init: InitPayload) {
        let permit = self.registry.probe_permit(self.user(), &init);
        let out = self.handover_out(&init.account_name);
        let (signing_out, lifetime) = out.isolated();
        let link = safe_cex_remote_signer::SignerLink::new(self.socket, signing_out.outbox());
        let registry = self.registry.clone();
        let user = self.user().to_owned();
        let token = self.identity.token.clone();
        self.start_probe_verification(
            init.account_name.clone(),
            Some((link.clone(), lifetime)),
            async move {
                let permit = match permit {
                    Ok(permit) => {
                        registry
                            .probe_join(&user, &init, token, None, Some(link), permit)
                            .await
                    }
                    Err(refusal) => Err(refusal),
                };
                (
                    Err(crate::delegated::Refused),
                    Some(ProbedInit { init, permit }),
                )
            },
        );
    }

    fn open_probed_handover(&mut self, mut init: InitPayload) {
        let permit = match self.registry.probe_permit(self.user(), &init) {
            Ok(permit) => permit,
            Err(refusal) => {
                self.handovers.forget(&init.account_name);
                self.drop_early(&init.account_name, "verification refused");
                self.refuse_handover(&init.account_name, refusal.transient);
                return;
            }
        };
        let Some(sealed) = init.sealed.take() else {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "delegated init refused");
            self.refuse_handover(&init.account_name, false);
            return;
        };
        let opening =
            match self
                .handovers
                .take(&init.account_name, &sealed, crate::accounts::now_ms())
            {
                Ok(opening) => opening,
                Err(_) => {
                    self.drop_early(&init.account_name, "delegated init refused");
                    self.refuse_handover(&init.account_name, false);
                    return;
                }
            };
        let registry = self.registry.clone();
        let user = self.user().to_owned();
        let token = self.identity.token.clone();
        self.start_probe_verification(init.account_name.clone(), None, async move {
            let candidate = opening.open(
                &init.exchange_name,
                &init.key,
                &sealed,
                crate::accounts::now_ms(),
            );
            let probe = match candidate.as_ref() {
                Ok(candidate) => Some(ProbedInit {
                    permit: registry
                        .probe_join(&user, &init, token, Some(candidate), None, permit)
                        .await,
                    init,
                }),
                Err(_) => None,
            };
            let candidate = if probe.as_ref().is_some_and(|probe| probe.permit.is_err()) {
                drop(candidate);
                Err(crate::delegated::Refused)
            } else {
                candidate
            };
            (candidate, probe)
        });
    }

    #[cfg(test)]
    fn open_handover(&mut self, mut init: InitPayload) {
        let Some(sealed) = init.sealed.take() else {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "delegated init refused");
            self.refuse_handover(&init.account_name, false);
            return;
        };
        let opening =
            match self
                .handovers
                .take(&init.account_name, &sealed, crate::accounts::now_ms())
            {
                Ok(opening) => opening,
                Err(_) => {
                    self.drop_early(&init.account_name, "delegated init refused");
                    self.refuse_handover(&init.account_name, false);
                    return;
                }
            };
        self.start_verification(init.account_name.clone(), async move {
            opening.open(
                &init.exchange_name,
                &init.key,
                &sealed,
                crate::accounts::now_ms(),
            )
        });
    }

    /// The engine removed this client without the session asking: its queue was
    /// full or a frame too large. Without this the page got nothing and saw no gap.
    pub async fn client_dropped(&mut self, name: &str, client: u64) {
        if self
            .joined
            .get(name)
            .is_none_or(|joined| joined.client.id != client)
        {
            return;
        }
        let recent = self
            .rejoined_at
            .get(name)
            .is_some_and(|at| at.elapsed() < REJOIN_AFTER_DROP_MIN);
        if recent {
            Log::warn(&format!(
                "engine dropped client {client} of {name} on socket {} again within {}s; left to the page's resnapshot",
                self.socket,
                REJOIN_AFTER_DROP_MIN.as_secs()
            ));
            return;
        }
        Log::warn(&format!(
            "engine dropped client {client} of {name} on socket {}: its queue was full or a frame too large; re-snapshotting",
            self.socket
        ));
        self.rejoined_at
            .insert(name.to_owned(), std::time::Instant::now());
        self.resnapshot(name).await;
    }

    /// A held client's account loaded: its client gets a full snapshot now.
    pub async fn account_loaded(&mut self, name: &str, client: u64) {
        if self
            .joined
            .get(name)
            .is_some_and(|joined| joined.client.id == client)
        {
            self.resnapshot(name).await;
        }
    }

    fn user(&self) -> &str {
        // Unauthenticated sockets share the public account under the empty user.
        self.identity.user.as_deref().unwrap_or("")
    }

    fn send(&self, text: String) {
        let out = envelope::account_name(&text)
            .map(|name| self.account_out(&name))
            .unwrap_or_else(|| self.out.clone());
        let _ = out.send(text);
    }

    fn account_out(&self, name: &str) -> Out {
        self.joined
            .get(name)
            .map(|joined| &joined.out)
            .cloned()
            .or_else(|| {
                self.registration_outputs
                    .get(name)
                    .map(|out| out.account(name))
            })
            .unwrap_or_else(|| self.out.account(name))
    }

    fn handover_out(&self, name: &str) -> Out {
        self.registration_outputs
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.account_out(name))
    }

    fn refuse_handover(&self, name: &str, transient: bool) {
        self.handover_out(name)
            .send(envelope::init_unverified(name, transient));
    }

    /// Exercise the trusted construction entry point from this authenticated
    /// socket without enabling a new socket operation.
    #[cfg(test)]
    pub(crate) fn delegated_test_build(
        &self,
        init: InitPayload,
        pair: crate::delegated::DelegatedPair,
    ) -> impl std::future::Future<Output = Result<Arc<Account>, String>> + use<> {
        let registry = self.registry.clone();
        let identity = self.identity.clone();
        let socket = self.socket;
        let outbox = self.out.outbox();
        async move {
            let user = identity.user.ok_or("Authenticated user required")?;
            registry
                .obtain_delegated(&user, &init, socket, identity.token, outbox, pair)
                .await
        }
    }

    /// Called once after the upgrade, for `resumable=1` sockets.
    pub async fn open(&mut self) {
        if !self.identity.resumable {
            return;
        }
        if self.identity.user.is_some() && !self.admission_token_valid() {
            self.send(envelope::session(&self.resume_id, false));
            return;
        }
        if self
            .user_socket
            .as_ref()
            .is_some_and(crate::user_drain::Socket::should_close)
        {
            self.send(envelope::session(&self.resume_id, false));
            return;
        }
        let mut resumed = false;
        if let (Some(from), Some(user), Some(token)) = (
            self.identity.resume_from.clone(),
            self.identity.user.clone(),
            self.identity.token.clone(),
        ) {
            let outputs = Arc::new(Mutex::new(HashMap::<String, Out>::new()));
            let (routes, socket_out) = (outputs.clone(), self.out.clone());
            let outbox = move |name: &str| {
                routes
                    .lock()
                    .unwrap()
                    .entry(name.to_owned())
                    .or_insert_with(|| socket_out.account(name).awaiting_instance_id())
                    .outbox()
            };
            let mut accounts =
                self.registry
                    .resume(&from, &user, self.socket, token.clone(), &outbox);
            // Not parked yet: the old socket dropped without a close frame. Close it now.
            if accounts.is_empty()
                && !self.token_expired()
                && let Some(parked) = self.registry.take_over(&from, &user)
            {
                let _ = tokio::time::timeout(TAKEOVER_WAIT, parked).await;
                if self.admission_token_valid() {
                    accounts = self
                        .registry
                        .resume(&from, &user, self.socket, token, &outbox);
                }
            }
            resumed = !accounts.is_empty();
            // `init` is logged either way; this tells a resume from a rebuild.
            Log::info(&format!(
                "resume {}: {} account(s)",
                self.socket,
                accounts.len()
            ));
            // The client re-inits each one; an identical init then keeps it.
            for account in accounts {
                let labels = account.labels(self.socket);
                let labels = if labels.is_empty() {
                    vec![account.name.clone()]
                } else {
                    labels
                };
                for label in labels {
                    let out = outputs
                        .lock()
                        .unwrap()
                        .entry(label.clone())
                        .or_insert_with(|| self.out.account(&label).awaiting_instance_id())
                        .clone();
                    let out = if label == account.name {
                        out
                    } else {
                        out.with_account_label(&label)
                    };
                    self.join(account.clone(), out).await;
                }
            }
        }
        if let Some(user) = &self.identity.user {
            self.taken_over = self.registry.open_session(&self.resume_id, user);
        }
        self.send(envelope::session(&self.resume_id, resumed));
    }

    pub async fn handle(&mut self, text: &str) -> Result<(), Close> {
        if self.token_expired() {
            // Refusal does not shorten TOKEN_GRACE. Init replies keep the
            // request's registration without changing the current holder.
            let name = envelope::account_name(text);
            let out = match serde_json::from_str::<Inbound>(text) {
                Ok(Inbound::Init(init)) => self.out.with_instance_id(init.instance_id.as_deref()),
                Ok(Inbound::Msg(AccountMsg {
                    op,
                    account_name: Some(name),
                    data,
                })) if op == "callMethod" => {
                    self.reply_failure(
                        &name,
                        &data.get("requestId").cloned().unwrap_or(Value::Null),
                        "AuthExpired",
                        crate::auth::TOKEN_EXPIRED,
                        Some("notSent"),
                    );
                    return Ok(());
                }
                _ => name
                    .as_deref()
                    .map(|name| self.account_out(name))
                    .unwrap_or_else(|| self.out.clone()),
            };
            out.send(envelope::error(name.as_deref(), crate::auth::TOKEN_EXPIRED));
            return Ok(());
        }
        let Ok(message) = serde_json::from_str::<Inbound>(text) else {
            if let Ok(frame) = serde_json::from_str::<Value>(text)
                && frame["t"]
                    .as_str()
                    .is_some_and(|kind| kind.starts_with("order."))
            {
                if self.identity.user.is_none() {
                    return Err(Close::Policy("Unauthenticated order rejected"));
                }
                let name = frame["d"]["accountName"].as_str();
                let out = name
                    .map(|name| self.handover_out(name))
                    .unwrap_or_else(|| self.out.clone());
                out.send(envelope::error(name, "not sent: invalid order frame"));
            }
            Log::warn("invalid envelope");
            return Ok(());
        };
        let authenticated = self.identity.user.is_some();
        match message {
            Inbound::OrderAction(request) => {
                if !authenticated {
                    return Err(Close::Policy("Unauthenticated order rejected"));
                }
                if text.len() > crate::order_contract::SUBMIT_MAX {
                    self.handover_out(&request.account_name)
                        .send(envelope::error(
                            Some(&request.account_name),
                            "not sent: action exceeds size limit",
                        ));
                    return Ok(());
                }
                self.order_action(request, text).await;
            }
            Inbound::OrderActionCancelQueued(cancel) => {
                if !authenticated {
                    return Err(Close::Policy("Unauthenticated order rejected"));
                }
                if text.len() > crate::order_contract::ACK_MAX {
                    self.handover_out(&cancel.account_name)
                        .send(envelope::error(
                            Some(&cancel.account_name),
                            "not sent: action queue cancellation exceeds size limit",
                        ));
                    return Ok(());
                }
                self.action_cancel_queued(cancel);
            }
            Inbound::OrderSubmit(submit) => {
                if !authenticated {
                    return Err(Close::Policy("Unauthenticated order rejected"));
                }
                if text.len() > crate::order_contract::SUBMIT_MAX {
                    self.handover_out(&submit.account_name)
                        .send(envelope::error(
                            Some(&submit.account_name),
                            "not sent: submit exceeds size limit",
                        ));
                    return Ok(());
                }
                self.order_submit(submit).await;
            }
            Inbound::OrderCancelQueued(cancel) => {
                if !authenticated {
                    return Err(Close::Policy("Unauthenticated order rejected"));
                }
                self.order_cancel_queued(cancel);
            }
            Inbound::Init(init) => self.init(*init).await?,
            Inbound::ApplySettings(request) => {
                if !authenticated {
                    return Err(Close::Policy("Unauthenticated settings action rejected"));
                }
                self.reset_settings(request);
            }
            Inbound::Msg(msg) => {
                if !authenticated
                    && !(msg.account_name.as_deref() == Some(PUBLIC_ACCOUNT)
                        && PUBLIC_OPS.contains(&msg.op.as_str()))
                {
                    return Err(Close::Policy(
                        "Unauthenticated exchange-server message rejected",
                    ));
                }
                self.msg(msg).await;
            }
            Inbound::SignRes(response) => {
                if !authenticated {
                    return Err(Close::Policy(
                        "Unauthenticated exchange-server sessions cannot sign",
                    ));
                }
                if self.pending_verifications.values().any(|pending| {
                    pending
                        .cosigned_link
                        .as_ref()
                        .is_some_and(|link| link.resolve(self.socket, response.clone()))
                }) {
                    return Ok(());
                }
                // Only links this socket owns can match: another socket's ids never resolve here.
                for joined in self.joined.values() {
                    if let Some(link) = &joined.account.link
                        && link.resolve(self.socket, response.clone())
                    {
                        break;
                    }
                }
            }
            Inbound::Dispose { account_name } => {
                if self.pending_verifications.contains_key(&account_name) {
                    self.drop_early(&account_name, "account disposed during verification");
                }
                self.cancel_verification(&account_name);
                self.handovers.forget(&account_name);
                Log::info(&format!(
                    "dispose request {account_name} socket {} session {}",
                    self.socket,
                    self.resume_id_prefix()
                ));
                self.registration_outputs.remove(&account_name);
                self.out.retire_account(&account_name);
                if let Some(joined) = self.joined.remove(&account_name) {
                    self.metrics.note_dispose();
                    self.leave(joined, Disposal::PageDisposed);
                }
            }
            Inbound::EchoRes(response) => {
                for joined in self.joined.values() {
                    if let Some(link) = &joined.account.link
                        && link.echo_res(&response).is_some()
                    {
                        break;
                    }
                }
            }
            Inbound::Ping(ping) => {
                self.forget_disposed();
                let names: Vec<String> = self.joined.keys().cloned().collect();
                self.send(envelope::pong(&names));
                let keep = ping.unwrap_or_default().accounts;
                if !keep.is_empty() {
                    let orphaned: Vec<String> = names
                        .into_iter()
                        .filter(|name| !keep.contains(name))
                        .collect();
                    for name in orphaned {
                        if let Some(joined) = self.joined.remove(&name) {
                            self.leave(joined, Disposal::Orphaned);
                        }
                    }
                }
            }
            Inbound::Perf(_) => {}
            Inbound::Verbose(_) => {
                if !authenticated {
                    return Err(Close::Policy(
                        "Unauthenticated exchange-server sessions cannot enable diagnostics",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn init(&mut self, init: InitPayload) -> Result<(), Close> {
        let out = self.out.with_instance_id(init.instance_id.as_deref());
        let out = if crate::accounts::identity::enabled(&init) {
            out.with_account_label(&init.account_name)
        } else {
            out
        };
        if self.token_expired() {
            out.send(envelope::error(
                Some(&init.account_name),
                crate::auth::TOKEN_EXPIRED,
            ));
            return Ok(());
        }
        let authenticated = self.identity.user.is_some();
        let public_only = init.public_only == Some(true) || init.account_name.starts_with("__pub_");
        if !authenticated && let Some(reason) = unauthenticated_init_refusal(&init) {
            out.account(&init.account_name)
                .send(envelope::error(Some(&init.account_name), reason));
            return Err(Close::Policy(reason));
        }
        if self.moved_by_drain() {
            Log::info(&format!(
                "init {} on socket {} moved: this box is draining",
                init.account_name, self.socket
            ));
            self.moved = true;
            return Err(Close::Moved);
        }
        self.registration_outputs
            .insert(init.account_name.clone(), out.clone());
        Log::info(&format!(
            "init {} ({}) socket {} session {}",
            init.account_name,
            init.exchange_name,
            self.socket,
            self.resume_id_prefix()
        ));
        self.cancel_verification(&init.account_name);
        // Refuse before routing can erase the classification or a rebuild can
        // dispose a live account with the same saved name.
        if safe_cex_venues::ConstructionContext::Hosted
            .validate_venue(&init.exchange_name)
            .is_err()
        {
            self.handovers.forget(&init.account_name);
            self.drop_early(&init.account_name, "unsupported venue");
            out.account(&init.account_name)
                .send(envelope::init_unsupported(
                    &init.account_name,
                    &init.exchange_name,
                ));
            return Ok(());
        }
        if init.signing_mode.as_deref() == Some("delegated") {
            self.admission_init(init);
            return Ok(());
        }
        self.handovers.forget(&init.account_name);
        if init.sealed.is_some() {
            self.drop_early(&init.account_name, "sealed init requires delegated signing");
            self.refuse_handover(&init.account_name, false);
            return Ok(());
        }
        self.forget_disposed();
        if let Some(joined) = self.joined.get(&init.account_name)
            && joined.account.delegated.is_some()
            && (public_only || joined.account.matches_init(self.user(), &init))
        {
            out.account(&init.account_name).send(envelope::init_refused(
                &init.account_name,
                "Account signing mode differs; account not attached",
            ));
            return Ok(());
        }
        let admission_enabled = self.registry.private_join_requires_proof();
        if !public_only && admission_enabled {
            if !self.admission_token_valid() {
                self.drop_early(&init.account_name, "verification refused");
                self.refuse_handover(&init.account_name, false);
                return Ok(());
            }
            if self.joined.get(&init.account_name).is_none_or(|joined| {
                joined.account.fingerprint != init.fingerprint()
                    || !joined.account.matches_init(self.user(), &init)
            }) {
                self.admission_init(init);
                return Ok(());
            }
        }
        if let Some(refusal) =
            self.registry
                .signing_mode_refusal(self.user(), &init, public_only, false)
        {
            out.account(&init.account_name)
                .send(envelope::init_refused(&init.account_name, &refusal));
            self.drop_early(&init.account_name, "init refused for signing mode");
            return Ok(());
        }
        if let Some(joined) = self.joined.get(&init.account_name)
            && joined.account.fingerprint == init.fingerprint()
            && joined.account.matches_init(self.user(), &init)
        {
            // Resumed (or repeated) with what it was built from: a fresh snapshot only.
            let out = out.account(&init.account_name);
            let joined = self.joined.get_mut(&init.account_name).unwrap();
            let previous = std::mem::replace(&mut joined.out, out.clone());
            previous.retire_order_receipts();
            let account = joined.account.clone();
            if account.link.is_some() {
                account.hold_registration(
                    self.socket,
                    out.outbox(),
                    self.identity.token.clone(),
                    &init.account_name,
                );
            } else {
                account.update_reply_outbox(self.socket, out.outbox(), &init.account_name);
            }
            previous.finish_registration(&out);
            self.resnapshot(&init.account_name).await;
            return Ok(());
        }
        // Existing private settings stay until an admitted client explicitly resets them.
        if let Some(refusal) =
            self.registry
                .rebuild_refusal(self.user(), &init, public_only, self.socket)
        {
            if first_refusal(
                &mut self.refused,
                &init.account_name,
                &refusal,
                init.instance_id.as_deref(),
            ) {
                out.account(&init.account_name)
                    .send(envelope::init_refused(&init.account_name, &refusal));
            }
            self.drop_early(&init.account_name, "init refused for other settings");
            return Ok(());
        }
        self.refused.remove(&init.account_name);
        if let Some(joined) = self.joined.remove(&init.account_name) {
            joined.out.retire_registration();
            // Stop callbacks keep the retired output, never the replacement's registration
            // or generation. Other holders retain their own output authority.
            let disposal = Disposal::rebuild(joined.account.is_public(), public_only);
            if joined.account.matches_init(self.user(), &init)
                && joined.account.live_holders_besides(self.socket).is_empty()
            {
                self.registry
                    .stop_for_new_settings(&joined.account, joined.out.outbox(), disposal);
                Self::leave_engine(joined);
            } else {
                self.leave(joined, disposal);
            }
        }
        self.out.retire_account(&init.account_name);
        let out = out.account(&init.account_name);
        let token = self.identity.token.clone();
        let account = self
            .registry
            .obtain(
                self.user(),
                &init,
                public_only,
                self.socket,
                token,
                out.outbox(),
            )
            .await;
        match account {
            Ok(account) => {
                self.metrics.note_init();
                let private = !account.is_public();
                self.join(account, out.clone()).await;
                if private && self.joined.contains_key(&init.account_name) {
                    self.registry
                        .interruptions
                        .deliver(self.user(), &init.account_name, &out);
                }
                // Algos stopped while no browser was attached: say why.
                // Chases that ended while it was gone are told as their replayed ends.
                if self.joined.contains_key(&init.account_name)
                    && let Some(kept) = self.registry.take_notice(
                        self.user(),
                        self.joined[&init.account_name].account.registry_name(),
                        &self.joined[&init.account_name].account.fingerprint,
                        self.identity.resume_from.as_deref(),
                    )
                {
                    Log::info(&format!(
                        "algo stop notice {} to socket {}: {} ({} chase ends)",
                        init.account_name,
                        self.socket,
                        kept.summary(),
                        kept.ends.len()
                    ));
                    for frame in kept.frames(&init.account_name) {
                        out.send(frame);
                    }
                }
            }
            Err(error) => {
                Log::warn(&format!("init {} refused: {error}", init.account_name));
                self.drop_early(&init.account_name, "init refused");
                out.send(envelope::error(Some(&init.account_name), &error));
            }
        }
        Ok(())
    }

    async fn join(&mut self, account: Arc<Account>, out: Out) {
        let name = out.account_label(&account.name).to_owned();
        match self.client(&account, out.clone()).await {
            Ok(client) => {
                if let Some(algos) = &account.algos {
                    let _ = algos.send(crate::algos::AlgoInput::Joined);
                }
                self.joined.insert(
                    name.clone(),
                    Joined {
                        account,
                        out,
                        client,
                        old: Vec::new(),
                        next_command: 0,
                        subscriptions: HashMap::new(),
                    },
                );
                self.apply_early(&name);
            }
            Err(error) => {
                self.drop_early(&name, "account did not join");
                self.account_stop_error(&account, error, &out);
                self.registry
                    .release_label(&account, self.socket, &name, Disposal::JoinFailed);
            }
        }
    }

    async fn client(&self, account: &Arc<Account>, out: Out) -> Result<Client, &'static str> {
        crate::auth::Verifier::check_expiry(self.token_expires_at_ms(), now_ms())?;
        let id = self.registry.next_id();
        let link = account.handle.join(id, true).await?;
        if self.token_expired() {
            account.handle.leave(id);
            return Err(crate::auth::TOKEN_EXPIRED);
        }
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let draining = Arc::new(AtomicBool::new(false));
        let hold = (!account.is_public()).then(|| Hold {
            client: id,
            loaded: self.loaded.clone(),
        });
        let pump = tokio::spawn(pump(
            link,
            id,
            self.dropped.clone(),
            account.handle.clone(),
            out.account_label(&account.name).to_owned(),
            out,
            pending.clone(),
            draining.clone(),
            hold,
        ));
        Ok(Client {
            id,
            pump,
            pending,
            draining,
        })
    }

    /// A new client on the same account takes a full snapshot (seq 1); the old
    /// one stops forwarding frames and leaves once its replies are in. False when
    /// no new client could join.
    async fn resnapshot(&mut self, name: &str) -> bool {
        if self.token_expired() {
            return false;
        }
        let Some((account, out)) = self
            .joined
            .get(name)
            .map(|joined| (joined.account.clone(), joined.out.clone()))
        else {
            return false;
        };
        match self.client(&account, out.clone()).await {
            Ok(client) => {
                let joined = self.joined.get_mut(name).unwrap();
                // Before the old client leaves, so the venue never unsubscribes in between.
                for command in joined.subscriptions.keys() {
                    if !matches!(account.handle.input(client.id, command), Ok(true)) {
                        Log::warn(&format!(
                            "{name}: subscription not carried to the re-snapshot"
                        ));
                    }
                }
                let old = std::mem::replace(&mut joined.client, client);
                old.draining.store(true, Ordering::Relaxed);
                joined.old.retain(|client| !client.pump.is_finished());
                if old.pending.lock().unwrap().is_empty() {
                    old.pump.abort();
                    account.handle.leave(old.id);
                } else {
                    joined.old.push(old);
                }
                true
            }
            Err(error) => {
                self.account_stop_error(&account, error, &out);
                false
            }
        }
    }

    fn account_stop_error(&self, account: &Account, error: &str, out: &Out) {
        // A latch alone is not evidence: an unrelated actor crash must still report.
        let frame = if error == "account stopped"
            && account.moving()
            && self.registry.planned_move(&account.user)
        {
            envelope::moving(&account.name, error)
        } else {
            envelope::error(Some(&account.name), error)
        };
        out.send(frame);
    }

    fn leave(&self, joined: Joined, disposal: Disposal) {
        let account = joined.account.clone();
        let label = joined.out.account_label(&account.name).to_owned();
        self.out.retire_account(&label);
        Self::leave_engine(joined);
        self.registry
            .release_label(&account, self.socket, &label, disposal);
    }

    fn leave_engine(joined: Joined) {
        joined.out.retire_order_receipts();
        for client in std::iter::once(joined.client).chain(joined.old) {
            client.pump.abort();
            joined.account.handle.leave(client.id);
        }
    }

    /// Accounts disposed under this socket (a failing signer) are no longer
    /// joined: the pong then omits them, which is what the page re-inits on.
    fn forget_disposed(&mut self) {
        let socket = self.socket;
        let registry = self.registry.clone();
        let gone: Vec<String> = self
            .joined
            .iter()
            .filter(|(_, joined)| !registry.holds(&joined.account, socket))
            .map(|(name, _)| name.clone())
            .collect();
        for name in gone {
            if let Some(joined) = self.joined.remove(&name) {
                Self::leave_engine(joined);
            }
        }
    }

    async fn msg(&mut self, msg: AccountMsg) {
        let Some(name) = msg.account_name.clone() else {
            return;
        };
        if msg.op == "delegated-key-req" {
            self.cancel_verification(&name);
            if let Some(venue) = self
                .handover_venue(&name, &msg.data)
                .filter(|venue| crate::delegated::enabled(venue))
            {
                self.issue_handover(&name, &venue);
            } else {
                self.handovers.forget(&name);
                self.drop_early(&name, "delegated init refused");
                self.refuse_handover(&name, false);
            }
            return;
        }
        if msg.op == "applySettings"
            && (self
                .joined
                .get(&name)
                .is_none_or(|joined| !self.registry.holds(&joined.account, self.socket))
                || self.pending_verifications.contains_key(&name))
        {
            self.handover_out(&name).send(envelope::msg(
                &name,
                "applySettings:done",
                json!({
                    "requestId":msg.data.get("requestId").cloned().unwrap_or(Value::Null),
                    "success":false,
                    "result":{"message":"Account is not admitted; settings were not applied"}
                }),
            ));
            return;
        }
        if let Some(pending) = self.pending_verifications.get(&name) {
            let out = pending.out.clone();
            if msg.op == "callMethod" {
                out.send(envelope::msg(
                    &name,
                    "callMethod:done",
                    json!({
                        "requestId": msg.data.get("requestId").cloned().unwrap_or(Value::Null),
                        "success": false,
                        "result": {"name":"Error", "message":"verifying", "outcome":"notSent"},
                    }),
                ));
            } else {
                out.send(envelope::error(Some(&name), "verifying"));
            }
            return;
        }
        self.forget_disposed();
        let topic = Self::topic_of(&msg);
        if !self.joined.contains_key(&name)
            && let Some(topic) = topic.clone()
        {
            self.queue_early(&name, topic);
            return;
        }
        // It only acks, and the page waits on it: answer it joined or not.
        if msg.op == "updatePositions" && !self.joined.contains_key(&name) {
            self.send(envelope::msg(
                &name,
                "updatePositions:done",
                json!({"requestId": msg.data.get("requestId"), "success": true}),
            ));
            return;
        }
        if !self.joined.contains_key(&name) {
            Log::warn(&format!("no joined account for op {}", msg.op));
            if msg.op == "callMethod" {
                self.reply_failure(
                    &name,
                    &msg.data.get("requestId").cloned().unwrap_or(Value::Null),
                    "Error",
                    "this account stopped and is restarting; reload if it does not return",
                    Some("notSent"),
                );
            } else if ["chaser:", "twap:"]
                .iter()
                .any(|prefix| msg.op.starts_with(prefix))
            {
                self.send(envelope::error(
                    Some(&name),
                    &format!(
                        "{} was not run: this account is not connected yet. Try again once it has loaded",
                        msg.op
                    ),
                ));
            }
            return;
        }
        let symbol = msg
            .data
            .get("symbol")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let interval = msg
            .data
            .get("interval")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match (msg.op.as_str(), symbol, interval) {
            ("sendSnapshot", _, _) => {
                self.resnapshot(&name).await;
            }
            _ if topic.is_some() => self.apply_topic(&name, topic.as_ref().unwrap()),
            ("callMethod", _, _) => self.call(&name, msg.data).await,
            ("applySettings", _, _) => self.apply_settings(&name, msg.data).await,
            ("updatePositions", _, _) => self.send(envelope::msg(
                &name,
                "updatePositions:done",
                json!({"requestId": msg.data.get("requestId"), "success": true}),
            )),
            (op, _, _)
                if self.joined[&name].account.delegated.is_some()
                    && (matches!(op, "exec" | "chaser:start" | "twap:start" | "twap:resume")
                        || op == "twap:state"
                            && msg.data.as_array().is_some_and(|rows| {
                                rows.iter().any(|row| {
                                    serde_json::from_value::<PersistedTwap>(row.clone()).is_ok_and(
                                        |entry| entry.state.status == TwapStatus::Running,
                                    )
                                })
                            })) =>
            {
                let message = format!(
                    "DelegatedActionUnavailable: DELEGATED {op} is unavailable; only place and reduce-only close are enabled"
                );
                if op == "exec" && msg.data.get("reqId").is_some() {
                    self.send(envelope::msg(
                        &name,
                        "exec-result",
                        json!({
                            "reqId": msg.data["reqId"], "handled": true, "success": false,
                            "error": message,
                        }),
                    ));
                } else {
                    self.send(envelope::error(Some(&name), &message));
                }
            }
            (op, _, _) if crate::algos::OPS.contains(&op) => {
                let account = &self.joined[&name].account;
                let mut data = msg.data;
                if op == "twap:start" && account.registry_name() != account.name {
                    if data["accountName"] != name {
                        self.account_out(&name).send(envelope::error(
                            Some(&name),
                            "TWAP must name this account label",
                        ));
                        return;
                    }
                    data["accountName"] = account.name.clone().into();
                }
                let algos = account.algos.clone();
                let sent = algos.is_some_and(|algos| {
                    algos
                        .send(crate::algos::AlgoInput::Op {
                            op: op.to_owned(),
                            data: data.to_string(),
                            origin: format!("socket:{}", self.socket),
                            twap_owner: self.joined[&name].account.twap_owner.snapshot(),
                            action: None,
                        })
                        .is_ok()
                });
                if !sent {
                    self.send(envelope::msg(
                        &name,
                        "error",
                        json!([format!("{op} is unavailable for this account")]),
                    ));
                }
            }
            (op, _, _) => Log::warn(&format!("ignored account op {op}")),
        }
    }

    // Capture this registration before the rebuild can yield to another owner.
    pub(crate) fn settings_change(
        &self,
        init: InitPayload,
        account: Arc<Account>,
    ) -> impl std::future::Future<Output = Result<Arc<Account>, String>> + use<> {
        let registry = self.registry.clone();
        let identity = self.identity.clone();
        let socket = self.socket;
        let outbox = self.account_out(&init.account_name).outbox();
        async move {
            let user = identity.user.ok_or("Authenticated user required")?;
            registry
                .apply_settings(&user, &init, account, socket, identity.token, outbox)
                .await
        }
    }

    async fn apply_settings(&mut self, name: &str, data: Value) {
        let out = self.account_out(name);
        let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
        let result = match serde_json::from_value::<InitPayload>(data["settings"].clone()) {
            Ok(init) if init.account_name == name => {
                let account = self.joined[name].account.clone();
                self.settings_change(init, account).await
            }
            _ => Err("Apply settings requires settings for this account label".into()),
        };
        let result = match result {
            Ok(account) => {
                if let Some(previous) = self.joined.remove(name) {
                    Self::leave_engine(previous);
                }
                // Keep captured provenance, but never reuse the retired engine's receipts.
                let replacement = out.with_fresh_order_receipts();
                if let Some(fallback) = self.registration_outputs.get_mut(name)
                    && fallback.shares_order_receipts(&out)
                {
                    *fallback = replacement.clone();
                }
                account.update_reply_outbox(self.socket, replacement.outbox(), name);
                self.join(account, replacement).await;
                if self.joined.contains_key(name) {
                    Ok(())
                } else {
                    Err("The account could not load the applied settings".to_owned())
                }
            }
            Err(error) => Err(error),
        };
        let reply = match result {
            Ok(()) => json!({"requestId":request_id,"success":true,"result":null}),
            Err(message) => {
                json!({"requestId":request_id,"success":false,"result":{"message":message}})
            }
        };
        out.send(envelope::msg(name, "applySettings:done", reply));
    }

    /// An unjoined account must name its venue; an existing join supplies its own.
    fn handover_venue(&self, name: &str, data: &Value) -> Option<String> {
        if let Some(venue) = data.get("exchangeName") {
            return venue.as_str().map(str::to_owned);
        }
        if let Some(joined) = self.joined.get(name) {
            return Some(joined.account.venue.clone());
        }
        None
    }

    fn issue_handover(&mut self, name: &str, venue: &str) {
        match self.handovers.issue(name, venue, crate::accounts::now_ms()) {
            Ok(offer) => {
                let out = self.handover_out(name);
                if out.send(envelope::delegated_key(name, &offer))
                    && !self.joined.contains_key(name)
                {
                    self.registry.interruptions.delegated_status(
                        self.user(),
                        name,
                        &out,
                        crate::interruption::DelegatedKeyStatus::WaitingForKey,
                    );
                }
            }
            Err(_) => {
                self.drop_early(name, "delegated init refused");
                self.refuse_handover(name, false);
            }
        }
    }

    /// Counts a book/candle (un)subscribe; only the first subscribe and the last
    /// unsubscribe reach the engine, whose per-client demand is a set.
    fn topic_of(msg: &AccountMsg) -> Option<Topic> {
        let field = |key: &str| msg.data.get(key).and_then(Value::as_str).map(str::to_owned);
        let symbol = field("symbol")?;
        let interval = field("interval");
        match msg.op.as_str() {
            "subscribeOrderBook" | "unsubscribeOrderBook" => {}
            "subscribeOHLCV" | "unsubscribeOHLCV" if interval.is_some() => {}
            _ => return None,
        }
        Some(Topic {
            reset: msg.op.starts_with("subscribe")
                && msg.data.get("reset").and_then(Value::as_bool) == Some(true),
            op: msg.op.clone(),
            symbol,
            interval,
        })
    }

    fn topic_line(&self, verb: &str, name: &str, topic: &Topic) -> String {
        format!(
            "{verb}: {} account {name} symbol {} interval {} socket {}",
            topic.op,
            topic.symbol,
            topic.interval.as_deref().unwrap_or("-"),
            self.socket
        )
    }

    fn queue_early(&mut self, name: &str, topic: Topic) {
        Log::info(&self.topic_line("subscription queued until join", name, &topic));
        let queue = self.early.entry(name.to_owned()).or_default();
        queue.push(topic);
        if queue.len() > EARLY_TOPICS {
            let oldest = queue.remove(0);
            Log::warn(&self.topic_line("subscription dropped: early queue full", name, &oldest));
        }
    }

    fn apply_early(&mut self, name: &str) {
        for topic in self.early.remove(name).unwrap_or_default() {
            Log::info(&self.topic_line("subscription applied at join", name, &topic));
            self.apply_topic(name, &topic);
        }
    }

    fn drop_early(&mut self, name: &str, reason: &str) {
        for topic in self.early.remove(name).unwrap_or_default() {
            Log::warn(&self.topic_line(&format!("subscription dropped: {reason}"), name, &topic));
        }
    }

    fn apply_topic(&mut self, name: &str, topic: &Topic) {
        let symbol = &topic.symbol;
        let add = topic.op.starts_with("subscribe");
        let (subscribe, unsubscribe, repair) = match topic.interval.as_deref() {
            Some(interval) if topic.op.ends_with("OHLCV") => (
                json!({"op": "subscribeCandles", "symbol": symbol, "interval": interval}),
                json!({"op": "unsubscribeCandles", "symbol": symbol, "interval": interval}),
                json!({"kind": "candles", "symbol": symbol, "interval": interval}),
            ),
            _ => (
                json!({"op": "subscribeOrderBook", "symbol": symbol}),
                json!({"op": "unsubscribeOrderBook", "symbol": symbol}),
                json!({"kind": "orderBook", "symbol": symbol}),
            ),
        };
        let key = subscribe.to_string();
        self.subscribe(name, subscribe, unsubscribe, add);
        if topic.reset {
            self.repair(name, topic, &key, repair);
        }
    }

    /// Re-sends the venue's subscribe for a topic this socket holds, rate limited per
    /// account and topic: refcounts above the venue absorb a chart's own resubscribe.
    fn repair(&mut self, name: &str, topic: &Topic, key: &str, repair: Value) {
        let joined = &self.joined[name];
        if !joined.subscriptions.contains_key(key) {
            return;
        }
        let account = &joined.account;
        if !self.registry.allow_repair(
            &account.user,
            account.registry_name(),
            key,
            std::time::Instant::now(),
        ) {
            Log::info(&self.topic_line(
                "subscription repair skipped: repaired recently",
                name,
                topic,
            ));
            return;
        }
        Log::info(&self.topic_line("subscription repair requested", name, topic));
        self.input(name, json!({"op": "repairSubscription", "topic": repair}));
    }

    fn subscribe(&mut self, name: &str, subscribe: Value, unsubscribe: Value, add: bool) {
        let key = subscribe.to_string();
        let held = self.joined[name]
            .subscriptions
            .get(&key)
            .copied()
            .unwrap_or(0);
        let next = match (add, held) {
            // An unsubscribe this socket never subscribed must not release another socket's demand.
            (false, 0) => return,
            (true, 0) | (false, 1) => {
                if !self.input(name, if add { subscribe } else { unsubscribe }) {
                    return;
                }
                u32::from(add)
            }
            (true, held) => (held + 1).min(MAX_SUBSCRIBERS_PER_KEY),
            (false, held) => held - 1,
        };
        let subscriptions = &mut self.joined.get_mut(name).unwrap().subscriptions;
        if next == 0 {
            subscriptions.remove(&key);
        } else {
            subscriptions.insert(key, next);
        }
    }

    fn input(&self, name: &str, command: Value) -> bool {
        if self.token_expired() {
            self.send(envelope::error(Some(name), crate::auth::TOKEN_EXPIRED));
            return false;
        }
        let joined = &self.joined[name];
        match joined
            .account
            .handle
            .input(joined.client.id, &command.to_string())
        {
            Ok(true) => true,
            Ok(false) => {
                self.send(envelope::error(
                    Some(name),
                    "account is busy; subscription not sent",
                ));
                false
            }
            Err(refused) => {
                self.send(envelope::error(Some(name), &refused.notice));
                false
            }
        }
    }

    async fn call(&mut self, name: &str, data: Value) {
        self.call_inner(name, data, None).await;
    }

    async fn call_inner(&mut self, name: &str, data: Value, order_frame: Option<OrderReceipt>) {
        let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
        let method = data
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let args = match data.get("args") {
            Some(Value::Array(args)) => args.clone(),
            None | Some(Value::Null) => Vec::new(),
            Some(_) => {
                self.call_failure(
                    name,
                    &request_id,
                    "TypeError",
                    "args must be an array",
                    None,
                    order_frame,
                );
                return;
            }
        };
        let account = self.joined[name].account.clone();
        let dispatch_out = self.account_out(name);
        let action_expires_at_ms = self.token_expires_at_ms();
        let symbols = if method == "cancelAllOrders"
            && (account.delegated.is_none()
                || matches!(order_frame, Some(OrderReceipt::Action { .. })))
        {
            let mut symbols: Vec<String> = account
                .handle
                .read(|store| {
                    store
                        .orders()
                        .iter()
                        .map(|order| order.symbol.clone())
                        .collect()
                })
                .await
                .unwrap_or_default();
            // Frames can lag a placement's ACK, on every per-symbol cancel mode.
            if account.handle.capabilities().cancel_all != CancelAll::Native {
                symbols.extend(account.placed.symbols());
            }
            if matches!(order_frame, Some(OrderReceipt::Action { .. }))
                && let Some(Value::Array(extra)) = args.first()
            {
                symbols.extend(extra.iter().filter_map(Value::as_str).map(str::to_owned));
            }
            symbols
        } else {
            Vec::new()
        };
        if let Some(OrderReceipt::Action {
            seq,
            kind,
            deadline,
        }) = order_frame
            && let Err(message) = crate::order_actions::ActionGuard::new(
                dispatch_out.clone(),
                deadline,
                action_expires_at_ms,
            )
            .check()
        {
            crate::order_actions::settle(
                &dispatch_out,
                name,
                &request_id,
                seq,
                kind,
                Err(json!({"message":message,"outcome":"unknown"})),
            );
            return;
        }
        // Dedicated action frames retain their full-write admission/ledger contract.
        // Legacy calls retain the narrower place/close activation scope.
        let plans = if matches!(order_frame, Some(OrderReceipt::Action { .. }))
            && method == "setAllLeverage"
            && !account.handle.capabilities().account_leverage
        {
            let leverage = args[0].as_f64().unwrap();
            account
                .handle
                .read(move |store| crate::full_writes::all_leverage(store, leverage))
                .await
                .unwrap_or_else(|| {
                    Err(methods::Refused {
                        name: "Error",
                        message: "Account closed before leverage dispatch".into(),
                    })
                })
        } else if account.delegated.is_some() {
            let translate = if matches!(order_frame, Some(OrderReceipt::Action { .. })) {
                methods::translate_delegated_action
            } else {
                methods::translate_delegated
            };
            translate(
                &account.venue,
                &method,
                &args,
                account.handle.capabilities(),
                &symbols,
                account.is_public(),
            )
        } else {
            methods::translate(
                &method,
                &args,
                account.handle.capabilities(),
                &symbols,
                account.is_public(),
            )
        };
        let mut plans = match plans {
            Ok(plans) => plans,
            Err(refused) => {
                if crate::audit::ORDER_METHODS.contains(&method.as_str()) {
                    let who = audit_who(&account);
                    let origin = format!("socket:{}", self.socket);
                    let fields = crate::audit::refused_call(&method, &args);
                    let detail = json!({"name": refused.name, "message": refused.message});
                    crate::audit::log(&who, &origin, &fields, crate::audit::result(&Err(detail)));
                }
                self.call_failure(
                    name,
                    &request_id,
                    refused.name,
                    &refused.message,
                    None,
                    order_frame,
                );
                return;
            }
        };
        if let Some(OrderReceipt::Action {
            seq,
            kind,
            deadline,
        }) = order_frame
            && let Err(message) = crate::order_actions::ActionGuard::new(
                dispatch_out.clone(),
                deadline,
                action_expires_at_ms,
            )
            .check()
        {
            crate::order_actions::settle(
                &dispatch_out,
                name,
                &request_id,
                seq,
                kind,
                Err(json!({"message":message,"outcome":"unknown"})),
            );
            return;
        }
        if let Some(receipt) = order_frame {
            // Reply maps completion only. Every command dispatch below carries
            // the original receipt, including its immutable deadline.
            for plan in &mut plans {
                if let Plan::Command { reply, command } | Plan::AfterLoad { reply, command } = plan
                {
                    *reply = match receipt {
                        OrderReceipt::Placement(seq) => methods::Reply::Intent {
                            client_order_id: request_id.as_str().unwrap_or_default().to_owned(),
                            seq,
                        },
                        OrderReceipt::Action { seq, kind, .. } => methods::Reply::Action {
                            seq,
                            kind,
                            unit: matches!(reply, methods::Reply::Unit),
                            idless_protection: kind == "place"
                                && crate::full_writes::protection_only(command),
                        },
                    };
                }
            }
        }
        let group = (plans.len() > 1).then(|| {
            Arc::new(Mutex::new(Group {
                remaining: plans.len(),
                failures: Vec::new(),
            }))
        });
        for plan in plans {
            match plan {
                Plan::Reply(result) => {
                    if let Some(OrderReceipt::Action { seq, kind, .. }) = order_frame {
                        crate::order_actions::settle(
                            &self.account_out(name),
                            name,
                            &request_id,
                            seq,
                            kind,
                            Ok(result),
                        );
                    } else {
                        self.send(envelope::msg(
                            name,
                            "callMethod:done",
                            json!({"requestId":request_id,"success":true,"result":result}),
                        ));
                    }
                }
                Plan::Nuke(mode) => {
                    if let Some(receipt @ OrderReceipt::Action { .. }) = order_frame {
                        self.send_action_nuke(name, &request_id, receipt, mode);
                        continue;
                    }
                    let handle = account.handle.clone();
                    // Asked of the whole account, so a page running its own legs after
                    // `algos` is told too: its positions read then waits the same 30s.
                    let waiting =
                        !methods::until_nuke_can_see(&handle, "all", std::time::Duration::ZERO)
                            .await
                            .is_ok();
                    if self.token_expired() {
                        self.reply_failure(
                            name,
                            &request_id,
                            "AuthExpired",
                            crate::auth::TOKEN_EXPIRED,
                            Some("notSent"),
                        );
                        continue;
                    }
                    if waiting {
                        self.send(envelope::algo_error(
                            name,
                            Value::String(NUKE_WAITING.to_owned()),
                        ));
                    }
                    let Some(algos) = account.algos.clone() else {
                        self.reply_failure(
                            name,
                            &request_id,
                            "Error",
                            "nuke is unavailable for this account",
                            Some("notSent"),
                        );
                        continue;
                    };
                    let input = crate::algos::AlgoInput::Nuke {
                        mode,
                        request_id: request_id.clone(),
                        action: None,
                    };
                    if !waiting || mode == "algos" {
                        if algos.send(input).is_err() {
                            self.reply_failure(
                                name,
                                &request_id,
                                "Error",
                                "nuke is unavailable for this account",
                                Some("notSent"),
                            );
                        }
                        continue;
                    }
                    // Off the socket loop: the wait must not hold the signatures loading needs.
                    let (out, name, request_id) =
                        (self.account_out(name), name.to_owned(), request_id.clone());
                    let expires_at_ms = self.token_expires_at_ms();
                    tokio::spawn(async move {
                        let refused =
                            match methods::until_nuke_can_see(&handle, mode, methods::READ_WAIT)
                                .await
                            {
                                Ok(())
                                    if crate::auth::Verifier::check_expiry(
                                        expires_at_ms,
                                        now_ms(),
                                    )
                                    .is_err() =>
                                {
                                    crate::auth::TOKEN_EXPIRED.to_owned()
                                }
                                Ok(()) if algos.send(input).is_ok() => return,
                                Ok(()) => "nuke is unavailable for this account".to_owned(),
                                Err(refused) => refused.message,
                            };
                        out.send(envelope::msg(
                            &name,
                            "callMethod:done",
                            json!({"requestId": request_id, "success": false, "result":
                                {"name": "Error", "message": refused, "outcome": "notSent"}}),
                        ));
                    });
                }
                Plan::Resnapshot(result) => {
                    if self.resnapshot(name).await {
                        self.send(envelope::msg(
                            name,
                            "callMethod:done",
                            json!({"requestId": request_id, "success": true, "result": result}),
                        ));
                    } else {
                        self.reply_failure(
                            name,
                            &request_id,
                            "Error",
                            "the account could not re-snapshot",
                            None,
                        );
                    }
                }
                // Off the socket loop: a read may wait for the account's first load.
                Plan::Read(read) => {
                    let from_frame = read.from_frame();
                    // Each frame read is a whole engine snapshot; each other read may wait for load.
                    let limit = if from_frame {
                        &account.frame_reads
                    } else {
                        &account.reads
                    };
                    let permit = {
                        match limit.clone().try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                self.reply_failure(
                                    name,
                                    &request_id,
                                    "Error",
                                    "too many account reads in flight; retry shortly",
                                    None,
                                );
                                continue;
                            }
                        }
                    };
                    let (handle, out) = (account.handle.clone(), self.account_out(name));
                    let (name, request_id) = (name.to_owned(), request_id.clone());
                    tokio::spawn(async move {
                        let _permit = permit;
                        let mut answer =
                            methods::read_when_loaded(&handle, read, methods::READ_WAIT).await;
                        if from_frame && answer.is_ok() {
                            use base64::Engine as _;
                            answer = handle
                                .snapshot_frame()
                                .await
                                .map(|frame| {
                                    let frame =
                                        base64::engine::general_purpose::STANDARD.encode(frame);
                                    json!({ "accountFrame": frame })
                                })
                                .ok_or_else(|| methods::Refused {
                                    name: "Error",
                                    message: "account is closed".into(),
                                });
                        }
                        let data = match answer {
                            Ok(result) => {
                                json!({"requestId": request_id, "success": true, "result": result})
                            }
                            Err(refused) => json!({"requestId": request_id, "success": false,
                                "result": {"name": refused.name, "message": refused.message}}),
                        };
                        out.send(envelope::msg(&name, "callMethod:done", data));
                    });
                }
                Plan::AfterLoad { command, reply } => self.send_command(
                    name,
                    &request_id,
                    &method,
                    &group,
                    command,
                    reply,
                    true,
                    order_frame,
                ),
                Plan::Command { command, reply } => self.send_command(
                    name,
                    &request_id,
                    &method,
                    &group,
                    command,
                    reply,
                    false,
                    order_frame,
                ),
            }
        }
    }

    /// Registers `command`'s completion and sends it; `after_load` first waits
    /// (off the socket loop) for the account's start, as TS `getAccount` does.
    #[allow(clippy::too_many_arguments)]
    fn send_command(
        &mut self,
        name: &str,
        request_id: &Value,
        method: &str,
        group: &Option<Arc<Mutex<Group>>>,
        mut command: Value,
        reply: methods::Reply,
        after_load: bool,
        order_frame: Option<OrderReceipt>,
    ) {
        let mut completion = Pending {
            request_id: request_id.clone(),
            method: method.to_owned(),
            reply,
            group: group.clone(),
            incomplete: false,
            audit: None,
        };
        if !self.joined[name].account.running.io_allowed() {
            self.command_failure(
                name,
                completion,
                "ServerDraining",
                "Router lease expired; no new work was sent",
                Some("notSent"),
                order_frame,
            );
            return;
        }
        // An after-load wait polls the account like a waiting read, under the same cap.
        let permit = if after_load {
            let account = &self.joined[name].account;
            let pool = after_load_pool(method, &account.reads, &account.metadata_refreshes);
            match pool.clone().try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => {
                    self.command_failure(
                        name,
                        completion,
                        "Error",
                        "too many account reads in flight; retry shortly",
                        None,
                        order_frame,
                    );
                    return;
                }
            }
        } else {
            None
        };
        let out = self.account_out(name);
        let expires_at_ms = self.token_expires_at_ms();
        let action_guard = match order_frame {
            Some(OrderReceipt::Action { deadline, .. }) => Some(
                crate::order_actions::ActionGuard::new(out.clone(), deadline, expires_at_ms),
            ),
            _ => None,
        };
        let joined = self.joined.get_mut(name).unwrap();
        joined.account.placed.record_command(&command);
        let Some(id) = joined.next_command.checked_add(1) else {
            self.command_failure(
                name,
                completion,
                "Error",
                "command IDs exhausted",
                None,
                order_frame,
            );
            return;
        };
        joined.next_command = id;
        command["command"] = id.into();
        let audit = crate::audit::command(&command).map(|fields| Audit {
            who: audit_who(&joined.account),
            origin: format!("socket:{}/client:{}", self.socket, joined.client.id),
            fields,
        });
        completion.audit = audit;
        joined.client.pending.lock().unwrap().insert(id, completion);
        let (client, handle) = (joined.client.id, joined.account.handle.clone());
        let (pending, out, name) = (joined.client.pending.clone(), out, name.to_owned());
        let waiter =
            after_load.then(|| (handle.clone(), pending.clone(), out.clone(), name.clone()));
        let send = move || {
            if let Err(message) = crate::auth::Verifier::check_expiry(expires_at_ms, now_ms()) {
                if let Some(entry) = pending.lock().unwrap().remove(&id) {
                    settle(
                        &out,
                        &name,
                        entry,
                        Err(failure_detail("AuthExpired", message, false)),
                    );
                }
                return;
            }
            // Each leg keeps the original age/registration through any startup
            // wait. Expiry cannot dispatch a write or prove the action not sent.
            if let Some(guard) = &action_guard
                && let Err(message) = guard.check()
            {
                if let Some(entry) = pending.lock().unwrap().remove(&id) {
                    settle(
                        &out,
                        &name,
                        entry,
                        Err(json!({"message":message,"outcome":"unknown"})),
                    );
                }
                return;
            }
            let failure = match handle.input(client, &command.to_string()) {
                Ok(true) => None,
                // Not queued, so not submitted: a definite failure, never retried.
                Ok(false) => Some("account is busy; command not sent".to_owned()),
                Err(refused) => Some(refused.notice.to_string()),
            };
            if let Some(message) = failure
                && let Some(pending) = pending.lock().unwrap().remove(&id)
            {
                let mut detail = failure_detail("Error", &message, false);
                if matches!(pending.reply, methods::Reply::Action { .. }) {
                    detail["outcome"] = "notSent".into();
                }
                settle(&out, &name, pending, Err(detail));
            }
        };
        let Some((handle, pending, out, name)) = waiter else {
            return send();
        };
        tokio::spawn(async move {
            let _permit = permit;
            match methods::until_account_loaded(&handle, methods::READ_WAIT).await {
                Ok(()) => send(),
                Err(refused) => {
                    if let Some(entry) = pending.lock().unwrap().remove(&id) {
                        settle(
                            &out,
                            &name,
                            entry,
                            Err(failure_detail(refused.name, &refused.message, false)),
                        );
                    }
                }
            }
        });
    }

    fn command_failure(
        &self,
        name: &str,
        pending: Pending,
        kind: &str,
        message: &str,
        outcome: Option<&str>,
        order_frame: Option<OrderReceipt>,
    ) {
        if pending.group.is_some() && matches!(pending.reply, methods::Reply::Action { .. }) {
            // This proves only one leg was not dispatched. Another leg may be
            // in flight: count every leg and let the group settle as unknown.
            settle(
                &self.account_out(name),
                name,
                pending,
                Err(failure_detail(kind, message, false)),
            );
        } else {
            self.call_failure(
                name,
                &pending.request_id,
                kind,
                message,
                outcome,
                order_frame,
            );
        }
    }

    fn reply_failure(
        &self,
        name: &str,
        request_id: &Value,
        kind: &str,
        message: &str,
        outcome: Option<&str>,
    ) {
        let mut result = json!({"name": kind, "message": message});
        if kind == "ServerDraining" {
            result["kind"] = "venueUnavailable".into();
        }
        if let Some(outcome) = outcome {
            result["outcome"] = outcome.into();
        }
        self.send(envelope::msg(
            name,
            "callMethod:done",
            json!({"requestId": request_id, "success": false, "result": result}),
        ));
    }

    /// On close: park private accounts for resumable sockets, release the rest.
    pub fn close(mut self) {
        self.pending_verifications.clear();
        let disposal = if self.moved && self.registry.planned_move(self.user()) {
            Disposal::Moving
        } else {
            Disposal::SocketClosed
        };
        let joined: Vec<Joined> = self.joined.drain().map(|(_, joined)| joined).collect();
        let mut parked = Vec::new();
        for joined in joined {
            joined.out.retire_order_receipts();
            for client in std::iter::once(joined.client).chain(joined.old) {
                client.pump.abort();
                joined.account.handle.leave(client.id);
            }
            if self.identity.resumable
                && !self.moved
                && self.identity.user.is_some()
                && !joined.account.is_public()
            {
                parked.push((joined.account, self.socket));
            } else {
                self.registry
                    .release(&joined.account, self.socket, disposal);
            }
        }
        if let Some(user) = &self.identity.user {
            self.registry.park(self.resume_id.clone(), user, parked);
            self.registry.session_closed(&self.resume_id);
        }
    }
}

/// One notification per account, reason and registration; legacy retries retain
/// master's last-reason suppression and existing cache clearing point.
fn first_refusal(
    refused: &mut HashMap<String, RefusalNotifications>,
    account: &str,
    refusal: &str,
    instance_id: Option<&str>,
) -> bool {
    let notifications = refused.entry(account.to_owned()).or_default();
    if let Some(instance_id) = instance_id.filter(|id| !id.is_empty()) {
        return notifications
            .registered
            .insert((refusal.to_owned(), instance_id.to_owned()));
    }
    if notifications.legacy_reason.as_deref() == Some(refusal) {
        return false;
    }
    notifications.legacy_reason = Some(refusal.to_owned());
    true
}

fn failure_detail(kind: &str, message: &str, unknown: bool) -> Value {
    let mut detail = json!({"name": kind, "message": message});
    if unknown {
        detail["outcome"] = "unknown".into();
    }
    detail
}

const WRITE_METHODS: [&str; 9] = [
    "placeOrders",
    "placeOrder",
    "placeOrderWithClientOrderId",
    "updateOrder",
    "cancelOrders",
    "cancelSymbolOrders",
    "cancelAllOrders",
    "setLeverage",
    "setAllLeverage",
];

fn audit_who(account: &Account) -> crate::audit::Who {
    crate::audit::Who {
        user: account.user.clone(),
        account: account.name.clone(),
        venue: account.venue.clone(),
    }
}

fn after_load_pool<'a>(
    method: &str,
    reads: &'a Arc<tokio::sync::Semaphore>,
    metadata_refreshes: &'a Arc<tokio::sync::Semaphore>,
) -> &'a Arc<tokio::sync::Semaphore> {
    if method == "updateSymbolMetadata" {
        metadata_refreshes
    } else {
        reads
    }
}

fn settle(out: &Out, name: &str, pending: Pending, outcome: Result<Value, Value>) {
    if let Some(audit) = &pending.audit {
        crate::audit::log(
            &audit.who,
            &audit.origin,
            &audit.fields,
            crate::audit::result(&outcome),
        );
    }
    let outcome = match &pending.group {
        None => outcome,
        Some(group) => {
            let mut group = group.lock().unwrap();
            group.remaining -= 1;
            if let Err(error) = &outcome {
                group
                    .failures
                    .push(error["message"].as_str().unwrap_or("failed").to_owned());
            }
            if group.remaining > 0 {
                return;
            }
            if group.failures.is_empty() {
                Ok(Value::Null)
            } else {
                Err(failure_detail(
                    "Error",
                    &format!(
                        "{name} {} failed: {}",
                        if pending.method == "setAllLeverage" {
                            "leverage update"
                        } else {
                            "cancel-all"
                        },
                        group.failures.join("; ")
                    ),
                    false,
                ))
            }
        }
    };
    if let methods::Reply::Action {
        kind,
        seq,
        unit,
        idless_protection,
    } = &pending.reply
    {
        // Unit-returning methods already discard native result bodies on LOCAL /
        // COSIGNED. Keep that mapping after an authoritative native completion;
        // never turn an error or absent completion into a unit success.
        let outcome = outcome.map(|value| {
            if *unit {
                Value::Null
            } else if *idless_protection && value.as_array().is_some_and(Vec::is_empty) {
                json!({"orderIds":[],"protectionApplied":true})
            } else {
                value
            }
        });
        crate::order_actions::settle(out, name, &pending.request_id, *seq, kind, outcome);
        return;
    }
    if let methods::Reply::Intent {
        client_order_id,
        seq,
    } = &pending.reply
    {
        order_frames::settle_intent(
            out,
            name,
            &pending.request_id,
            client_order_id,
            *seq,
            outcome,
        );
        return;
    }
    let data = match outcome {
        Ok(result) => {
            let result = match pending.reply {
                methods::Reply::Result
                | methods::Reply::Intent { .. }
                | methods::Reply::Action { .. } => result,
                methods::Reply::Unit => Value::Null,
                methods::Reply::OrderIds => methods::worker_order_ids(result),
                methods::Reply::Placed { client_order_id } => {
                    json!({"orderIds": result, "clientOrderId": client_order_id})
                }
            };
            let mut data =
                json!({"requestId": pending.request_id, "success": true, "result": result});
            if pending.incomplete {
                data["completeness"] = json!({"incomplete": true});
            }
            data
        }
        Err(detail) => json!({"requestId": pending.request_id, "success": false, "result": detail}),
    };
    let _ = out.send(envelope::msg(name, "callMethod:done", data));
}

/// How often a client's `store.socketHealth` is re-derived, as the browser engine's sync.
const SOCKET_HEALTH_EVERY: std::time::Duration = std::time::Duration::from_secs(1);
/// Last-message times alone change on every frame; they go out at most this often.
const SOCKET_HEALTH_TIMES_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether `health` should replace what the client last got: any flag or close
/// diagnostic at once (a socket coming up or going down), timestamps alone only
/// once `since_sent` reaches `SOCKET_HEALTH_TIMES_EVERY`.
pub fn socket_health_due(sent: &Value, health: &Value, since_sent: std::time::Duration) -> bool {
    if sent == health {
        return false;
    }
    let state = |value: &Value| {
        let mut value = value.clone();
        if let Some(fields) = value.as_object_mut() {
            fields.remove("publicLastMessage");
            fields.remove("privateLastMessage");
        }
        value
    };
    state(sent) != state(health) || since_sent >= SOCKET_HEALTH_TIMES_EVERY
}

/// `store.socketHealth` from the host's socket state, as `summarizeSocketHealth`.
pub fn socket_health(summary: SocketSummary) -> Value {
    json!({
        "publicHealthy": summary.public_healthy,
        "privateHealthy": summary.private_healthy,
        "publicInitialized": summary.public_initialized,
        "privateInitialized": summary.private_initialized,
        "publicLastMessage": summary.public_last_message,
        "privateLastMessage": summary.private_last_message,
        "publicCloseCode": summary.public.close_code,
        "publicCloseCountTotal": summary.public.close_count_total,
        "publicCloseStreak": summary.public.close_streak,
        "publicReconnectPaused": false,
        "privateCloseCode": summary.private.close_code,
        "privateCloseCountTotal": summary.private.close_count_total,
        "privateCloseStreak": summary.private.close_streak,
        "privateReconnectPaused": false,
    })
}

/// Forwards one client's account output: frames as base64 SCXF with a per-client
/// seq (the first frame is the snapshot), notices as the existing msg ops, and
/// `store.socketHealth`, which frames do not carry, once the snapshot is out.
/// Where a private account's pump reports that its held snapshot may go.
struct Hold {
    client: u64,
    loaded: tokio::sync::mpsc::UnboundedSender<(String, u64)>,
}

/// Bounded: a starting engine is busy, and a pump waiting on it lets its client queue fill.
async fn account_loaded(handle: &AccountHandle) -> bool {
    let read = handle.read(|store| {
        let loaded = store.loaded();
        loaded.orders && loaded.positions
    });
    matches!(
        tokio::time::timeout(HOLD_LOAD_CHECK, read).await,
        Ok(Some(true))
    )
}

/// Asks the session for a full snapshot once the account loads or the hold
/// limit passes; frames stay withheld until it replaces this client.
async fn release_hold(
    handle: &AccountHandle,
    name: &str,
    hold: &mut Option<Hold>,
    since: std::time::Instant,
) {
    if hold.is_some() && (since.elapsed() >= SNAPSHOT_HOLD_LIMIT || account_loaded(handle).await) {
        let hold = hold.take().expect("holding");
        let _ = hold.loaded.send((name.to_owned(), hold.client));
    }
}

#[allow(clippy::too_many_arguments)]
async fn pump(
    mut link: ClientLink,
    client: u64,
    dropped: tokio::sync::mpsc::UnboundedSender<(String, u64)>,
    handle: AccountHandle,
    name: String,
    out: Out,
    pending: Arc<Mutex<HashMap<u32, Pending>>>,
    draining: Arc<AtomicBool>,
    mut hold: Option<Hold>,
) {
    // Set while frames are withheld: when holding began.
    let mut held: Option<std::time::Instant> = None;
    let mut seq = 0u64;
    let mut health_tick = tokio::time::interval(SOCKET_HEALTH_EVERY);
    let mut hold_tick = tokio::time::interval(SNAPSHOT_HOLD_CHECK);
    let mut sent_health = (Value::Null, std::time::Instant::now());
    let report_health = |sent: &mut (Value, std::time::Instant)| {
        let sockets = handle.socket_health();
        let health = socket_health(sockets.summary(now_ms()));
        if !socket_health_due(&sent.0, &health, sent.1.elapsed()) {
            return true;
        }
        // The host tracker's own view, on every state change: whether a private
        // socket was ever seen opening and taking a frame.
        if socket_health_due(&sent.0, &health, std::time::Duration::ZERO) {
            Log::info(&format!(
                "{name} socketHealth privateInitialized={} privateHealthy={} publicInitialized={} tracker public={:?} business={:?} private={:?}",
                health["privateInitialized"],
                health["privateHealthy"],
                health["publicInitialized"],
                sockets.public,
                sockets.business,
                sockets.private,
            ));
        }
        *sent = (health.clone(), std::time::Instant::now());
        out.send(envelope::msg(
            &name,
            "store",
            json!({"socketHealth": health}),
        ))
    };
    loop {
        let item = tokio::select! {
            item = link.outgoing.recv() => match item {
                Some(item) => item,
                None => {
                    // The engine closed this client's link; a drain or leave aborts the pump first.
                    if !draining.load(Ordering::Relaxed) {
                        let _ = dropped.send((name.clone(), client));
                    }
                    break;
                }
            },
            _ = hold_tick.tick(), if held.is_some() && hold.is_some() => {
                release_hold(&handle, &name, &mut hold, held.expect("holding")).await;
                continue;
            }
            _ = health_tick.tick() => {
                if seq > 0 && !draining.load(Ordering::Relaxed) && !report_health(&mut sent_health) {
                    break;
                }
                continue;
            }
        };
        match item {
            Outgoing::Frame(bytes) => {
                if draining.load(Ordering::Relaxed) {
                    continue;
                }
                // An unloaded private account sends nothing yet: the client keeps
                // what it shows until a re-snapshot carries the loaded account.
                if seq == 0 && hold.is_some() && held.is_none() {
                    if account_loaded(&handle).await {
                        hold = None;
                    } else {
                        held = Some(std::time::Instant::now());
                    }
                }
                // Withheld until the release re-snapshot, which carries the whole account;
                // the hold tick checks the load, never this frame.
                if held.is_some() {
                    continue;
                }
                seq += 1;
                if !out.send(envelope::frame(&name, seq, seq == 1, &bytes)) {
                    break;
                }
                if seq == 1 && !report_health(&mut sent_health) {
                    break;
                }
            }
            Outgoing::Text(text) => {
                let Ok(notice) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                notice_to_msg(&out, &name, &notice, &pending);
                if draining.load(Ordering::Relaxed) && pending.lock().unwrap().is_empty() {
                    break;
                }
            }
        }
    }
    // The account dropped this client: whatever still waits has an unknown outcome.
    let waiting: Vec<Pending> = pending
        .lock()
        .unwrap()
        .drain()
        .map(|(_, pending)| pending)
        .collect();
    for pending in waiting {
        let unknown = WRITE_METHODS.contains(&pending.method.as_str());
        let detail = failure_detail(
            "EngineUnavailableError",
            "account connection closed",
            unknown,
        );
        settle(&out, &name, pending, Err(detail));
    }
}

/// The engine's notices that startup is still waiting or retrying a read, for the journal
/// too: while held, the page sees nothing, and a start that never finished was silent.
fn startup_waiting_line(name: &str, notice: &Value) -> Option<String> {
    use safe_cex_engine::log::code::{STARTUP_DEGRADED, STARTUP_INCOMPLETE};
    (notice["code"] == STARTUP_INCOMPLETE || notice["code"] == STARTUP_DEGRADED).then(|| {
        format!(
            "{name} {}",
            notice["message"].as_str().unwrap_or("startup incomplete")
        )
    })
}

fn notice_to_msg(
    out: &Out,
    name: &str,
    notice: &Value,
    pending: &Arc<Mutex<HashMap<u32, Pending>>>,
) {
    let kind = notice["type"].as_str().unwrap_or("");
    // As the SDK bridge: a partial history read is answered with its rows and a
    // positive `completeness` marker, never as complete.
    if kind == "event" && notice["code"] == "history.incomplete" {
        let owner = notice["fields"]["command"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok());
        if let Some(id) = owner
            && let Some(waiting) = pending.lock().unwrap().get_mut(&id)
        {
            waiting.incomplete = true;
        }
    }
    let command = notice["command"]
        .as_u64()
        .and_then(|id| u32::try_from(id).ok());
    // Only completions settle a request; other notices leave it waiting.
    let settles = matches!(kind, "done" | "error");
    let waiting = command
        .filter(|_| settles)
        .and_then(|id| pending.lock().unwrap().remove(&id));
    match (kind, waiting) {
        ("done", Some(pending)) => {
            let outcome = match (
                notice.get("result").and_then(Value::as_str),
                notice.get("error"),
            ) {
                (Some(result), _) => Ok(serde_json::from_str(result).unwrap_or(Value::Null)),
                (None, Some(error)) => {
                    let mut detail = notice.get("detail").cloned().unwrap_or_else(|| json!({}));
                    if !detail.is_object() {
                        detail = json!({});
                    }
                    detail["message"] = error.clone();
                    if detail.get("name").is_none() {
                        detail["name"] = "ExchangeError".into();
                    }
                    Err(detail)
                }
                (None, None) => Ok(Value::Null),
            };
            settle(out, name, pending, outcome);
        }
        ("error", Some(pending)) => {
            let message = notice["error"].as_str().unwrap_or("command failed");
            settle(
                out,
                name,
                pending,
                Err(failure_detail("Error", message, false)),
            );
        }
        ("error", None) => {
            let _ = out.send(envelope::msg(name, "error", json!([notice["error"]])));
        }
        ("event" | "accountAuth", _) => {
            if let Some(line) = startup_waiting_line(name, notice) {
                Log::warn(&line);
            }
            let _ = out.send(envelope::msg(name, "engine-event", notice.clone()));
            if notice["code"] == "order.rejected" && notice["level"] == "error" {
                let _ = out.send(envelope::msg(name, "error", json!([notice["message"]])));
            }
        }
        ("deactivate", _) => {
            let _ = out.send(envelope::msg(name, "deactivate-status", notice.clone()));
        }
        ("startupFailed", _) => {
            let detail = notice.get("detail").cloned().unwrap_or(Value::Null);
            let _ = out.send(envelope::msg(
                name,
                "start-status",
                json!({"ok": false, "detail": detail}),
            ));
            let message = notice["error"]
                .as_str()
                .or(detail.as_str())
                .or(detail["message"].as_str())
                .unwrap_or("account startup failed");
            let _ = out.send(envelope::msg(name, "error", json!([message])));
        }
        _ => {}
    }
}

/// `validateUnauthenticatedPublicInit`: only the public Bybit pseudo-account,
/// public-only, with no account, credential, routing or diagnostic field.
pub fn unauthenticated_init_refusal(init: &InitPayload) -> Option<&'static str> {
    if init.account_name != PUBLIC_ACCOUNT || init.exchange_name != PUBLIC_EXCHANGE {
        return Some("Unauthenticated exchange-server sessions only support public Bybit data");
    }
    if init.public_only != Some(true) {
        return Some("Unauthenticated exchange-server sessions must be public-only");
    }
    let present = [
        init.signing_mode.is_some(),
        init.sealed.is_some(),
        !init.key.is_empty(),
        init.passphrase.is_some(),
        init.testnet.is_some(),
        init.application_id.is_some(),
        init.api_key_index.is_some(),
        init.paradex_address.is_some(),
        init.subkey_address.is_some(),
        init.subaccount.is_some(),
        init.orderly_exchange_id.is_some(),
        init.grvt_trading_account_id.is_some(),
        init.grvt_signer_address.is_some(),
        init.tealstreet_proxy.is_some(),
        init.tealstreet_tunnel.is_some(),
        init.tealstreet_ws_route.is_some(),
        init.proxy_region.is_some(),
        init.verbose.is_some(),
        init.hyperliquid_builder.is_some(),
        init.vault.is_some(),
        init.bybit_site_id.is_some(),
        init.enable_spot_markets.is_some(),
        init.enable_inverse_markets.is_some(),
        init.enable_option_markets.is_some(),
    ];
    present
        .contains(&true)
        .then_some("Unauthenticated public Bybit init cannot include account or routing fields")
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_remote_signer::Outbox;

    mod apply_settings;
    mod cosigned_venues;
    mod delegated_probe;
    mod token_expiry;

    fn synthetic_candidate() -> crate::delegated::Credentials {
        serde_json::from_str(r#"{"key":"synthetic-key","secret":"synthetic-secret"}"#).unwrap()
    }

    #[tokio::test]
    async fn sealed_init_is_opened_off_loop_without_account_attachment() {
        let registry = crate::accounts::tests::registry();
        let (mut session, mut sent) = user_session(registry, "a");
        let mut completed = session.take_verified();
        let offer = session
            .handovers
            .issue("acct", "bybit", crate::accounts::now_ms())
            .unwrap();
        let sealed = crate::delegated::tests::seal(
            &offer,
            "bybit",
            "synthetic-key",
            br#"{"key":"synthetic-key","secret":"synthetic-secret"}"#,
        );
        let wire = json!({"t":"init", "d":{"accountName":"acct", "exchangeName":"bybit", "key":"synthetic-key", "signingMode":"delegated", "sealed":sealed}});
        let Inbound::Init(init) = serde_json::from_value(wire).unwrap() else {
            panic!("init expected")
        };
        assert_eq!(init.signing_mode.as_deref(), Some("delegated"));
        assert!(init.sealed.is_some());
        assert!(serde_json::to_value(&init).unwrap().get("sealed").is_none());
        // Exercise the opening boundary directly: the shared opt-in is supplied
        // by the separate account-build change, not enabled by this test.
        session.open_handover(*init);
        assert!(session.pending_verifications.contains_key("acct"));
        assert!(session.joined.is_empty());
        assert!(sent.try_recv().is_err());
        let completion = completed.recv().await.unwrap();
        assert!(completion.candidate.is_ok());
        session.verification_completed(completion).await.unwrap();
        let refusal: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(refusal["d"]["data"][1]["transient"], true);
        assert!(session.joined.is_empty());
        session.close();
    }

    #[test]
    fn local_and_cosigned_settings_keep_the_legacy_engine_bytes_and_fingerprint() {
        let legacy = acct_init();
        let options = crate::accounts::engine_options(&legacy, false).to_string();
        let fingerprint = legacy.fingerprint();
        assert!(legacy.signing_mode.is_none() && legacy.sealed.is_none());
        let serialized = serde_json::to_value(&legacy).unwrap();
        assert!(serialized.get("signingMode").is_none());
        assert!(serialized.get("sealed").is_none());
        for mode in ["local", "cosigned"] {
            let mut explicit = legacy.clone();
            explicit.signing_mode = Some(mode.into());
            assert_eq!(
                crate::accounts::engine_options(&explicit, false).to_string(),
                options
            );
            assert_eq!(explicit.fingerprint(), fingerprint);
        }
    }

    #[tokio::test]
    async fn unauthenticated_connections_cannot_request_handover_keys() {
        let (out, mut sent, _) = Out::channel(16);
        let mut session = Session::new(
            crate::accounts::tests::registry(),
            Arc::default(),
            Identity {
                user: None,
                chat_handle: None,
                token: None,
                resume_from: None,
                resumable: false,
            },
            out,
        );
        let refused = session
            .handle(r#"{"t":"msg","d":{"op":"delegated-key-req","accountName":"acct","data":{"exchangeName":"bybit"}}}"#)
            .await
            .unwrap_err();
        assert_eq!(refused.code(), 1008);
        assert!(sent.try_recv().is_err());
        session.close();
    }

    #[tokio::test]
    async fn verification_completion_checks_socket_and_drain_after_the_await() {
        let registry = crate::accounts::tests::registry();
        let (mut first, mut first_sent) = user_session(registry.clone(), "a");
        let mut first_results = first.take_verified();
        first.start_verification("acct".into(), async { Ok(synthetic_candidate()) });
        let mut completion = first_results.recv().await.unwrap();
        let (mut second, mut second_sent) = user_session(registry.clone(), "a");
        second.start_verification("acct".into(), std::future::pending());
        // Same user and generation still cannot transfer a socket's result.
        completion.generation = second.pending_verifications["acct"].generation;
        second.verification_completed(completion).await.unwrap();
        assert!(second_sent.try_recv().is_err());
        assert!(second.pending_verifications.contains_key("acct"));
        first.start_verification("acct".into(), async { Ok(synthetic_candidate()) });
        let completion = first_results.recv().await.unwrap();
        registry.start_drain();
        assert_eq!(
            first.verification_completed(completion).await,
            Err(Close::Moved)
        );
        assert!(first_sent.try_recv().is_err());
        first.close();
        second.close();
    }

    #[tokio::test]
    async fn pending_verification_keeps_socket_responsive_and_refuses_commands() {
        let registry = crate::accounts::tests::registry();
        let (mut session, mut sent) = user_session(registry.clone(), "a");
        let mut verified = session.take_verified();
        session.handle(r#"{"t":"msg","d":{"op":"subscribeOHLCV","accountName":"acct","data":{"symbol":"BTCUSDT","interval":"1"}}}"#).await.unwrap();
        assert_eq!(session.early["acct"].len(), 1);
        let (release, wait) = tokio::sync::oneshot::channel();
        session.start_verification("acct".into(), async move {
            wait.await.unwrap();
            Ok(synthetic_candidate())
        });
        for (op, data) in [
            (
                "callMethod",
                json!({"requestId": "pending", "method": "cancelAllOrders", "args": []}),
            ),
            ("subscribeOrderBook", json!({"symbol": "BTCUSDT"})),
            ("updatePositions", json!({})),
            ("chaser:start", json!({})),
        ] {
            session
                .handle(
                    &json!({"t":"msg", "d":{"op":op, "accountName":"acct", "data":data}})
                        .to_string(),
                )
                .await
                .unwrap();
            let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
            if op == "callMethod" {
                assert_eq!(frame["d"]["op"], "callMethod:done");
                assert_eq!(frame["d"]["data"]["result"]["message"], "verifying");
                assert_eq!(frame["d"]["data"]["result"]["outcome"], "notSent");
            } else {
                assert_eq!(frame["d"]["op"], "error");
                assert_eq!(frame["d"]["data"][0], "verifying");
            }
        }
        assert!(session.joined.is_empty());
        assert_eq!(
            session.early["acct"].len(),
            1,
            "pre-init topics wait for the verification outcome"
        );
        session.handle(r#"{"t":"ping"}"#).await.unwrap();
        let pong: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(pong["t"], "pong");
        assert_eq!(pong["d"]["accounts"], json!([]));
        assert!(sent.try_recv().is_err(), "no account frames while pending");
        release.send(()).unwrap();
        session
            .verification_completed(verified.recv().await.unwrap())
            .await
            .unwrap();
        let refusal: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(refusal["d"]["data"][1]["kind"], "unverified");
        assert_eq!(refusal["d"]["data"][1]["transient"], true);
        assert!(
            session.early.is_empty(),
            "a refused init retires its early topics"
        );
        assert!(
            session.joined.is_empty(),
            "opening alone cannot attach an account"
        );
        session.close();
    }

    #[tokio::test]
    async fn two_users_switch_during_verification_await_without_reassigning_the_result() {
        let registry = crate::accounts::tests::registry();
        let (mut alice, mut alice_sent) = user_session(registry.clone(), "alice");
        let mut alice_results = alice.take_verified();
        let (release, wait) = tokio::sync::oneshot::channel();
        let (started, waiting) = tokio::sync::oneshot::channel();
        alice.start_verification("acct".into(), async move {
            started.send(()).unwrap();
            wait.await.unwrap();
            Ok(synthetic_candidate())
        });
        waiting.await.unwrap();
        // Login changes while the device/venue proof is still awaited. The new
        // authenticated connection has its own init for the same saved name.
        let (mut bob, mut bob_sent) = user_session(registry, "bob");
        let mut bob_results = bob.take_verified();
        let (bob_release, bob_wait) = tokio::sync::oneshot::channel();
        bob.start_verification("acct".into(), async move {
            bob_wait.await.unwrap();
            Ok(synthetic_candidate())
        });
        release.send(()).unwrap();
        let alice_result = alice_results.recv().await.unwrap();
        alice.close();
        bob.verification_completed(alice_result).await.unwrap();
        assert!(bob_sent.try_recv().is_err());
        assert!(alice_sent.try_recv().is_err());
        assert!(bob.pending_verifications.contains_key("acct"));
        assert!(bob.joined.is_empty());
        bob_release.send(()).unwrap();
        bob.verification_completed(bob_results.recv().await.unwrap())
            .await
            .unwrap();
        let own: Value = serde_json::from_str(&bob_sent.try_recv().unwrap()).unwrap();
        assert_eq!(own["d"]["data"][1]["kind"], "unverified");
        bob.close();
    }

    #[tokio::test]
    async fn queued_completions_are_discarded_after_reinit_or_dispose_and_close_aborts_awaits() {
        let registry = crate::accounts::tests::registry();
        let (mut session, mut sent) = user_session(registry, "a");
        let mut results = session.take_verified();
        session.start_verification("acct".into(), async { Ok(synthetic_candidate()) });
        let stale = results.recv().await.unwrap();
        let (release, wait) = tokio::sync::oneshot::channel();
        session.start_verification("acct".into(), async move {
            wait.await.unwrap();
            Ok(synthetic_candidate())
        });
        session.verification_completed(stale).await.unwrap();
        assert!(sent.try_recv().is_err());
        assert!(session.pending_verifications.contains_key("acct"));
        session
            .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(
            release.send(()).is_err(),
            "dispose aborted the pending await"
        );
        session.start_verification("acct".into(), async { Ok(synthetic_candidate()) });
        let stale = results.recv().await.unwrap();
        session
            .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
            .await
            .unwrap();
        session.verification_completed(stale).await.unwrap();
        assert!(sent.try_recv().is_err());
        let (release, wait) = tokio::sync::oneshot::channel();
        session.start_verification("acct".into(), async move {
            wait.await.unwrap();
            Ok(synthetic_candidate())
        });
        tokio::task::yield_now().await;
        session.close();
        tokio::task::yield_now().await;
        assert!(release.send(()).is_err(), "close aborted the pending await");
    }

    #[tokio::test]
    async fn malformed_sealed_init_keeps_the_existing_account_and_other_windows_unchanged() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_counted(
                    &registry,
                    link,
                    acct_init().fingerprint(),
                );
                let (mut session, mut sent) = user_session(registry.clone(), "u");
                let before = account.fingerprint.clone();
                for mode in [None, Some("delegated")] {
                    let mut init = acct_init();
                    init.signing_mode = mode.map(str::to_owned);
                    init.sealed = Some(crate::delegated::Sealed {
                        attempt_id: "stale".into(),
                        enc: String::new(),
                        ciphertext: String::new(),
                    });
                    session.init(init).await.unwrap();
                    let refusal: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
                    assert_eq!(refusal["d"]["data"][1]["initRefused"], true);
                    assert!(session.joined.is_empty());
                    assert!(registry.held("u", "acct", 7).is_some());
                    assert_eq!(account.fingerprint, before);
                }
                session.close();
                registry.release(&account, 7, Disposal::PageDisposed);
            })
            .await;
    }

    #[tokio::test]
    async fn delegated_key_requests_use_the_existing_authenticated_socket() {
        use base64::Engine as _;
        let registry = crate::accounts::tests::registry();
        let (mut session, mut sent) = user_session(registry, "handover-user");
        session
            .handle(r#"{"t":"msg","d":{"op":"delegated-key-req","accountName":"acct","data":{"exchangeName":"bybit"}}}"#)
            .await
            .unwrap();
        let frame = sent.try_recv().expect("a handover key response");
        let frame: Value = serde_json::from_str(&frame).unwrap();
        if crate::delegated::enabled_venues().is_empty() {
            assert_eq!(frame["d"]["op"], "error");
            assert_eq!(frame["d"]["data"][1]["initRefused"], true);
            assert_eq!(frame["d"]["data"][1]["kind"], "unverified");
            session.close();
            return;
        }
        assert_eq!(frame["t"], "msg");
        assert_eq!(frame["d"]["op"], "delegated-key");
        assert_eq!(frame["d"]["accountName"], "acct");
        assert_eq!(frame["d"]["bootId"], crate::state::boot_id());
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(frame["d"]["publicKey"].as_str().unwrap())
                .unwrap()
                .len(),
            32
        );
        assert!(!frame["d"]["attemptId"].as_str().unwrap().is_empty());
        assert!(frame["d"]["expiresAtMs"].as_u64().unwrap() > crate::accounts::now_ms());
        session.close();
    }
    mod delegated_instance_id;
    mod delegated_venue_admission;
    mod instance_id;

    const WEB_CHASE: &str = r#"{"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"}"#;
    const WEB_TWAP: &str = r#"{"accountName":"acct","activeMarket":{"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true},"size":0.1,"duration":600,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"}"#;

    // Legacy session fixtures use an unflagged venue; enabled Bybit has its own suite.
    fn acct_init() -> InitPayload {
        InitPayload {
            account_name: "acct".into(),
            exchange_name: "bybitinverse".into(),
            key: "k".into(),
            ..InitPayload::default()
        }
    }

    fn algo_op(account: &Account, op: &str, data: &str) {
        account
            .algos
            .as_ref()
            .unwrap()
            .send(crate::algos::AlgoInput::Op {
                op: op.into(),
                data: data.into(),
                origin: "test".into(),
                twap_owner: account.twap_owner.snapshot(),
                action: None,
            })
            .unwrap();
    }

    #[test]
    fn session_start_carries_only_the_authenticated_public_chat_handle() {
        let registry = crate::accounts::tests::registry();
        let (out, _sent, _) = Out::channel(256);
        let user = "auth0|opaque-user";
        let identity = Identity {
            user: Some(user.into()),
            chat_handle: crate::auth::ChatHandle::parse("Trader-063249"),
            token: Some(("test-token".into(), u64::MAX)),
            resume_from: None,
            resumable: true,
        };
        let session = Session::new(
            registry.clone(),
            Arc::new(Metrics::default()),
            identity,
            out,
        );
        assert_eq!(session.user(), user);
        assert_eq!(
            registry.account_log_label(user, "acct", "bingx"),
            format!(
                "{} chat=Trader-063249 acct bingx",
                crate::accounts::user_tag(user)
            )
        );
        let (out, _sent, _) = Out::channel(256);
        let public = Identity {
            user: None,
            chat_handle: crate::auth::ChatHandle::parse("Forged_handle"),
            token: None,
            resume_from: None,
            resumable: false,
        };
        let public = Session::new(registry.clone(), Arc::new(Metrics::default()), public, out);
        assert_eq!(
            registry.account_log_label(public.user(), "acct", "bingx"),
            format!("{} acct bingx", crate::accounts::user_tag(public.user()))
        );
    }

    fn user_session(registry: Arc<Registry>, user: &str) -> (Session, crate::out::Receiver) {
        let (out, sent, _) = Out::channel(256);
        let identity = Identity {
            user: Some(user.into()),
            chat_handle: None,
            token: Some(("t".into(), u64::MAX)),
            resume_from: None,
            resumable: true,
        };
        let session = Session::new(registry, Arc::new(Metrics::default()), identity, out);
        (session, sent)
    }

    #[tokio::test(start_paused = true)]
    async fn deferred_output_drops_replaced_account_owner() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let mut first = acct_init();
                first.key = "synthetic-account-owner-A".into();
                let account_a = crate::accounts::testing::trading_account(&registry, "u", &first);
                let (mut session, mut sent) = user_session(registry.clone(), "u");
                session.init(first.clone()).await.unwrap();
                assert!(Arc::ptr_eq(&session.joined["acct"].account, &account_a));
                while sent.try_recv().is_ok() {}
                let old_update = envelope::msg(
                    "acct",
                    "store",
                    json!({"balance":{"info":{"syntheticOwner":"A"}}}),
                );
                account_a.notify(old_update.clone());
                assert_eq!(
                    sent.try_recv().unwrap(),
                    old_update,
                    "current owner delivery control"
                );
                let old_callback = session.account_out("acct").outbox();
                account_a.notify(old_update.clone());
                session.init(first.clone()).await.unwrap();
                let mut repeated = Vec::new();
                while let Ok(text) = sent.try_recv() {
                    repeated.push(text);
                }
                assert!(
                    repeated.contains(&old_update),
                    "same-owner re-snapshot keeps queued output"
                );
                account_a.notify(old_update.clone());
                // A prepared second init wins while A's output waits for the socket.
                let mut second = first;
                second.key = "synthetic-account-owner-B".into();
                let account_b = crate::accounts::testing::trading_account(&registry, "u", &second);
                session.init(second).await.unwrap();
                assert!(Arc::ptr_eq(&session.joined["acct"].account, &account_b));
                assert!(
                    !old_callback(old_update.clone()),
                    "late A callback cannot adopt B's generation"
                );
                let new_update = envelope::msg(
                    "acct",
                    "store",
                    json!({"balance":{"info":{"syntheticOwner":"B"}}}),
                );
                account_b.notify(new_update.clone());
                let mut queued = Vec::new();
                while let Ok(text) = sent.try_recv() {
                    queued.push(text);
                }
                assert!(
                    !queued.contains(&old_update),
                    "A output survived replacement: {queued:?}"
                );
                assert!(queued.contains(&new_update), "B output must still reach B");
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn deferred_read_reply_drops_replaced_account_owner() {
        tokio::task::LocalSet::new().run_until(async {
            let registry = crate::accounts::tests::registry();
            let first = acct_init();
            let (account_a, inbox) = crate::accounts::testing::account_on(
                crate::accounts::testing::HeldLoad {
                    placed: Arc::default(), load_after_ms: None, ticker: false,
                },
                ("u", "acct", "bybit"),
                Some(safe_cex_remote_signer::SignerLink::new(0, Arc::new(|_| false))),
                first.fingerprint(), Arc::default(),
            );
            drop(inbox);
            crate::accounts::testing::register(&registry, &account_a);
            let (mut session, mut sent) = user_session(registry.clone(), "u");
            session.init(first.clone()).await.unwrap();
            let permits = account_a.frame_reads.available_permits();
            session.call("acct", json!({"requestId":"old-owner-read", "method":"fetchOrders", "args":[]})).await;
            assert_eq!(account_a.frame_reads.available_permits(), permits - 1, "A's detached read owns its permit while startup is held");
            tokio::task::yield_now().await;
            let second = InitPayload { key: "owner-B".into(), ..first };
            let account_b = crate::accounts::testing::trading_account(&registry, "u", &second);
            session.init(second).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &account_b));
            session.call("acct", json!({"requestId":"current-owner-read", "method":"fetchOrders", "args":[]})).await;
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            assert_eq!(account_a.frame_reads.available_permits(), permits, "A's read settled after replacement");
            let mut replies = Vec::new();
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                if frame["d"]["op"] == "callMethod:done" { replies.push(frame["d"]["data"].clone()); }
            }
            assert!(!replies.iter().any(|reply| reply["requestId"] == "old-owner-read"), "stale read leaked: {replies:?}");
            assert!(replies.iter().any(|reply| reply["requestId"] == "current-owner-read" && reply["success"] == true), "current owner read control: {replies:?}");
        }).await;
    }

    #[tokio::test(start_paused = true)]
    async fn retained_stop_callback_drops_replaced_account_owner() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for replaced in [false, true] {
                    let registry = crate::accounts::tests::registry();
                    let first = acct_init();
                    let account_a =
                        crate::accounts::testing::trading_account(&registry, "u", &first);
                    let (mut owner, mut owner_sent) = user_session(registry.clone(), "u");
                    owner.init(first.clone()).await.unwrap();
                    while owner_sent.try_recv().is_ok() {}
                    let keep = registry.keep_notice(&account_a, &owner.resume_id);
                    let old_end =
                        envelope::msg("acct", "chaser:end", json!({"id":"synthetic-owner-A"}));
                    let mut kept: crate::algos::Kept = "synthetic retained owner A".into();
                    kept.ends.push(old_end.clone());
                    let (release, wait) = tokio::sync::oneshot::channel();
                    let callback = tokio::spawn(async move {
                        wait.await.unwrap();
                        keep(kept);
                    });
                    let current = if replaced {
                        let second = InitPayload {
                            key: "synthetic-owner-B".into(),
                            ..first
                        };
                        let account_b =
                            crate::accounts::testing::trading_account(&registry, "u", &second);
                        owner.init(second.clone()).await.unwrap();
                        assert!(Arc::ptr_eq(&owner.joined["acct"].account, &account_b));
                        (registry.keep_notice(&account_b, &owner.resume_id))(
                            "synthetic current owner B".into(),
                        );
                        second
                    } else {
                        first
                    };
                    release.send(()).unwrap();
                    callback.await.unwrap();
                    let (mut next, mut sent) = user_session(registry.clone(), "u");
                    next.init(current.clone()).await.unwrap();
                    let mut frames = Vec::new();
                    while let Ok(frame) = sent.try_recv() {
                        frames.push(frame);
                    }
                    let old_notices = frames
                        .iter()
                        .filter(|frame| frame.contains("synthetic retained owner A"))
                        .count();
                    assert_eq!(
                        old_notices,
                        usize::from(!replaced),
                        "retained A notice crossed replacement={replaced}: {frames:?}"
                    );
                    assert_eq!(
                        frames.iter().filter(|frame| **frame == old_end).count(),
                        usize::from(!replaced)
                    );
                    assert_eq!(
                        frames
                            .iter()
                            .filter(|frame| frame.contains("synthetic current owner B"))
                            .count(),
                        usize::from(replaced),
                        "late A callback must not overwrite B's retained notice"
                    );
                    let (mut again, mut sent) = user_session(registry.clone(), "u");
                    again.init(current).await.unwrap();
                    while let Ok(frame) = sent.try_recv() {
                        assert!(
                            !frame.contains("synthetic retained owner A")
                                && !frame.contains("synthetic current owner B"),
                            "retained notice replayed twice: {frame}"
                        );
                        assert_ne!(frame, old_end);
                    }
                }
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn retained_stop_notice_survives_replacement_registry_gap() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for unregister_b in [false, true] {
                    let registry = crate::accounts::tests::registry();
                    let first = acct_init();
                    let account_a =
                        crate::accounts::testing::trading_account(&registry, "u", &first);
                    let (mut owner, mut owner_sent) = user_session(registry.clone(), "u");
                    owner.init(first.clone()).await.unwrap();
                    while owner_sent.try_recv().is_ok() {}
                    let keep_a = registry.keep_notice(&account_a, &owner.resume_id);
                    let old_end = envelope::msg("acct", "chaser:end", json!({"id":"gap-owner-A"}));
                    let mut kept_a: crate::algos::Kept = "gap retired owner A".into();
                    kept_a.ends.push(old_end.clone());
                    let (release, wait) = tokio::sync::oneshot::channel();
                    let callback = tokio::spawn(async move {
                        wait.await.unwrap();
                        keep_a(kept_a);
                    });

                    let second = InitPayload {
                        key: "gap-owner-B".into(),
                        ..first
                    };
                    let account_b =
                        crate::accounts::testing::trading_account(&registry, "u", &second);
                    owner.init(second.clone()).await.unwrap();
                    assert!(Arc::ptr_eq(&owner.joined["acct"].account, &account_b));
                    let current_end =
                        envelope::msg("acct", "chaser:end", json!({"id":"gap-owner-B"}));
                    let mut kept_b: crate::algos::Kept = "gap owed owner B".into();
                    kept_b.ends.push(current_end.clone());
                    (registry.keep_notice(&account_b, &owner.resume_id))(kept_b);
                    if unregister_b {
                        owner
                            .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                            .await
                            .unwrap();
                        assert!(!owner.joined.contains_key("acct"));
                    }

                    release.send(()).unwrap();
                    callback.await.unwrap();
                    if unregister_b {
                        let rebuilt =
                            crate::accounts::testing::trading_account(&registry, "u", &second);
                        assert!(!Arc::ptr_eq(&account_b, &rebuilt));
                    }
                    let (mut next, mut sent) = user_session(registry.clone(), "u");
                    next.init(second.clone()).await.unwrap();
                    let mut frames = Vec::new();
                    while let Ok(frame) = sent.try_recv() {
                        assert!(!frame.contains("gap retired owner A"));
                        assert_ne!(frame, old_end);
                        frames.push(frame);
                    }
                    assert_eq!(
                        frames
                            .iter()
                            .filter(|frame| frame.contains("gap owed owner B"))
                            .count(),
                        1,
                        "B's owed notice must survive unregister_b={unregister_b}: {frames:?}"
                    );
                    assert_eq!(
                        frames.iter().filter(|frame| **frame == current_end).count(),
                        1
                    );

                    let (mut again, mut sent) = user_session(registry.clone(), "u");
                    again.init(second).await.unwrap();
                    while let Ok(frame) = sent.try_recv() {
                        assert!(!frame.contains("gap owed owner B"));
                        assert!(!frame.contains("gap retired owner A"));
                        assert_ne!(frame, current_end);
                        assert_ne!(frame, old_end);
                    }
                }
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn retained_stop_replay_requires_same_account_fingerprint() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for same_fingerprint in [true, false] {
                    let registry = crate::accounts::tests::registry();
                    let first = acct_init();
                    let account_a =
                        crate::accounts::testing::trading_account(&registry, "u", &first);
                    let (mut owner, mut sent) = user_session(registry.clone(), "u");
                    owner.init(first.clone()).await.unwrap();
                    while sent.try_recv().is_ok() {}
                    let keep = registry.keep_notice(&account_a, &owner.resume_id);
                    registry.dispose(&account_a, Disposal::NoBrowser);
                    let old_end =
                        envelope::msg("acct", "chaser:end", json!({"id":"retained-owner-A"}));
                    let mut kept: crate::algos::Kept = "retained before replacement".into();
                    kept.ends.push(old_end.clone());
                    keep(kept);
                    let current = if same_fingerprint {
                        first
                    } else {
                        InitPayload {
                            key: "different-owner-B".into(),
                            ..first
                        }
                    };
                    let rebuilt =
                        crate::accounts::testing::trading_account(&registry, "u", &current);
                    assert!(!Arc::ptr_eq(&account_a, &rebuilt));
                    owner.identity.resume_from = Some(owner.resume_id.clone());
                    owner.init(current.clone()).await.unwrap();
                    assert!(Arc::ptr_eq(&owner.joined["acct"].account, &rebuilt));
                    let mut frames = Vec::new();
                    while let Ok(frame) = sent.try_recv() {
                        frames.push(frame);
                    }
                    assert_eq!(
                        frames
                            .iter()
                            .filter(|frame| frame.contains("retained before replacement"))
                            .count(),
                        usize::from(same_fingerprint),
                        "retained replay must follow its source fingerprint: {frames:?}",
                    );
                    assert_eq!(
                        frames.iter().filter(|frame| **frame == old_end).count(),
                        usize::from(same_fingerprint)
                    );
                    let (mut next, mut sent) = user_session(registry.clone(), "u");
                    next.init(current).await.unwrap();
                    while let Ok(frame) = sent.try_recv() {
                        assert!(!frame.contains("retained before replacement"));
                        assert_ne!(frame, old_end);
                    }
                }
            })
            .await;
    }

    async fn callback_snapshot(sent: &mut crate::out::Receiver) -> (u64, bool) {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                let text = sent.recv().await.expect("account output remains open");
                let frame: Value = serde_json::from_str(&text).unwrap();
                if frame["d"]["op"] == "frame" && frame["d"]["snapshot"] == true {
                    assert_eq!(frame["d"]["accountName"], "acct");
                    return (
                        frame["d"]["seq"].as_u64().unwrap(),
                        frame["d"]["snapshot"].as_bool().unwrap(),
                    );
                }
            }
        })
        .await
        .expect("account snapshot arrives")
    }

    async fn callback_account_loaded(account: &Account) {
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while !account_loaded(&account.handle).await {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("control account loads before its first snapshot");
    }

    async fn queued_callback_keeps_its_original_account(held_load: bool) {
        let registry = crate::accounts::tests::registry();
        let first = InitPayload {
            key: "synthetic-callback-owner-A".into(),
            ..acct_init()
        };
        let account_a = if held_load {
            let (account, inbox) = crate::accounts::testing::account_on(
                crate::accounts::testing::HeldLoad {
                    placed: Arc::default(),
                    load_after_ms: None,
                    ticker: false,
                },
                ("u", "acct", "bybitinverse"),
                Some(safe_cex_remote_signer::SignerLink::new(
                    0,
                    Arc::new(|_| false),
                )),
                first.fingerprint(),
                Arc::default(),
            );
            drop(inbox);
            crate::accounts::tests::register(&registry, &account);
            account
        } else {
            crate::accounts::testing::trading_account(&registry, "u", &first)
        };
        if !held_load {
            callback_account_loaded(&account_a).await;
        }
        let (mut session, mut sent) = user_session(registry.clone(), "u");
        let mut callbacks = if held_load {
            session.take_loaded()
        } else {
            session.take_dropped()
        };
        session.init(first.clone()).await.unwrap();
        let client_a = session.joined["acct"].client.id;
        if held_load {
            let mut hold = Some(Hold {
                client: client_a,
                loaded: session.loaded.clone(),
            });
            // Exercise the real timeout-release producer without a 30-second sleep.
            release_hold(
                &account_a.handle,
                "acct",
                &mut hold,
                std::time::Instant::now() - SNAPSHOT_HOLD_LIMIT,
            )
            .await;
            assert!(hold.is_none());
        } else {
            assert_eq!(callback_snapshot(&mut sent).await, (1, true));
            account_a.handle.leave(client_a);
        }
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            while callbacks.is_empty() {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("A's callback is queued before changing owner");

        let second = InitPayload {
            key: "synthetic-callback-owner-B".into(),
            ..first
        };
        let account_b = crate::accounts::testing::trading_account(&registry, "u", &second);
        callback_account_loaded(&account_b).await;
        session.init(second).await.unwrap();
        assert!(Arc::ptr_eq(&session.joined["acct"].account, &account_b));
        let client_b = session.joined["acct"].client.id;
        assert_ne!(client_a, client_b);
        assert_eq!(callback_snapshot(&mut sent).await, (1, true));
        session
            .msg(AccountMsg {
                op: "subscribeOHLCV".into(),
                data: json!({"symbol": "BTCUSDT", "interval": "1m"}),
                account_name: Some("acct".into()),
            })
            .await;
        let subscriptions = session.joined["acct"].subscriptions.clone();
        assert!(!subscriptions.is_empty());
        let old_clients = session.joined["acct"].old.len();
        let next_command = session.joined["acct"].next_command;

        let stale = callbacks
            .try_recv()
            .expect("A remained queued during B's init");
        assert_eq!(stale, ("acct".into(), client_a));
        if held_load {
            session.account_loaded(&stale.0, stale.1).await;
        } else {
            session.client_dropped(&stale.0, stale.1).await;
        }
        let current = &session.joined["acct"];
        assert!(Arc::ptr_eq(&current.account, &account_b));
        assert_eq!(current.client.id, client_b, "A must not re-snapshot B");
        assert_eq!(current.old.len(), old_clients);
        assert_eq!(current.next_command, next_command);
        assert_eq!(current.subscriptions, subscriptions);
        assert!(!session.rejoined_at.contains_key("acct"));
        while let Ok(text) = sent.try_recv() {
            let frame: Value = serde_json::from_str(&text).unwrap();
            if frame["d"]["op"] == "frame" {
                assert_eq!(frame["d"]["snapshot"], false, "no B sequence reset");
                assert!(frame["d"]["seq"].as_u64().unwrap() > 1);
            }
        }

        if held_load {
            let mut hold = Some(Hold {
                client: client_b,
                loaded: session.loaded.clone(),
            });
            release_hold(
                &account_b.handle,
                "acct",
                &mut hold,
                std::time::Instant::now() - SNAPSHOT_HOLD_LIMIT,
            )
            .await;
        } else {
            account_b.handle.leave(client_b);
        }
        let current = tokio::time::timeout(std::time::Duration::from_secs(3), callbacks.recv())
            .await
            .expect("B's callback is produced")
            .unwrap();
        assert_eq!(current, ("acct".into(), client_b));
        if held_load {
            session.account_loaded(&current.0, current.1).await;
        } else {
            session.client_dropped(&current.0, current.1).await;
            assert!(session.rejoined_at.contains_key("acct"));
        }
        assert_ne!(session.joined["acct"].client.id, client_b);
        assert_eq!(session.joined["acct"].subscriptions, subscriptions);
        assert_eq!(callback_snapshot(&mut sent).await, (1, true));
        session.close();
    }

    #[tokio::test]
    async fn a_queued_held_load_callback_cannot_rejoin_a_replacement_owner() {
        tokio::task::LocalSet::new()
            .run_until(queued_callback_keeps_its_original_account(true))
            .await;
    }

    #[tokio::test]
    async fn a_queued_dropped_client_callback_cannot_rejoin_a_replacement_owner() {
        tokio::task::LocalSet::new()
            .run_until(queued_callback_keeps_its_original_account(false))
            .await;
    }
    /// User u's account under `registry`, with a chase running and a socket of u joined to it.
    async fn chasing_session(
        registry: &Arc<Registry>,
    ) -> (Arc<Account>, Session, crate::out::Receiver) {
        let init = acct_init();
        let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
        let account =
            crate::accounts::tests::held_account_counted(registry, link, init.fingerprint());
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let (mut session, sent) = user_session(registry.clone(), "u");
        assert!(session.init(init).await.is_ok());
        algo_op(&account, "chaser:start", WEB_CHASE);
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert_eq!(registry.running_algos(), (1, 0), "the chase runs");
        (account, session, sent)
    }

    /// A key or routing edit rebuilds the account: the old one's algos stop on purpose,
    /// with their finals to this page, never left repricing for a page told they stopped.
    #[tokio::test(start_paused = true)]
    async fn a_rebuild_stops_old_algos_and_drops_their_late_output() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let (account, mut session, mut sent) = chasing_session(&registry).await;
                let mut chase = Value::Null;
                while let Ok(frame) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    if frame["d"]["op"] == "store"
                        && let Some(row) = frame["d"]["data"]["activeChasers"].get(0)
                    {
                        chase = row["id"].clone();
                    }
                }
                assert!(chase.is_string(), "{chase}");
                // The helper's stand-in socket: the page's own is the only holder.
                registry.release(&account, 7, Disposal::PageDisposed);
                let edited = InitPayload {
                    key: "k2".into(),
                    ..acct_init()
                };
                let replacement =
                    crate::accounts::testing::trading_account(&registry, "u", &edited);
                let _ = session.init(edited).await;
                assert!(Arc::ptr_eq(&session.joined["acct"].account, &replacement));
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                assert_eq!(account.running.counts(), (0, 0), "the old chase stopped");
                let mut told = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    if frame["d"]["op"] == "error" && frame["d"]["data"][1]["algo"]["id"] == chase {
                        told.push(frame["d"]["data"][0].clone());
                    }
                }
                assert_eq!(
                    told.len(),
                    0,
                    "old chase output is dropped after replacement: {told:?}"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn review_3086_crash_before_drain_close_is_not_a_planned_move() {
        tokio::task::LocalSet::new().run_until(async {
            for mode in ["user", "box"] {
                let registry = crate::accounts::tests::registry();
                let init = acct_init();
                let account = crate::accounts::testing::trading_account(&registry, "u", &init);
                let (mut original, _frames) = user_session(registry.clone(), "u");
                assert!(original.init(init).await.is_ok());
                account.handle.dispose();
                tokio::time::timeout(std::time::Duration::from_secs(2), async {
                    while account.handle.join(99_998, true).await.is_ok() {
                        account.handle.leave(99_998);
                        tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    }
                }).await.expect("actor is already dead before any drain disposal");
                assert!(!account.moving());
                if mode == "user" {
                    registry.start_user_drain("u", crate::user_drain::DEFAULT_CAP_MS);
                } else {
                    registry.start_drain();
                }
                assert_eq!(original.drain_close(), Some(Close::Moved));
                original.close();
                let (mut late, mut frames) = user_session(registry, "u");
                late.join(account, late.out.clone()).await;
                let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
                assert_eq!(frame["d"]["data"], json!(["account stopped"]),
                    "{mode}: closing a draining session must not relabel an earlier actor crash: {frame}");
            }
        }).await;
    }

    #[tokio::test]
    async fn planned_move_stop_errors_carry_the_reason_but_real_actor_stops_do_not() {
        tokio::task::LocalSet::new()
            .run_until(async {
                for mode in ["user", "box", "crash", "crash_during_drain"] {
                    let registry = crate::accounts::tests::registry();
                    let init = acct_init();
                    let account = crate::accounts::testing::trading_account(&registry, "u", &init);
                    let (mut session, _frames) = user_session(registry.clone(), "u");
                    assert!(session.init(init).await.is_ok());
                    match mode {
                        "user" => {
                            registry.start_user_drain("u", crate::user_drain::DEFAULT_CAP_MS);
                        }
                        "box" => registry.start_drain(),
                        "crash_during_drain" => {
                            registry.start_user_drain("u", crate::user_drain::DEFAULT_CAP_MS);
                            account.handle.dispose();
                        }
                        _ => account.handle.dispose(),
                    }
                    if matches!(mode, "user" | "box") {
                        assert_eq!(session.drain_close(), Some(Close::Moved));
                        session.close();
                    }
                    tokio::time::timeout(std::time::Duration::from_secs(2), async {
                        while account.handle.join(99_999, true).await.is_ok() {
                            account.handle.leave(99_999);
                            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                        }
                    })
                    .await
                    .expect("the account really ended");
                    let (mut late, mut frames) = user_session(registry, "u");
                    late.join(account, late.out.clone()).await;
                    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
                    assert_eq!(frame["d"]["op"], "error");
                    assert_eq!(frame["d"]["data"][0], "account stopped");
                    if matches!(mode, "user" | "box") {
                        assert_eq!(frame["d"]["data"][1]["reason"], "moving", "{mode}: {frame}");
                    } else {
                        assert_eq!(
                            frame["d"]["data"],
                            json!(["account stopped"]),
                            "{mode}: {frame}"
                        );
                    }
                }
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_drain_moves_an_idle_socket_at_once() {
        let registry = crate::accounts::tests::registry();
        let (mut idle, _sent) = user_session(registry.clone(), "u");
        assert_eq!(idle.drain_close(), None, "nothing moves before a drain");
        registry.start_drain();
        assert_eq!(idle.drain_close(), Some(Close::Moved));
        assert_eq!(Close::Moved.code(), 4100);
        assert_eq!(Close::Moved.reason(), "draining: reconnect");
    }

    #[tokio::test(start_paused = true)]
    async fn a_drained_socket_running_a_chase_stays_until_the_chase_ends_then_moves() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let (account, mut session, _sent) = chasing_session(&registry).await;
                let mut ended = registry.watch_algos_ended();
                registry.start_drain();
                assert_eq!(session.drain_close(), None, "kept while its chase runs");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                assert_eq!(
                    session.drain_close(),
                    None,
                    "the drain never stops the chase"
                );
                assert_eq!(registry.running_algos(), (1, 0));
                algo_op(&account, "chaser:terminateAll", "{}");
                tokio::time::timeout(std::time::Duration::from_secs(60), ended.changed())
                    .await
                    .expect("told as the chase ends")
                    .unwrap();
                assert_eq!(registry.running_algos(), (0, 0));
                assert_eq!(session.drain_close(), Some(Close::Moved));
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_drain_refuses_users_new_here_and_admits_a_live_users_second_socket() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let (_account, _first, _sent) = chasing_session(&registry).await;
                registry.start_drain();
                let (mut stranger, _) = user_session(registry.clone(), "v");
                assert_eq!(stranger.init(acct_init()).await, Err(Close::Moved));
                assert!(!stranger.joined.contains_key("acct"));
                let (mut second_tab, _) = user_session(registry.clone(), "u");
                assert_eq!(second_tab.init(acct_init()).await, Ok(()));
                assert!(second_tab.joined.contains_key("acct"));
                assert_eq!(second_tab.drain_close(), None, "its account runs a chase");
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn break_glass_tells_the_page_what_stopped_before_it_moves_every_socket() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let (_account, mut session, mut sent) = chasing_session(&registry).await;
                while sent.try_recv().is_ok() {}
                let mut drain = registry.watch_drain();
                // What the page had been sent when every socket was told to move.
                let before_move = tokio::task::spawn_local(async move {
                    while *drain.borrow_and_update() != crate::accounts::Drain::Now {
                        drain.changed().await.unwrap();
                    }
                    let mut frames = Vec::new();
                    while let Ok(frame) = sent.try_recv() {
                        frames.push(frame);
                    }
                    frames
                });
                assert!(
                    registry
                        .break_glass(std::time::Duration::from_secs(25))
                        .await
                );
                let frames = before_move.await.unwrap();
                assert!(
                    frames.iter().any(|frame| frame.contains(r#""op":"error""#)
                        && frame.contains("Chase on BTCUSDT stopped")),
                    "the chase's own final notice first: {frames:?}"
                );
                assert_eq!(registry.running_algos(), (0, 0));
                assert_eq!(session.drain_close(), Some(Close::Moved));
            })
            .await;
    }

    #[test]
    fn metadata_refreshes_wait_in_their_own_pool() {
        let reads = Arc::new(tokio::sync::Semaphore::new(0));
        let refreshes = Arc::new(tokio::sync::Semaphore::new(1));
        assert!(
            after_load_pool("updateSymbolMetadata", &reads, &refreshes)
                .clone()
                .try_acquire_owned()
                .is_ok()
        );
        assert!(
            after_load_pool("getAccount", &reads, &refreshes)
                .clone()
                .try_acquire_owned()
                .is_err()
        );
    }

    fn chart_subscribe(op: &str) -> AccountMsg {
        AccountMsg {
            op: op.into(),
            data: json!({"symbol": "BTCUSDT", "interval": "1"}),
            account_name: Some("acct".into()),
        }
    }

    fn early_session(registry: Arc<Registry>) -> Session {
        let (out, _sent, _) = Out::channel(64);
        let identity = Identity {
            user: Some("u".into()),
            chat_handle: None,
            token: Some(("t".into(), u64::MAX)),
            resume_from: None,
            resumable: false,
        };
        Session::new(registry, Arc::new(Metrics::default()), identity, out)
    }

    /// A chart's subscribe that reached the server before its account joined was
    /// dropped (no joined account for op subscribeOHLCV), and the chart froze.
    #[tokio::test]
    async fn a_chart_subscribe_before_join_is_applied_at_join() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let joined_then = |early: bool| async move {
                    let registry = crate::accounts::tests::registry();
                    let init = InitPayload {
                        account_name: "acct".into(),
                        exchange_name: "bybitinverse".into(),
                        key: "k".into(),
                        ..InitPayload::default()
                    };
                    let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                    let account = crate::accounts::tests::held_account_running_algos(
                        link,
                        init.fingerprint(),
                    );
                    crate::accounts::tests::register(&registry, &account);
                    settle().await;
                    let (mut session, _sent) = sign_silent_session(registry);
                    if early {
                        session.msg(chart_subscribe("subscribeOHLCV")).await;
                        assert!(!session.joined.contains_key("acct"));
                        assert_eq!(session.early["acct"].len(), 1);
                    }
                    assert!(session.init(init).await.is_ok());
                    settle().await;
                    if !early {
                        session.msg(chart_subscribe("subscribeOHLCV")).await;
                    }
                    assert!(!session.early.contains_key("acct"));
                    let mut held: Vec<_> = session.joined["acct"]
                        .subscriptions
                        .clone()
                        .into_iter()
                        .collect();
                    held.sort();
                    held
                };
                // Before join it was dropped; now it lands exactly as one sent after join.
                assert_eq!(joined_then(true).await, joined_then(false).await);
            })
            .await;
    }

    /// A page whose client queue filled was removed by the engine with no log and
    /// no frames; the session now says so and gives it a fresh client and snapshot.
    #[tokio::test]
    async fn a_client_the_engine_drops_is_logged_and_re_snapshotted() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account =
                    crate::accounts::tests::held_account_running_algos(link, init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                let (mut session, _sent) = sign_silent_session(registry);
                let mut dropped = session.take_dropped();
                assert!(session.init(init).await.is_ok());
                settle().await;
                let first = session.joined["acct"].client.id;
                // As a full queue does: the engine side removes the peer, the session did not ask.
                account.handle.leave(first);
                let (name, client) =
                    tokio::time::timeout(std::time::Duration::from_secs(3), dropped.recv())
                        .await
                        .expect("the drop is reported")
                        .unwrap();
                assert_eq!((name.as_str(), client), ("acct", first));
                session.client_dropped(&name, client).await;
                assert_ne!(session.joined["acct"].client.id, first);
                // A second drop within the window is logged, not re-snapshotted again.
                let second = session.joined["acct"].client.id;
                session.client_dropped("acct", second).await;
                assert_eq!(session.joined["acct"].client.id, second);
            })
            .await;
    }

    /// A chart's forced reset after its own unsubscribe repairs the stream it holds again,
    /// once per window per account and topic.
    #[tokio::test]
    async fn a_reset_subscribe_repairs_its_held_topic_once_per_window() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account =
                    crate::accounts::tests::held_account_running_algos(link, init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                let (mut session, _sent) = sign_silent_session(registry.clone());
                assert!(session.init(init).await.is_ok());
                settle().await;
                let candle = |op: &str, reset: bool| AccountMsg {
                    op: op.into(),
                    data: json!({"symbol": "BTCUSDT", "interval": "1m", "reset": reset}),
                    account_name: Some("acct".into()),
                };
                let key = json!({"op": "subscribeCandles", "symbol": "BTCUSDT", "interval": "1m"})
                    .to_string();
                // A probe records a repair when it allows one: ask once, after the reset.
                let recorded = |key: &str| {
                    !registry.allow_repair(
                        &account.user,
                        &account.name,
                        key,
                        std::time::Instant::now(),
                    )
                };

                session.msg(candle("subscribeOHLCV", false)).await;
                session.msg(candle("unsubscribeOHLCV", true)).await;
                assert!(session.joined["acct"].subscriptions.is_empty());

                session.msg(candle("subscribeOHLCV", true)).await;
                assert!(recorded(&key), "the reset of a held topic was repaired");
                let later = std::time::Instant::now() + crate::accounts::REPAIR_EVERY;
                assert!(registry.allow_repair(&account.user, &account.name, &key, later));
                assert!(
                    registry.allow_repair(&account.user, "other", &key, std::time::Instant::now()),
                    "another account's same topic has its own window"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn an_early_subscribe_whose_account_never_joins_is_dropped_not_kept() {
        let mut session = early_session(crate::accounts::tests::registry());
        session.msg(chart_subscribe("subscribeOHLCV")).await;
        session.msg(chart_subscribe("unsubscribeOHLCV")).await;
        assert_eq!(session.early["acct"].len(), 2);
        session.drop_early("acct", "init refused");
        assert!(!session.early.contains_key("acct"));
        for _ in 0..(EARLY_TOPICS + 5) {
            session.msg(chart_subscribe("subscribeOHLCV")).await;
        }
        assert_eq!(session.early["acct"].len(), EARLY_TOPICS);
    }

    /// On the wire: an algo error raised during a drop, then the resume. The op is
    /// a chase the book refuses; every algo error takes this one `Port::emit` path.
    #[tokio::test]
    async fn an_algo_error_during_a_drop_reaches_the_resuming_socket_once() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let refuse = |account: &Account| {
                    let algos = account.algos.as_ref().unwrap();
                    algos
                        .send(crate::algos::AlgoInput::Op {
                            op: "chaser:start".into(),
                            data: "not a chase".into(),
                            origin: "test".into(),
                            twap_owner: account.twap_owner.snapshot(),
                            action: None,
                        })
                        .unwrap();
                };
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_running_algos(
                    link.clone(),
                    init.fingerprint(),
                );
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                // Delivered and answered by a later frame: nothing to replay.
                refuse(&account);
                settle().await;
                link.heard(7);
                // Into a dead socket that still looks live, then parked: both unheard.
                refuse(&account);
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                refuse(&account);
                settle().await;

                let (out, mut sent, _) = Out::channel(64);
                let identity = Identity {
                    user: Some("u".into()),
                    chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some("r1".into()),
                    resumable: true,
                };
                let mut session =
                    Session::new(registry, Arc::new(Metrics::default()), identity, out);
                session.open().await;
                settle().await;
                let mut errors = Vec::new();
                let mut resumed = false;
                while let Ok(frame) = sent.try_recv() {
                    assert!(frame.starts_with(r#"{"t":"#), "t-first: {frame}");
                    let message: Value = serde_json::from_str(&frame).unwrap();
                    resumed |= message["t"] == "session" && message["d"]["resumed"] == true;
                    if message["d"]["op"] == "error" {
                        assert_eq!(message["d"]["accountName"], "acct");
                        errors.push(message["d"]["data"].clone());
                    }
                }
                assert!(resumed);
                assert_eq!(
                    errors.len(),
                    2,
                    "the two unheard errors, once each: {errors:?}"
                );
                assert!(
                    errors
                        .iter()
                        .all(|data| data[0].is_string() && data[1]["forceToast"] == true),
                    "shown even with the reconnect still settling: {errors:?}"
                );

                // Once a frame shows this socket got them, a re-join has nothing to say.
                session.heard();
                let joined = || {
                    account
                        .algos
                        .as_ref()
                        .unwrap()
                        .send(crate::algos::AlgoInput::Joined)
                        .unwrap();
                };
                joined();
                settle().await;
                while let Ok(frame) = sent.try_recv() {
                    assert!(!frame.contains(r#""op":"error""#), "{frame}");
                }
                // An error answered on this socket stays answered on a same-socket re-join.
                refuse(&account);
                settle().await;
                session.heard();
                while sent.try_recv().is_ok() {}
                joined();
                settle().await;
                while let Ok(frame) = sent.try_recv() {
                    assert!(!frame.contains(r#""op":"error""#), "stale repeat: {frame}");
                }
            })
            .await;
    }

    /// A stalled or failing signer ends here: the page must see the account gone,
    /// so its existing re-init runs, and a call in the gap must be answered.
    #[tokio::test(start_paused = true)]
    async fn a_disposed_account_leaves_the_pong_and_refuses_calls_by_name() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_running_algos(
                    link,
                    init.fingerprint(),
                );
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                let (out, mut sent, _) = Out::channel(64);
                let identity = Identity {
                    user: Some("u".into()),
            chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some("r1".into()),
                    resumable: true,
                };
                let mut session =
                    Session::new(registry.clone(), Arc::new(Metrics::default()), identity, out);
                session.open().await;
                settle().await;
                session.handle(r#"{"t":"ping"}"#).await.ok();
                settle().await;
                let mut frames = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    frames.push(frame);
                }
                assert!(
                    frames.iter().any(|f| f.contains(r#"{"t":"pong""#) && f.contains("acct")),
                    "held and listed: {frames:?}"
                );

                registry.dispose(&account, Disposal::SettingsChanged);
                settle().await;
                session.handle(r#"{"t":"ping"}"#).await.ok();
                session
                    .handle(
                        r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"n1","method":"nuke","args":[]}}}"#,
                    )
                    .await
                    .ok();
                let mut frames = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    frames.push(frame);
                }
                assert!(
                    frames
                        .iter()
                        .any(|f| f.contains(r#"{"t":"pong""#) && !f.contains("acct")),
                    "the page's re-init needs it missing: {frames:?}"
                );
                let reply = frames
                    .iter()
                    .find(|f| f.contains("callMethod:done"))
                    .expect("a call in the gap is answered");
                assert!(reply.contains("n1") && reply.contains("stopped"), "{reply}");
                assert!(reply.contains("notSent"), "{reply}");
            })
            .await;
    }

    #[tokio::test]
    async fn an_algo_op_on_an_unjoined_account_is_answered() {
        let registry = crate::accounts::tests::registry();
        let (out, mut sent, _) = Out::channel(64);
        let identity = Identity {
            user: Some("u".into()),
            chat_handle: None,
            token: Some(("t".into(), u64::MAX)),
            resume_from: None,
            resumable: true,
        };
        let mut session = Session::new(registry, Arc::new(Metrics::default()), identity, out);
        session
            .handle(r#"{"t":"msg","d":{"op":"chaser:start","accountName":"acct","data":{}}}"#)
            .await
            .ok();
        let mut frames = Vec::new();
        while let Ok(frame) = sent.try_recv() {
            frames.push(frame);
        }
        assert!(
            frames
                .iter()
                .any(|f| f.contains("chaser:start was not run") && f.contains("acct")),
            "{frames:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_repeated_init_moves_signing_with_the_holders_record() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account =
                    crate::accounts::tests::held_account_running_algos(link, init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                let (mut session, _sent) = sign_silent_session(registry);
                assert!(session.init(init.clone()).await.is_ok());
                settle().await;
                crate::accounts::tests::hold_live(&account, u64::MAX - 1, Arc::new(|_| true));
                assert!(session.init(init).await.is_ok());
                settle().await;
                assert_eq!(
                    crate::accounts::tests::signing_holder(&account),
                    Some(session.socket)
                );
            })
            .await;
    }

    fn sign_silent_session(registry: Arc<Registry>) -> (Session, crate::out::Receiver) {
        let (out, sent, _) = Out::channel(64);
        let identity = Identity {
            user: Some("u".into()),
            chat_handle: None,
            token: Some(("t".into(), u64::MAX)),
            resume_from: Some("r1".into()),
            resumable: true,
        };
        let session = Session::new(registry, Arc::new(Metrics::default()), identity, out);
        (session, sent)
    }

    const NUKE_CALL: &str = r#"{"t":"msg","d":{"op":"callMethod","accountName":"acct","data":{"requestId":"n1","method":"nuke","args":[]}}}"#;

    /// Startup held and the signer silent, with a position the store cannot show yet.
    #[tokio::test(start_paused = true)]
    async fn a_nuke_during_load_says_it_waits_then_closes_what_it_can_see() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, placed) =
                    crate::accounts::tests::held_load_account(link, Some(5_000.));
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                let (mut session, mut sent) = sign_silent_session(registry);
                session.open().await;
                settle().await;
                while sent.try_recv().is_ok() {}

                session.handle(NUKE_CALL).await.ok();
                settle().await;
                let mut frames = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    frames.push(frame);
                }
                assert!(
                    frames
                        .iter()
                        .any(|f| f.contains("Nuke is waiting") && f.contains("forceToast")),
                    "told at once: {frames:?}"
                );
                assert!(
                    !frames.iter().any(|f| f.contains("callMethod:done")),
                    "no verdict before it has seen positions: {frames:?}"
                );

                assert_eq!(placed.load(std::sync::atomic::Ordering::SeqCst), 0);
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                assert_eq!(
                    placed.load(std::sync::atomic::Ordering::SeqCst),
                    1,
                    "the close goes out once positions are known"
                );
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn a_nuke_on_an_account_that_never_loads_fails_visibly_and_never_succeeds() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, placed) = crate::accounts::tests::held_load_account(link, None);
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                let (mut session, mut sent) = sign_silent_session(registry);
                session.open().await;
                settle().await;
                while sent.try_recv().is_ok() {}
                session.handle(NUKE_CALL).await.ok();
                tokio::time::sleep(methods::READ_WAIT + std::time::Duration::from_secs(1)).await;
                let mut frames = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    frames.push(frame);
                }
                let reply = frames
                    .iter()
                    .find(|f| f.contains("callMethod:done"))
                    .expect("answered within the bound");
                assert!(reply.contains(r#""success":false"#), "{reply}");
                assert!(reply.contains("did not load"), "{reply}");
                assert!(reply.contains("notSent"), "{reply}");
                assert_eq!(placed.load(std::sync::atomic::Ordering::SeqCst), 0);
            })
            .await;
    }

    /// A running chase's signs settle as a resume attaches, so a new error can land
    /// between the attach and the Joined: it must not cost the one from the gap.
    #[tokio::test]
    async fn an_error_between_the_attach_and_the_join_keeps_the_one_from_the_gap() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_running_algos(
                    link.clone(),
                    init.fingerprint(),
                );
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                let algos = account.algos.as_ref().unwrap();
                let refuse = || {
                    algos
                        .send(crate::algos::AlgoInput::Op {
                            op: "chaser:start".into(),
                            data: "not a chase".into(),
                            origin: "test".into(),
                            twap_owner: account.twap_owner.snapshot(),
                            action: None,
                        })
                        .unwrap();
                };
                refuse();
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                let frames = Arc::new(Mutex::new(Vec::<String>::new()));
                let record = frames.clone();
                let outbox: Outbox = Arc::new(move |frame| {
                    record.lock().unwrap().push(frame);
                    true
                });
                let resumed =
                    registry.resume("r1", "u", 8, ("t".into(), u64::MAX), &|_| outbox.clone());
                assert_eq!(resumed.len(), 1);
                refuse();
                settle().await;
                let errors = || {
                    frames
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|frame| frame.contains(r#""op":"error""#))
                        .count()
                };
                assert_eq!(errors(), 1, "the new error went straight to the new socket");
                algos.send(crate::algos::AlgoInput::Joined).unwrap();
                settle().await;
                assert_eq!(
                    errors(),
                    2,
                    "the gap's error too, and the new one not twice"
                );
            })
            .await;
    }

    /// A second device holds the account while the signing tab drops. It sees the
    /// error live, and its frames, or the hand-over to it, must not cost the tab its copy.
    #[tokio::test]
    async fn a_second_device_sees_the_error_and_the_dropped_tab_still_gets_it() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_running_algos(
                    link.clone(),
                    init.fingerprint(),
                );
                crate::accounts::tests::register(&registry, &account);
                let second = Arc::new(Mutex::new(Vec::<String>::new()));
                let record = second.clone();
                let outbox: Outbox = Arc::new(move |frame| {
                    record.lock().unwrap().push(frame);
                    true
                });
                crate::accounts::tests::hold_live(&account, 3, outbox);
                crate::accounts::tests::hold_live(&account, 7, Arc::new(|_| true));
                settle().await;
                let refuse = || {
                    account
                        .algos
                        .as_ref()
                        .unwrap()
                        .send(crate::algos::AlgoInput::Op {
                            op: "chaser:start".into(),
                            data: "not a chase".into(),
                            origin: "test".into(),
                            twap_owner: account.twap_owner.snapshot(),
                            action: None,
                        })
                        .unwrap();
                };
                let errors = |frames: &[String]| {
                    frames
                        .iter()
                        .filter(|frame| frame.contains(r#""op":"error""#))
                        .count()
                };
                // The tab is gone but looks live; the second device is fine.
                refuse();
                settle().await;
                link.heard(3);
                assert_eq!(
                    errors(&second.lock().unwrap()),
                    1,
                    "the second device, live"
                );
                // Its drop is seen: signing hands over, which attaches and marks it heard.
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                assert_eq!(crate::accounts::tests::signing(&account), Some(3));
                // Raised with the tab parked: only the second device can be told now.
                refuse();
                settle().await;
                link.heard(3);
                assert_eq!(errors(&second.lock().unwrap()), 2);

                let (out, mut sent, _) = Out::channel(64);
                let identity = Identity {
                    user: Some("u".into()),
                    chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some("r1".into()),
                    resumable: true,
                };
                let mut session =
                    Session::new(registry, Arc::new(Metrics::default()), identity, out);
                session.open().await;
                settle().await;
                let mut replayed = 0;
                while let Ok(frame) = sent.try_recv() {
                    replayed += usize::from(frame.contains(r#""op":"error""#));
                }
                assert_eq!(replayed, 2, "the tab gets both, once each");
                assert_eq!(
                    errors(&second.lock().unwrap()),
                    2,
                    "the second device is not told again"
                );
            })
            .await;
    }

    /// A flapping link: the replay goes into a socket that dies too, before any
    /// frame from it, so the next resume repeats it.
    #[tokio::test]
    async fn a_replay_into_a_socket_that_also_dies_is_repeated_on_the_next_resume() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let settle = || tokio::time::sleep(std::time::Duration::from_millis(200));
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account = crate::accounts::tests::held_account_running_algos(
                    link.clone(),
                    init.fingerprint(),
                );
                crate::accounts::tests::register(&registry, &account);
                settle().await;
                account
                    .algos
                    .as_ref()
                    .unwrap()
                    .send(crate::algos::AlgoInput::Op {
                        op: "chaser:start".into(),
                        data: "not a chase".into(),
                        origin: "test".into(),
                        twap_owner: account.twap_owner.snapshot(),
                        action: None,
                    })
                    .unwrap();
                settle().await;
                registry.park("r1".into(), "u", vec![(account.clone(), 7)]);
                let resume = |from: &str| Identity {
                    user: Some("u".into()),
                    chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some(from.into()),
                    resumable: true,
                };
                let errors = |sent: &mut crate::out::Receiver| {
                    let mut count = 0;
                    while let Ok(frame) = sent.try_recv() {
                        count += usize::from(frame.contains(r#""op":"error""#));
                    }
                    count
                };
                let (out, mut sent, _) = Out::channel(64);
                let mut first = Session::new(
                    registry.clone(),
                    Arc::new(Metrics::default()),
                    resume("r1"),
                    out,
                );
                first.open().await;
                settle().await;
                assert_eq!(errors(&mut sent), 1, "replayed to the first resume");
                let dead = first.resume_id.clone();
                first.close();
                let (out, mut sent, _) = Out::channel(64);
                let mut second =
                    Session::new(registry, Arc::new(Metrics::default()), resume(&dead), out);
                second.open().await;
                settle().await;
                assert_eq!(errors(&mut sent), 1, "no frame came back: replayed again");
            })
            .await;
    }

    /// A second socket's init after the dispose must not cost the device that
    /// owned the chase its notice, on the wire and on a fresh init after resume.
    #[tokio::test(start_paused = true)]
    async fn the_owner_is_told_of_stopped_algos_after_another_socket_inits() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, _algos) =
                    crate::accounts::tests::held_account_with(Some(link), init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                (registry.keep_notice(&account, "r1"))("Chase on BTCUSDT stopped".into());
                let wire = format!(
                    r#"{{"t":"init","d":{}}}"#,
                    serde_json::to_string(&init).unwrap()
                );
                let told = async |from: Option<&str>| {
                    let (out, mut sent, _) = Out::channel(64);
                    let identity = Identity {
                        user: Some("u".into()),
                        chat_handle: None,
                        token: Some(("t".into(), u64::MAX)),
                        resume_from: from.map(str::to_owned),
                        resumable: from.is_some(),
                    };
                    let mut session = Session::new(
                        registry.clone(),
                        Arc::new(Metrics::default()),
                        identity,
                        out,
                    );
                    session.open().await;
                    assert!(session.handle(&wire).await.is_ok());
                    let mut frames = Vec::new();
                    while let Ok(frame) = sent.try_recv() {
                        if frame.contains("Chase on BTCUSDT stopped") {
                            frames.push(frame);
                        }
                    }
                    frames
                };
                assert_eq!(told(None).await.len(), 1, "the other socket is told");
                let owner = told(Some("r1")).await;
                assert_eq!(owner.len(), 1, "and the owner still is");
                let frame: Value = serde_json::from_str(&owner[0]).unwrap();
                assert_eq!(frame["d"]["op"], "error");
                assert_eq!(frame["d"]["data"][1]["forceToast"], true);
                assert_eq!(told(Some("r1")).await.len(), 0, "once");
            })
            .await;
    }

    /// A chase that ended while no browser held the account reaches the next
    /// init as its replayed end, with no empty error beside it.
    #[tokio::test(start_paused = true)]
    async fn the_next_init_gets_the_end_of_a_chase_it_missed() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, _algos) =
                    crate::accounts::tests::held_account_with(Some(link), init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                let end = r#"{"t":"msg","d":{"op":"chaser:complete","data":{"id":"c1","filled":0.001,"replayed":true}}}"#;
                (registry.keep_notice(&account, "r1"))(crate::algos::Kept {
                    notices: Vec::new(),
                    ends: vec![end.to_owned()],
                });
                let (out, mut sent, _) = Out::channel(64);
                let identity = Identity {
                    user: Some("u".into()),
            chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some("r1".into()),
                    resumable: true,
                };
                let mut session =
                    Session::new(registry.clone(), Arc::new(Metrics::default()), identity, out);
                session.open().await;
                let wire = format!(
                    r#"{{"t":"init","d":{}}}"#,
                    serde_json::to_string(&init).unwrap()
                );
                assert!(session.handle(&wire).await.is_ok());
                let mut frames = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    frames.push(frame);
                }
                assert_eq!(frames.iter().filter(|frame| *frame == end).count(), 1, "{frames:?}");
                assert!(
                    !frames.iter().any(|frame| frame.contains(r#""op":"error""#)),
                    "{frames:?}"
                );
            })
            .await;
    }

    /// Grace expiry stops a chase and a TWAP with no browser to tell; the owner's
    /// next init is told once per algo, each notice naming its kind and the id
    /// its store row had, so the page can dedupe it against its own backstop.
    #[tokio::test(start_paused = true)]
    async fn the_next_init_is_told_each_stopped_algo_by_kind_and_id() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let (account, session, mut sent) = chasing_session(&registry).await;
                algo_op(&account, "twap:start", WEB_TWAP);
                tokio::time::sleep(std::time::Duration::from_millis(600)).await;
                assert_eq!(registry.running_algos(), (1, 1));
                let (mut chase, mut twap) = (Value::Null, Value::Null);
                while let Ok(frame) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    let rows = &frame["d"]["data"];
                    if frame["d"]["op"] == "store" {
                        if let Some(row) = rows["activeChasers"].get(0) {
                            chase = row["id"].clone();
                        }
                        if let Some(row) = rows["activeTWAPs"].get(0) {
                            twap = row["id"].clone();
                        }
                    }
                }
                assert!(chase.is_string() && twap.is_string(), "{chase} {twap}");
                // The helper's stand-in socket: the page's own is the only holder.
                registry.release(&account, 7, Disposal::PageDisposed);
                let owner = session.resume_id.clone();
                session.close();
                tokio::time::sleep(std::time::Duration::from_secs(80)).await;
                assert_eq!(
                    registry.running_algos(),
                    (0, 0),
                    "grace expiry stopped them"
                );
                // The rebuilt account the next init finds.
                let init = acct_init();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (rebuilt, _algos) =
                    crate::accounts::tests::held_account_with(Some(link), init.fingerprint());
                crate::accounts::tests::register(&registry, &rebuilt);
                let (out, mut sent, _) = Out::channel(256);
                let identity = Identity {
                    user: Some("u".into()),
                    chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: Some(owner),
                    resumable: true,
                };
                let mut next = Session::new(
                    registry.clone(),
                    Arc::new(Metrics::default()),
                    identity,
                    out,
                );
                next.open().await;
                let wire = format!(
                    r#"{{"t":"init","d":{}}}"#,
                    serde_json::to_string(&init).unwrap()
                );
                assert!(next.handle(&wire).await.is_ok());
                let mut notices = Vec::new();
                while let Ok(frame) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&frame).unwrap();
                    if frame["d"]["op"] == "error" {
                        notices.push(frame["d"]["data"].clone());
                    }
                }
                for (kind, id, text) in [
                    (
                        "chase",
                        &chase,
                        "Chase on BTCUSDT stopped: no browser for 60s.",
                    ),
                    (
                        "twap",
                        &twap,
                        "TWAP on BTCUSDT stopped: no browser for 60s.",
                    ),
                ] {
                    let algo = serde_json::json!({"kind": kind, "id": id});
                    let told: Vec<&Value> = notices
                        .iter()
                        .filter(|data| data[1]["algo"] == algo)
                        .collect();
                    assert_eq!(told.len(), 1, "{algo} told once: {notices:?}");
                    assert!(
                        told[0][0]
                            .as_str()
                            .is_some_and(|told| told.starts_with(text)),
                        "{told:?}"
                    );
                    assert_eq!(told[0][1]["forceToast"], true);
                }
            })
            .await;
    }

    /// On the wire: a second client's init with other engine settings, as the
    /// browser sends it, against an account another live socket holds.
    #[tokio::test(start_paused = true)]
    async fn a_refused_init_answers_one_t_first_actionable_error_frame() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "bybitinverse".into(),
                    key: "k".into(),
                    ..InitPayload::default()
                };
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let (account, _algos) =
                    crate::accounts::tests::held_account_with(Some(link), init.fingerprint());
                crate::accounts::tests::register(&registry, &account);
                let (out, mut sent, _) = Out::channel(64);
                let identity = Identity {
                    user: Some("u".into()),
            chat_handle: None,
                    token: Some(("t".into(), u64::MAX)),
                    resume_from: None,
                    resumable: false,
                };
                let mut session = Session::new(registry, Arc::new(Metrics::default()), identity, out);
                let other = InitPayload {
                    enable_spot_markets: Some(true),
                    ..init
                };
                let wire = format!(r#"{{"t":"init","d":{}}}"#, serde_json::to_string(&other).unwrap());
                for _ in 0..3 {
                    assert!(session.handle(&wire).await.is_ok());
                }
                let frame = sent.try_recv().expect("the refusal frame");
                assert_eq!(
                    frame,
                    r#"{"t":"msg","d":{"accountName":"acct","data":["acct is already running with different settings (enableSpotMarkets). The change was not applied. Join with the current settings and use apply-settings before reinitializing.",{"forceToast":true,"initRefused":true}],"op":"error"}}"#
                );
                assert!(sent.try_recv().is_err(), "retries are quiet");
                assert_eq!(crate::accounts::tests::signing(&account), Some(7));
            })
            .await;
    }

    #[test]
    fn a_startup_still_waiting_is_logged_with_what_it_waits_for() {
        let waiting = json!({"type": "event", "level": "error", "code": "startup.incomplete",
            "message": "Startup incomplete: still waiting for markets, tickers"});
        assert_eq!(
            startup_waiting_line("bbbbb", &waiting).as_deref(),
            Some("bbbbb Startup incomplete: still waiting for markets, tickers")
        );
        let retrying = json!({"type": "event", "level": "warn", "code": "startup.degraded",
            "message": "Public startup delayed; still retrying: transport request timed out"});
        assert_eq!(
            startup_waiting_line("bbbbb", &retrying).as_deref(),
            Some("bbbbb Public startup delayed; still retrying: transport request timed out")
        );
        let other = json!({"type": "event", "code": "order.rejected", "message": "x"});
        assert_eq!(startup_waiting_line("bbbbb", &other), None);
    }

    #[test]
    fn a_settings_refusal_is_sent_once_until_its_fields_change_or_it_resolves() {
        let mut refused = HashMap::new();
        let spot = "bear5 is open elsewhere with different settings (enableSpotMarkets)";
        assert!(first_refusal(&mut refused, "bear5", spot, None));
        for _ in 0..4 {
            assert!(
                !first_refusal(&mut refused, "bear5", spot, None),
                "a backoff retry is quiet"
            );
        }
        assert!(
            first_refusal(&mut refused, "other", spot, None),
            "per account"
        );
        let both = "bear5 is open elsewhere with different settings (enableSpotMarkets, testnet)";
        assert!(
            first_refusal(&mut refused, "bear5", both, None),
            "changed fields show again"
        );
        // A successful init clears it (Session::init), so a later refusal shows again.
        refused.remove("bear5");
        assert!(first_refusal(&mut refused, "bear5", both, None));
    }

    /// Every Hosted init logged engine dropped client once: while startup was held the
    /// pump awaited an engine read per frame, the engine was busy starting, and the
    /// 32-frame queue filled. Held frames must drain without waiting on the engine.
    #[tokio::test]
    async fn held_startup_frames_drain_without_waiting_on_the_engine() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let handle = crate::accounts::tests::idle_handle_on_own_thread();
                // The engine busy starting up: a turn that takes a while.
                let busy = handle.clone();
                tokio::task::spawn_local(async move {
                    let _ = busy
                        .read(|_| std::thread::sleep(std::time::Duration::from_millis(1_500)))
                        .await;
                });
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                let (frames, outgoing) = tokio::sync::mpsc::channel(32);
                let (_cancel, canceled) = tokio::sync::watch::channel(false);
                let (out, _sent, _) = Out::channel(64);
                let (loaded, _loaded_rx) = tokio::sync::mpsc::unbounded_channel();
                let (dropped, _dropped_rx) = tokio::sync::mpsc::unbounded_channel();
                tokio::task::spawn_local(pump(
                    safe_cex_native::ClientLink { outgoing, canceled },
                    1,
                    dropped,
                    handle,
                    "acct".into(),
                    out,
                    Arc::default(),
                    Arc::default(),
                    Some(Hold { client: 1, loaded }),
                ));
                let mut accepted = 0;
                for _ in 0..100 {
                    if frames
                        .try_send(safe_cex_native::Outgoing::Frame(Arc::from(&b"frame"[..])))
                        .is_ok()
                    {
                        accepted += 1;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(3)).await;
                }
                assert_eq!(accepted, 100, "the held pump stalled on the busy engine");
            })
            .await;
    }

    #[tokio::test]
    async fn update_positions_before_join_is_still_answered() {
        let (out, mut sent, _) = Out::channel(64);
        let identity = Identity {
            user: Some("u".into()),
            chat_handle: None,
            token: Some(("t".into(), u64::MAX)),
            resume_from: None,
            resumable: false,
        };
        let mut session = Session::new(
            crate::accounts::tests::registry(),
            Arc::new(Metrics::default()),
            identity,
            out,
        );
        session
            .msg(AccountMsg {
                op: "updatePositions".into(),
                data: json!({"requestId": 9}),
                account_name: Some("acct".into()),
            })
            .await;
        let reply = sent.try_recv().expect("a reply");
        assert!(reply.contains("updatePositions:done"), "{reply}");
        assert!(reply.contains(r#""requestId":9"#), "{reply}");
    }

    /// The COSIGNED data-source path end to end: a source account on the native
    /// actor, frames through this session's pump (base64, seq, snapshot,
    /// re-snapshot), ingested by a frame-fed engine that then writes with no reads.
    mod cosigned_path {
        use super::super::*;
        use base64::Engine as _;
        use safe_cex_engine::model::{
            Limits, Market, MinMax, Order, OrderSide, OrderStatus, OrderType, Position,
            PositionSide, Precision,
        };
        use safe_cex_engine::{
            Command, Cx, Engine, HttpRequest, Method, Response, SocketId, TimerId, Venue,
        };
        use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone, Default)]
        struct Tape(Rc<RefCell<Option<Events>>>);
        impl Transport for Tape {
            fn connect(
                &self,
                socket: SocketId,
                _: String,
                events: Events,
            ) -> tokio::sync::mpsc::UnboundedSender<String> {
                *self.0.borrow_mut() = Some(events.clone());
                let _ = events.send(HostEvent::WsOpened(socket));
                tokio::sync::mpsc::unbounded_channel().0
            }
            fn request(&self, _: HttpRequest, _: Events) {}
        }

        fn order(id: &str) -> Order {
            Order {
                id: id.into(),
                parent_id: None,
                status: OrderStatus::Open,
                symbol: "BTCUSDT".into(),
                kind: OrderType::Limit,
                side: OrderSide::Buy,
                price: 100.,
                amount: 1.,
                filled: 0.,
                remaining: 1.,
                reduce_only: false,
                close: None,
                client_order_id: None,
                time_in_force: None,
                last_update: 0.,
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            }
        }

        /// The source loads an account and streams changes; the frame-fed side
        /// loads markets only and writes from what it ingested.
        struct Mirror {
            state: Vec<u8>,
            applied: Rc<RefCell<Vec<Vec<u8>>>>,
        }
        impl Venue for Mirror {
            type Pending = ();
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
            }
            fn start(&mut self, cx: &mut Cx<'_, ()>) {
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
                if cx.frame_fed() {
                    return;
                }
                cx.store.set_orders(vec![order("o1")]);
                cx.store.set_positions(vec![Position {
                    id: "p1".into(),
                    symbol: "BTCUSDT".into(),
                    side: PositionSide::Long,
                    entry_price: 100.,
                    notional: 100.,
                    leverage: 1.,
                    margin_type: None,
                    position_mode: None,
                    unrealized_pnl: 0.,
                    realized_pnl: None,
                    contracts: 1.,
                    liquidation_price: 0.,
                    margin_size: None,
                    last_update: None,
                }]);
                cx.connect("test");
            }
            fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
            /// A private update: a new order and changed venue state.
            fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
                self.state = text.as_bytes().to_vec();
                cx.store.upsert_order(order("o2"));
            }
            fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
            fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
            fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
            fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
            fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
                let (id, path) = match &command {
                    Command::PlaceOrders { command, .. } => (*command, "place".to_owned()),
                    Command::CancelOrders { command, ids } => {
                        (*command, format!("cancel/{}", ids[0]))
                    }
                    Command::AmendOrder { command, id, .. } => (*command, format!("amend/{id}")),
                    _ => return,
                };
                // Cancels and amends route through the stored (ingested) order.
                if let Some(order) = path.split('/').nth(1)
                    && cx.store.order(order).is_none()
                {
                    cx.fail(id, "order not ingested");
                    return;
                }
                cx.request(
                    Method::Post,
                    format!("https://venue.test/{path}"),
                    vec![],
                    None,
                    (),
                );
            }
            fn export_venue_state(&self) -> Option<Vec<u8>> {
                Some(self.state.clone())
            }
            fn apply_venue_state(&mut self, _: &mut Cx<'_, ()>, state: &[u8]) {
                self.applied.borrow_mut().push(state.to_vec());
            }
            fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
                None
            }
            fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
                Box::new(std::iter::empty())
            }
        }

        /// The next `op:'frame'` envelope the pump sent: (seq, snapshot, bytes).
        async fn next_frame(sent: &mut crate::out::Receiver) -> (u64, bool, Vec<u8>) {
            tokio::time::timeout(std::time::Duration::from_secs(3), async {
                loop {
                    let text = sent.recv().await.expect("pump alive");
                    let envelope: Value = serde_json::from_str(&text).unwrap();
                    if envelope["d"]["op"] == "frame" {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(envelope["d"]["data"].as_str().unwrap())
                            .unwrap();
                        return (
                            envelope["d"]["seq"].as_u64().unwrap(),
                            envelope["d"]["snapshot"].as_bool().unwrap(),
                            bytes,
                        );
                    }
                }
            })
            .await
            .expect("a frame")
        }

        fn spawn_pump(
            handle: &safe_cex_native::AccountHandle,
            link: safe_cex_native::ClientLink,
            client: u64,
        ) -> (crate::out::Receiver, tokio::task::JoinHandle<()>) {
            let (out, sent, _) = Out::channel(256);
            let (loaded, _loaded_rx) = tokio::sync::mpsc::unbounded_channel();
            let (dropped, _dropped_rx) = tokio::sync::mpsc::unbounded_channel();
            let task = tokio::task::spawn_local(pump(
                link,
                client,
                dropped,
                handle.clone(),
                "acct".into(),
                out,
                Arc::default(),
                Arc::default(),
                Some(Hold { client, loaded }),
            ));
            (sent, task)
        }

        #[tokio::test]
        async fn a_frame_fed_engine_writes_from_the_servers_frames_with_no_reads() {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let tape = Tape::default();
                    let source = safe_cex_engine::test_support::seeded_engine(Mirror {
                        state: b"v1".to_vec(),
                        applied: Rc::default(),
                    });
                    let config = ServerConfig {
                        flush_every: std::time::Duration::from_millis(1),
                        ..ServerConfig::default()
                    };
                    let (handle, _task) =
                        safe_cex_native::spawn_account(Host::new(source, tape.clone()), config)
                            .unwrap();
                    let applied = Rc::new(RefCell::new(Vec::new()));
                    let mut main: Engine<Mirror> =
                        safe_cex_engine::test_support::seeded_engine(Mirror {
                            state: Vec::new(),
                            applied: applied.clone(),
                        });
                    main.set_frame_fed().unwrap();
                    main.start(0.);
                    assert!(safe_cex_engine::test_support::http_requests(&mut main).is_empty());

                    // Join: the snapshot (seq 1) carries the account and VENUE_STATE.
                    let link = handle.join(1, true).await.unwrap();
                    let (mut sent, first) = spawn_pump(&handle, link, 1);
                    let (seq, snapshot, bytes) = next_frame(&mut sent).await;
                    assert_eq!((seq, snapshot), (1, true));
                    main.apply_frame(1., &bytes, snapshot).unwrap();
                    assert_eq!(applied.borrow().last().map(Vec::as_slice), Some(&b"v1"[..]));
                    assert!(main.store().order("o1").is_some());
                    assert_eq!(main.store().positions().len(), 1);

                    // A private update: a delta (seq 2+) carries the order and the new state.
                    let events = tape.0.borrow().clone().unwrap();
                    events
                        .send(HostEvent::WsMessage(SocketId(1), "v2".into()))
                        .unwrap();
                    loop {
                        let (seq, snapshot, bytes) = next_frame(&mut sent).await;
                        assert!(seq >= 2 && !snapshot);
                        main.apply_frame(2., &bytes, snapshot).unwrap();
                        if main.store().order("o2").is_some() {
                            break;
                        }
                    }
                    assert_eq!(applied.borrow().last().map(Vec::as_slice), Some(&b"v2"[..]));

                    // Writes from ingested state: one request each, no reads.
                    for (command, order) in [
                        (
                            Command::PlaceOrders {
                                command: 1,
                                orders: vec![],
                            },
                            None,
                        ),
                        (
                            Command::CancelOrders {
                                command: 2,
                                ids: vec!["o1".into()],
                            },
                            Some("o1"),
                        ),
                        (
                            Command::AmendOrder {
                                command: 3,
                                id: "o2".into(),
                                price: Some(101.),
                                amount: None,
                            },
                            Some("o2"),
                        ),
                    ] {
                        main.handle_command(3., command, None).unwrap();
                        let requests = safe_cex_engine::test_support::http_requests(&mut main);
                        assert_eq!(requests.len(), 1, "{order:?}: {requests:?}");
                        assert_eq!(requests[0].method, Method::Post);
                    }

                    // A re-snapshot (a new client, seq restarts at 1) resets and re-applies.
                    first.abort();
                    handle.leave(1);
                    let link = handle.join(2, true).await.unwrap();
                    let (mut sent, _second) = spawn_pump(&handle, link, 2);
                    let (seq, snapshot, bytes) = next_frame(&mut sent).await;
                    assert_eq!((seq, snapshot), (1, true));
                    main.apply_frame(4., &bytes, snapshot).unwrap();
                    assert!(
                        main.store().order("o1").is_some() && main.store().order("o2").is_some()
                    );
                    assert_eq!(applied.borrow().last().map(Vec::as_slice), Some(&b"v2"[..]));
                    assert!(safe_cex_engine::test_support::http_requests(&mut main).is_empty());
                })
                .await;
        }
    }

    mod hold {
        use super::super::*;
        use safe_cex_engine::model::{
            Limits, Market, MinMax, OrderSide, OrderStatus, OrderType, Position, PositionSide,
            Precision,
        };
        use safe_cex_engine::{Command, Cx, HttpRequest, Response, SocketId, TimerId, Venue};
        use safe_cex_native::{Events, Host, HostEvent, ServerConfig, Transport};
        use std::cell::RefCell;
        use std::rc::Rc;

        #[derive(Clone, Default)]
        struct Tape(Rc<RefCell<Option<Events>>>);
        impl Transport for Tape {
            fn connect(
                &self,
                socket: SocketId,
                _: String,
                events: Events,
            ) -> tokio::sync::mpsc::UnboundedSender<String> {
                *self.0.borrow_mut() = Some(events.clone());
                let _ = events.send(HostEvent::WsOpened(socket));
                tokio::sync::mpsc::unbounded_channel().0
            }
            fn request(&self, _: HttpRequest, _: Events) {}
        }

        /// Markets at start; orders and positions only when a message arrives.
        struct LateAccount;
        impl Venue for LateAccount {
            type Pending = ();
            fn capabilities(&self) -> safe_cex_engine::Capabilities {
                safe_cex_engine::Capabilities::BASE
            }
            fn start(&mut self, cx: &mut Cx<'_, ()>) {
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
                cx.connect("test");
            }
            fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
            fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: &str) {
                cx.store.set_orders(vec![safe_cex_engine::model::Order {
                    id: "o1".into(),
                    parent_id: None,
                    status: OrderStatus::Open,
                    symbol: "BTCUSDT".into(),
                    kind: OrderType::Limit,
                    side: OrderSide::Buy,
                    price: 100.,
                    amount: 1.,
                    filled: 0.,
                    remaining: 1.,
                    reduce_only: false,
                    close: None,
                    client_order_id: None,
                    time_in_force: None,
                    last_update: 0.,
                    trigger_type: None,
                    take_profit: None,
                    stop_loss: None,
                    extra: None,
                }]);
                cx.store.set_positions(vec![Position {
                    id: "p1".into(),
                    symbol: "BTCUSDT".into(),
                    side: PositionSide::Long,
                    entry_price: 100.,
                    notional: 100.,
                    leverage: 1.,
                    margin_type: None,
                    position_mode: None,
                    unrealized_pnl: 0.,
                    realized_pnl: None,
                    contracts: 1.,
                    liquidation_price: 0.,
                    margin_size: None,
                    last_update: None,
                }]);
            }
            fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
            fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
            fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
            fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
            fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
            fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
                None
            }
            fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
                Box::new(std::iter::empty())
            }
        }

        #[tokio::test]
        async fn an_unloaded_private_account_sends_no_snapshot_until_it_loads() {
            tokio::task::LocalSet::new()
                .run_until(async {
                    let tape = Tape::default();
                    let engine = safe_cex_engine::test_support::seeded_engine(LateAccount);
                    let config = ServerConfig {
                        flush_every: std::time::Duration::from_millis(1),
                        ..ServerConfig::default()
                    };
                    let (handle, _task) =
                        safe_cex_native::spawn_account(Host::new(engine, tape.clone()), config)
                            .unwrap();
                    let link = handle.join(1, true).await.unwrap();
                    let (out, mut sent, _) = Out::channel(64);
                    let (loaded, mut loaded_rx) = tokio::sync::mpsc::unbounded_channel();
                    let (dropped, _dropped_rx) = tokio::sync::mpsc::unbounded_channel();
                    let pump = tokio::task::spawn_local(pump(
                        link,
                        1,
                        dropped,
                        handle.clone(),
                        "acct".into(),
                        out,
                        Arc::default(),
                        Arc::default(),
                        Some(Hold { client: 1, loaded }),
                    ));
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    while let Ok(text) = sent.try_recv() {
                        assert!(
                            !text.contains("\"op\":\"frame\""),
                            "no frame while orders and positions are unloaded: {text}"
                        );
                    }
                    assert!(loaded_rx.try_recv().is_err(), "not loaded yet");
                    let events = tape.0.borrow().clone().unwrap();
                    events
                        .send(HostEvent::WsMessage(SocketId(1), "load".into()))
                        .unwrap();
                    let ready =
                        tokio::time::timeout(std::time::Duration::from_secs(3), loaded_rx.recv())
                            .await;
                    assert_eq!(ready.unwrap(), Some(("acct".to_owned(), 1)));
                    while let Ok(text) = sent.try_recv() {
                        assert!(
                            !text.contains("\"op\":\"frame\""),
                            "the re-snapshot carries it"
                        );
                    }
                    pump.abort();
                })
                .await;
        }
    }

    fn public_init() -> InitPayload {
        InitPayload {
            account_name: PUBLIC_ACCOUNT.into(),
            exchange_name: PUBLIC_EXCHANGE.into(),
            public_only: Some(true),
            ..InitPayload::default()
        }
    }

    #[test]
    fn unauthenticated_init_is_only_the_bare_public_bybit_account() {
        assert_eq!(unauthenticated_init_refusal(&public_init()), None);
        let mut other = public_init();
        other.account_name = "mine".into();
        assert!(unauthenticated_init_refusal(&other).is_some());
        let mut keyed = public_init();
        keyed.key = "k".into();
        assert!(unauthenticated_init_refusal(&keyed).is_some());
        let mut routed = public_init();
        routed.tealstreet_proxy = Some(true);
        assert!(unauthenticated_init_refusal(&routed).is_some());
        for venue_field in [
            InitPayload {
                vault: Some(String::new()),
                ..public_init()
            },
            InitPayload {
                bybit_site_id: Some("BRA_BTL".into()),
                ..public_init()
            },
            InitPayload {
                enable_spot_markets: Some(false),
                ..public_init()
            },
            InitPayload {
                enable_inverse_markets: Some(true),
                ..public_init()
            },
            InitPayload {
                enable_option_markets: Some(true),
                ..public_init()
            },
        ] {
            assert!(
                unauthenticated_init_refusal(&venue_field).is_some(),
                "{venue_field:?}"
            );
        }
    }

    /// A LOCAL engine turns these into the account's `error` line; `engine-event`
    /// alone reaches no toast in the browser.
    #[test]
    fn an_order_rejected_error_event_also_reaches_the_error_channel() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        notice_to_msg(
            &out,
            "acct",
            &json!({"type":"event","level":"error","code":"order.rejected","message":"EC_PostOnlyWillTakeLiquidity"}),
            &pending,
        );
        let event: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(event["d"]["op"], "engine-event");
        let error: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(error["d"]["op"], "error");
        assert_eq!(error["d"]["data"], json!(["EC_PostOnlyWillTakeLiquidity"]));

        // A warning, and any other code, stays on `engine-event` only.
        for event in [
            json!({"type":"event","level":"warn","code":"order.rejected","message":"m"}),
            json!({"type":"event","level":"error","code":"ws.closed","message":"m"}),
        ] {
            notice_to_msg(&out, "acct", &event, &pending);
            sent.try_recv().unwrap();
            assert!(sent.try_recv().is_err(), "{event}");
        }
    }

    #[test]
    fn a_failed_start_tells_the_client_the_venue_reason() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let notice = json!({
            "type": "startupFailed",
            "detail": {"message": "API key does not exist", "kind": "authInvalid"},
        });
        notice_to_msg(&out, "acct", &notice, &pending);
        sent.try_recv().unwrap();
        let frame = sent.try_recv().unwrap();
        assert!(frame.contains("API key does not exist"), "{frame}");
    }

    #[test]
    fn a_done_notice_settles_its_request_and_others_stay_pending() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        pending.lock().unwrap().insert(
            1,
            Pending {
                request_id: "r1".into(),
                method: "placeOrder".into(),
                reply: methods::Reply::Result,
                group: None,
                incomplete: false,
                audit: None,
            },
        );
        notice_to_msg(&out, "acct", &json!({"type":"event","command":1}), &pending);
        assert_eq!(
            pending.lock().unwrap().len(),
            1,
            "a non-completion keeps the request"
        );
        sent.try_recv().unwrap();
        notice_to_msg(
            &out,
            "acct",
            &json!({"type":"done","command":1,"result":"[\"o1\"]"}),
            &pending,
        );
        let reply: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(reply["d"]["op"], "callMethod:done");
        assert_eq!(
            reply["d"]["data"],
            json!({"requestId":"r1","success":true,"result":["o1"]})
        );
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn a_partial_history_read_replies_with_its_completeness_marker() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        pending.lock().unwrap().insert(
            3,
            Pending {
                request_id: "r3".into(),
                method: "fetchHistoricalFills".into(),
                reply: methods::Reply::Result,
                group: None,
                incomplete: false,
                audit: None,
            },
        );
        let event = json!({"type":"event","level":"info","code":"history.incomplete","message":"partial","fields":{"command":3}});
        notice_to_msg(&out, "acct", &event, &pending);
        sent.try_recv().unwrap();
        notice_to_msg(
            &out,
            "acct",
            &json!({"type":"done","command":3,"result":"[{\"id\":\"f1\"}]"}),
            &pending,
        );
        let reply: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(
            reply["d"]["data"],
            json!({"requestId":"r3","success":true,"result":[{"id":"f1"}],"completeness":{"incomplete":true}})
        );
    }

    #[test]
    fn socket_health_goes_out_at_once_on_a_state_change_and_paces_timestamps() {
        use std::time::Duration;
        let down = json!({"privateInitialized": false, "privateLastMessage": null});
        let up = json!({"privateInitialized": true, "privateLastMessage": 10});
        let later = json!({"privateInitialized": true, "privateLastMessage": 20});
        assert!(socket_health_due(&Value::Null, &down, Duration::ZERO));
        assert!(
            socket_health_due(&down, &up, Duration::ZERO),
            "a socket coming up"
        );
        assert!(!socket_health_due(&up, &up, Duration::from_secs(60)));
        assert!(!socket_health_due(&up, &later, Duration::from_secs(4)));
        assert!(socket_health_due(&up, &later, Duration::from_secs(5)));
    }

    #[test]
    fn socket_health_carries_the_store_fields_the_browser_derives() {
        let health = socket_health(safe_cex_native::SocketSummary {
            public_healthy: true,
            private_healthy: true,
            public_initialized: true,
            private_initialized: true,
            public_last_message: Some(5),
            private_last_message: Some(7),
            public: safe_cex_native::SocketDiagnostics {
                close_code: Some(1006),
                close_count_total: 3,
                close_streak: 1,
            },
            private: safe_cex_native::SocketDiagnostics::default(),
        });
        assert_eq!(
            health,
            json!({"publicHealthy":true,"privateHealthy":true,"publicInitialized":true,
                "privateInitialized":true,"publicLastMessage":5,"privateLastMessage":7,
                "publicCloseCode":1006,"publicCloseCountTotal":3,"publicCloseStreak":1,
                "publicReconnectPaused":false,"privateCloseCode":null,"privateCloseCountTotal":0,
                "privateCloseStreak":0,"privateReconnectPaused":false})
        );
    }

    #[test]
    fn a_failed_write_reports_its_engine_detail() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        pending.lock().unwrap().insert(
            2,
            Pending {
                request_id: json!(7),
                method: "placeOrder".into(),
                reply: methods::Reply::Result,
                group: None,
                incomplete: false,
                audit: None,
            },
        );
        let notice =
            json!({"type":"done","command":2,"error":"rejected","detail":{"outcome":"unknown"}});
        notice_to_msg(&out, "acct", &notice, &pending);
        let reply: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(reply["d"]["data"]["success"], false);
        assert_eq!(reply["d"]["data"]["result"]["outcome"], "unknown");
        assert_eq!(reply["d"]["data"]["result"]["message"], "rejected");
    }

    #[test]
    fn a_placement_with_client_order_id_returns_the_ids_and_the_client_order_id() {
        let (out, mut sent, _) = Out::channel(16);
        let pending = Arc::new(Mutex::new(HashMap::new()));
        pending.lock().unwrap().insert(
            3,
            Pending {
                request_id: json!(9),
                method: "placeOrderWithClientOrderId".into(),
                reply: methods::Reply::Placed {
                    client_order_id: "cid-1".into(),
                },
                group: None,
                incomplete: false,
                audit: None,
            },
        );
        let notice = json!({"type":"done","command":3,"result":"[\"o1\"]"});
        notice_to_msg(&out, "acct", &notice, &pending);
        let reply: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(
            reply["d"]["data"],
            json!({"requestId":9,"success":true,"result":{"orderIds":["o1"],"clientOrderId":"cid-1"}})
        );
    }
}

#[cfg(test)]
#[path = "session_delegated_writes_tests.rs"]
mod delegated_writes_tests;

#[cfg(test)]
#[path = "session/tests/delegated_e2e.rs"]
mod delegated_e2e;

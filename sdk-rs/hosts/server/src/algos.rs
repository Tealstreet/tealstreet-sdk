//! Server-side algos (chaser, TWAP, exec) for one private account: an
//! `AccountPort` over `AccountHandle`, run on an engine thread's `LocalSet`.
use safe_cex_algos::projection::{self, Snapshot};
use safe_cex_algos::{AccountPort, AlgoCx, AlgoStore, Change, CommandFuture, Registry, Scope};
use safe_cex_chaser::ActiveChaser;
use safe_cex_engine::engine::{Command, CommandId, Fill, VenueAction};
use safe_cex_engine::error::{ErrorKind, Outcome, VenueError};
use safe_cex_engine::model::OrderStatus;
use safe_cex_native::{AccountHandle, Outgoing};
use serde_json::Value;
use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// What a session asks of an account's algos.
pub enum AlgoInput {
    Op {
        op: String,
        data: String,
        /// The socket that sent it, for the order audit line.
        origin: String,
        twap_owner: safe_cex_algo_twap::TwapOwner,
        action: Option<Box<crate::order_actions::NukeReceipt>>,
    },
    /// A hosted `nuke`/`nukePositions`/`nukeOrders` call: exec's nuke, answered
    /// with `callMethod:done` once every leg has settled.
    Nuke {
        mode: &'static str,
        request_id: Value,
        action: Option<Box<crate::order_actions::NukeReceipt>>,
    },
    /// A socket joined (a resume or another tab): resend the algo store rows.
    Joined,
    /// Revoke local scheduling without canceling exchange orders or replaying work.
    RouterFence { stopped: StopNotice },
    /// The account is going away: settle algos, then dispose the engine.
    /// The reason reaches the order audit log and the client. With no browser
    /// to tell, `stopped` keeps the notice for the account's next init.
    Dispose {
        reason: String,
        stopped: Option<StopNotice>,
    },
}

/// Where algo replies go: the account's current signing socket.
pub type Emit = Box<dyn Fn(String) + Send>;

/// Who holds the account now.
#[derive(Default)]
pub struct Holding {
    pub live: Vec<u64>,
    pub parked: bool,
}

/// Sends a frame, if given, to every live holder; says who holds the account.
pub type Broadcast = Box<dyn Fn(Option<String>) -> Holding + Send>;

/// The algo a stop notice is about, as the page's `activeChasers` or
/// `activeTWAPs` row names it, so the page can dedupe the notice exactly.
#[derive(Clone, Debug, PartialEq)]
pub struct AlgoRef {
    /// `"chase"` or `"twap"`.
    pub kind: &'static str,
    pub id: String,
}

/// One error notice for the page: one algo's stop, or errors it may have missed.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub text: String,
    pub algo: Option<AlgoRef>,
}

impl Notice {
    pub fn frame(&self, account_name: &str) -> String {
        crate::envelope::algo_notice(account_name, &self.text, self.algo.as_ref())
    }
}

/// What a client that is not there to see it is told on its next init: why its
/// algos stopped, and the ends of chases it missed, as replayable frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Kept {
    pub notices: Vec<Notice>,
    pub ends: Vec<String>,
}

impl Kept {
    /// The notices, then the replayed chase ends, as frames for the page.
    pub fn frames(&self, account_name: &str) -> Vec<String> {
        self.notices
            .iter()
            .map(|notice| notice.frame(account_name))
            .chain(self.ends.iter().cloned())
            .collect()
    }

    /// The notices' text, for the log.
    pub fn summary(&self) -> String {
        let texts: Vec<&str> = self
            .notices
            .iter()
            .map(|notice| notice.text.as_str())
            .collect();
        texts.join("\n")
    }
}

impl From<&str> for Kept {
    fn from(text: &str) -> Self {
        Self {
            notices: vec![Notice {
                text: text.to_owned(),
                algo: None,
            }],
            ends: Vec::new(),
        }
    }
}

/// Keeps a stop notice for a client that is not there to see it.
pub type StopNotice = Box<dyn Fn(Kept) + Send>;

/// The chases and TWAPs running on one account, readable off its engine thread.
pub struct Running {
    chases: AtomicUsize,
    twaps: AtomicUsize,
    /// Bumped whenever an account's count falls, so drained sockets recheck.
    ended: Arc<tokio::sync::watch::Sender<u64>>,
    user: OnceLock<Arc<crate::user_drain::User>>,
    work: Mutex<(Vec<crate::user_drain::Job>, usize)>,
    lease: OnceLock<crate::router_lease::Permit>,
    lease_parked: AtomicBool,
    restores: OnceLock<crate::interruption::RestoreGuard>,
}

impl Running {
    pub fn new(ended: Arc<tokio::sync::watch::Sender<u64>>) -> Self {
        Self {
            chases: AtomicUsize::new(0),
            twaps: AtomicUsize::new(0),
            ended,
            user: OnceLock::new(),
            work: Mutex::default(),
            lease: OnceLock::new(),
            lease_parked: AtomicBool::new(false),
            restores: OnceLock::new(),
        }
    }

    pub(crate) fn attach_restores(&self, guard: crate::interruption::RestoreGuard) {
        let _ = self.restores.set(guard);
    }
    fn restore_epoch(&self) -> crate::interruption::RestoreEpoch {
        let lease = self
            .lease
            .get()
            .and_then(|permit| permit.restore_generation());
        self.restores.get().map_or(
            crate::interruption::RestoreEpoch {
                marker: None,
                lease,
            },
            |guard| guard.epoch(lease),
        )
    }
    pub(crate) fn attach_lease(&self, permit: crate::router_lease::Permit) {
        let _ = self.lease.set(permit);
    }
    pub(crate) fn io_allowed(&self) -> bool {
        self.lease.get().is_none_or(|lease| lease.allowed())
    }
    pub(crate) fn claim_lease_park(&self) -> bool {
        !self.io_allowed() && !self.lease_parked.swap(true, Ordering::AcqRel)
    }
    pub(crate) fn parked_by_lease(&self) -> bool {
        self.lease_parked.load(Ordering::Acquire)
    }
    /// `(chases, twaps)`.
    pub fn counts(&self) -> (usize, usize) {
        (
            self.chases.load(Ordering::Relaxed),
            self.twaps.load(Ordering::Relaxed),
        )
    }

    pub fn any(&self) -> bool {
        self.counts() != (0, 0)
    }

    pub(crate) fn attach(&self, user: Arc<crate::user_drain::User>) {
        let _ = self.user.set(user);
    }

    pub(crate) fn jobs(&self) -> (Vec<crate::user_drain::Job>, usize) {
        self.work.lock().unwrap().clone()
    }

    pub(crate) fn refresh(&self, chases: &[ActiveChaser], twaps: &[Value], pending: usize) {
        let mut jobs: Vec<_> = chases
            .iter()
            .map(|chase| crate::user_drain::Job {
                kind: "chase",
                id: chase.id.clone(),
                parked: false,
            })
            .collect();
        jobs.extend(twaps.iter().filter_map(|twap| {
            Some(crate::user_drain::Job {
                kind: "twap",
                id: twap.get("id")?.as_str()?.to_owned(),
                parked: twap.get("status").and_then(Value::as_str) == Some("paused"),
            })
        }));
        let pending = pending + chases.len() + twaps.len() - jobs.len();
        let previous = std::mem::replace(&mut *self.work.lock().unwrap(), (jobs, pending));
        if previous.1 > pending {
            self.ended
                .send_modify(|revision| *revision = revision.wrapping_add(1));
        }
        self.set(chases.len(), twaps.len());
    }

    fn set(&self, chases: usize, twaps: usize) {
        let before =
            self.chases.swap(chases, Ordering::Relaxed) + self.twaps.swap(twaps, Ordering::Relaxed);
        if chases + twaps < before {
            self.ended
                .send_modify(|ended| *ended = ended.wrapping_add(1));
        }
    }
}

impl Default for Running {
    fn default() -> Self {
        Self::new(Arc::new(tokio::sync::watch::channel(0).0))
    }
}

/// How often the algo host recounts its chases and TWAPs: a chase ends on its own task.
const RECOUNT_EVERY: Duration = Duration::from_millis(250);

/// Errors kept for a browser that may not have seen them, oldest dropped first.
const UNHEARD_ERRORS: usize = 32;
/// Two resume windows: later, no socket that missed it can still come back.
const UNHEARD_TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// An error someone may not have seen.
struct Unheard {
    sent: Instant,
    /// The sockets it went to, any of which can be dead and look live for 30s.
    told: Vec<u64>,
    /// A parked session, or nobody at all, was not told.
    owed: bool,
    /// An error's message, for the stop notice; None for an outcome notice.
    error: Option<String>,
    /// The frame replayed to a page that missed it.
    frame: String,
    /// The order a stop's retrying warning names: a later end of that cancel supersedes it.
    retrying: Option<String>,
    /// A chase's end: kept past a dispose for the account's next init.
    end: bool,
}

/// A page that was away must not hear that a cancel is still retrying once
/// its end is known.
fn drop_superseded(unheard: &mut VecDeque<Unheard>, op: &str, data: &str) {
    if !matches!(op, "warning" | "error") {
        return;
    }
    let text = notice_text(data);
    let settled = safe_cex_chaser::settled_orders(&text);
    if !settled.is_empty() {
        unheard.retain(|kept| {
            kept.retrying
                .as_deref()
                .is_none_or(|id| !settled.contains(&id))
        });
    }
}

/// The order a retrying warning names, so its end can supersede it.
fn retrying_of(op: &str, data: &str) -> Option<String> {
    (op == "warning")
        .then(|| safe_cex_chaser::retrying_order(&notice_text(data)).map(str::to_owned))
        .flatten()
}

/// An algo notice's text: chaser notices are JSON strings.
fn notice_text(data: &str) -> std::borrow::Cow<'_, str> {
    serde_json::from_str::<String>(data).map_or(std::borrow::Cow::Borrowed(data), Into::into)
}

/// Bound on settling chaser cancels before the engine is disposed anyway.
const SHUTDOWN_DEADLINE: Duration = Duration::from_secs(10);

pub const OPS: [&str; 10] = [
    "chaser:start",
    "chaser:terminate",
    "chaser:terminateAll",
    "twap:start",
    "twap:resume",
    "twap:cleanup",
    "twap:pause",
    "twap:stop",
    "twap:state",
    "exec",
];

struct Port {
    handle: AccountHandle,
    /// Changes when the feed rejoins after the account dropped it.
    client: Cell<u64>,
    store: RefCell<AlgoStore>,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<Change>>>,
    next: Cell<CommandId>,
    pending: RefCell<HashMap<CommandId, oneshot::Sender<Result<String, VenueError>>>>,
    name: String,
    who: crate::audit::Who,
    emit: Emit,
    broadcast: Broadcast,
    placed: crate::accounts::PlacedSymbols,
    signer: Option<Arc<safe_cex_remote_signer::SignerLink>>,
    /// A send proves nothing until a frame follows it on that socket.
    unheard: RefCell<VecDeque<Unheard>>,
    /// Errors raised so far, to tell whether a shutdown raised any.
    errors: Cell<u64>,
    running: Arc<Running>,
}

/// A nuke belongs to the registration that requested it, even while another
/// holder keeps the account resident. Fence every native leg after an await;
/// suppressing the final receipt alone would still permit new retired writes.
struct ActionNukePort {
    port: Rc<Port>,
    guard: crate::order_actions::ActionGuard,
}

impl AccountPort for ActionNukePort {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.port.store()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        self.port.subscribe()
    }
    fn next_command_id(&self) -> CommandId {
        self.port.next_command_id()
    }
    fn command(&self, command: Command) -> CommandFuture {
        if let Err(message) = self.guard.check() {
            return Box::pin(
                async move { Err(VenueError::new(message).outcome(Outcome::Unknown)) },
            );
        }
        self.port.command(command)
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.port.capabilities()
    }
    fn emit(&self, op: &str, data: String) {
        self.port.emit(op, data);
    }
}

impl AccountPort for Port {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.store.borrow()
    }
    fn subscribe(&self) -> mpsc::UnboundedReceiver<Change> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.subscribers.borrow_mut().push(tx);
        rx
    }
    fn next_command_id(&self) -> CommandId {
        let id = self.next.get().wrapping_add(1).max(1);
        self.next.set(id);
        id
    }
    fn command(&self, command: Command) -> CommandFuture {
        if !self.running.io_allowed() {
            return Box::pin(async {
                Err(
                    VenueError::new("Router lease expired; no new work was sent")
                        .kind(ErrorKind::VenueUnavailable)
                        .outcome(Outcome::NotSent),
                )
            });
        }
        if let Command::PlaceOrders { orders, .. } = &command {
            self.placed
                .record(orders.iter().map(|order| order.symbol.as_str()));
        }
        let audit = crate::audit::typed(&command).map(|fields| AuditLine {
            who: self.who.clone(),
            fields,
            write: |line| println!("{line}"),
        });
        let id = command.completion_id();
        let (tx, rx) = oneshot::channel();
        if let Some(id) = id {
            self.pending.borrow_mut().insert(id, tx);
        }
        let sent = self.handle.command(self.client.get(), command);
        if !sent && let Some(id) = id {
            self.pending.borrow_mut().remove(&id);
        }
        Box::pin(async move {
            let result = if !sent {
                let mut error = VenueError::new("account is busy; command not sent");
                error.outcome = Some(Outcome::NotSent);
                Err(error)
            } else {
                match id {
                    Some(_) => rx.await.unwrap_or_else(|_| {
                        let mut error = VenueError::new("account stopped");
                        error.outcome = Some(Outcome::Unknown);
                        Err(error)
                    }),
                    None => Ok("null".into()),
                }
            };
            if let Some(audit) = audit {
                audit.settled(crate::audit::venue_result(&result));
            }
            result
        })
    }
    fn emit(&self, op: &str, data: String) {
        drop_superseded(&mut self.unheard.borrow_mut(), op, &data);
        if op == "error" {
            self.errors.set(self.errors.get() + 1);
            crate::state::Log::warn(&format!("algo error {}: {data}", self.name));
            let frame = algo_error(&self.name, &data);
            let holding = (self.broadcast)(Some(frame.clone()));
            self.keep_unheard(Some(data), frame, holding, None, false);
            return;
        }
        let retrying = retrying_of(op, &data);
        if owed_on_rejoin(op) {
            let replay = algo_msg(&self.name, op, replayed(op, &data));
            let frame = algo_msg(&self.name, op, data);
            let mut holding = (self.broadcast)(None);
            if let Some(signer) = &self.signer {
                holding.live = vec![signer.socket()];
            }
            (self.emit)(frame);
            self.keep_unheard(None, replay, holding, retrying, op == "chaser:complete");
            return;
        }
        // Every tab on the account, not only the one that signs.
        (self.broadcast)(Some(algo_msg(&self.name, op, data)));
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        *self.handle.capabilities()
    }
}

/// An algo's reply as the worker posted it: `{op, data}` with `data` verbatim.
/// The web spreads an array `data` into listener arguments and wraps a lone
/// value, so wrapping here nested arrays and turned `twap:stateUpdate`'s
/// object into an array its handler discards.
fn algo_msg(account_name: &str, op: &str, data: String) -> String {
    let data: Value = serde_json::from_str(&data).unwrap_or(Value::String(data));
    crate::envelope::msg(account_name, op, data)
}

fn algo_error(account_name: &str, data: &str) -> String {
    let data: Value = serde_json::from_str(data).unwrap_or_else(|_| Value::String(data.to_owned()));
    crate::envelope::algo_error(account_name, data)
}

/// One notice per stopped algo, and with `may_rest` that its orders may still be
/// open: with no browser, a cancel cannot be signed.
fn stop_notices(
    chasers: &[ActiveChaser],
    twaps: &[Value],
    store: &AlgoStore,
    reason: &str,
    may_rest: bool,
) -> Vec<Notice> {
    // A slice's stop is its TWAP's: one notice per user action.
    let told_by_twap = |chase: &&ActiveChaser| {
        chase.twap_id.as_deref().is_some_and(|twap_id| {
            twaps
                .iter()
                .any(|twap| twap["id"].as_str() == Some(twap_id))
        })
    };
    let chases = chasers
        .iter()
        .filter(|chase| !told_by_twap(chase))
        .map(|chase| {
            let resting = may_rest
                && chase.current_order_ids.iter().any(|id| {
                    store.order(id).is_none_or(|order| {
                        !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                    })
                });
            let check = if resting {
                " Its order may still be open: check it."
            } else {
                ""
            };
            Notice {
                text: format!("Chase on {} stopped: {reason}.{check}", chase.symbol),
                algo: Some(AlgoRef {
                    kind: "chase",
                    id: chase.id.clone(),
                }),
            }
        });
    let twaps = twaps.iter().map(|twap| {
        let market = twap["activeMarket"]["symbol"]
            .as_str()
            .unwrap_or("this account");
        let check = if may_rest {
            " Its orders may still be open: check them."
        } else {
            ""
        };
        Notice {
            text: format!("TWAP on {market} stopped: {reason}.{check}"),
            algo: Some(AlgoRef {
                kind: "twap",
                id: twap["id"].as_str().unwrap_or_default().to_owned(),
            }),
        }
    });
    chases.chain(twaps).collect()
}

/// Errors a page may have missed, as one notice the way they were always told.
fn errors_notice(errors: Vec<String>) -> Option<Notice> {
    (!errors.is_empty()).then(|| Notice {
        text: errors.join("\n"),
        algo: None,
    })
}

/// A command's audit line, also written if its waiter gives up before the
/// result (a chaser's bounded cancel), whose answer nothing then hears.
struct AuditLine {
    who: crate::audit::Who,
    fields: serde_json::Map<String, Value>,
    write: fn(&Value),
}

impl AuditLine {
    fn settled(self, result: Value) {
        (self.write)(&crate::audit::line(&self.who, "algo", &self.fields, result));
        std::mem::forget(self);
    }
}

impl Drop for AuditLine {
    fn drop(&mut self) {
        let result = serde_json::json!({
            "message": "no answer before the caller stopped waiting; the command may still complete",
            "outcome": "unknown",
        });
        (self.write)(&crate::audit::line(&self.who, "algo", &self.fields, result));
    }
}

/// The terminateAll audit result: ok only when every cancel settled.
fn shutdown_result(deadline_passed: bool, errors_raised: bool) -> Value {
    if deadline_passed {
        serde_json::json!({"message": "cancels unsettled: shutdown deadline passed; orders may rest"})
    } else if errors_raised {
        serde_json::json!({"message": "cancels failed; orders may rest"})
    } else {
        "ok".into()
    }
}

impl Port {
    fn still_unheard(&self, error: &Unheard) -> bool {
        let heard = |socket: &u64| {
            self.signer
                .as_ref()
                .is_some_and(|signer| signer.heard_since(*socket, error.sent))
        };
        error.sent.elapsed() < UNHEARD_TTL && (error.owed || !error.told.iter().all(heard))
    }

    fn keep_unheard(
        &self,
        error: Option<String>,
        frame: String,
        holding: Holding,
        retrying: Option<String>,
        end: bool,
    ) {
        let mut unheard = self.unheard.borrow_mut();
        unheard.retain(|error| self.still_unheard(error));
        if unheard.len() == UNHEARD_ERRORS {
            unheard.pop_front();
        }
        unheard.push_back(Unheard {
            sent: Instant::now(),
            owed: holding.parked || holding.live.is_empty(),
            told: holding.live,
            error,
            frame,
            retrying,
            end,
        });
    }

    /// The errors no frame followed, once.
    /// At a dispose with no browser: the unheard errors' messages and chase ends' frames.
    fn take_unheard_for_dispose(&self) -> (Vec<String>, Vec<String>) {
        let mut errors = Vec::new();
        let mut ends = Vec::new();
        for kept in self.unheard.borrow_mut().drain(..) {
            if !self.still_unheard(&kept) {
                continue;
            }
            match kept.error {
                Some(error) => errors.push(error),
                None if kept.end => ends.push(kept.frame),
                None => {}
            }
        }
        (errors, ends)
    }

    fn take_unheard(&self) -> Vec<String> {
        self.unheard
            .borrow_mut()
            .drain(..)
            .filter(|error| self.still_unheard(error))
            .filter_map(|error| error.error)
            .collect()
    }

    /// For a joining socket: the unheard errors it was not sent. Each stays kept
    /// until a frame follows it there, since this socket may be dying too.
    fn take_for_join(&self) -> Vec<String> {
        let Some(signer) = &self.signer else {
            return self.take_unheard();
        };
        let joining = signer.socket();
        let holding = (self.broadcast)(None);
        let mut unheard = self.unheard.borrow_mut();
        unheard.retain(|error| self.still_unheard(error));
        let mut replay = Vec::new();
        for error in unheard
            .iter_mut()
            .filter(|error| !error.told.contains(&joining))
        {
            error.sent = Instant::now();
            error.told.retain(|socket| holding.live.contains(socket));
            error.told.push(joining);
            error.owed = holding.parked;
            replay.push(error.frame.clone());
        }
        replay
    }

    fn apply(&self, snapshot: Snapshot, fills: Vec<Fill>, rejoined: bool) -> Change {
        let change = projection::apply(&mut self.store.borrow_mut(), snapshot, fills, rejoined);
        self.placed.sync_markets(self.store.borrow().markets.keys());
        change
    }

    fn settle(&self, notice: &Value) {
        if notice["type"].as_str() != Some("done") {
            return;
        }
        let Some(id) = notice["command"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
        else {
            return;
        };
        let Some(waiter) = self.pending.borrow_mut().remove(&id) else {
            return;
        };
        let _ = waiter.send(completion(notice));
    }
}

/// Algo clients are trusted and in-process: a deep queue keeps a burst from
/// dropping them (and the fills its frames carry) as slow peers.
pub const ALGO_QUEUE: usize = 4096;

/// IDs for rejoined algo clients, apart from the registry's sequential ones.
static REJOIN_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1 << 62);

/// Everything the account sent since the last batch, coalesced.
#[derive(Default)]
pub struct Batch {
    pub frames: usize,
    /// Every fill of every coalesced frame, in order.
    pub fills: Vec<Fill>,
    pub notices: Vec<Value>,
    /// The account had dropped this client; it rejoined with a fresh snapshot.
    pub rejoined: bool,
}

/// The algo host's subscription to its account.
pub struct Feed {
    handle: AccountHandle,
    link: safe_cex_native::ClientLink,
    client: u64,
    rejoins: std::collections::VecDeque<tokio::time::Instant>,
}

/// At most this many rejoins per window; past it the account is treated as gone.
pub const MAX_REJOINS: usize = 5;
pub const REJOIN_WINDOW: Duration = Duration::from_secs(60);

impl Feed {
    pub async fn join(handle: AccountHandle, client: u64) -> Result<Self, &'static str> {
        let link = handle.join_with_capacity(client, false, ALGO_QUEUE).await?;
        Ok(Self {
            handle,
            link,
            client,
            rejoins: Default::default(),
        })
    }

    pub fn client(&self) -> u64 {
        self.client
    }

    fn take(&self, batch: &mut Batch, item: Outgoing) {
        match item {
            Outgoing::Frame(frame) => {
                batch.frames += 1;
                // Fills ride only in their flush frame; the store keeps none.
                batch
                    .fills
                    .extend(safe_cex_engine::wire::fills(&frame).unwrap_or_default());
            }
            Outgoing::Text(text) => {
                if let Ok(notice) = serde_json::from_str(&text) {
                    batch.notices.push(notice);
                }
            }
        }
    }

    /// The next batch: one awaited item plus everything already queued. A
    /// dropped client rejoins; `None` only when the account itself is gone.
    pub async fn next(&mut self) -> Option<Batch> {
        let mut batch = Batch::default();
        match self.link.outgoing.recv().await {
            Some(item) => self.take(&mut batch, item),
            None => {
                let now = tokio::time::Instant::now();
                while self
                    .rejoins
                    .front()
                    .is_some_and(|at| now.duration_since(*at) > REJOIN_WINDOW)
                {
                    self.rejoins.pop_front();
                }
                if self.rejoins.len() >= MAX_REJOINS {
                    crate::state::Log::warn(
                        "algo feed dropped repeatedly; giving up on the account",
                    );
                    return None;
                }
                // Back off so a feed dropped right after its snapshot cannot spin.
                tokio::time::sleep(Duration::from_millis(100) * self.rejoins.len() as u32).await;
                self.rejoins.push_back(now);
                let client = REJOIN_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                self.link = self
                    .handle
                    .join_with_capacity(client, false, ALGO_QUEUE)
                    .await
                    .ok()?;
                self.client = client;
                batch.rejoined = true;
            }
        }
        while let Ok(item) = self.link.outgoing.try_recv() {
            self.take(&mut batch, item);
        }
        Some(batch)
    }
}

/// Runs one account's algos until `Dispose` (or the inbox closes), then
/// settles chases and disposes the engine. Call inside a `LocalSet`.
/// As `rust.exchange.ts` `syncBalanceInfo` (every 2 s) and the Lighter tier sync
/// (every 15 s): `balance.info` comes from reads frames do not carry.
const BALANCE_INFO_EVERY: Duration = Duration::from_secs(2);
const TIER_EVERY_TICKS: u32 = 15 / 2;
/// Resent unchanged this often, so a socket that took the account over since gets it.
const BALANCE_INFO_RESEND_TICKS: u32 = 15;

/// One read through the account port: its JSON result, or nothing on failure
/// (the engine reports read failures itself; the last payload stays).
async fn venue_read(port: &Port, action: VenueAction) -> Option<Value> {
    let command = Command::Venue {
        command: port.next_command_id(),
        action,
    };
    let text = port.command(command).await.ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()
        .filter(|value| !value.is_null())
}

/// `balance.info` as the browser engine composes it: the venue payload, with
/// Lighter's tier at `lighterAccountLimits.user_tier_name`.
pub fn balance_info(payload: Option<&Value>, tier: Option<&Value>) -> Option<Value> {
    let mut info = payload.cloned();
    if let Some(tier) = tier {
        let mut object = match info {
            Some(Value::Object(object)) => object,
            _ => serde_json::Map::new(),
        };
        object.insert(
            "lighterAccountLimits".into(),
            serde_json::json!({ "user_tier_name": tier }),
        );
        info = Some(Value::Object(object));
    }
    info
}

/// Engine cache reads (Bybit's last wallet poll, Lighter's last limits read): no
/// venue request, so no egress weight. Still skipped while no live socket holds
/// the account, since nobody would receive the result.
async fn sync_balance_info(
    port: Rc<Port>,
    name: String,
    held: std::sync::Arc<std::sync::atomic::AtomicBool>,
    payload: bool,
    tier: bool,
) {
    let mut every = tokio::time::interval(BALANCE_INFO_EVERY);
    let (mut info_payload, mut tier_name) = (None, None);
    let mut sent = None::<Value>;
    let mut tick = 0u32;
    loop {
        every.tick().await;
        tick = tick.wrapping_add(1);
        // As TS: only once the account has started; and only for a live holder.
        if !port.store().loaded.is_some_and(|loaded| loaded.balance)
            || !held.load(std::sync::atomic::Ordering::Relaxed)
        {
            // A socket that comes back gets the current info at once.
            sent = None;
            continue;
        }
        if payload && let Some(read) = venue_read(&port, VenueAction::FetchBalanceInfo).await {
            info_payload = Some(read);
        }
        if tier
            && tick % TIER_EVERY_TICKS == 1
            && let Some(read) = venue_read(&port, VenueAction::FetchAccountTier).await
        {
            tier_name = Some(read);
        }
        let Some(info) = balance_info(info_payload.as_ref(), tier_name.as_ref()) else {
            continue;
        };
        if sent.as_ref() != Some(&info) || tick.is_multiple_of(BALANCE_INFO_RESEND_TICKS) {
            let rows = serde_json::json!({ "balance": { "info": info } });
            (port.broadcast)(Some(crate::envelope::msg(&name, "store", rows)));
            sent = Some(info);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    handle: AccountHandle,
    client: u64,
    who: crate::audit::Who,
    emit: Emit,
    broadcast: Broadcast,
    link_signer: Option<Arc<safe_cex_remote_signer::SignerLink>>,
    placed: crate::accounts::PlacedSymbols,
    held: Arc<AtomicBool>,
    running: Arc<Running>,
    twap_owner: Arc<crate::twap_owner::Binding>,
    inbox: mpsc::UnboundedReceiver<AlgoInput>,
) {
    run_with_identity(
        handle,
        client,
        who,
        emit,
        broadcast,
        link_signer,
        placed,
        held,
        running,
        twap_owner,
        None,
        inbox,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_with_identity(
    handle: AccountHandle,
    client: u64,
    who: crate::audit::Who,
    emit: Emit,
    broadcast: Broadcast,
    link_signer: Option<Arc<safe_cex_remote_signer::SignerLink>>,
    placed: crate::accounts::PlacedSymbols,
    held: Arc<AtomicBool>,
    running: Arc<Running>,
    twap_owner: Arc<crate::twap_owner::Binding>,
    identity: Option<String>,
    mut inbox: mpsc::UnboundedReceiver<AlgoInput>,
) {
    // An algo host that cannot run never takes the account with it: the account's
    // own stop disposes the engine directly once this host is gone.
    let Ok(mut feed) = Feed::join(handle.clone(), client).await else {
        crate::state::Log::warn(&format!(
            "{}: algo host could not join its account; algos are unavailable",
            who.account
        ));
        return;
    };
    let name = who.account.clone();
    let port = Rc::new(Port {
        handle: handle.clone(),
        client: Cell::new(client),
        store: RefCell::new(AlgoStore {
            venue: who.venue.clone(),
            ..AlgoStore::default()
        }),
        subscribers: RefCell::default(),
        next: Cell::new(0),
        pending: RefCell::default(),
        name: name.clone(),
        who: who.clone(),
        emit,
        broadcast,
        placed: placed.clone(),
        signer: link_signer.clone(),
        unheard: RefCell::default(),
        errors: Cell::new(0),
        running: running.clone(),
    });
    let scope = Scope::new();
    let cx = AlgoCx::new(port.clone(), scope.clone());
    let chasers = safe_cex_chaser::ChaserFactory::new(cx.clone());
    let book = chasers.book();
    let binding = twap_owner.clone();
    let factory = if identity.is_some() {
        safe_cex_algo_twap::factory::TwapFactory::new_with_owner(
            &name,
            cx.clone(),
            Rc::new(book.clone()),
            twap_owner.snapshot(),
        )
    } else {
        safe_cex_algo_twap::factory::TwapFactory::with_owner(
            &name,
            twap_owner.snapshot(),
            cx.clone(),
            Rc::new(book.clone()),
        )
    };
    let twap = Rc::new(factory.guard_owner(Rc::new(move || binding.snapshot())));
    // Exec's chase/TWAP handoff shares this account's book and TWAP schedule,
    // under the server's trusted account name, never one from the payload.
    // A nuke ends with the venue's cancel-all, scoped as hosted `cancelAllOrders`.
    let sweep_handle = handle.clone();
    let exec = Rc::new(
        safe_cex_exec::native::ExecFactory::with_algos(name.clone(), book.clone(), (*twap).clone())
            .with_cancel_all(Rc::new(move || {
                use safe_cex_engine::CancelAll;
                use safe_cex_exec::native::CancelAllScope;
                match sweep_handle.capabilities().cancel_all {
                    CancelAll::Native => CancelAllScope::Native,
                    CancelAll::EachOpenSymbol | CancelAll::EachPlacedSymbol => {
                        CancelAllScope::Symbols(placed.symbols())
                    }
                }
            })),
    );
    let registry = Registry::new(vec![
        Box::new(chasers),
        Box::new(twap.clone()),
        Box::new(exec.clone()),
    ]);
    let mut published = String::new();
    // On its own task, since `feed.next()` must not be cancelled by a timer. Stopped
    // chases count until their cleanup ends: it may still need a signature.
    {
        let (book, twap, exec, name) = (book.clone(), twap.clone(), exec.clone(), name.clone());
        let running = running.clone();
        cx.spawn(async move {
            let mut every = tokio::time::interval(RECOUNT_EVERY);
            every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                every.tick().await;
                running.refresh(
                    &book.active_chasers(),
                    &twap.active_twaps(&name),
                    exec.pending_operations() + twap.pending_placements(),
                );
            }
        });
    }
    // `store` rows the frames do not carry (`activeChasers`, `activeTWAPs`).
    let publish = |published: &mut String| {
        let rows = serde_json::json!({
            "activeChasers": book.active_chasers(),
            "activeTWAPs": twap.active_twaps(&name),
        });
        let text = rows.to_string();
        if *published != text {
            (port.broadcast)(Some(crate::envelope::msg(&name, "store", rows)));
            *published = text;
        }
    };
    let capabilities = handle.capabilities();
    if capabilities.balance_info || capabilities.account_tier {
        cx.spawn(sync_balance_info(
            port.clone(),
            name.clone(),
            held,
            capabilities.balance_info,
            capabilities.account_tier,
        ));
    }
    // True only when the account itself is going away; a failed algo feed ends
    // the algos and leaves the account running.
    let ended = loop {
        tokio::select! {
            batch = feed.next() => {
                let Some(batch) = batch else { break None };
                for notice in &batch.notices {
                    port.settle(notice);
                }
                if batch.rejoined {
                    crate::state::Log::warn(&format!("{name}: algo feed was dropped; rejoined with a fresh snapshot"));
                    port.client.set(feed.client());
                    // Their completions went to the dropped client: the outcome is unknown.
                    for (_, waiter) in port.pending.borrow_mut().drain() {
                        let mut error = VenueError::new("algo feed rejoined; command outcome unknown");
                        error.outcome = Some(Outcome::Unknown);
                        let _ = waiter.send(Err(error));
                    }
                }
                // One store read per batch, carrying every coalesced frame's fills.
                if (batch.frames > 0 || batch.rejoined)
                    && let Some(snapshot) = handle.read(projection::project).await
                {
                    let change = port.apply(snapshot, batch.fills, batch.rejoined);
                    port.subscribers.borrow_mut().retain(|tx| tx.send(change.clone()).is_ok());
                    publish(&mut published);
                }
            }
            input = inbox.recv() => match input {
                Some(AlgoInput::RouterFence { stopped }) => {
                    for row in twap.active_twaps(&name) {
                        if let Some(id) = row["id"].as_str() {
                            let _ = registry.dispatch("twap:pause", &serde_json::to_string(id).unwrap(), &cx);
                        }
                    }
                    let paused = twap.active_twaps(&name);
                    // No cancellation cleanup: already-dispatched orders may still rest or fill.
                    scope.dispose();
                    let kept: Kept = "Router ownership expired: chases stopped and TWAPs paused. Resting orders were kept; an in-flight order may still fill. Check Open Orders, then explicitly resume TWAPs or start a new chase after reconnecting.".into();
                    for frame in kept.frames(&name) { (port.broadcast)(Some(frame)); }
                    (port.broadcast)(Some(crate::envelope::msg(&name, "store", serde_json::json!({
                        "activeChasers": [], "activeTWAPs": paused,
                    }))));
                    stopped(kept);
                    running.refresh(&[], &paused, 0);
                    handle.dispose();
                    if let Some(signer) = &link_signer { signer.mark_unavailable(); }
                    return;
                }
                Some(AlgoInput::Op { op, mut data, origin, twap_owner: captured_owner, action }) => {
                    if (op.starts_with("twap:") || op == "exec") && captured_owner != twap_owner.snapshot() {
                        let message = "Algo operation cancelled-for-owner-change: authenticated connection binding changed";
                        port.emit("error", serde_json::json!(message).to_string());
                        if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"notSent"}))); }
                        continue;
                    }
                    if let Some(receipt) = action.as_ref() && let Err(message) = receipt.guard().check() {
                        if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"notSent"}))); }
                        continue;
                    }
                    if !running.io_allowed() {
                        port.emit("error", crate::user_drain::refusal().to_string());
                        continue;
                    }
                    // The page acted on frames this store may not have read yet (a chase
                    // started right after load found no market): read the engine first.
                    if let Some(snapshot) = handle.read(projection::project).await {
                        let change = port.apply(snapshot, Vec::new(), false);
                        port.subscribers.borrow_mut().retain(|tx| tx.send(change.clone()).is_ok());
                    }
                    if !running.io_allowed() {
                        port.emit("error", crate::user_drain::refusal().to_string());
                        continue;
                    }
                    if (op.starts_with("twap:") || op == "exec") && captured_owner != twap_owner.snapshot() {
                        let message = "Algo operation cancelled-for-owner-change: authenticated connection binding changed";
                        port.emit("error", serde_json::json!(message).to_string());
                        if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"notSent"}))); }
                        continue;
                    }
                    if let Some(receipt) = action.as_ref() && let Err(message) = receipt.guard().check() {
                        if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"notSent"}))); }
                        continue;
                    }
                    let epoch = running.restore_epoch();
                    if op == "twap:state" && let Ok(mut rows) = serde_json::from_str::<Vec<Value>>(&data) {
                        for row in &mut rows {
                            let pause = row.get("id").and_then(Value::as_str).is_some_and(|id|
                                running.restores.get().is_some_and(|guard| guard.must_pause(id, epoch)));
                            if pause && let Some(state) = row.get_mut("state").and_then(Value::as_object_mut)
                                && state.get("status").and_then(Value::as_str) == Some("running") {
                                state.insert("status".into(), "paused".into());
                            }
                        }
                        data = serde_json::to_string(&rows).unwrap();
                    }
                    let resume = (op == "twap:resume").then(|| serde_json::from_str::<String>(&data).ok()).flatten()
                        .filter(|id| twap.active_twaps(&name).iter().any(|row| row["id"] == id.as_str() && row["status"] == "paused"));
                    if let Some(id) = &resume && let Some(guard) = running.restores.get()
                        && let Err(error) = guard.reserve_resume(id, epoch) {
                        port.emit("error", serde_json::to_string(error).unwrap());
                        continue;
                    }
                    let dispatch = || {
                        // Factory identity/counters belong to the resident account.
                        // Only this EXEC's adapter carries the action command port
                        // and rechecks its fence before deferred factory handoffs.
                        let result = if op == "exec" && let Some(receipt) = action.as_ref() {
                            let guard = receipt.guard();
                            let command_port = Rc::new(ActionNukePort { port: port.clone(), guard: guard.clone() });
                            Some(exec.handle_guarded(&data, &cx, command_port,
                                Rc::new(move || guard.check().map_err(str::to_owned))))
                        } else { registry.dispatch(&op, &data, &cx) };
                        running.refresh(&book.active_chasers(), &twap.active_twaps(&name), exec.pending_operations() + twap.pending_placements());
                        result
                    };
                    let dispatched = if crate::user_drain::starts_job(&op) {
                        if let Some(user) = running.user.get() {
                            match user.admit() {
                                // Admission can wait: re-check the owner after it, right before dispatch.
                                Ok(_admission) if (op.starts_with("twap:") || op == "exec") && captured_owner != twap_owner.snapshot() => {
                                    let message = "Algo operation cancelled-for-owner-change: authenticated connection binding changed";
                                    port.emit("error", serde_json::json!(message).to_string());
                                    if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"notSent"}))); }
                                    continue;
                                }
                                Ok(_admission) => dispatch(),
                                Err(error) => {
                                    port.emit("error", error.to_string());
                                    continue;
                                }
                            }
                        } else { dispatch() }
                    } else { dispatch() };
                    if matches!(dispatched, Some(Ok(()))) && running.restore_epoch() == epoch
                        && let Some(id) = &resume && let Some(guard) = running.restores.get()
                        && twap.active_twaps(&name).iter().any(|row| row["id"] == id.as_str() && row["status"] == "running") {
                        guard.acknowledge_resume(id, epoch);
                    }
                    let result = match &dispatched {
                        Some(Ok(())) => Value::from("ok"),
                        Some(Err(error)) => {
                            serde_json::json!({"message": error.chars().take(160).collect::<String>()})
                        }
                        None => serde_json::json!({"message": "unhandled op"}),
                    };
                    crate::audit::log(&who, &origin, &crate::audit::algo_op(&op, &data), result);
                    if let Some(Err(error)) = dispatched {
                        port.emit("error", Value::String(error).to_string());
                    }
                    publish(&mut published);
                    if let Some(receipt) = action {
                        // Dispatch admission is not proof that an asynchronous job or
                        // its venue writes completed. Ordinary algo events reconcile it.
                        receipt.finish(Err(serde_json::json!({"message":"Algo operation dispatched; reconcile its account events","outcome":"unknown"})));
                    }
                }
                Some(AlgoInput::Nuke { mode, request_id, action }) => {
                    if let Some(receipt) = action.as_ref() && let Err(message) = receipt.guard().check() {
                        if let Some(receipt) = action { receipt.finish(Err(serde_json::json!({"message":message,"outcome":"unknown"}))); }
                        continue;
                    }
                    if !running.io_allowed() || action.as_ref().is_some_and(|receipt| !receipt.active()) {
                        if let Some(receipt) = action { receipt.finish(Err(crate::user_drain::refusal())); }
                        else { (port.emit)(crate::envelope::msg(&name, "callMethod:done", serde_json::json!({
                            "requestId":request_id,"success":false,"result":crate::user_drain::refusal(),
                        }))); }
                        continue;
                    }
                    // The engine may have loaded ahead of this store's frames: read it
                    // now, so the nuke never acts on rows older than what was checked.
                    let seen = handle.read(projection::project).await.map(|snapshot| {
                        let loaded = (snapshot.loaded.markets, snapshot.loaded.positions, snapshot.loaded.orders);
                        let change = port.apply(snapshot, Vec::new(), false);
                        port.subscribers.borrow_mut().retain(|tx| tx.send(change.clone()).is_ok());
                        loaded
                    });
                    let admission = running.user.get().map(|user| user.admit_cleanup());
                    let nuke_cx = match action.as_ref().map(|receipt| receipt.guard()) {
                        Some(guard) => AlgoCx::new(
                            Rc::new(ActionNukePort { port: port.clone(), guard }),
                            cx.scope.clone(),
                        ),
                        None => cx.clone(),
                    };
                    let action_refusal = action.as_ref().and_then(|receipt| receipt.guard().check().err());
                    let started = match (action_refusal, admission.as_ref()) {
                        (Some(message), _) => Err(serde_json::json!({"message":message,"outcome":"unknown"})),
                        _ if action.as_ref().is_some_and(|receipt| !receipt.active() || !running.io_allowed()) => Err(serde_json::json!({"message":"Action registration retired before dispatch","outcome":"notSent"})),
                        (_, Some(Err(error))) => Err(error.clone()),
                        _ => match seen {
                        Some((markets, positions, orders))
                            if crate::methods::nuke_can_see(mode, markets, positions, orders) =>
                        {
                            exec.nuke(mode, &nuke_cx)
                        }
                        _ => Err(serde_json::json!({
                            "message": crate::methods::NUKE_NOT_LOADED,
                            "outcome": "notSent",
                        })),
                        },
                    };
                    running.refresh(&book.active_chasers(), &twap.active_twaps(&name), exec.pending_operations() + twap.pending_placements());
                    drop(admission);
                    let (port, name, who) = (port.clone(), name.clone(), who.clone());
                    let action_started = action.is_some();
                    let settle = move |outcome: Result<Value, Value>| {
                        let audit = match &outcome {
                            Ok(_) => Value::from("ok"),
                            Err(error) => error["message"].clone(),
                        };
                        crate::audit::log(&who, "callMethod", &crate::audit::algo_op("nuke", &Value::from(mode).to_string()), audit);
                        if let Some(receipt) = action { receipt.finish(outcome); return; }
                        let data = match outcome {
                            // Every leg was accepted; the result names the algos stopped first.
                            Ok(done) => {
                                let mut result = serde_json::json!({"stoppedChasers": done["stoppedChasers"], "stoppedTwaps": done["stoppedTwaps"]});
                                for field in ["placedSymbols", "warning"] {
                                    if let Some(value) = done.get(field) {
                                        result[field] = value.clone();
                                    }
                                }
                                serde_json::json!({"requestId": request_id, "success": true, "result": result})
                            }
                            Err(mut error) => {
                                if error.get("name").is_none() {
                                    error["name"] = "Error".into();
                                }
                                serde_json::json!({"requestId": request_id, "success": false, "result": error})
                            }
                        };
                        (port.emit)(crate::envelope::msg(&name, "callMethod:done", data));
                    };
                    let reply = NukeReply(Some(Box::new(settle)));
                    match started {
                        Ok(nuke) => cx.spawn(async move {
                            let result = nuke.await.map_err(|mut error| {
                                // Once native nuke started, a failed later leg cannot
                                // prove the whole action was never sent.
                                if action_started { error["outcome"] = "unknown".into(); }
                                error
                            });
                            reply.settle(result)
                        }),
                        Err(error) => reply.settle(Err(error)),
                    }
                }
                // Chases and TWAPs run on through a socket drop, until the
                // resume window expires (Dispose) or the browser is back.
                Some(AlgoInput::Joined) => {
                    published.clear();
                    publish(&mut published);
                    // What it stopped for while the browser was gone, once.
                    for frame in port.take_for_join() {
                        crate::state::Log::info(&format!("algo notice replayed {name}: {frame}"));
                        (port.emit)(frame);
                    }
                }
                Some(AlgoInput::Dispose { reason, stopped }) => break Some((reason, stopped)),
                None => break Some(("account disposed".to_owned(), None)),
            },
        }
    };
    // Disposal contract: stop TWAPs, let chaser cancels settle, then the scope, then the engine.
    let (disposing, reason, stopped) = match ended {
        Some((reason, stopped)) => (true, reason, stopped),
        None => (false, "algo feed ended".to_owned(), None),
    };
    let reason = reason.as_str();
    let (chasers, twaps) = (book.active_chasers(), twap.active_twaps(&name));
    let stop_notices = |may_rest| stop_notices(&chasers, &twaps, &port.store(), reason, may_rest);
    let (stopped_notices, unsettled_notices) = (stop_notices(false), stop_notices(true));
    // Kept for the next init only when no browser is there to be sent them.
    let unheard_notices = || -> (Option<Notice>, Vec<String>) {
        let (errors, ends) = port.take_unheard_for_dispose();
        let errors = errors
            .into_iter()
            .map(|data| serde_json::from_str(&data).unwrap_or(data))
            .collect();
        (errors_notice(errors), ends)
    };
    let mut kept = Kept::default();
    match &stopped {
        Some(stopped) => {
            let (errors, ends) = unheard_notices();
            kept.notices.extend(errors);
            kept.notices.extend(unsettled_notices.iter().cloned());
            kept.ends = ends;
            if kept != Kept::default() {
                stopped(kept.clone());
            }
        }
        None => {
            for notice in &stopped_notices {
                (port.broadcast)(Some(notice.frame(&name)));
            }
        }
    }
    twap.terminate_all();
    let errors_before = port.errors.get();
    let settling = async {
        twap.placements_settled().await;
        book.shutdown(reason).await;
    };
    // The main loop has ended, so the cancels' answers and the rows they close
    // must still be read here, or every drain waits out its deadline.
    let pumped = async {
        tokio::pin!(settling);
        let mut feed_open = true;
        loop {
            tokio::select! {
                biased;
                () = &mut settling => break,
                batch = feed.next(), if feed_open => {
                    let Some(batch) = batch else {
                        feed_open = false;
                        continue;
                    };
                    for notice in &batch.notices {
                        port.settle(notice);
                    }
                    if batch.rejoined {
                        port.client.set(feed.client());
                        for (_, waiter) in port.pending.borrow_mut().drain() {
                            let mut error =
                                VenueError::new("algo feed rejoined; command outcome unknown");
                            error.outcome = Some(Outcome::Unknown);
                            let _ = waiter.send(Err(error));
                        }
                    }
                    if (batch.frames > 0 || batch.rejoined)
                        && let Some(snapshot) = handle.read(projection::project).await
                    {
                        let change = port.apply(snapshot, batch.fills, batch.rejoined);
                        port.subscribers
                            .borrow_mut()
                            .retain(|tx| tx.send(change.clone()).is_ok());
                    }
                }
            }
        }
    };
    let deadline_passed = tokio::time::timeout(SHUTDOWN_DEADLINE, pumped)
        .await
        .is_err();
    if deadline_passed {
        crate::state::Log::error(
            &format!(
                "{name}: chaser shutdown passed its deadline; cancels may be unsettled and orders may rest"
            ),
            &crate::telemetry::Report {
                kind: "algos.shutdown_deadline_passed",
                summary: "chaser shutdown passed its deadline; cancels may be unsettled and orders may rest",
                user_tag: Some(crate::accounts::user_tag(&who.user)),
                venue: Some(&who.venue),
                ..Default::default()
            },
        );
        // A live browser was told they stopped; now it must check the orders.
        if stopped.is_none() {
            for notice in &unsettled_notices {
                (port.broadcast)(Some(notice.frame(&name)));
            }
        }
    }
    crate::audit::log(
        &who,
        reason,
        &crate::audit::algo_op("terminateAll", "{}"),
        shutdown_result(deadline_passed, port.errors.get() != errors_before),
    );
    // Its cancels can fail too, with no browser to tell: a fuller notice replaces the first.
    if let Some(stopped) = &stopped {
        let (raised, ended) = unheard_notices();
        if raised.is_some() || !ended.is_empty() {
            kept.notices.extend(raised);
            kept.ends.extend(ended);
            stopped(kept);
        }
    }
    scope.dispose();
    handle.leave(feed.client());
    *running.work.lock().unwrap() = (Vec::new(), 0);
    running.set(0, 0);
    if !disposing {
        crate::state::Log::warn(&format!(
            "{name}: algo feed ended; algos stopped, the account keeps running"
        ));
        return;
    }
    handle.dispose();
    if let Some(signer) = &link_signer {
        signer.mark_unavailable();
    }
}

/// Answers a hosted nuke exactly once. Disposing the account aborts the nuke's
/// task mid-leg; the client is then told the outcome is unknown instead of
/// waiting out its deadline with no answer.
type NukeSettle = Box<dyn FnOnce(Result<Value, Value>)>;
struct NukeReply(Option<NukeSettle>);
impl NukeReply {
    fn settle(mut self, outcome: Result<Value, Value>) {
        if let Some(settle) = self.0.take() {
            settle(outcome);
        }
    }
}
impl Drop for NukeReply {
    fn drop(&mut self) {
        if let Some(settle) = self.0.take() {
            settle(Err(serde_json::json!({
                "message": "Nuke interrupted: the account closed before every leg settled; check positions and open orders",
                "outcome": "unknown",
            })));
        }
    }
}

/// A command's completion as the chaser sees it. The kind travels too: the
/// chaser retries a cancel only on a transport kind.
fn completion(notice: &Value) -> Result<String, VenueError> {
    match (notice["result"].as_str(), notice["error"].as_str()) {
        (Some(result), _) => Ok(result.to_owned()),
        (None, Some(message)) => {
            let mut error = VenueError::new(message);
            error.kind = notice["detail"]["kind"].as_str().and_then(error_kind);
            error.outcome = match notice["detail"]["outcome"].as_str() {
                Some("unknown") => Some(Outcome::Unknown),
                Some("rejected") => Some(Outcome::Rejected),
                Some("notSent") => Some(Outcome::NotSent),
                _ => None,
            };
            Err(error)
        }
        (None, None) => Ok("null".into()),
    }
}

/// A chase's end as replayed to a page that was away: marked, so the page tells it
/// once, since the fill toast it would have shown went with the page.
fn replayed(op: &str, data: &str) -> String {
    match serde_json::from_str::<Value>(data) {
        Ok(Value::Object(mut end)) if op == "chaser:complete" => {
            end.insert("replayed".into(), Value::Bool(true));
            Value::Object(end).to_string()
        }
        _ => data.to_owned(),
    }
}

/// A chase's end and its outcome warnings are owed to a page that was away, as errors are.
fn owed_on_rejoin(op: &str) -> bool {
    matches!(op, "warning" | "chaser:complete")
}

fn error_kind(kind: &str) -> Option<ErrorKind> {
    Some(match kind {
        "rejected" => ErrorKind::Rejected,
        "invalidInput" => ErrorKind::InvalidInput,
        "insufficientBalance" => ErrorKind::InsufficientBalance,
        "authInvalid" => ErrorKind::AuthInvalid,
        "authExpired" => ErrorKind::AuthExpired,
        "authPermission" => ErrorKind::AuthPermission,
        "rateLimited" => ErrorKind::RateLimited,
        "network" => ErrorKind::Network,
        "timeout" => ErrorKind::Timeout,
        "venueUnavailable" => ErrorKind::VenueUnavailable,
        "unsupported" => ErrorKind::Unsupported,
        "internal" => ErrorKind::Internal,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_retrying_warning_is_not_replayed_once_its_cancel_ended() {
        let kept = |op: &str, message: &str| {
            let data = serde_json::to_string(message).unwrap();
            Unheard {
                sent: Instant::now(),
                told: Vec::new(),
                owed: true,
                error: None,
                frame: data.clone(),
                retrying: retrying_of(op, &data),
                end: false,
            }
        };
        let retry = "Chaser stopped, but BTCUSDT order 71b464ec is still open: Signing provider is unavailable. Retrying the cancel; it can still fill until then. Cancel it in Open Orders or use Nuke";
        let mut unheard = VecDeque::from([
            kept("warning", retry),
            kept("warning", &retry.replace("71b464ec", "other")),
            kept("chaser:complete", r#"{"id":"c1"}"#),
        ]);
        let ended =
            serde_json::to_string("Chaser stopped: BTCUSDT order 71b464ec is now cancelled")
                .unwrap();
        drop_superseded(&mut unheard, "log", &ended);
        assert_eq!(unheard.len(), 3, "only a warning or error ends a cancel");
        drop_superseded(&mut unheard, "warning", &ended);
        assert_eq!(unheard.len(), 2);
        assert!(
            unheard
                .iter()
                .all(|kept| kept.retrying.as_deref() != Some("71b464ec"))
        );
        let failed = serde_json::to_string(
            "Chaser could not cancel BTCUSDT order a1, other: refused; it may still be open. Cancel it in Open Orders or use Nuke",
        )
        .unwrap();
        drop_superseded(&mut unheard, "error", &failed);
        assert_eq!(unheard.len(), 1, "the chase's end stays");
    }

    #[test]
    fn a_chase_end_replayed_to_a_page_that_was_away_says_so() {
        let end = r#"{"id":"c1","symbol":"BTCUSDT","size":0.001,"filled":0.001,"stopReason":"Completed"}"#;
        let replay: Value = serde_json::from_str(&replayed("chaser:complete", end)).unwrap();
        assert_eq!(replay["replayed"], true);
        assert_eq!(replay["filled"], 0.001);
        assert_eq!(
            replayed("warning", "\"Chaser stopped: x\""),
            "\"Chaser stopped: x\""
        );
    }

    #[test]
    fn a_chase_end_and_its_warnings_are_replayed_to_a_page_that_was_away() {
        assert!(owed_on_rejoin("chaser:complete"));
        assert!(owed_on_rejoin("warning"));
        assert!(!owed_on_rejoin("log"));
        assert!(!owed_on_rejoin("twap:stateUpdate"));
    }

    #[test]
    fn a_failed_command_keeps_its_kind_for_the_chaser() {
        let notice = json!({"type":"done","command":7,"error":"HTTP transport request failed",
            "detail":{"kind":"network","outcome":"unknown"}});
        let error = completion(&notice).unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::Network));
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        for kind in [
            ErrorKind::Rejected,
            ErrorKind::InvalidInput,
            ErrorKind::InsufficientBalance,
            ErrorKind::AuthInvalid,
            ErrorKind::AuthExpired,
            ErrorKind::AuthPermission,
            ErrorKind::RateLimited,
            ErrorKind::Network,
            ErrorKind::Timeout,
            ErrorKind::VenueUnavailable,
            ErrorKind::Unsupported,
            ErrorKind::Internal,
        ] {
            assert_eq!(error_kind(kind.as_str()), Some(kind));
        }
    }

    #[test]
    fn an_abandoned_command_still_writes_its_audit_line_once() {
        thread_local! {
            static LINES: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let audit = || AuditLine {
            who: crate::audit::Who {
                user: "u".into(),
                account: "acct".into(),
                venue: "bybit".into(),
            },
            fields: serde_json::Map::new(),
            write: |line| LINES.with(|lines| lines.borrow_mut().push(line.clone())),
        };
        drop(audit());
        audit().settled("ok".into());
        LINES.with(|lines| {
            let lines = lines.borrow();
            assert_eq!(lines.len(), 2);
            assert_eq!(lines[0]["result"]["outcome"], "unknown");
            assert_eq!(lines[1]["result"], "ok");
        });
    }

    #[test]
    fn the_chaser_waits_out_a_sign_before_calling_a_cancel_unanswered() {
        assert!(
            std::time::Duration::from_millis(safe_cex_chaser::ANSWER_BOUND_MS)
                > safe_cex_remote_signer::SIGN_TIMEOUT
        );
    }

    #[test]
    fn the_audit_reads_ok_only_when_every_cancel_settled() {
        assert_eq!(shutdown_result(false, false), Value::from("ok"));
        assert!(
            shutdown_result(true, false)["message"]
                .as_str()
                .unwrap()
                .contains("unsettled")
        );
        assert!(
            shutdown_result(false, true)["message"]
                .as_str()
                .unwrap()
                .contains("failed")
        );
    }

    /// What `exchangeAccountController` hands its listeners for an algo op.
    fn web_listener_args(frame: &str) -> (String, Vec<Value>) {
        let frame: Value = serde_json::from_str(frame).unwrap();
        let op = frame["d"]["op"].as_str().unwrap().to_owned();
        let data = frame["d"]["data"].clone();
        let args = match op.as_str() {
            // `emit(op, ...(Array.isArray(data) ? data : [data]))`
            "error" | "warning" | "log" | "exec-log" | "exec-result" => match data {
                Value::Array(items) => items,
                other => vec![other],
            },
            // `const [event] = Array.isArray(data) ? data : [data]`
            "chaser:complete" => match data {
                Value::Array(items) => items.into_iter().take(1).collect(),
                other => vec![other],
            },
            // Persisted only when `data` is a non-array object.
            "twap:stateUpdate" => match data {
                Value::Object(_) => vec![data],
                _ => vec![json!({})],
            },
            other => panic!("unexpected algo op {other}"),
        };
        (op, args)
    }

    fn emitted(op: &str, data: Value) -> Vec<Value> {
        let (seen, args) = web_listener_args(&algo_msg("acct", op, data.to_string()));
        assert_eq!(seen, op);
        args
    }

    #[test]
    fn algo_messages_carry_data_verbatim() {
        let frame: Value = serde_json::from_str(&algo_msg(
            "acct",
            "twap:stateUpdate",
            json!({"t": 1}).to_string(),
        ))
        .unwrap();
        assert_eq!(
            frame["d"],
            json!({"op": "twap:stateUpdate", "accountName": "acct", "data": {"t": 1}})
        );
    }

    #[test]
    fn every_algo_op_reaches_web_listeners_in_the_worker_shape() {
        // TWAP: the state object clients persist, and errors as one message.
        let state =
            json!({"twap-1": {"options": {"accountName": "acct"}, "state": {"status": "running"}}});
        assert_eq!(emitted("twap:stateUpdate", state.clone()), [state]);
        assert_eq!(
            emitted("error", json!("TWAP BTCUSDT rejected: x")),
            [json!("TWAP BTCUSDT rejected: x")]
        );
        // Chaser: warning, error and log messages, and the completion event.
        for op in ["warning", "error", "log"] {
            assert_eq!(
                emitted(op, json!("[CHASERc1] m")),
                [json!("[CHASERc1] m")],
                "{op}"
            );
        }
        let complete = json!({"id": "c1", "symbol": "BTCUSDT", "filled": 1.0});
        assert_eq!(emitted("chaser:complete", complete.clone()), [complete]);
        // Exec: the result and log objects.
        let result = json!({"reqId": "r1", "handled": true, "success": true});
        assert_eq!(emitted("exec-result", result.clone()), [result]);
        let log = json!({"message": "m", "reqId": "r1"});
        assert_eq!(emitted("exec-log", log.clone()), [log]);
        // The server's own refusal of an algo op.
        assert_eq!(
            emitted("error", Value::String("unhandled op".into())),
            [json!("unhandled op")]
        );
    }

    #[test]
    fn balance_info_is_the_payload_with_the_lighter_tier_merged_in() {
        let wallet = serde_json::json!({"accountType": "UNIFIED", "coin": []});
        assert_eq!(balance_info(Some(&wallet), None), Some(wallet.clone()));
        let assets = serde_json::json!([{"asset": "USDT"}]);
        assert_eq!(balance_info(Some(&assets), None), Some(assets));
        assert_eq!(
            balance_info(None, Some(&Value::from("premium"))),
            Some(serde_json::json!({"lighterAccountLimits": {"user_tier_name": "premium"}}))
        );
        assert_eq!(
            balance_info(Some(&wallet), Some(&Value::from("plus"))).unwrap()["accountType"],
            "UNIFIED"
        );
        assert_eq!(balance_info(None, None), None);
    }
}

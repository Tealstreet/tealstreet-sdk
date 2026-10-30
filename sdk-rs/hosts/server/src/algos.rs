//! Server-side algos (chaser, TWAP, exec) for one private account: an
//! `AccountPort` over `AccountHandle`, run on an engine thread's `LocalSet`.
use safe_cex_algos::projection::{self, Snapshot};
use safe_cex_algos::{AccountPort, AlgoCx, AlgoStore, Change, CommandFuture, Registry, Scope};
use safe_cex_chaser::ActiveChaser;
use safe_cex_engine::engine::{Command, CommandId, Fill, VenueAction};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::OrderStatus;
use safe_cex_native::{AccountHandle, Outgoing};
use serde_json::Value;
use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

/// What a session asks of an account's algos.
pub enum AlgoInput {
    Op {
        op: String,
        data: String,
        /// The socket that sent it, for the order audit line.
        origin: String,
    },
    /// A hosted `nuke`/`nukePositions`/`nukeOrders` call: exec's nuke, answered
    /// with `callMethod:done` once every leg has settled.
    Nuke {
        mode: &'static str,
        request_id: Value,
    },
    /// A socket joined (a resume or another tab): resend the algo store rows.
    Joined,
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

/// Keeps a stop notice for a client that is not there to see it.
pub type StopNotice = Box<dyn Fn(String) + Send>;

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
    data: String,
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
        if let Command::PlaceOrders { orders, .. } = &command {
            self.placed
                .record(orders.iter().map(|order| order.symbol.as_str()));
        }
        let audit = crate::audit::typed(&command).map(|fields| (self.who.clone(), fields));
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
            if let Some((who, fields)) = audit {
                crate::audit::log(&who, "algo", &fields, crate::audit::venue_result(&result));
            }
            result
        })
    }
    fn emit(&self, op: &str, data: String) {
        if op == "error" {
            self.errors.set(self.errors.get() + 1);
            crate::state::Log::warn(&format!("algo error {}: {data}", self.name));
            let holding = (self.broadcast)(Some(algo_error(&self.name, &data)));
            self.keep_unheard(data, holding);
            return;
        }
        (self.emit)(algo_msg(&self.name, op, data));
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

/// Which algos stopped, and with `may_rest` that their orders may still be open:
/// with no browser, a cancel cannot be signed.
fn stop_notice(
    chasers: &[ActiveChaser],
    twaps: &[Value],
    store: &AlgoStore,
    reason: &str,
    may_rest: bool,
) -> Option<String> {
    let chases = chasers.iter().map(|chase| {
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
        format!("Chase on {} stopped: {reason}.{check}", chase.symbol)
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
        format!("TWAP on {market} stopped: {reason}.{check}")
    });
    let lines: Vec<String> = chases.chain(twaps).collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
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

    fn keep_unheard(&self, data: String, holding: Holding) {
        let mut unheard = self.unheard.borrow_mut();
        unheard.retain(|error| self.still_unheard(error));
        if unheard.len() == UNHEARD_ERRORS {
            unheard.pop_front();
        }
        unheard.push_back(Unheard {
            sent: Instant::now(),
            owed: holding.parked || holding.live.is_empty(),
            told: holding.live,
            data,
        });
    }

    /// The errors no frame followed, once.
    fn take_unheard(&self) -> Vec<String> {
        self.unheard
            .borrow_mut()
            .drain(..)
            .filter(|error| self.still_unheard(error))
            .map(|error| error.data)
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
            replay.push(error.data.clone());
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
        let result = match (notice["result"].as_str(), notice["error"].as_str()) {
            (Some(result), _) => Ok(result.to_owned()),
            (None, Some(message)) => {
                let mut error = VenueError::new(message);
                error.outcome = match notice["detail"]["outcome"].as_str() {
                    Some("unknown") => Some(Outcome::Unknown),
                    Some("rejected") => Some(Outcome::Rejected),
                    Some("notSent") => Some(Outcome::NotSent),
                    _ => None,
                };
                Err(error)
            }
            (None, None) => Ok("null".into()),
        };
        let _ = waiter.send(result);
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
            (port.emit)(crate::envelope::msg(&name, "store", rows));
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
    });
    let scope = Scope::new();
    let cx = AlgoCx::new(port.clone(), scope.clone());
    let chasers = safe_cex_chaser::ChaserFactory::new(cx.clone());
    let book = chasers.book();
    let twap = Rc::new(safe_cex_algo_twap::factory::TwapFactory::new(
        &name,
        cx.clone(),
        Rc::new(book.clone()),
    ));
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
                    CancelAll::EachOpenSymbol => CancelAllScope::Symbols(Vec::new()),
                    CancelAll::EachPlacedSymbol => CancelAllScope::Symbols(placed.symbols()),
                }
            })),
    );
    let registry = Registry::new(vec![
        Box::new(chasers),
        Box::new(twap.clone()),
        Box::new(exec.clone()),
    ]);
    let mut published = String::new();
    // `store` rows the frames do not carry (`activeChasers`, `activeTWAPs`).
    let publish = |published: &mut String| {
        let rows = serde_json::json!({
            "activeChasers": book.active_chasers(),
            "activeTWAPs": twap.active_twaps(&name),
        });
        let text = rows.to_string();
        if *published != text {
            (port.emit)(crate::envelope::msg(&name, "store", rows));
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
                Some(AlgoInput::Op { op, data, origin }) => {
                    let dispatched = registry.dispatch(&op, &data, &cx);
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
                }
                Some(AlgoInput::Nuke { mode, request_id }) => {
                    let started = exec.nuke(mode, &cx);
                    let (port, name, who) = (port.clone(), name.clone(), who.clone());
                    let settle = move |outcome: Result<Value, Value>| {
                        let audit = match &outcome {
                            Ok(_) => Value::from("ok"),
                            Err(error) => error["message"].clone(),
                        };
                        crate::audit::log(&who, "callMethod", &crate::audit::algo_op("nuke", &Value::from(mode).to_string()), audit);
                        let data = match outcome {
                            // Every leg was accepted; the result names the algos stopped first.
                            Ok(done) => serde_json::json!({"requestId": request_id, "success": true,
                                "result": {"stoppedChasers": done["stoppedChasers"], "stoppedTwaps": done["stoppedTwaps"]}}),
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
                        Ok(nuke) => cx.spawn(async move { reply.settle(nuke.await) }),
                        Err(error) => reply.settle(Err(error)),
                    }
                }
                // Chases and TWAPs run on through a socket drop, until the
                // resume window expires (Dispose) or the browser is back.
                Some(AlgoInput::Joined) => {
                    published.clear();
                    publish(&mut published);
                    // What it stopped for while the browser was gone, once.
                    for data in port.take_for_join() {
                        crate::state::Log::info(&format!("algo error replayed {name}: {data}"));
                        (port.emit)(algo_error(&name, &data));
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
    let stop_notice = |may_rest| stop_notice(&chasers, &twaps, &port.store(), reason, may_rest);
    let (stopped_notice, unsettled_notice) = (stop_notice(false), stop_notice(true));
    // Kept for the next init only when no browser is there to be sent them.
    let unheard_notices = || -> Vec<String> {
        port.take_unheard()
            .into_iter()
            .map(|data| serde_json::from_str(&data).unwrap_or(data))
            .collect()
    };
    let mut notices = Vec::new();
    match &stopped {
        Some(stopped) => {
            notices = unheard_notices();
            notices.extend(unsettled_notice.clone());
            if !notices.is_empty() {
                stopped(notices.join("\n"));
            }
        }
        None => {
            if let Some(notice) = stopped_notice {
                (port.broadcast)(Some(crate::envelope::algo_error(
                    &name,
                    Value::String(notice),
                )));
            }
        }
    }
    twap.terminate_all();
    let errors_before = port.errors.get();
    let deadline_passed = tokio::time::timeout(SHUTDOWN_DEADLINE, book.shutdown(reason))
        .await
        .is_err();
    if deadline_passed {
        crate::state::Log::warn(&format!(
            "{name}: chaser shutdown passed its deadline; cancels may be unsettled and orders may rest"
        ));
        // A live browser was told they stopped; now it must check the orders.
        if stopped.is_none()
            && let Some(notice) = unsettled_notice
        {
            (port.broadcast)(Some(crate::envelope::algo_error(
                &name,
                Value::String(notice),
            )));
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
        let raised = unheard_notices();
        if !raised.is_empty() {
            notices.extend(raised);
            stopped(notices.join("\n"));
        }
    }
    scope.dispose();
    handle.leave(feed.client());
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

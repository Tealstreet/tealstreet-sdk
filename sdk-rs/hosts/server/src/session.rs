//! One `/account` socket: the JSON envelope over this user's accounts.
//! Every account op resolves through (this socket's user, accounts it joined).
use crate::accounts::now_ms;
use crate::accounts::{Account, PUBLIC_ACCOUNT, PUBLIC_EXCHANGE, Registry};
use crate::envelope::{self, AccountMsg, Inbound, InitPayload};
use crate::methods::{self, Plan};
use crate::out::Out;
use crate::state::{Log, Metrics};
use safe_cex_engine::capabilities::CancelAll;
use safe_cex_native::{AccountHandle, ClientLink, Outgoing, SocketSummary};
use safe_cex_remote_signer::Outbox;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;

/// Who connected, as verified before the upgrade.
#[derive(Clone)]
pub struct Identity {
    /// `None` for an unauthenticated public-data socket.
    pub user: Option<String>,
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
    client: Client,
    old: Vec<Client>,
    next_command: u32,
    /// Book and candle subscribe commands this socket holds, by command text and count.
    /// The engine keeps one demand per client, so a re-snapshot replays each key once.
    subscriptions: HashMap<String, u32>,
}

pub struct Session {
    registry: Arc<Registry>,
    metrics: Arc<Metrics>,
    identity: Identity,
    pub socket: u64,
    pub resume_id: String,
    out: Out,
    joined: HashMap<String, Joined>,
    /// A held client's account has loaded: `(account name, client id)`.
    loaded: tokio::sync::mpsc::UnboundedSender<(String, u64)>,
    loaded_rx: Option<tokio::sync::mpsc::UnboundedReceiver<(String, u64)>>,
}

/// A private account's first snapshot waits for its orders and positions, so a
/// fresh build never shows the client an empty account; past this it goes anyway.
pub const SNAPSHOT_HOLD_LIMIT: std::time::Duration = std::time::Duration::from_secs(30);
const SNAPSHOT_HOLD_CHECK: std::time::Duration = std::time::Duration::from_millis(250);

const PUBLIC_OPS: [&str; 5] = [
    "sendSnapshot",
    "subscribeOrderBook",
    "subscribeOHLCV",
    "unsubscribeOrderBook",
    "unsubscribeOHLCV",
];

/// Why the socket must close with 1008.
pub struct PolicyViolation(pub &'static str);

impl Session {
    pub fn new(
        registry: Arc<Registry>,
        metrics: Arc<Metrics>,
        identity: Identity,
        out: Out,
    ) -> Self {
        let socket = registry.next_id();
        let (loaded, loaded_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            registry,
            metrics,
            identity,
            socket,
            resume_id: crate::accounts::resume_id(),
            out,
            joined: HashMap::new(),
            loaded,
            loaded_rx: Some(loaded_rx),
        }
    }

    /// The socket loop's receiver of held clients whose accounts have loaded;
    /// each goes to [`Session::account_loaded`].
    pub fn take_loaded(&mut self) -> tokio::sync::mpsc::UnboundedReceiver<(String, u64)> {
        self.loaded_rx.take().expect("taken once")
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
        let _ = self.out.send(text);
    }

    fn outbox(&self) -> Outbox {
        let out = self.out.clone();
        Arc::new(move |text: String| out.send(text))
    }

    /// Called once after the upgrade, for `resumable=1` sockets.
    pub async fn open(&mut self) {
        if !self.identity.resumable {
            return;
        }
        let mut resumed = false;
        if let (Some(from), Some(user), Some(token)) = (
            self.identity.resume_from.clone(),
            self.identity.user.clone(),
            self.identity.token.clone(),
        ) {
            let accounts = self
                .registry
                .resume(&from, &user, self.socket, token, &self.outbox());
            resumed = !accounts.is_empty();
            // The client re-inits each one; an identical init then keeps it.
            for account in accounts {
                self.join(account).await;
            }
        }
        self.send(envelope::session(&self.resume_id, resumed));
    }

    pub async fn handle(&mut self, text: &str) -> Result<(), PolicyViolation> {
        let Ok(message) = serde_json::from_str::<Inbound>(text) else {
            Log::warn("invalid envelope");
            return Ok(());
        };
        let authenticated = self.identity.user.is_some();
        match message {
            Inbound::Init(init) => self.init(*init).await?,
            Inbound::Msg(msg) => {
                if !authenticated
                    && !(msg.account_name.as_deref() == Some(PUBLIC_ACCOUNT)
                        && PUBLIC_OPS.contains(&msg.op.as_str()))
                {
                    return Err(PolicyViolation(
                        "Unauthenticated exchange-server message rejected",
                    ));
                }
                self.msg(msg).await;
            }
            Inbound::SignRes(response) => {
                if !authenticated {
                    return Err(PolicyViolation(
                        "Unauthenticated exchange-server sessions cannot sign",
                    ));
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
                if let Some(joined) = self.joined.remove(&account_name) {
                    self.metrics.note_dispose();
                    self.leave(joined);
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
                            self.leave(joined);
                        }
                    }
                }
            }
            Inbound::Perf(_) => {}
            Inbound::Verbose(_) => {
                if !authenticated {
                    return Err(PolicyViolation(
                        "Unauthenticated exchange-server sessions cannot enable diagnostics",
                    ));
                }
            }
        }
        Ok(())
    }

    async fn init(&mut self, init: InitPayload) -> Result<(), PolicyViolation> {
        let authenticated = self.identity.user.is_some();
        let public_only = init.public_only == Some(true) || init.account_name.starts_with("__pub_");
        if !authenticated && let Some(reason) = unauthenticated_init_refusal(&init) {
            self.send(envelope::error(Some(&init.account_name), reason));
            return Err(PolicyViolation(reason));
        }
        Log::info(&format!(
            "init {} ({})",
            init.account_name, init.exchange_name
        ));
        if let Some(joined) = self.joined.get(&init.account_name)
            && joined.account.fingerprint == init.fingerprint()
        {
            // Resumed (or repeated) with what it was built from: a fresh snapshot only.
            self.resnapshot(&init.account_name).await;
            if let Some(link) = &self.joined[&init.account_name].account.link {
                link.attach(self.socket, self.outbox());
            }
            return Ok(());
        }
        if let Some(joined) = self.joined.remove(&init.account_name) {
            self.leave(joined);
        }
        let token = self.identity.token.clone();
        let account = self
            .registry
            .obtain(
                self.user(),
                &init,
                public_only,
                self.socket,
                token,
                self.outbox(),
            )
            .await;
        match account {
            Ok(account) => {
                self.metrics.note_init();
                self.join(account).await;
            }
            Err(error) => {
                Log::warn(&format!("init {} refused: {error}", init.account_name));
                self.send(envelope::error(Some(&init.account_name), &error));
            }
        }
        Ok(())
    }

    async fn join(&mut self, account: Arc<Account>) {
        let name = account.name.clone();
        match self.client(&account).await {
            Ok(client) => {
                if let Some(algos) = &account.algos {
                    let _ = algos.send(crate::algos::AlgoInput::Joined);
                }
                self.joined.insert(
                    name,
                    Joined {
                        account,
                        client,
                        old: Vec::new(),
                        next_command: 0,
                        subscriptions: HashMap::new(),
                    },
                );
            }
            Err(error) => {
                self.send(envelope::error(Some(&name), error));
                self.registry.release(&account, self.socket);
            }
        }
    }

    async fn client(&self, account: &Arc<Account>) -> Result<Client, &'static str> {
        let id = self.registry.next_id();
        let link = account.handle.join(id, true).await?;
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let draining = Arc::new(AtomicBool::new(false));
        let hold = (!account.is_public()).then(|| Hold {
            client: id,
            loaded: self.loaded.clone(),
        });
        let pump = tokio::spawn(pump(
            link,
            account.handle.clone(),
            account.name.clone(),
            self.out.clone(),
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
        let Some(account) = self.joined.get(name).map(|joined| joined.account.clone()) else {
            return false;
        };
        match self.client(&account).await {
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
                self.send(envelope::error(Some(name), error));
                false
            }
        }
    }

    fn leave(&self, joined: Joined) {
        for client in std::iter::once(joined.client).chain(joined.old) {
            client.pump.abort();
            joined.account.handle.leave(client.id);
        }
        self.registry.release(&joined.account, self.socket);
    }

    async fn msg(&mut self, msg: AccountMsg) {
        let Some(name) = msg.account_name.clone() else {
            return;
        };
        if !self.joined.contains_key(&name) {
            Log::warn(&format!("no joined account for op {}", msg.op));
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
            ("subscribeOrderBook", Some(symbol), _) => self.subscribe(
                &name,
                json!({"op": "subscribeOrderBook", "symbol": symbol}),
                json!({"op": "unsubscribeOrderBook", "symbol": symbol}),
                true,
            ),
            ("unsubscribeOrderBook", Some(symbol), _) => self.subscribe(
                &name,
                json!({"op": "subscribeOrderBook", "symbol": symbol}),
                json!({"op": "unsubscribeOrderBook", "symbol": symbol}),
                false,
            ),
            ("subscribeOHLCV", Some(symbol), Some(interval)) => self.subscribe(
                &name,
                json!({"op": "subscribeCandles", "symbol": symbol, "interval": interval}),
                json!({"op": "unsubscribeCandles", "symbol": symbol, "interval": interval}),
                true,
            ),
            ("unsubscribeOHLCV", Some(symbol), Some(interval)) => self.subscribe(
                &name,
                json!({"op": "subscribeCandles", "symbol": symbol, "interval": interval}),
                json!({"op": "unsubscribeCandles", "symbol": symbol, "interval": interval}),
                false,
            ),
            ("callMethod", _, _) => self.call(&name, msg.data).await,
            ("updatePositions", _, _) => self.send(envelope::msg(
                &name,
                "updatePositions:done",
                json!({"requestId": msg.data.get("requestId"), "success": true}),
            )),
            (op, _, _) if crate::algos::OPS.contains(&op) => {
                let algos = self.joined[&name].account.algos.clone();
                let sent = algos.is_some_and(|algos| {
                    algos
                        .send(crate::algos::AlgoInput::Op {
                            op: op.to_owned(),
                            data: msg.data.to_string(),
                            origin: format!("socket:{}", self.socket),
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

    /// Counts a book/candle (un)subscribe; only the first subscribe and the last
    /// unsubscribe reach the engine, whose per-client demand is a set.
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
                self.reply_failure(
                    name,
                    &request_id,
                    "TypeError",
                    "args must be an array",
                    None,
                );
                return;
            }
        };
        let account = self.joined[name].account.clone();
        let symbols = if method == "cancelAllOrders" {
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
            // As `cancelEachPlacedSymbol`: frames can lag a placement's ACK.
            if account.handle.capabilities().cancel_all == CancelAll::EachPlacedSymbol {
                symbols.extend(account.placed.symbols());
            }
            symbols
        } else {
            Vec::new()
        };
        let plans = match methods::translate(
            &method,
            &args,
            account.handle.capabilities(),
            &symbols,
            account.is_public(),
        ) {
            Ok(plans) => plans,
            Err(refused) => {
                if crate::audit::ORDER_METHODS.contains(&method.as_str()) {
                    let who = audit_who(&account);
                    let origin = format!("socket:{}", self.socket);
                    let fields = crate::audit::refused_call(&method, &args);
                    let detail = json!({"name": refused.name, "message": refused.message});
                    crate::audit::log(&who, &origin, &fields, crate::audit::result(&Err(detail)));
                }
                self.reply_failure(name, &request_id, refused.name, &refused.message, None);
                return;
            }
        };
        let group = (plans.len() > 1).then(|| {
            Arc::new(Mutex::new(Group {
                remaining: plans.len(),
                failures: Vec::new(),
            }))
        });
        for plan in plans {
            match plan {
                Plan::Reply(result) => self.send(envelope::msg(
                    name,
                    "callMethod:done",
                    json!({"requestId": request_id, "success": true, "result": result}),
                )),
                Plan::Nuke(mode) => {
                    let algos = self.joined[name].account.algos.clone();
                    let sent = algos.is_some_and(|algos| {
                        algos
                            .send(crate::algos::AlgoInput::Nuke {
                                mode,
                                request_id: request_id.clone(),
                            })
                            .is_ok()
                    });
                    if !sent {
                        self.reply_failure(
                            name,
                            &request_id,
                            "Error",
                            "nuke is unavailable for this account",
                            Some("notSent"),
                        );
                    }
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
                    let (handle, out) = (account.handle.clone(), self.out.clone());
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
                Plan::AfterLoad { command, reply } => {
                    self.send_command(name, &request_id, &method, &group, command, reply, true)
                }
                Plan::Command { command, reply } => {
                    self.send_command(name, &request_id, &method, &group, command, reply, false)
                }
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
    ) {
        // An after-load wait polls the account like a waiting read, under the same cap.
        let permit = if after_load {
            match self.joined[name].account.reads.clone().try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => {
                    self.reply_failure(
                        name,
                        request_id,
                        "Error",
                        "too many account reads in flight; retry shortly",
                        None,
                    );
                    return;
                }
            }
        } else {
            None
        };
        let joined = self.joined.get_mut(name).unwrap();
        joined.account.placed.record_command(&command);
        let Some(id) = joined.next_command.checked_add(1) else {
            self.reply_failure(name, request_id, "Error", "command IDs exhausted", None);
            return;
        };
        joined.next_command = id;
        command["command"] = id.into();
        let audit = crate::audit::command(&command).map(|fields| Audit {
            who: audit_who(&joined.account),
            origin: format!("socket:{}/client:{}", self.socket, joined.client.id),
            fields,
        });
        joined.client.pending.lock().unwrap().insert(
            id,
            Pending {
                request_id: request_id.clone(),
                method: method.to_owned(),
                reply,
                group: group.clone(),
                incomplete: false,
                audit,
            },
        );
        let (client, handle) = (joined.client.id, joined.account.handle.clone());
        let (pending, out, name) = (
            joined.client.pending.clone(),
            self.out.clone(),
            name.to_owned(),
        );
        let waiter =
            after_load.then(|| (handle.clone(), pending.clone(), out.clone(), name.clone()));
        let send = move || {
            let failure = match handle.input(client, &command.to_string()) {
                Ok(true) => None,
                // Not queued, so not submitted: a definite failure, never retried.
                Ok(false) => Some("account is busy; command not sent".to_owned()),
                Err(refused) => Some(refused.notice.to_string()),
            };
            if let Some(message) = failure
                && let Some(pending) = pending.lock().unwrap().remove(&id)
            {
                settle(
                    &out,
                    &name,
                    pending,
                    Err(failure_detail("Error", &message, false)),
                );
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

    fn reply_failure(
        &self,
        name: &str,
        request_id: &Value,
        kind: &str,
        message: &str,
        outcome: Option<&str>,
    ) {
        let mut result = json!({"name": kind, "message": message});
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
        let joined: Vec<Joined> = self.joined.drain().map(|(_, joined)| joined).collect();
        let mut parked = Vec::new();
        for joined in joined {
            for client in std::iter::once(joined.client).chain(joined.old) {
                client.pump.abort();
                joined.account.handle.leave(client.id);
            }
            if self.identity.resumable
                && self.identity.user.is_some()
                && !joined.account.is_public()
            {
                parked.push((joined.account, self.socket));
            } else {
                self.registry.release(&joined.account, self.socket);
            }
        }
        if let Some(user) = &self.identity.user {
            self.registry.park(self.resume_id.clone(), user, parked);
        }
    }
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
                    &format!("{name} cancel-all failed: {}", group.failures.join("; ")),
                    false,
                ))
            }
        }
    };
    let data = match outcome {
        Ok(result) => {
            let result = match pending.reply {
                methods::Reply::Result => result,
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

async fn account_loaded(handle: &AccountHandle) -> bool {
    handle
        .read(|store| {
            let loaded = store.loaded();
            loaded.orders && loaded.positions
        })
        .await
        .unwrap_or(false)
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

async fn pump(
    mut link: ClientLink,
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
                None => break,
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
                if let Some(since) = held {
                    release_hold(&handle, &name, &mut hold, since).await;
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
            let _ = out.send(envelope::msg(name, "engine-event", notice.clone()));
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
                    let pump = tokio::task::spawn_local(pump(
                        link,
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

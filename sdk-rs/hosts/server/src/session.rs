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
}

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
        Self {
            registry,
            metrics,
            identity,
            socket,
            resume_id: crate::accounts::resume_id(),
            out,
            joined: HashMap::new(),
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
        let pump = tokio::spawn(pump(
            link,
            account.handle.clone(),
            account.name.clone(),
            self.out.clone(),
            pending.clone(),
            draining.clone(),
        ));
        Ok(Client {
            id,
            pump,
            pending,
            draining,
        })
    }

    /// A new client on the same account takes a full snapshot (seq 1); the old
    /// one stops forwarding frames and leaves once its replies are in.
    async fn resnapshot(&mut self, name: &str) {
        let Some(account) = self.joined.get(name).map(|joined| joined.account.clone()) else {
            return;
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
            }
            Err(error) => self.send(envelope::error(Some(name), error)),
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
            ("sendSnapshot", _, _) => self.resnapshot(&name).await,
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
                Plan::Read(read) => {
                    let closed = || methods::Refused {
                        name: "Error",
                        message: "account is closed".into(),
                    };
                    let from_frame = read.from_frame();
                    let mut answer = account
                        .handle
                        .read(move |store| methods::read_store(&read, store))
                        .await
                        .unwrap_or_else(|| Err(closed()));
                    if from_frame && answer.is_ok() {
                        use base64::Engine as _;
                        answer = account
                            .handle
                            .snapshot_frame()
                            .await
                            .map(|frame| {
                                let frame = base64::engine::general_purpose::STANDARD.encode(frame);
                                json!({ "accountFrame": frame })
                            })
                            .ok_or_else(closed);
                    }
                    match answer {
                        Ok(result) => self.send(envelope::msg(
                            name,
                            "callMethod:done",
                            json!({"requestId": request_id, "success": true, "result": result}),
                        )),
                        Err(refused) => self.reply_failure(
                            name,
                            &request_id,
                            refused.name,
                            &refused.message,
                            None,
                        ),
                    }
                }
                Plan::Command { mut command, reply } => {
                    account.placed.record_command(&command);
                    let joined = self.joined.get_mut(name).unwrap();
                    let Some(id) = joined.next_command.checked_add(1) else {
                        self.reply_failure(
                            name,
                            &request_id,
                            "Error",
                            "command IDs exhausted",
                            None,
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
                    joined.client.pending.lock().unwrap().insert(
                        id,
                        Pending {
                            request_id: request_id.clone(),
                            method: method.clone(),
                            reply,
                            group: group.clone(),
                            incomplete: false,
                            audit,
                        },
                    );
                    let client = joined.client.id;
                    let sent = joined.account.handle.input(client, &command.to_string());
                    let failure = match sent {
                        Ok(true) => None,
                        // Not queued, so not submitted: a definite failure, never retried.
                        Ok(false) => Some("account is busy; command not sent".to_owned()),
                        Err(refused) => Some(refused.notice.to_string()),
                    };
                    if let Some(message) = failure {
                        let pending = self.joined[name].client.pending.lock().unwrap().remove(&id);
                        if let Some(pending) = pending {
                            settle(
                                &self.out,
                                name,
                                pending,
                                Err(failure_detail("Error", &message, false)),
                            );
                        }
                    }
                }
            }
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
            // Chases die with the socket that may have started them, as today.
            if let Some(algos) = &joined.account.algos {
                let _ = algos.send(crate::algos::AlgoInput::SocketClosed);
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
const SOCKET_HEALTH_EVERY: std::time::Duration = std::time::Duration::from_secs(5);

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
async fn pump(
    mut link: ClientLink,
    handle: AccountHandle,
    name: String,
    out: Out,
    pending: Arc<Mutex<HashMap<u32, Pending>>>,
    draining: Arc<AtomicBool>,
) {
    let mut seq = 0u64;
    let mut health_tick = tokio::time::interval(SOCKET_HEALTH_EVERY);
    let mut sent_health = Value::Null;
    let report_health = |sent: &mut Value| {
        let health = socket_health(handle.socket_health().summary(now_ms()));
        if *sent == health {
            return true;
        }
        *sent = health.clone();
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

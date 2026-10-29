//! Server-side algos (chaser, TWAP, exec) for one private account: an
//! `AccountPort` over `AccountHandle`, run on an engine thread's `LocalSet`.
use safe_cex_algos::{AccountPort, AlgoCx, AlgoStore, Change, CommandFuture, Registry, Scope};
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::{
    Balance, InstrumentClassification, Loaded, Market, Order, Position, Ticker,
};
use safe_cex_native::{AccountHandle, Outgoing};
use serde_json::Value;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// What a session asks of an account's algos.
pub enum AlgoInput {
    Op {
        op: String,
        data: String,
    },
    /// A socket of this account closed: its chases end, as today.
    SocketClosed,
    /// The account is going away: settle algos, then dispose the engine.
    Dispose,
}

/// Where algo replies go: the account's current signing socket.
pub type Emit = Box<dyn Fn(String) + Send>;

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

struct Snapshot {
    orders: Vec<Order>,
    positions: Vec<Position>,
    tickers: Vec<Ticker>,
    markets: Vec<Market>,
    balance: Balance,
    loaded: Loaded,
    classifications: Option<Vec<InstrumentClassification>>,
}

struct Port {
    handle: AccountHandle,
    /// Changes when the feed rejoins after the account dropped it.
    client: Cell<u64>,
    store: RefCell<AlgoStore>,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<Change>>>,
    next: Cell<CommandId>,
    pending: RefCell<HashMap<CommandId, oneshot::Sender<Result<String, VenueError>>>>,
    name: String,
    emit: Emit,
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
            if !sent {
                let mut error = VenueError::new("account is busy; command not sent");
                error.outcome = Some(Outcome::NotSent);
                return Err(error);
            }
            match id {
                Some(_) => rx.await.unwrap_or_else(|_| {
                    let mut error = VenueError::new("account stopped");
                    error.outcome = Some(Outcome::Unknown);
                    Err(error)
                }),
                None => Ok("null".into()),
            }
        })
    }
    fn emit(&self, op: &str, data: String) {
        let data: Value = serde_json::from_str(&data).unwrap_or(Value::String(data));
        (self.emit)(crate::envelope::msg(
            &self.name,
            op,
            Value::Array(vec![data]),
        ));
    }
}

fn project(store: &safe_cex_engine::Store) -> Snapshot {
    Snapshot {
        orders: store.orders().to_vec(),
        positions: store.positions().to_vec(),
        tickers: store.tickers().to_vec(),
        markets: store.markets().to_vec(),
        balance: store.balance().clone(),
        loaded: store.loaded(),
        classifications: store.instrument_classifications().map(<[_]>::to_vec),
    }
}

/// Applies one engine read to the algo store; returns what changed.
/// `rejoined`: fills sent while the feed was dropped are gone, so algos
/// re-read orders and positions and reconcile before acting.
fn apply(store: &mut AlgoStore, snapshot: Snapshot, fills: Vec<Fill>, rejoined: bool) -> Change {
    let orders = store.orders != snapshot.orders;
    let positions = store.positions != snapshot.positions;
    let tickers: HashMap<String, Ticker> = snapshot
        .tickers
        .into_iter()
        .map(|ticker| (ticker.symbol.clone(), ticker))
        .collect();
    let tickers_changed = store.tickers != tickers;
    if orders {
        store.update_ids.orders += 1;
    }
    if positions {
        store.update_ids.positions += 1;
    }
    store.orders = snapshot.orders;
    store.positions = snapshot.positions;
    store.tickers = tickers;
    store.instrument_classifications = snapshot.classifications.map(|labels| {
        labels
            .into_iter()
            .filter_map(|label| {
                let market = snapshot.markets.get(label.market as usize)?;
                Some((market.symbol.clone(), label))
            })
            .collect()
    });
    store.markets = snapshot
        .markets
        .into_iter()
        .map(|market| (market.symbol.clone(), market))
        .collect();
    if snapshot.loaded.balance {
        store.balance = Some(snapshot.balance);
    }
    // The engine exposes no private-socket health: report it as unknown
    // (uninitialized), never as healthy.
    store.private_socket = Default::default();
    Change {
        orders: orders || rejoined,
        positions: positions || rejoined,
        tickers: tickers_changed,
        fills,
        // Set by the port on a feed rejoin (fe7lgm).
        resync: rejoined,
    }
}

impl Port {
    fn apply(&self, snapshot: Snapshot, fills: Vec<Fill>, rejoined: bool) -> Change {
        apply(&mut self.store.borrow_mut(), snapshot, fills, rejoined)
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
pub async fn run(
    handle: AccountHandle,
    client: u64,
    venue: String,
    name: String,
    emit: Emit,
    link_signer: Option<std::sync::Arc<safe_cex_remote_signer::SignerLink>>,
    mut inbox: mpsc::UnboundedReceiver<AlgoInput>,
) {
    let Ok(mut feed) = Feed::join(handle.clone(), client).await else {
        handle.dispose();
        if let Some(signer) = &link_signer {
            signer.mark_unavailable();
        }
        return;
    };
    let port = Rc::new(Port {
        handle: handle.clone(),
        client: Cell::new(client),
        store: RefCell::new(AlgoStore {
            venue,
            ..AlgoStore::default()
        }),
        subscribers: RefCell::default(),
        next: Cell::new(0),
        pending: RefCell::default(),
        name: name.clone(),
        emit,
    });
    let scope = Scope::new();
    let cx = AlgoCx::new(port.clone(), scope.clone());
    let chasers = safe_cex_chaser::ChaserFactory::new(cx.clone());
    let book = chasers.book();
    let twap = Rc::new(safe_cex_algo_twap::factory::TwapFactory::new(
        cx.clone(),
        Rc::new(book.clone()),
    ));
    let registry = Registry::new(vec![
        Box::new(chasers),
        Box::new(SharedTwap(twap.clone())),
        // Exec's chase/TWAP handoff shares this account's book and TWAP schedule,
        // under the server's trusted account name, never one from the payload.
        Box::new(safe_cex_exec::native::ExecFactory::with_algos(
            name.clone(),
            book.clone(),
            (*twap).clone(),
        )),
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
    loop {
        tokio::select! {
            batch = feed.next() => {
                let Some(batch) = batch else { break };
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
                    && let Some(snapshot) = handle.read(project).await
                {
                    let change = port.apply(snapshot, batch.fills, batch.rejoined);
                    port.subscribers.borrow_mut().retain(|tx| tx.send(change.clone()).is_ok());
                    publish(&mut published);
                }
            }
            input = inbox.recv() => match input {
                Some(AlgoInput::Op { op, data }) => {
                    if let Some(Err(error)) = registry.dispatch(&op, &data, &cx) {
                        port.emit("error", Value::String(error).to_string());
                    }
                    publish(&mut published);
                }
                Some(AlgoInput::SocketClosed) => {
                    let _ = registry.dispatch("chaser:terminateAll", "{}", &cx);
                }
                Some(AlgoInput::Dispose) | None => break,
            },
        }
    }
    // Disposal contract: stop TWAPs, let chaser cancels settle, then the scope, then the engine.
    twap.terminate_all();
    if tokio::time::timeout(SHUTDOWN_DEADLINE, book.shutdown("account disposed"))
        .await
        .is_err()
    {
        crate::state::Log::warn(&format!(
            "{name}: chaser shutdown passed its deadline; cancels may be unsettled and orders may rest"
        ));
    }
    scope.dispose();
    handle.leave(feed.client());
    handle.dispose();
    if let Some(signer) = &link_signer {
        signer.mark_unavailable();
    }
}

struct SharedTwap(Rc<safe_cex_algo_twap::factory::TwapFactory>);
impl safe_cex_algos::AlgoFactory for SharedTwap {
    fn ops(&self) -> &'static [&'static str] {
        self.0.ops()
    }
    fn handle(&self, op: &str, data: &str, cx: &AlgoCx) -> Result<(), String> {
        self.0.handle(op, data, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::model::{Limits, MinMax, Precision};

    fn market(symbol: &str) -> Market {
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
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
        }
    }

    fn snapshot(classified: bool, balance_loaded: bool) -> Snapshot {
        Snapshot {
            orders: Vec::new(),
            positions: Vec::new(),
            tickers: Vec::new(),
            markets: vec![market("BTCUSDT"), market("ETHUSDT")],
            balance: Balance::default(),
            loaded: Loaded {
                balance: balance_loaded,
                orders: true,
                markets: true,
                tickers: true,
                positions: true,
            },
            classifications: classified.then(|| {
                vec![InstrumentClassification {
                    market: 1,
                    symbol_type: Some("stock".into()),
                    market_region: None,
                    underlying_ticker: None,
                }]
            }),
        }
    }

    #[test]
    fn classifications_map_market_index_to_symbol_and_stay_none_until_sent() {
        let mut store = AlgoStore::default();
        apply(&mut store, snapshot(false, false), Vec::new(), false);
        assert!(store.instrument_classifications.is_none());
        assert!(
            store.balance.is_none(),
            "no balance before its section loads"
        );
        apply(&mut store, snapshot(true, true), Vec::new(), false);
        let labels = store.instrument_classifications.as_ref().unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels["ETHUSDT"].symbol_type.as_deref(), Some("stock"));
        assert!(store.balance.is_some());
        assert!(
            !store.private_socket.initialized && !store.private_socket.healthy,
            "unknown, never claimed healthy"
        );
        assert_eq!(store.markets.len(), 2);
    }

    #[test]
    fn unchanged_reads_report_no_change() {
        let mut store = AlgoStore::default();
        apply(&mut store, snapshot(true, true), Vec::new(), false);
        let fill = Fill {
            id: None,
            order_id: "o1".into(),
            symbol: "BTCUSDT".into(),
            side: safe_cex_engine::model::OrderSide::Buy,
            price: 1.,
            amount: 2.,
        };
        let change = apply(&mut store, snapshot(true, true), vec![fill.clone()], false);
        assert_eq!(
            change.fills,
            vec![fill],
            "a frame's fills reach subscribers"
        );
        assert!(!change.orders && !change.positions && !change.tickers);
        assert_eq!(store.update_ids.orders, 0);
    }
}

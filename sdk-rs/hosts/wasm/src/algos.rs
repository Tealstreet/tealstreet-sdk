//! The Rust algos (chaser, TWAP, exec) on this engine, for the LOCAL worker, as
//! the exchange server runs them natively (`hosts/server/src/algos.rs`): an
//! `AccountPort` over the engine, on a host-driven executor.
//!
//! Algo work runs only inside the `algo*` exports, which return engine actions
//! like `command`. Completions and frames that arrive between them wake algo
//! tasks; `algoDeadline` then reports work due now, so the host calls
//! `algoAdvance` as it does a timer.
use std::cell::{Cell, Ref, RefCell};
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use safe_cex_algo_twap::factory::TwapFactory;
use safe_cex_algos::executor::{self, Executor};
use safe_cex_algos::host_executor::HostExecutor;
use safe_cex_algos::{
    AccountPort, AlgoCx, AlgoStore, Change, CommandFuture, Registry, Scope, mpsc, oneshot,
    projection,
};
use safe_cex_chaser::ChaserBook;
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::{Action, Notice};
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::{AnyEngine, WasmEngine};

/// Algo command ids. The JS host numbers its own commands up from 1, so it
/// would need two billion of them in one engine's life to reach this range.
const FIRST_COMMAND_ID: CommandId = 0x8000_0000;

fn is_algo_command(id: CommandId) -> bool {
    id >= FIRST_COMMAND_ID
}

/// Bound on settling chaser cancels before the scope is disposed anyway.
const SHUTDOWN_DEADLINE_MS: u64 = 10_000;

/// Run-then-submit rounds per host call: each round's completions may queue
/// more commands; past this the rest wait for the next call.
const MAX_ROUNDS: usize = 64;

/// As the server's `PlacedSymbols`: symbols an algo placement was attempted on,
/// kept for cancel-all on `eachPlacedSymbol` venues; only the account's
/// markets, and at most this many.
const MAX_PLACED_SYMBOLS: usize = 256;

type Emits = Rc<RefCell<Vec<(String, String)>>>;

struct Port {
    store: RefCell<AlgoStore>,
    subscribers: RefCell<Vec<mpsc::UnboundedSender<Change>>>,
    next: Cell<CommandId>,
    pending: RefCell<HashMap<CommandId, oneshot::Sender<Result<String, VenueError>>>>,
    /// Commands algos sent since the engine last took them.
    queued: RefCell<Vec<Command>>,
    emits: Emits,
    capabilities: safe_cex_engine::Capabilities,
    placed: RefCell<BTreeSet<String>>,
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
        let id = self
            .next
            .get()
            .checked_add(1)
            .unwrap_or(FIRST_COMMAND_ID)
            .max(FIRST_COMMAND_ID);
        self.next.set(id);
        id
    }
    fn command(&self, command: Command) -> CommandFuture {
        let id = command.completion_id();
        if id.is_some_and(|id| !is_algo_command(id)) {
            let mut error = VenueError::new("algo command id outside the algo range; not sent");
            error.outcome = Some(safe_cex_engine::Outcome::NotSent);
            return Box::pin(std::future::ready(Err(error)));
        }
        if let Command::PlaceOrders { orders, .. } = &command {
            self.record_placed(orders.iter().map(|order| order.symbol.as_str()));
        }
        let (tx, rx) = oneshot::channel();
        if let Some(id) = id {
            self.pending.borrow_mut().insert(id, tx);
        }
        self.queued.borrow_mut().push(command);
        Box::pin(async move {
            match id {
                Some(_) => rx.await.unwrap_or_else(|_| {
                    let mut error = VenueError::new("account stopped");
                    error.outcome = Some(safe_cex_engine::Outcome::Unknown);
                    Err(error)
                }),
                None => Ok("null".into()),
            }
        })
    }
    fn emit(&self, op: &str, data: String) {
        self.emits.borrow_mut().push((op.to_owned(), data));
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.capabilities
    }
}

impl Port {
    fn record_placed<'a>(&self, symbols: impl IntoIterator<Item = &'a str>) {
        let store = self.store.borrow();
        let mut placed = self.placed.borrow_mut();
        for symbol in symbols {
            if placed.len() < MAX_PLACED_SYMBOLS && store.markets.contains_key(symbol) {
                placed.insert(symbol.to_owned());
            }
        }
    }

    /// One engine read applied to the algo store; subscribers hear of it when
    /// it carries fills or changed what they watch.
    /// Engines without a store (host test doubles) have nothing to project.
    fn refresh(&self, store: Option<&safe_cex_engine::Store>, fills: Vec<Fill>) {
        let Some(store) = store else { return };
        let change = projection::apply(
            &mut self.store.borrow_mut(),
            projection::project(store),
            fills,
            false,
        );
        let algo_store = self.store.borrow();
        self.placed
            .borrow_mut()
            .retain(|symbol| algo_store.markets.contains_key(symbol));
        drop(algo_store);
        if change.orders || change.positions || change.tickers || !change.fills.is_empty() {
            self.subscribers
                .borrow_mut()
                .retain(|tx| tx.send(change.clone()).is_ok());
        }
    }

    fn settle(&self, command: CommandId, result: Result<String, VenueError>) {
        if let Some(waiter) = self.pending.borrow_mut().remove(&command) {
            let _ = waiter.send(result);
        }
    }
}

/// What the server's settle rebuilds from a done notice: the message (or the
/// partial-batch JSON) and the outcome.
fn completion_error(error: &VenueError) -> VenueError {
    let mut settled = VenueError::new(error.legacy_text());
    settled.outcome = error.outcome;
    settled
}

/// One account's running algos.
struct Algos {
    executor: HostExecutor,
    port: Rc<Port>,
    cx: AlgoCx,
    scope: Scope,
    registry: Registry,
    book: Rc<ChaserBook>,
    twap: Rc<TwapFactory>,
    name: String,
    published: String,
    /// Set once `algoDispose` started the shutdown.
    disposing: bool,
    /// Set by the shutdown task once the scope is disposed.
    finished: Rc<Cell<bool>>,
}

/// Restores the thread's previous executor when algo work returns.
struct Installed(Option<Rc<dyn Executor>>);

impl Drop for Installed {
    fn drop(&mut self) {
        executor::set_current(self.0.take());
    }
}

impl Algos {
    fn install(&self) -> Installed {
        Installed(executor::set_current(Some(Rc::new(self.executor.clone()))))
    }

    /// Move the algo clock first, so an op or a disposal reads the host's time.
    fn advance(&self, now_ms: f64, wall_ms: f64) {
        let _installed = self.install();
        self.executor.advance(now_ms, wall_ms);
    }

    /// Builds the registry exactly as the server's `run`.
    fn start(
        name: String,
        venue: String,
        engine: &dyn AnyEngine,
        emits: Emits,
        now_ms: f64,
        wall_ms: f64,
    ) -> Self {
        let executor = HostExecutor::new(now_ms, wall_ms);
        let _installed = Installed(executor::set_current(Some(Rc::new(executor.clone()))));
        let port = Rc::new(Port {
            store: RefCell::new(AlgoStore {
                venue,
                ..AlgoStore::default()
            }),
            subscribers: RefCell::default(),
            next: Cell::new(FIRST_COMMAND_ID - 1),
            pending: RefCell::default(),
            queued: RefCell::default(),
            emits,
            capabilities: engine.capabilities(),
            placed: RefCell::default(),
        });
        port.refresh(engine.store(), Vec::new());
        let scope = Scope::new();
        let cx = AlgoCx::new(port.clone(), scope.clone());
        let chasers = safe_cex_chaser::ChaserFactory::new(cx.clone());
        let book = chasers.book();
        let twap = Rc::new(TwapFactory::new(&name, cx.clone(), Rc::new(book.clone())));
        #[cfg_attr(not(feature = "algos-exec"), allow(unused_mut))]
        let mut factories: Vec<Box<dyn safe_cex_algos::AlgoFactory>> =
            vec![Box::new(chasers), Box::new(twap.clone())];
        // Exec's chase/TWAP handoff shares this account's book and TWAP
        // schedule, under the host's account name, never one from the payload.
        #[cfg(feature = "algos-exec")]
        {
            let sweep = port.clone();
            factories.push(Box::new(Rc::new(
                safe_cex_exec::native::ExecFactory::with_algos(
                    name.clone(),
                    book.clone(),
                    (*twap).clone(),
                )
                .with_cancel_all(Rc::new(move || {
                    use safe_cex_engine::CancelAll;
                    use safe_cex_exec::native::CancelAllScope;
                    match sweep.capabilities.cancel_all {
                        CancelAll::Native => CancelAllScope::Native,
                        CancelAll::EachOpenSymbol => CancelAllScope::Symbols(Vec::new()),
                        CancelAll::EachPlacedSymbol => CancelAllScope::Symbols(
                            sweep.placed.borrow().iter().cloned().collect(),
                        ),
                    }
                })),
            )));
        }
        let registry = Registry::new(factories);
        let mut algos = Self {
            executor,
            port,
            cx,
            scope,
            registry,
            book,
            twap,
            name,
            published: String::new(),
            disposing: false,
            finished: Rc::default(),
        };
        algos.publish();
        algos
    }

    /// `store` rows the frames do not carry (`activeChasers`, `activeTWAPs`).
    fn publish(&mut self) {
        let rows = serde_json::json!({
            "activeChasers": self.book.active_chasers(),
            "activeTWAPs": self.twap.active_twaps(&self.name),
        });
        let text = rows.to_string();
        if self.published != text {
            self.port.emit("store", text.clone());
            self.published = text;
        }
    }

    fn dispatch(&mut self, op: &str, data: &str) -> Option<()> {
        let dispatched = {
            let _installed = self.install();
            self.registry.dispatch(op, data, &self.cx)?
        };
        if let Err(error) = dispatched {
            self.port.emit("error", Value::String(error).to_string());
        }
        self.publish();
        Some(())
    }

    /// Completions for algo commands settle their waiters; the JS host never
    /// sees them. Everything else passes through in order.
    fn route(algos: Option<&Self>, actions: Vec<Action>) -> Vec<Action> {
        actions
            .into_iter()
            .filter_map(|action| match action {
                Action::Notice(Notice::CommandDone { command, result })
                    if is_algo_command(command) =>
                {
                    if let Some(algos) = algos {
                        algos.port.settle(command, Ok(result));
                    }
                    None
                }
                Action::Notice(Notice::CommandFailed { command, error })
                    if is_algo_command(command) =>
                {
                    if let Some(algos) = algos {
                        algos.port.settle(command, Err(completion_error(&error)));
                    }
                    None
                }
                Action::Notice(
                    Notice::CommandError { command, .. } | Notice::CommandProgress { command, .. },
                ) if is_algo_command(command) => None,
                other => Some(other),
            })
            .collect()
    }

    /// Advance the clock, run every task, hand queued commands to the engine,
    /// and repeat while completions wake more work. Returns the engine's actions.
    fn run(&mut self, engine: &mut dyn AnyEngine, now_ms: f64, wall_ms: f64) -> Vec<Action> {
        let mut out = Vec::new();
        {
            let _installed = self.install();
            self.port.refresh(engine.store(), Vec::new());
            self.executor.advance(now_ms, wall_ms);
            for _ in 0..MAX_ROUNDS {
                let queued = std::mem::take(&mut *self.port.queued.borrow_mut());
                if queued.is_empty() && !self.executor.has_ready_tasks() {
                    break;
                }
                for command in queued {
                    let id = command.completion_id();
                    if engine.handle_command(now_ms, command, None).is_err()
                        && let Some(id) = id
                    {
                        let mut error = VenueError::new("engine refused the algo command");
                        error.outcome = Some(safe_cex_engine::Outcome::NotSent);
                        self.port.settle(id, Err(error));
                    }
                }
                out.extend(Self::route(Some(self), engine.drain()));
                self.port.refresh(engine.store(), Vec::new());
                self.executor.run_until_stalled();
            }
        }
        if !self.finished.get() {
            self.publish();
        }
        out
    }

    /// The disposal contract: stop TWAPs, let chaser cancels settle (bounded),
    /// then dispose the scope. The JS host disposes the engine after that.
    fn dispose(&mut self, reason: String) {
        if self.disposing {
            return;
        }
        self.disposing = true;
        let _installed = self.install();
        if !self.book.active_chasers().is_empty() || !self.twap.active_twaps(&self.name).is_empty()
        {
            self.port.emit(
                "error",
                Value::String(format!("Chases and TWAPs stopped: {reason}")).to_string(),
            );
        }
        self.twap.terminate_all();
        let (book, scope, port, finished) = (
            self.book.clone(),
            self.scope.clone(),
            self.port.clone(),
            self.finished.clone(),
        );
        // Outside the scope: this task is what disposes it.
        drop(executor::spawn(async move {
            if executor::timeout(SHUTDOWN_DEADLINE_MS, book.shutdown(&reason))
                .await
                .is_err()
            {
                port.emit(
                    "warning",
                    Value::from(
                        "Chaser shutdown passed its deadline; cancels may be unsettled and orders may rest",
                    )
                    .to_string(),
                );
            }
            scope.dispose();
            finished.set(true);
        }));
    }
}

/// The engine's algo state: absent until `algoStart`, gone once disposed.
#[derive(Default)]
pub(crate) struct AlgoHost {
    venue: String,
    running: Option<Algos>,
    /// Shared with the port, so emits made while shutting down outlive it.
    emits: Emits,
}

impl AlgoHost {
    pub(crate) fn new(venue: &str) -> Self {
        Self {
            venue: venue.to_owned(),
            ..Self::default()
        }
    }

    pub(crate) fn route(&self, actions: Vec<Action>) -> Vec<Action> {
        Algos::route(self.running.as_ref(), actions)
    }

    /// A frame the host took: its fills and the store reach algo subscribers.
    pub(crate) fn frame(&self, store: Option<&safe_cex_engine::Store>, fills: Vec<Fill>) {
        if let Some(algos) = &self.running {
            algos.port.refresh(store, fills);
        }
    }

    pub(crate) fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// Runs algo work, then drops algos whose shutdown has finished.
    fn run(&mut self, engine: &mut dyn AnyEngine, now_ms: f64, wall_ms: f64) -> String {
        let Some(algos) = &mut self.running else {
            return String::new();
        };
        let actions = algos.run(engine, now_ms, wall_ms);
        if algos.finished.get() {
            self.running = None;
        }
        crate::encode(&actions)
    }

    #[cfg(test)]
    fn executor(&self) -> Option<HostExecutor> {
        self.running.as_ref().map(|algos| algos.executor.clone())
    }
}

/// An algo reply as the worker posts it: `{op, data}` with `data` verbatim.
fn emit_json(op: &str, data: &str) -> Value {
    let data = serde_json::from_str(data).unwrap_or_else(|_| Value::String(data.to_owned()));
    serde_json::json!({ "op": op, "data": data })
}

#[wasm_bindgen]
impl WasmEngine {
    /// Start this account's algos. `now_ms` is the clock engine calls take as
    /// `now`; `wall_ms` is epoch milliseconds.
    #[wasm_bindgen(js_name = algoStart)]
    pub fn algo_start(
        &mut self,
        account_name: &str,
        now_ms: f64,
        wall_ms: f64,
    ) -> Result<(), JsError> {
        if self.algos.running.is_some() {
            return Err(JsError::new("algos are already running"));
        }
        let algos = Algos::start(
            account_name.to_owned(),
            self.algos.venue.clone(),
            &*self.inner,
            self.algos.emits.clone(),
            now_ms,
            wall_ms,
        );
        self.algos.running = Some(algos);
        Ok(())
    }

    /// One algo op (`chaser:start`, `twap:stop`, `exec`, ...); returns the
    /// engine actions its work produced, as `command` does.
    #[wasm_bindgen(js_name = algoOp)]
    pub fn algo_op(
        &mut self,
        op: &str,
        data_json: &str,
        now_ms: f64,
        wall_ms: f64,
    ) -> Result<String, JsError> {
        let Some(algos) = &mut self.algos.running else {
            return Err(JsError::new("algos are not running"));
        };
        if algos.disposing {
            return Err(JsError::new("algos are stopping"));
        }
        algos.advance(now_ms, wall_ms);
        algos
            .dispatch(op, data_json)
            .ok_or_else(|| JsError::new("unhandled op"))?;
        Ok(self.algos.run(&mut *self.inner, now_ms, wall_ms))
    }

    /// Run due algo work: timers up to `now_ms`, and tasks completions or
    /// frames woke. Returns the engine actions, as `command` does.
    #[wasm_bindgen(js_name = algoAdvance)]
    pub fn algo_advance(&mut self, now_ms: f64, wall_ms: f64) -> String {
        self.algos.run(&mut *self.inner, now_ms, wall_ms)
    }

    /// When `algoAdvance` is next due: the earliest algo timer, or the current
    /// algo clock when woken work waits. `undefined` when nothing is pending.
    #[wasm_bindgen(js_name = algoDeadline)]
    pub fn algo_deadline(&self) -> Option<f64> {
        let algos = self.algos.running.as_ref()?;
        if algos.executor.has_ready_tasks() || !algos.port.queued.borrow().is_empty() {
            return Some(algos.executor.monotonic_ms());
        }
        algos.executor.next_deadline()
    }

    /// Algo replies since the last call, as a JSON array of `{op, data}`.
    #[wasm_bindgen(js_name = algoEmits)]
    pub fn algo_emits(&mut self) -> String {
        let emits = std::mem::take(&mut *self.algos.emits.borrow_mut());
        let out: Vec<Value> = emits.iter().map(|(op, data)| emit_json(op, data)).collect();
        Value::Array(out).to_string()
    }

    /// Stop every algo: TWAPs at once, chases once their cancels settle (at
    /// most 10 s on the algo clock). Keep calling `algoAdvance` at
    /// `algoDeadline` until `algoRunning` is false, then dispose the engine.
    #[wasm_bindgen(js_name = algoDispose)]
    pub fn algo_dispose(&mut self, now_ms: f64, wall_ms: f64, reason: Option<String>) -> String {
        let Some(algos) = &mut self.algos.running else {
            return String::new();
        };
        algos.advance(now_ms, wall_ms);
        algos.dispose(reason.unwrap_or_else(|| "account disposed".to_owned()));
        self.algos.run(&mut *self.inner, now_ms, wall_ms)
    }

    /// Algos are running or still shutting down.
    #[wasm_bindgen(js_name = algoRunning)]
    pub fn algo_running(&self) -> bool {
        self.algos.is_running()
    }
}

#[cfg(test)]
#[path = "algo_tests.rs"]
mod tests;

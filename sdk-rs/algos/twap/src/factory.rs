//! Runs one account's [`TwapBook`] on the shared algo runtime: `twap:*` ops
//! arrive through [`AlgoFactory`], timers run as one driver task, and effects
//! become engine commands, chaser calls and wire replies.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use safe_cex_algos::{AlgoCx, AlgoFactory};
use safe_cex_chaser::{ChaserBook, ChaserOptions, ClientIdMinter};
use safe_cex_engine::engine::Command;
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::{OrderSide as EngineSide, PlaceOrderOpts};
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::{
    ChaserProgress, ChaserRequest, OrderKind, OrderSide, SliceFailure, TickerQuote, TwapBook,
    TwapEffect, TwapEnv, TwapMarketView, TwapOptions, TwapOrderRequest, TwapOwner,
};

/// The account's chaser runtime, as TWAP slices use it.
pub trait TwapChasers {
    /// An `Err` refuses child admission before dispatch and is logged to the client.
    fn start_with_id(&self, cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String>;
    fn progress(&self, chaser_id: &str) -> Option<ChaserProgress>;
    /// Stop further writes after an uncertain TWAP outcome; current and late orders stay.
    fn halt(&self, cx: &AlgoCx, chaser_id: &str);
    /// Stop the chaser and cancel its open order.
    fn remove(&self, cx: &AlgoCx, chaser_id: &str);
}

pub const TWAP_OPS: &[&str] = &[
    "twap:start",
    "twap:resume",
    "twap:pause",
    "twap:stop",
    "twap:cleanup",
    "twap:state",
];

/// One account's TWAPs, bound to the account's context: the timer driver and
/// placements outlive the socket that sent an op. Before disposing the account
/// scope, the server calls [`TwapFactory::terminate_all`].
///
/// Clones share one book: box one into the `Registry` and hand another to a
/// typed caller such as the exec `twap` command.
#[derive(Clone)]
pub struct TwapFactory {
    inner: Rc<Inner>,
    captured_owner: Option<TwapOwner>,
}

type OwnerProvider = Rc<dyn Fn() -> TwapOwner>;

struct Inner {
    /// The account's context, never a socket's.
    cx: AlgoCx,
    book: RefCell<TwapBook>,
    chasers: Rc<dyn TwapChasers>,
    env: RefCell<CxEnv>,
    rescheduled: Notify,
    driver_started: Cell<bool>,
    client_ids: ClientIdMinter,
    /// Slice placements awaiting their answer: each may yet land.
    placing: Cell<usize>,
    placed: Notify,
    owner_provider: RefCell<Option<OwnerProvider>>,
}

fn start_owner_change_error(dispatched: bool) -> VenueError {
    if dispatched {
        VenueError::new("TWAP start outcome is unknown: authenticated connection binding changed after slice dispatch")
            .outcome(Outcome::Unknown)
    } else {
        VenueError::new(
            "TWAP start cancelled-for-owner-change: authenticated connection binding changed",
        )
        .outcome(Outcome::NotSent)
    }
}

impl TwapFactory {
    /// New TWAPs only; persisted restores require [`Self::with_owner`].
    pub fn new(account_name: &str, account_cx: AlgoCx, chasers: Rc<dyn TwapChasers>) -> Self {
        Self::from_book(TwapBook::new(account_name), account_cx, chasers)
    }

    /// The host supplies its authenticated owner/session, independently of restore data.
    pub fn with_owner(
        account_name: &str,
        owner: TwapOwner,
        account_cx: AlgoCx,
        chasers: Rc<dyn TwapChasers>,
    ) -> Self {
        Self::from_book(
            TwapBook::with_owner(account_name, owner),
            account_cx,
            chasers,
        )
    }

    /// Interim WASM compatibility for hosts not yet supplying an authenticated owner.
    pub fn with_legacy_restore(
        account_name: &str,
        account_cx: AlgoCx,
        chasers: Rc<dyn TwapChasers>,
    ) -> Self {
        Self::from_book(
            TwapBook::with_legacy_restore(account_name),
            account_cx,
            chasers,
        )
    }

    fn from_book(book: TwapBook, account_cx: AlgoCx, chasers: Rc<dyn TwapChasers>) -> Self {
        Self {
            captured_owner: None,
            inner: Rc::new(Inner {
                cx: account_cx,
                book: RefCell::new(book),
                chasers,
                env: RefCell::new(CxEnv::seeded()),
                rescheduled: Notify::new(),
                driver_started: Cell::new(false),
                client_ids: ClientIdMinter::default(),
                placing: Cell::new(0),
                placed: Notify::new(),
                owner_provider: RefCell::new(None),
            }),
        }
    }

    /// A shared host supplies its current authenticated binding, independent of queued data.
    pub fn guard_owner(self, owner: OwnerProvider) -> Self {
        self.inner.owner_provider.replace(Some(owner));
        self
    }

    /// Carry the current binding through a typed caller's deferred work.
    pub fn capture_owner(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            captured_owner: self
                .captured_owner
                .clone()
                .or_else(|| self.inner.owner_snapshot()),
        }
    }

    fn owner_is_current(&self) -> bool {
        self.captured_owner
            .as_ref()
            .is_none_or(|owner| self.inner.owner_snapshot().as_ref() == Some(owner))
    }

    /// Preserve identity-based label remapping with the host's trusted generation.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_with_owner(
        account_name: &str,
        account_cx: AlgoCx,
        chasers: Rc<dyn TwapChasers>,
        owner: TwapOwner,
    ) -> Self {
        Self::from_book(
            TwapBook::new_with_owner(account_name, owner),
            account_cx,
            chasers,
        )
    }

    /// Stop every TWAP (`twap:stop` each) and send the final state; nothing is
    /// spawned. Chase slices are the chaser book's: await `ChaserBook::shutdown`
    /// before disposing the account scope.
    pub fn terminate_all(&self) {
        let effects = self.inner.book.borrow_mut().terminate_all();
        Inner::run(&self.inner, &self.inner.cx, effects);
        self.inner.rescheduled.notify_one();
    }

    /// A nuke's first step: stop every running TWAP so it places no further
    /// slice, and return the ids it stopped. Its chase slices stop with the
    /// chaser book's `stop_for_nuke`; then await [`Self::placements_settled`].
    pub fn stop_for_nuke(&self) -> Vec<String> {
        let stopped = self.inner.book.borrow().running_ids();
        self.terminate_all();
        stopped
    }

    /// Until no slice placement is on the wire, so a sweep sees what landed.
    pub async fn placements_settled(&self) {
        while self.inner.placing.get() > 0 {
            self.inner.placed.notified().await;
        }
    }

    /// A stopped row can still have a slice on the wire; hosts must await it.
    pub fn pending_placements(&self) -> usize {
        self.inner.placing.get()
    }

    /// Start a TWAP for a typed caller such as the exec `twap` command, with
    /// the same validation and effects as `twap:start`. Returns the new id.
    pub fn start(&self, options: TwapOptions) -> Result<String, VenueError> {
        if !self.owner_is_current() {
            return Err(start_owner_change_error(false));
        }
        let cx = &self.inner.cx;
        Inner::refresh_owner(&self.inner, cx);
        if self
            .captured_owner
            .as_ref()
            .is_some_and(|owner| self.inner.book.borrow().owner.as_ref() != Some(owner))
        {
            return Err(start_owner_change_error(false));
        }
        let owner = self.inner.book.borrow().owner.clone();
        let (started, effects, chaser_dispatched) = {
            let mut env = self.inner.env.borrow_mut();
            env.now = cx.now_ms();
            let view = CxView {
                cx,
                chasers: &*self.inner.chasers,
                client_ids: &self.inner.client_ids,
                chaser_dispatched: Cell::new(false),
            };
            let (started, effects) = self
                .inner
                .book
                .borrow_mut()
                .start(options, &mut *env, &view);
            (started, effects, view.chaser_dispatched.get())
        };
        let dispatched = Inner::run(&self.inner, cx, effects) || chaser_dispatched;
        Inner::refresh_owner(&self.inner, cx);
        if started.is_ok() && self.inner.book.borrow().owner != owner {
            return Err(start_owner_change_error(dispatched));
        }
        Inner::ensure_driver(&self.inner, cx);
        started.map_err(|message| VenueError::new(message).outcome(Outcome::NotSent))
    }

    /// `twap:stop` for a typed caller; false when no running TWAP has `id`.
    /// Its chase slices keep their own lifecycle in the chaser book, as in TS.
    pub fn stop(&self, id: &str) -> bool {
        if !self.owner_is_current() {
            return false;
        }
        let (found, effects) = self.inner.book.borrow_mut().stop(id);
        Inner::run(&self.inner, &self.inner.cx, effects);
        self.inner.rescheduled.notify_one();
        found
    }

    /// The store's `activeTWAPs` for `account_name`.
    pub fn active_twaps(&self, account_name: &str) -> Vec<Value> {
        if !self.owner_is_current() {
            return Vec::new();
        }
        Inner::refresh_owner(&self.inner, &self.inner.cx);
        self.inner.book.borrow().active_twaps(account_name)
    }

    /// The TWAP that owns a direct limit order, for `extra.isFromTwap`.
    pub fn order_owner(&self, account_name: &str, order_id: &str) -> Option<String> {
        self.inner
            .book
            .borrow()
            .order_owner(account_name, order_id)
            .map(str::to_owned)
    }
}

impl AlgoFactory for TwapFactory {
    fn ops(&self) -> &'static [&'static str] {
        TWAP_OPS
    }

    /// `_op_cx` is the sending socket's; TWAPs run on the account's context.
    fn handle(&self, op: &str, data: &str, _op_cx: &AlgoCx) -> Result<(), String> {
        let cx = &self.inner.cx;
        Inner::refresh_owner(&self.inner, cx);
        let data: Value = serde_json::from_str(data).map_err(|error| format!("{op}: {error}"))?;
        let effects = {
            let mut env = self.inner.env.borrow_mut();
            env.now = cx.now_ms();
            let view = CxView {
                cx,
                chasers: &*self.inner.chasers,
                client_ids: &self.inner.client_ids,
                chaser_dispatched: Cell::new(false),
            };
            self.inner
                .book
                .borrow_mut()
                .handle(op, &data, &mut *env, &view)
        };
        Inner::run(&self.inner, cx, effects);
        Inner::ensure_driver(&self.inner, cx);
        Ok(())
    }
}

impl Inner {
    fn owner_snapshot(&self) -> Option<TwapOwner> {
        self.owner_provider.borrow().as_ref().map(|owner| owner())
    }

    fn refresh_owner(inner: &Rc<Self>, cx: &AlgoCx) -> bool {
        let Some(owner) = inner.owner_snapshot() else {
            return false;
        };
        let had_rows = inner.book.borrow().ids().next().is_some();
        if !inner.book.borrow_mut().rebind_owner(Some(owner)) {
            return false;
        }
        if had_rows {
            report(
                cx,
                "TWAPs stopped: authenticated connection binding changed",
            );
        }
        cx.port.emit("twap:stateUpdate", "{}".into());
        inner.rescheduled.notify_one();
        true
    }

    fn remainder_settled(
        inner: &Rc<Self>,
        cx: &AlgoCx,
        twap_id: &str,
        index: usize,
        outcome: Result<Vec<String>, VenueError>,
    ) {
        let effects = inner
            .book
            .borrow_mut()
            .remainder_settled(twap_id, index, outcome);
        Self::run(inner, cx, effects);
        inner.rescheduled.notify_one();
    }

    fn ensure_driver(inner: &Rc<Self>, cx: &AlgoCx) {
        if inner.driver_started.replace(true) {
            inner.rescheduled.notify_one();
            return;
        }
        let (inner, cx) = (inner.clone(), cx.clone());
        cx.clone().spawn(async move {
            loop {
                let due_at = inner.book.borrow().wake_at();
                match due_at {
                    None => inner.rescheduled.notified().await,
                    Some(due_at) => {
                        let wait = (due_at - cx.now_ms()).max(0.0);
                        let woken = safe_cex_algos::executor::timeout(
                            wait.ceil() as u64,
                            inner.rescheduled.notified(),
                        )
                        .await;
                        if woken.is_err() {
                            Self::wake(&inner, &cx, due_at);
                        }
                    }
                }
            }
        });
    }

    /// The timer for `due_at` fired, so the book sees at least that time.
    fn wake(inner: &Rc<Self>, cx: &AlgoCx, due_at: f64) {
        if Self::refresh_owner(inner, cx) {
            return;
        }
        let effects = {
            let mut env = inner.env.borrow_mut();
            env.now = cx.now_ms().max(due_at);
            let view = CxView {
                cx,
                chasers: &*inner.chasers,
                client_ids: &inner.client_ids,
                chaser_dispatched: Cell::new(false),
            };
            inner.book.borrow_mut().wake(&mut *env, &view)
        };
        Self::run(inner, cx, effects);
    }

    fn run(inner: &Rc<Self>, cx: &AlgoCx, effects: Vec<TwapEffect>) -> bool {
        let mut dispatched = false;
        for effect in effects {
            if Self::refresh_owner(inner, cx) {
                return dispatched;
            }
            match effect {
                TwapEffect::Outcome { twap_id, error } => {
                    let elements = error.elements.as_deref().map(|elements| &elements[..]);
                    let unknown = elements.map_or(
                        0,
                        |rows: &[safe_cex_engine::write_plan::ElementOutcome]| {
                            rows.iter()
                                .filter(|row| {
                                    matches!(
                                        row.result,
                                        safe_cex_engine::write_plan::ElementResult::Unknown { .. }
                                    )
                                })
                                .count()
                        },
                    );
                    cx.port.emit(
                        "twap:outcome",
                        json!({ "id": twap_id, "error": error }).to_string(),
                    );
                    cx.port.emit("telemetry", json!({ "kind": "twap.unknown_outcome", "twapId": twap_id, "outcome": "unknown", "unknown": unknown, "error": error }).to_string());
                }
                TwapEffect::HaltChaser { chaser_id } => inner.chasers.halt(cx, &chaser_id),
                TwapEffect::PlaceOrder {
                    twap_id,
                    settle_token,
                    element_index,
                    request,
                } => {
                    dispatched |=
                        Self::place(inner, cx, twap_id, settle_token, element_index, &request);
                }
                TwapEffect::RemoveChaser { chaser_id, .. } => inner.chasers.remove(cx, &chaser_id),
                TwapEffect::Log { message } => report(cx, &message),
                TwapEffect::StateUpdate { data } => {
                    cx.port.emit("twap:stateUpdate", data.to_string())
                }
                TwapEffect::Diagnostic { message } => eprintln!("[TWAP] {message}"),
            }
        }
        dispatched
    }

    fn place(
        inner: &Rc<Self>,
        cx: &AlgoCx,
        twap_id: String,
        settle_token: Option<u64>,
        element_index: usize,
        request: &TwapOrderRequest,
    ) -> bool {
        let command = match place_command(cx, request) {
            Ok(command) => command,
            Err(message) => {
                let failure = VenueError::new(message).outcome(Outcome::NotSent);
                match settle_token {
                    Some(token) => Self::settled(inner, cx, &twap_id, token, Err(failure)),
                    None => {
                        Self::remainder_settled(inner, cx, &twap_id, element_index, Err(failure))
                    }
                }
                return false;
            }
        };
        let owner = inner.owner_snapshot();
        let future = cx.port.command(command);
        inner.placing.set(inner.placing.get() + 1);
        let (inner, task_cx) = (inner.clone(), cx.clone());
        cx.spawn(async move {
            let outcome = placement_outcome(future.await);
            inner.placing.set(inner.placing.get() - 1);
            inner.placed.notify_waiters();
            if owner != inner.owner_snapshot() {
                Self::refresh_owner(&inner, &task_cx);
                return;
            }
            match (settle_token, outcome) {
                (Some(token), outcome) => Self::settled(&inner, &task_cx, &twap_id, token, outcome),
                (None, outcome) => {
                    Self::remainder_settled(&inner, &task_cx, &twap_id, element_index, outcome)
                }
            }
        });
        true
    }

    fn settled(
        inner: &Rc<Self>,
        cx: &AlgoCx,
        twap_id: &str,
        token: u64,
        outcome: Result<Vec<String>, SliceFailure>,
    ) {
        let effects = {
            let mut env = inner.env.borrow_mut();
            env.now = cx.now_ms();
            inner
                .book
                .borrow_mut()
                .placement_settled(twap_id, token, outcome, &mut *env)
        };
        Self::run(inner, cx, effects);
        inner.rescheduled.notify_one();
    }
}

/// Preserve fully known refusals and their native partial-placement facts.
/// Unreadable identities or unresolved outcomes remain execution uncertainty.
pub fn placement_outcome(result: Result<String, VenueError>) -> Result<Vec<String>, SliceFailure> {
    match result {
        Ok(result) => serde_json::from_str::<Vec<String>>(&result)
            .map_err(|_| crate::unknown_outcome(VenueError::new("unreadable order identities"))),
        Err(error) if crate::known_refusal(&error) => Err(error),
        Err(error) => Err(crate::unknown_outcome(error)),
    }
}

fn place_command(cx: &AlgoCx, request: &TwapOrderRequest) -> Result<Command, String> {
    let mut opts = json!({
        "symbol": request.symbol,
        "type": match request.kind {
            OrderKind::Limit => "limit",
            OrderKind::Market => "market",
        },
        "side": request.side,
        "amount": request.amount,
        "price": request.price,
        "reduceOnly": request.reduce_only,
    });
    // The engine refuses a null caller id; an absent one mints its own.
    if let Some(id) = &request.client_order_id {
        opts["clientOrderId"] = json!(id);
    }
    let opts: PlaceOrderOpts = serde_json::from_value(opts).map_err(|error| error.to_string())?;
    Ok(Command::PlaceOrders {
        command: cx.port.next_command_id(),
        orders: vec![opts],
    })
}

struct CxView<'a> {
    cx: &'a AlgoCx,
    chasers: &'a dyn TwapChasers,
    client_ids: &'a ClientIdMinter,
    chaser_dispatched: Cell<bool>,
}

impl TwapMarketView for CxView<'_> {
    fn ticker(&self, symbol: &str) -> Option<TickerQuote> {
        self.cx
            .port
            .store()
            .tickers
            .get(symbol)
            .map(|ticker| TickerQuote {
                bid: Some(ticker.bid),
                ask: Some(ticker.ask),
                last: Some(ticker.last),
            })
    }

    fn chaser(&self, chaser_id: &str) -> Option<ChaserProgress> {
        self.chasers.progress(chaser_id)
    }

    fn start_chaser(&self, request: &ChaserRequest) -> Result<(), String> {
        self.chasers.start_with_id(self.cx, request)?;
        self.chaser_dispatched.set(true);
        Ok(())
    }

    fn amount_step(&self, symbol: &str) -> Option<f64> {
        let store = self.cx.port.store();
        store
            .markets
            .get(symbol)
            .map(|market| market.precision.amount)
    }

    /// As the chaser's: only the account's markets, never an order the venue would split.
    fn client_order_id(&self, request: &TwapOrderRequest) -> Option<String> {
        let store = self.cx.port.store();
        let market = store.markets.get(&request.symbol)?;
        let max = market.limits.amount.max;
        if max > 0.0 && request.amount > max {
            return None;
        }
        self.client_ids.mint(&store.venue)
    }
}

/// `Date.now()`, `Math.random()` and `nanoid()` for the server.
struct CxEnv {
    now: f64,
    state: u64,
}

const NANOID_ALPHABET: &[u8] = b"useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict";

impl CxEnv {
    fn seeded() -> Self {
        use std::hash::{BuildHasher, RandomState};
        Self {
            now: 0.0,
            state: RandomState::new().hash_one(0u8) | 1,
        }
    }

    /// xorshift64*: sizing jitter and ids, not secrets.
    fn next(&mut self) -> u64 {
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        self.state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl TwapEnv for CxEnv {
    fn now_ms(&self) -> f64 {
        self.now
    }

    fn random_unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn new_id(&mut self) -> String {
        (0..21)
            .map(|_| NANOID_ALPHABET[(self.next() >> 58) as usize] as char)
            .collect()
    }
}

/// The account's chaser book, shared with the `chaser:*` ops: pass
/// `ChaserFactory::book()` and register that same `ChaserFactory`.
impl TwapChasers for Rc<ChaserBook> {
    fn halt(&self, _cx: &AlgoCx, id: &str) {
        ChaserBook::halt_keep_order(self, id);
    }

    fn start_with_id(&self, _cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String> {
        // TS slices pass no origin, account or terminate rule, and the chaser
        // ignores `delay`.
        let options = ChaserOptions {
            symbol: request.symbol.clone(),
            size: request.size,
            side: match request.side {
                OrderSide::Buy => EngineSide::Buy,
                OrderSide::Sell => EngineSide::Sell,
            },
            max_attempts: f64::from(request.max_attempts),
            reduce_only: Some(request.reduce_only),
            origin: None,
            account_name: None,
            terminate: None,
            client_id: None,
            twap_id: Some(request.twap_id.clone()),
        };
        ChaserBook::start_with_id(self, request.id.clone(), options)
    }

    fn progress(&self, chaser_id: &str) -> Option<ChaserProgress> {
        ChaserBook::progress(self, chaser_id).map(|progress| ChaserProgress {
            remaining_size: progress.remaining_size,
            stopped: progress.stopped,
            ended: progress.ended,
            order_may_be_open: progress.order_may_be_open,
        })
    }

    fn remove(&self, _cx: &AlgoCx, chaser_id: &str) {
        ChaserBook::remove(self, chaser_id);
    }
}

/// A TWAP error the user must see: the `error` op is what the web toasts.
/// TS used `log` (`LogSeverity.Error`), which reaches only the exchange log.
fn report(cx: &AlgoCx, message: &str) {
    cx.port.emit("error", json!(message).to_string());
}

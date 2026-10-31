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
    TwapEffect, TwapEnv, TwapMarketView, TwapOptions, TwapOrderRequest,
};

/// The account's chaser runtime, as TWAP slices use it.
pub trait TwapChasers {
    /// An `Err` is the refusal the client sees as an error log.
    fn start_with_id(&self, cx: &AlgoCx, request: &ChaserRequest) -> Result<(), String>;
    fn progress(&self, chaser_id: &str) -> Option<ChaserProgress>;
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
}

struct Inner {
    /// The account's context, never a socket's.
    cx: AlgoCx,
    book: RefCell<TwapBook>,
    chasers: Rc<dyn TwapChasers>,
    env: RefCell<CxEnv>,
    rescheduled: Notify,
    driver_started: Cell<bool>,
    client_ids: ClientIdMinter,
}

impl TwapFactory {
    /// `account_name` is the account's `accountName`; TWAPs naming another are refused.
    pub fn new(account_name: &str, account_cx: AlgoCx, chasers: Rc<dyn TwapChasers>) -> Self {
        Self {
            inner: Rc::new(Inner {
                cx: account_cx,
                book: RefCell::new(TwapBook::new(account_name)),
                chasers,
                env: RefCell::new(CxEnv::seeded()),
                rescheduled: Notify::new(),
                driver_started: Cell::new(false),
                client_ids: ClientIdMinter::default(),
            }),
        }
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
    /// chaser book's `stop_for_nuke`.
    pub fn stop_for_nuke(&self) -> Vec<String> {
        let stopped = self.inner.book.borrow().running_ids();
        self.terminate_all();
        stopped
    }

    /// Start a TWAP for a typed caller such as the exec `twap` command, with
    /// the same validation and effects as `twap:start`. Returns the new id.
    pub fn start(&self, options: TwapOptions) -> Result<String, String> {
        let cx = &self.inner.cx;
        let (started, effects) = {
            let mut env = self.inner.env.borrow_mut();
            env.now = cx.now_ms();
            let view = CxView {
                cx,
                chasers: &*self.inner.chasers,
                client_ids: &self.inner.client_ids,
            };
            self.inner
                .book
                .borrow_mut()
                .start(options, &mut *env, &view)
        };
        Inner::run(&self.inner, cx, effects);
        Inner::ensure_driver(&self.inner, cx);
        started
    }

    /// `twap:stop` for a typed caller; false when no running TWAP has `id`.
    /// Its chase slices keep their own lifecycle in the chaser book, as in TS.
    pub fn stop(&self, id: &str) -> bool {
        let (found, effects) = self.inner.book.borrow_mut().stop(id);
        Inner::run(&self.inner, &self.inner.cx, effects);
        self.inner.rescheduled.notify_one();
        found
    }

    /// The store's `activeTWAPs` for `account_name`.
    pub fn active_twaps(&self, account_name: &str) -> Vec<Value> {
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
        let data: Value = serde_json::from_str(data).map_err(|error| format!("{op}: {error}"))?;
        let effects = {
            let mut env = self.inner.env.borrow_mut();
            env.now = cx.now_ms();
            let view = CxView {
                cx,
                chasers: &*self.inner.chasers,
                client_ids: &self.inner.client_ids,
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
        let effects = {
            let mut env = inner.env.borrow_mut();
            env.now = cx.now_ms().max(due_at);
            let view = CxView {
                cx,
                chasers: &*inner.chasers,
                client_ids: &inner.client_ids,
            };
            inner.book.borrow_mut().wake(&mut *env, &view)
        };
        Self::run(inner, cx, effects);
    }

    fn run(inner: &Rc<Self>, cx: &AlgoCx, effects: Vec<TwapEffect>) {
        for effect in effects {
            match effect {
                TwapEffect::PlaceOrder {
                    twap_id,
                    settle_token,
                    request,
                } => {
                    Self::place(inner, cx, twap_id, settle_token, &request);
                }
                TwapEffect::RemoveChaser { chaser_id, .. } => inner.chasers.remove(cx, &chaser_id),
                TwapEffect::Log { message } => report(cx, &message),
                TwapEffect::StateUpdate { data } => {
                    cx.port.emit("twap:stateUpdate", data.to_string())
                }
                TwapEffect::Diagnostic { message } => eprintln!("[TWAP] {message}"),
            }
        }
    }

    fn place(
        inner: &Rc<Self>,
        cx: &AlgoCx,
        twap_id: String,
        settle_token: Option<u64>,
        request: &TwapOrderRequest,
    ) {
        let command = match place_command(cx, request) {
            Ok(command) => command,
            Err(message) => {
                let failure = SliceFailure {
                    message,
                    nothing_placed: true,
                };
                match settle_token {
                    Some(token) => Self::settled(inner, cx, &twap_id, token, Err(failure)),
                    None => report_remainder(cx, &request.symbol, &failure.message),
                }
                return;
            }
        };
        let future = cx.port.command(command);
        let (inner, task_cx) = (inner.clone(), cx.clone());
        let symbol = request.symbol.clone();
        cx.spawn(async move {
            let outcome = placement_outcome(future.await);
            match (settle_token, outcome) {
                (Some(token), outcome) => Self::settled(&inner, &task_cx, &twap_id, token, outcome),
                (None, Err(failure)) => report_remainder(&task_cx, &symbol, &failure.message),
                (None, Ok(_)) => {}
            }
        });
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

/// A slice placement's ids, or its failure. Ids that cannot be read and unknown
/// outcomes say the order may be live. Only a whole refusal proves nothing was
/// placed; an unclassified error or a partial one does not.
pub fn placement_outcome(result: Result<String, VenueError>) -> Result<Vec<String>, SliceFailure> {
    match result {
        Ok(result) => serde_json::from_str::<Vec<String>>(&result).map_err(|_| SliceFailure {
            message: format!("order ids unknown, the order may be live (result {result})"),
            nothing_placed: false,
        }),
        Err(error) if error.outcome == Some(Outcome::Unknown) => Err(SliceFailure {
            message: format!("outcome unknown, the order may be live: {}", error.message),
            nothing_placed: false,
        }),
        Err(error) => Err(SliceFailure {
            nothing_placed: matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                && error.partial.is_none(),
            message: error.message,
        }),
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
        self.chasers.start_with_id(self.cx, request)
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
        };
        ChaserBook::start_with_id(self, request.id.clone(), options)
    }

    fn progress(&self, chaser_id: &str) -> Option<ChaserProgress> {
        ChaserBook::progress(self, chaser_id).map(|progress| ChaserProgress {
            remaining_size: progress.remaining_size,
            stopped: progress.stopped,
        })
    }

    fn remove(&self, _cx: &AlgoCx, chaser_id: &str) {
        ChaserBook::remove(self, chaser_id);
    }
}

/// A chase slice's market remainder failed; the TWAP goes on without it.
fn report_remainder(cx: &AlgoCx, symbol: &str, message: &str) {
    report(
        cx,
        &format!("TWAP {symbol} slice remainder failed: {message}"),
    );
}

/// A TWAP error the user must see: the `error` op is what the web toasts.
/// TS used `log` (`LogSeverity.Error`), which reaches only the exchange log.
fn report(cx: &AlgoCx, message: &str) {
    cx.port.emit("error", json!(message).to_string());
}

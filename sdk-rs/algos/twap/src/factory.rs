//! Runs one account's [`TwapBook`] on the shared algo runtime: `twap:*` ops
//! arrive through [`AlgoFactory`], timers run as one driver task, and effects
//! become engine commands, chaser calls and wire replies.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algos::{AlgoCx, AlgoFactory, Scope};
use safe_cex_engine::engine::Command;
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::PlaceOrderOpts;
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::{
    ChaserProgress, ChaserRequest, OrderKind, TickerQuote, TwapBook, TwapEffect, TwapEnv,
    TwapMarketView, TwapOrderRequest,
};

/// The account's chaser runtime, as TWAP slices use it.
pub trait TwapChasers {
    fn start_with_id(&self, cx: &AlgoCx, request: &ChaserRequest);
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

/// One account's TWAPs. Build one per account registry.
pub struct TwapFactory {
    inner: Rc<Inner>,
}

struct Inner {
    book: RefCell<TwapBook>,
    chasers: Rc<dyn TwapChasers>,
    env: RefCell<CxEnv>,
    rescheduled: Notify,
    /// The driver task's scope; a disposed one means the driver must restart.
    driver: RefCell<Option<Scope>>,
}

impl TwapFactory {
    pub fn new(chasers: Rc<dyn TwapChasers>) -> Self {
        Self {
            inner: Rc::new(Inner {
                book: RefCell::new(TwapBook::new()),
                chasers,
                env: RefCell::new(CxEnv::seeded()),
                rescheduled: Notify::new(),
                driver: RefCell::new(None),
            }),
        }
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

    fn handle(&self, op: &str, data: &str, cx: &AlgoCx) -> Result<(), String> {
        let data: Value = serde_json::from_str(data).map_err(|error| format!("{op}: {error}"))?;
        let effects = {
            let mut env = self.inner.env.borrow_mut();
            env.now = cx.now_ms();
            let view = CxView {
                cx,
                chasers: &*self.inner.chasers,
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
        if inner
            .driver
            .borrow()
            .as_ref()
            .is_some_and(|scope| !scope.is_disposed())
        {
            inner.rescheduled.notify_one();
            return;
        }
        inner.driver.replace(Some(cx.scope.clone()));
        let (inner, cx) = (inner.clone(), cx.clone());
        cx.clone().spawn(async move {
            loop {
                let due_at = inner.book.borrow().wake_at();
                match due_at {
                    None => inner.rescheduled.notified().await,
                    Some(due_at) => {
                        let wait = (due_at - cx.now_ms()).max(0.0);
                        let woken = tokio::time::timeout(
                            Duration::from_millis(wait.ceil() as u64),
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
                TwapEffect::StartChaser { request, .. } => {
                    inner.chasers.start_with_id(cx, &request)
                }
                TwapEffect::RemoveChaser { chaser_id, .. } => inner.chasers.remove(cx, &chaser_id),
                // `exchange.emitter.emit('log', message, LogSeverity.Error)` on the wire.
                TwapEffect::Log { message } => {
                    cx.port.emit("log", json!([message, "error"]).to_string())
                }
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
                if let Some(token) = settle_token {
                    Self::settled(inner, cx, &twap_id, token, Err(message));
                }
                return;
            }
        };
        let future = cx.port.command(command);
        let (inner, task_cx) = (inner.clone(), cx.clone());
        cx.spawn(async move {
            let outcome = placement_outcome(future.await);
            if let Some(token) = settle_token {
                Self::settled(&inner, &task_cx, &twap_id, token, outcome);
            }
        });
    }

    fn settled(
        inner: &Rc<Self>,
        cx: &AlgoCx,
        twap_id: &str,
        token: u64,
        outcome: Result<Vec<String>, String>,
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

/// A slice placement's ids, or the message its failure log carries. Ids that
/// cannot be read and unknown outcomes say the order may be live: the TWAP
/// advances as TS does, but never as if nothing was placed.
pub fn placement_outcome(result: Result<String, VenueError>) -> Result<Vec<String>, String> {
    match result {
        Ok(result) => serde_json::from_str::<Vec<String>>(&result)
            .map_err(|_| format!("order ids unknown, the order may be live (result {result})")),
        Err(error) if error.outcome == Some(Outcome::Unknown) => Err(format!(
            "outcome unknown, the order may be live: {}",
            error.message
        )),
        Err(error) => Err(error.message),
    }
}

fn place_command(cx: &AlgoCx, request: &TwapOrderRequest) -> Result<Command, String> {
    let opts: PlaceOrderOpts = serde_json::from_value(json!({
        "symbol": request.symbol,
        "type": match request.kind {
            OrderKind::Limit => "limit",
            OrderKind::Market => "market",
        },
        "side": request.side,
        "amount": request.amount,
        "price": request.price,
        "reduceOnly": request.reduce_only,
    }))
    .map_err(|error| error.to_string())?;
    Ok(Command::PlaceOrders {
        command: cx.port.next_command_id(),
        orders: vec![opts],
    })
}

struct CxView<'a> {
    cx: &'a AlgoCx,
    chasers: &'a dyn TwapChasers,
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

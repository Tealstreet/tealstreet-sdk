use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use safe_cex_algos::executor::Instant;
use safe_cex_algos::{AlgoCx, AlgoStore, Change};
use safe_cex_engine::engine::{Command, Query};
use safe_cex_engine::error::{ErrorKind, Outcome, VenueError};
use safe_cex_engine::model::{
    CancelOrderRef, Order, OrderSide, OrderStatus, PlaceOrderOpts, RejectReason,
};
use serde::Deserialize;

use crate::chase::{
    ActiveChaser, Chase, ChaserComplete, ChaserOptions, Dirty, adjust, placement_filled,
};
use crate::client_id::ClientIdMinter;

pub const KEPT_ORDER_STOP_REASON: &str = "User cancelled, kept order in book";
pub const UNCONFIRMED_ORDER_STOP_REASON: &str = "Could not confirm order placement";

const PRICE_CHECK_MIN_INTERVAL_MS: u64 = 50;
/// After a send whose outcome is unknown.
const LOST_PLACEMENT_CHECKS_MS: [u64; 4] = [1_000, 2_000, 4_000, 8_000];
/// A stop's cancel goes on through a network or signing outage this long: past
/// the 60 s a reconnect can take, up to a laptop waking from a short sleep.
const CANCEL_RETRY_BOUND_MS: u64 = 600_000;
const CANCEL_RETRY_MIN_MS: u64 = 1_000;
const CANCEL_RETRY_MAX_MS: u64 = 10_000;
/// A cancel or fills read sent into a link that died with no answer can hang
/// for minutes; past this a cancel is tried again and fills count as unread.
/// Longer than a hosted sign's 15 s, so an offline signer is reported as that.
pub const ANSWER_BOUND_MS: u64 = 16_000;
/// A chase with no ticker change this long says so: it cannot re-quote without one.
const NO_MARKET_DATA_MS: u64 = 30_000;
/// A fills read that failed for a passing reason waits this long, then twice as long.
const FILLS_RETRY_MIN_MS: u64 = 1_000;
/// Ended chases remembered for `progress`, more than can wait on a slice timeout at once.
const ENDED_KEPT: usize = 256;
/// How an order whose cancel was retried ended.
enum Settled {
    Filled(f64),
    /// Cancelled, after filling this much.
    Cancelled(f64),
    FillsUnread,
}

/// Opens the warning that a stop's cancel is being retried.
const RETRYING: &str = "Chaser stopped, but ";
/// Open the message that ends a stop's cancel, retried or not.
const SETTLED: [&str; 2] = ["Chaser stopped: ", "Chaser could not cancel "];

/// The order a stop's retrying warning names, for a host that keeps warnings.
pub fn retrying_order(message: &str) -> Option<&str> {
    message.strip_prefix(RETRYING)?;
    named_orders(message).into_iter().next()
}

/// The orders a message ending a stop's cancel names: a kept retrying
/// warning for one of them is superseded.
pub fn settled_orders(message: &str) -> Vec<&str> {
    if SETTLED.iter().any(|prefix| message.starts_with(prefix)) {
        named_orders(message)
    } else {
        Vec::new()
    }
}

fn named_orders(message: &str) -> Vec<&str> {
    let Some((_, rest)) = message.split_once(" order ") else {
        return Vec::new();
    };
    // "o1 is ...", "o1: ..." or a list "o1, o2: ...".
    let mut ids = Vec::new();
    for word in rest.split(' ') {
        match word.strip_suffix(',') {
            Some(id) => ids.push(id),
            None => {
                ids.push(word.trim_end_matches(':'));
                break;
            }
        }
    }
    ids.retain(|id| !id.is_empty());
    ids
}

/// Close All walks positions only, so it never reaches a stranded chase order.
const CANCEL_WAY_OUT: &str = "Cancel it in Open Orders or use Nuke";
/// Each chase runs its own supervisor, price checks and REST reconciles.
pub const MAX_CHASES_PER_ACCOUNT: usize = 50;
const MISSING_ORDER_CONFIRMATIONS: u32 = 2;
/// Freeze cancel+replace while partial fills land: a cancel racing an
/// in-flight fill makes the replacement overfill the target.
const HOLD_STEADY_AFTER_PARTIAL_FILL_MS: u128 = 1000;
const SETTLE_AFTER_PLACEMENT_MS: u64 = 1000;
/// Venues whose order rows lag or vanish, so fills are tracked by event and
/// reconciled over REST before any replacement.
const FILL_TRACKED_VENUES: [&str; 3] = ["hyperliquid", "grvt", "bybitdemo"];
/// Their engines mark a row cancelled on "unknown order" (-2011), which a filled
/// order also answers, so only the venue's fills tell. An order id read here
/// takes no time bound: the order id alone selects the rows.
fn cancel_answer_hides_fills(venue: &str) -> bool {
    venue.starts_with("binance") || venue == "asterdex"
}
/// Venues whose order amounts are lots; their fills read in base units.
fn amounts_in_lots(venue: &str) -> bool {
    matches!(venue, "kucoin" | "mexc" | "blofin" | "okx" | "bitrue")
}
/// How long a row may trail its cancel's answer (Phemex acks `Init`) before fills are read.
const ROW_CLOSE_WAIT_MS: u64 = 1_000;
const ROW_CLOSE_POLL_MS: u64 = 100;
/// Fills are read from this long before the chase started, for clock skew.
const FILLS_FROM_MARGIN_MS: f64 = 60_000.0;

/// Progress of one chase, for TWAP slices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub remaining_size: f64,
    pub stopped: bool,
    /// Finished: nothing of this chase is still working.
    pub ended: bool,
    /// A placement or cancellation has not proved whether an order remains open.
    pub order_may_be_open: bool,
}

/// The chases of one account: the TS `ChaserOrderManager`.
pub struct ChaserBook {
    cx: AlgoCx,
    chases: RefCell<Vec<Rc<Chase>>>,
    /// How recent chases ended, for a caller that waits on one (a TWAP slice).
    ended: RefCell<VecDeque<(String, Progress)>>,
    client_ids: ClientIdMinter,
    next_id: Cell<u64>,
}

/// Worth trying again: the request never reached a verdict from the venue.
/// A COSIGNED account's browser signs its writes: with it gone a requote cannot be placed.
pub const SIGNER_OFFLINE_STOP_REASON: &str =
    "Stopped: the browser that signs for this account went offline";

fn signer_offline(error: &VenueError) -> bool {
    error.message.contains("Signing provider is unavailable")
}

fn fills_read_retryable(error: &VenueError) -> bool {
    matches!(
        error.kind,
        Some(ErrorKind::Network | ErrorKind::Timeout | ErrorKind::RateLimited)
    )
}

/// A placement refusal a re-priced retry only repeats, as (stop reason, message text):
/// a refused key, or an order no price fixes (a reduce-only size above the position,
/// an unaffordable or malformed order, an unsupported type).
fn settled_refusal(error: &VenueError) -> Option<(&'static str, &'static str)> {
    match error.kind? {
        ErrorKind::AuthInvalid | ErrorKind::AuthPermission => Some((
            "The venue refused this account's key",
            "the venue refused this account's key",
        )),
        ErrorKind::InvalidInput | ErrorKind::InsufficientBalance | ErrorKind::Unsupported => {
            Some(("The venue refused the order", "the venue refused the order"))
        }
        _ => None,
    }
}

fn cancel_retryable(error: &VenueError) -> bool {
    matches!(
        error.kind,
        Some(
            ErrorKind::Network
                | ErrorKind::Timeout
                | ErrorKind::VenueUnavailable
                | ErrorKind::RateLimited
        )
    ) || signer_offline(error)
}

fn has_unknown_outcome(error: &VenueError) -> bool {
    use safe_cex_engine::write_plan::ElementResult;
    error.outcome == Some(Outcome::Unknown)
        || error.elements.as_ref().is_some_and(|rows| {
            rows.iter().any(|row| match &row.result {
                ElementResult::Unknown { .. } => true,
                ElementResult::NotSent { error: Some(error) } => has_unknown_outcome(error),
                _ => false,
            })
        })
}

fn stop_twap_unknown_cancel(chase: &Chase, error: &VenueError) -> bool {
    if chase.options.twap_id.is_none() || !has_unknown_outcome(error) {
        return false;
    }
    // Retain uncertainty for TWAP's journal and relinquish every remaining cancel.
    chase.note_order_may_be_open();
    chase.release_orders();
    chase.mark_complete("The outcome is unknown. Check Open Orders");
    true
}

impl ChaserBook {
    pub fn new(cx: AlgoCx) -> Rc<Self> {
        Rc::new(Self {
            cx,
            chases: RefCell::new(Vec::new()),
            ended: RefCell::new(VecDeque::new()),
            client_ids: ClientIdMinter::default(),
            next_id: Cell::new(0),
        })
    }

    fn new_id(&self) -> String {
        let sequence = self.next_id.get() + 1;
        self.next_id.set(sequence);
        format!("{:x}{sequence:x}", self.cx.now_ms() as u64)
    }

    /// Start a chase with a fresh id.
    pub fn start(self: &Rc<Self>, options: ChaserOptions) -> Result<String, String> {
        let id = self.new_id();
        self.start_with_id(id.clone(), options)?;
        Ok(id)
    }

    /// Start a chase under a caller-chosen id (TWAP slices).
    pub fn start_with_id(
        self: &Rc<Self>,
        id: String,
        options: ChaserOptions,
    ) -> Result<(), String> {
        if !(options.size.is_finite() && options.size > 0.0) {
            return Err(format!(
                "Chase for {} was not started: size must be positive",
                options.symbol
            ));
        }
        if self.chases.borrow().len() >= MAX_CHASES_PER_ACCOUNT {
            return Err(format!(
                "Chase for {} was not started: {MAX_CHASES_PER_ACCOUNT} chases are already active on this account",
                options.symbol
            ));
        }
        if self.chases.borrow().iter().any(|chase| chase.id == id) {
            return Err(format!("Chase {id} already exists"));
        }
        let chase = Rc::new(Chase::new(
            id,
            options,
            &self.cx.port.store(),
            self.cx.scope.child(),
            self.cx.now_ms(),
        ));
        self.chases.borrow_mut().push(chase.clone());
        let book = self.clone();
        chase.scope.clone().spawn(async move {
            book.clone().run(chase.clone()).await;
            chase.scope.dispose();
        });
        Ok(())
    }

    pub fn progress(&self, id: &str) -> Option<Progress> {
        self.find(id)
            .map(|chase| Progress {
                remaining_size: chase.remaining(),
                stopped: chase.is_stopped(),
                ended: false,
                order_may_be_open: chase.order_may_be_open(),
            })
            .or_else(|| {
                let ended = self.ended.borrow();
                ended
                    .iter()
                    .find(|(ended, _)| ended == id)
                    .map(|(_, progress)| *progress)
            })
    }

    pub fn active_chasers(&self) -> Vec<ActiveChaser> {
        self.chases
            .borrow()
            .iter()
            .map(|chase| chase.active())
            .collect()
    }

    /// User STOP: stop and cancel the resting order.
    pub fn remove(self: &Rc<Self>, id: &str) {
        if let Some(chase) = self.find(id) {
            chase.mark_complete("User cancelled");
            self.check_and_mark_if_complete(&chase);
            self.spawn_cleanup(chase);
        }
    }

    /// `chase cancel keep`: stop, leaving the resting order in the book.
    pub fn remove_keep_order(self: &Rc<Self>, id: &str) {
        let Some(chase) = self.find(id) else { return };
        chase.mark_complete(KEPT_ORDER_STOP_REASON);
        chase.take_current();
        chase.watch_scope.dispose();
        if chase.claim_complete_emission() {
            let book = self.clone();
            self.cx.spawn(async move {
                book.emit_complete(&chase).await;
                book.detach(&chase);
            });
        }
    }

    /// Unknown halts relinquish both current and still-pending orders, without cancelling.
    pub fn halt_keep_order(self: &Rc<Self>, id: &str) {
        if let Some(chase) = self.find(id) {
            chase.release_orders();
        }
        self.remove_keep_order(id);
    }

    /// Stop every chase; their cleanups run in the background.
    pub fn terminate_all(self: &Rc<Self>, reason: &str) {
        for chase in self.stop_all(reason) {
            self.spawn_cleanup(chase);
        }
    }

    /// Stop every chase and wait until each has cancelled its resting order.
    /// Await this before disposing the account scope: disposal aborts a chase
    /// mid-flight and would leave its order live and untracked.
    pub async fn shutdown(self: &Rc<Self>, reason: &str) {
        for chase in self.stop_all(reason) {
            self.settle_once(chase).await;
        }
    }

    /// A nuke's first step: stop every running chase, wait until each has cancelled
    /// its resting order, and return the ids it stopped. Unlike `shutdown`, the
    /// account stays up, so no disposal warning is emitted.
    pub async fn stop_for_nuke(self: &Rc<Self>, reason: &str) -> Vec<String> {
        let chases = self.chases.borrow().clone();
        let mut stopped = Vec::new();
        for chase in &chases {
            if !chase.is_stopped() {
                chase.mark_complete(reason);
                stopped.push(chase.id.clone());
            }
        }
        for chase in chases {
            self.settle_once(chase).await;
        }
        stopped
    }

    fn stop_all(&self, reason: &str) -> Vec<Rc<Chase>> {
        let chases = self.chases.borrow().clone();
        for chase in &chases {
            if !chase.is_stopped() {
                chase.mark_complete(reason);
                self.warn(&format!(
                    "Chaser for {} was stopped because the exchange connection was disposed",
                    chase.options.symbol
                ));
            }
        }
        chases
    }

    fn find(&self, id: &str) -> Option<Rc<Chase>> {
        self.chases
            .borrow()
            .iter()
            .find(|chase| chase.id == id)
            .cloned()
    }

    fn detach(&self, chase: &Rc<Chase>) {
        self.chases
            .borrow_mut()
            .retain(|other| !Rc::ptr_eq(other, chase));
    }

    fn store(&self) -> std::cell::Ref<'_, AlgoStore> {
        self.cx.port.store()
    }

    fn emit_json(&self, op: &str, message: &str) {
        self.cx
            .port
            .emit(op, serde_json::to_string(message).unwrap_or_default());
    }

    fn warn(&self, message: &str) {
        self.emit_json("warning", message);
    }

    fn error(&self, message: &str) {
        self.emit_json("error", message);
    }

    /// The store shows the order finished: filled or cancelled, nothing to cancel.
    fn known_closed(&self, id: &str) -> bool {
        self.store().order(id).is_some_and(|order| {
            matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
        })
    }

    /// A stopping chase's cancel the venue refused leaves the order open: say so,
    /// rather than reporting the chase finished as if nothing were left behind.
    /// Not for an order the store has since seen finish, while the cancel waited.
    fn cancel_failed(&self, symbol: &str, ids: &[String], error: &VenueError) -> bool {
        let gone = [
            "already canceled",
            "already filled",
            "never placed",
            "not exist",
            // The engine holds no such row: the cancel succeeded or the order finished.
            "Order not found",
        ]
        .iter()
        .any(|phrase| error.message.contains(phrase));
        let open: Vec<&str> = ids
            .iter()
            .filter(|id| !self.known_closed(id))
            .map(String::as_str)
            .collect();
        let told = !gone && !open.is_empty();
        if told {
            self.error(&format!(
                "Chaser could not cancel {symbol} order {}: {}; it may still be open. {CANCEL_WAY_OUT}",
                open.join(", "),
                error.message
            ));
        }
        told
    }

    fn log(&self, chase: &Chase, message: &str) {
        self.emit_json("log", &format!("[CHASER{}] {message}", chase.id));
    }

    fn fill_tracked(&self) -> bool {
        FILL_TRACKED_VENUES.contains(&self.store().venue.as_str())
    }

    /// A venue that answers a placement with its client id (Lighter) later shows
    /// the order under its own id; the chase follows it there.
    fn adopt_venue_ids(&self, chase: &Chase) {
        let adopted: Vec<(String, String)> = {
            let store = self.store();
            chase
                .current()
                .into_iter()
                .filter_map(|placed| {
                    let row = store.orders.iter().find(|order| {
                        order.symbol == chase.options.symbol
                            && order.id != placed
                            && order.client_order_id.as_deref() == Some(placed.as_str())
                    })?;
                    Some((placed, row.id.clone()))
                })
                .collect()
        };
        for (placed, venue_id) in adopted {
            chase.adopt_venue_id(&placed, &venue_id);
        }
    }

    fn tracked_orders(&self, chase: &Chase) -> Vec<Order> {
        let current = chase.current();
        self.store()
            .orders
            .iter()
            .filter(|order| current.contains(&order.id))
            .cloned()
            .collect()
    }

    fn position_gone(&self, chase: &Chase) -> bool {
        let side = chase.options.reduced_side();
        !self.store().positions.iter().any(|position| {
            position.symbol == chase.options.symbol
                && position.side == side
                && position.contracts != 0.0
        })
    }

    /// Bybit Demo: position rows still arrive over REST when order frames do not.
    fn reached_target_position(&self, chase: &Chase) -> bool {
        let store = self.store();
        let Some(target) = chase.target_position_contracts else {
            return false;
        };
        if store.venue != "bybitdemo" {
            return false;
        }
        let side = chase.options.entry_side();
        let contracts = store
            .positions
            .iter()
            .find(|position| position.symbol == chase.options.symbol && position.side == side)
            .map_or(0.0, |position| position.contracts.abs());
        contracts >= target - 1e-12
    }

    fn best_price(&self, chase: &Chase) -> Result<f64, String> {
        let store = self.store();
        let symbol = &chase.options.symbol;
        let (Some(market), Some(ticker)) = (store.markets.get(symbol), store.tickers.get(symbol))
        else {
            return Err("No market or ticker found".into());
        };
        let step = market.precision.price;
        let bid = if ticker.bid != 0.0 {
            ticker.bid
        } else if ticker.last != 0.0 && step != 0.0 {
            (ticker.last - step).max(0.0)
        } else {
            ticker.last
        };
        let ask = if ticker.ask != 0.0 {
            ticker.ask
        } else if ticker.last != 0.0 && step != 0.0 {
            ticker.last + step
        } else {
            ticker.last
        };
        let distance = step * f64::from(chase.min_tick_distance());
        // Stale or locked quotes: buys stay below the ask, sells above the bid.
        let raw = match chase.options.side {
            OrderSide::Buy if ask > 0.0 => (bid - distance).min(ask - step),
            OrderSide::Buy => bid - distance,
            OrderSide::Sell if bid > 0.0 => (ask + distance).max(bid + step),
            OrderSide::Sell => ask + distance,
        };
        // f64::max would turn a NaN quote into a one-tick order; refuse it.
        if !(raw.is_finite() && step.is_finite() && step > 0.0) {
            return Err("No usable quote".into());
        }
        Ok(adjust(raw.max(step), step))
    }

    fn check_and_mark_if_complete(&self, chase: &Chase) {
        if chase.options.reduce_only() && self.position_gone(chase) {
            chase.mark_complete("Completed");
            return;
        }
        let orders = self.tracked_orders(chase);
        if !orders.is_empty()
            && orders
                .iter()
                .all(|order| order.status == OrderStatus::Closed)
        {
            chase.mark_complete("Completed");
            chase.take_current();
        }
    }

    async fn command(&self, build: impl FnOnce(u32) -> Command) -> Result<String, VenueError> {
        let id = self.cx.port.next_command_id();
        self.cx.port.command(build(id)).await
    }

    async fn cancel(&self, symbol: &str, ids: &[String]) -> Result<String, VenueError> {
        let orders = ids
            .iter()
            .map(|id| CancelOrderRef {
                id: id.clone(),
                symbol: symbol.into(),
            })
            .collect();
        // A row the store lost (a stale snapshot, a reconnect) still rests at the
        // venue: name it by symbol, or a by-id cancel is refused as unknown.
        let unlisted = ids.iter().any(|id| self.store().order(id).is_none());
        let capabilities = self.cx.port.capabilities();
        self.command(|command| {
            if unlisted {
                Command::CancelOrdersWithSymbols { command, orders }
            } else {
                safe_cex_algos::cancel_command(&capabilities, command, orders)
            }
        })
        .await
    }

    /// The venue's fills for `ids`, applied; a fill the account had not shown
    /// is told, since no fill event announced it.
    async fn confirm_fills(&self, chase: &Chase, ids: &[String]) -> Result<(), String> {
        if ids.is_empty() {
            return Ok(());
        }
        for (id, filled) in self.filled_quantities(chase, ids).await? {
            if filled > chase.filled_by(&id) {
                chase.set_filled_amount(&id, filled);
                self.warn(&format!(
                    "Chaser for {}: order {id} filled {filled}; its fill was missed and found at the venue",
                    chase.options.symbol
                ));
            }
        }
        Ok(())
    }

    fn position_contracts(&self, chase: &Chase) -> f64 {
        self.store()
            .positions
            .iter()
            .filter(|position| position.symbol == chase.options.symbol)
            .map(|position| position.contracts.abs())
            .sum()
    }

    /// A cancel that got no answer in time is a timeout, tried again like a lost link.
    async fn bounded_cancel(&self, symbol: &str, ids: &[String]) -> Result<String, VenueError> {
        safe_cex_algos::executor::timeout(ANSWER_BOUND_MS, self.cancel(symbol, ids))
            .await
            .unwrap_or_else(|_| {
                Err(VenueError::new(format!(
                    "The cancel got no answer in {} s",
                    ANSWER_BOUND_MS / 1000
                ))
                .kind(ErrorKind::Timeout)
                .outcome(Outcome::Unknown))
            })
    }

    async fn run(self: Rc<Self>, chase: Rc<Chase>) {
        let changes = self.cx.port.subscribe();
        self.log(
            &chase,
            &format!("processInstance starting for {}", chase.options.symbol),
        );
        let ids = self.try_place_orders(&chase).await;
        chase.set_current(ids);
        if chase.current().is_empty() {
            if !chase.is_stopped() {
                self.warn(&format!(
                    "Chaser for {} could not start - no order was placed",
                    chase.options.symbol
                ));
                chase.mark_complete("No order placed");
            }
            self.cleanup(chase).await;
            return;
        }
        if chase.is_stopped() {
            self.cleanup(chase).await;
            return;
        }
        self.spawn_watcher(&chase, changes);
        self.cx.sleep_ms(SETTLE_AFTER_PLACEMENT_MS).await;
        let mut last_check: Option<Instant> = None;
        loop {
            if chase.is_stopped() {
                break;
            }
            chase.wake.notified().await;
            if chase.is_stopped() {
                break;
            }
            let dirty = chase.take_dirty();
            // Ticker-only storms are rate limited to one trailing check; order
            // and position changes are checked at once.
            if !dirty.orders
                && !dirty.positions
                && let Some(at) = last_check
            {
                let elapsed = at.elapsed().as_millis() as u64;
                if elapsed < PRICE_CHECK_MIN_INTERVAL_MS {
                    safe_cex_algos::executor::sleep_ms(PRICE_CHECK_MIN_INTERVAL_MS - elapsed).await;
                    chase.take_dirty();
                }
            }
            last_check = Some(Instant::now());
            self.run_price_check(&chase).await;
        }
        self.cleanup(chase).await;
    }

    /// Applies fill state the moment a change arrives, even during a
    /// cancel/replace pass, and confirms a vanished order across two frames.
    fn spawn_watcher(
        self: &Rc<Self>,
        chase: &Rc<Chase>,
        mut changes: tokio::sync::mpsc::UnboundedReceiver<Change>,
    ) {
        self.spawn_market_data_watch(chase);
        let book = self.clone();
        let chase = chase.clone();
        let fill_tracked = self.fill_tracked();
        chase.watch_scope.clone().spawn(async move {
            let mut seen = false;
            let mut missing = 0;
            let mut placements = chase.placements();
            chase.position_seen.set(Some(book.position_contracts(&chase)));
            while let Some(change) = changes.recv().await {
                if chase.is_stopped() {
                    chase.wake.notify_one();
                    return;
                }
                if change.tickers {
                    chase.tickers_seen.set(chase.tickers_seen.get() + 1);
                }
                if change.resync {
                    chase.needs_reconcile.set(true);
                }
                if fill_tracked {
                    let ever = chase.ever();
                    for fill in change.fills.iter().filter(|fill| {
                        ever.contains(&fill.order_id) && fill.symbol == chase.options.symbol
                    }) {
                        chase.add_fill_amount(&fill.order_id, fill.amount);
                    }
                }
                if change.positions || change.resync {
                    let contracts = book.position_contracts(&chase);
                    if chase.position_seen.replace(Some(contracts)) != Some(contracts) {
                        chase.verify_fills.set(true);
                    }
                }
                if change.orders {
                    book.adopt_venue_ids(&chase);
                    book.apply_replaced_fills(&chase);
                    if chase.placements() != placements {
                        placements = chase.placements();
                        seen = false;
                        missing = 0;
                    }
                    let orders = book.tracked_orders(&chase);
                    if orders.is_empty() {
                        if seen && !chase.pending_update() {
                            missing += 1;
                            if missing >= MISSING_ORDER_CONFIRMATIONS {
                                chase.mark_complete("Tracked orders disappeared from store");
                                book.warn(&format!(
                                    "Chaser for {} stopped: its order left the order book (filled or cancelled externally); any resting remainder was cancelled",
                                    chase.options.symbol
                                ));
                                return;
                            }
                        }
                    } else {
                        seen = true;
                        missing = 0;
                        book.apply_order_fill_state(&chase, &orders);
                    }
                }
                if chase.is_stopped() {
                    chase.wake.notify_one();
                    return;
                }
                chase.mark_dirty(Dirty { orders: change.orders, positions: change.positions, tickers: change.tickers });
            }
        });
    }

    /// A resting chase re-quotes only on a ticker change, so a dead feed would leave
    /// it sitting in silence. Told once per silence.
    fn spawn_market_data_watch(self: &Rc<Self>, chase: &Rc<Chase>) {
        let book = self.clone();
        let watched = chase.clone();
        chase.watch_scope.clone().spawn(async move {
            let mut told = false;
            loop {
                let seen = watched.tickers_seen.get();
                book.cx.sleep_ms(NO_MARKET_DATA_MS).await;
                if watched.is_stopped() {
                    return;
                }
                if watched.tickers_seen.get() != seen {
                    told = false;
                } else if !told {
                    told = true;
                    book.warn(&format!(
                        "Chaser for {} has had no market data update for {} s and re-quotes only when one arrives. \
                         If the market is not just quiet, check your connection or stop the chase.",
                        watched.options.symbol,
                        NO_MARKET_DATA_MS / 1000
                    ));
                }
            }
        });
    }

    /// A fill landing on an order already replaced still comes off what is left.
    fn apply_replaced_fills(&self, chase: &Chase) {
        let (ever, current) = (chase.ever(), chase.current());
        let store = self.store();
        for order in store
            .orders
            .iter()
            .filter(|order| ever.contains(&order.id) && !current.contains(&order.id))
        {
            chase.subtract_filled_size(order);
        }
    }

    /// Returns true when the chase is over (all filled or stopped).
    fn apply_order_fill_state(&self, chase: &Chase, orders: &[Order]) -> bool {
        for order in orders {
            let post_only_reject = order.status == OrderStatus::Canceled
                && order.extra.as_ref().and_then(|extra| extra.reject_reason)
                    == Some(RejectReason::PostOnly);
            if !post_only_reject {
                chase.subtract_filled_size(order);
            }
        }
        if orders
            .iter()
            .all(|order| order.status == OrderStatus::Closed)
        {
            chase.mark_complete("Completed");
        }
        chase.is_stopped()
    }

    async fn run_price_check(&self, chase: &Rc<Chase>) {
        if chase.verify_fills.replace(false) {
            let current = chase.current();
            if let Err(error) = self.confirm_fills(chase, &current).await {
                self.log(
                    chase,
                    &format!("fills check after a position change failed: {error}"),
                );
            }
            if chase.is_stopped() {
                return;
            }
        }
        if chase.options.terminate.is_some() {
            let last = self
                .store()
                .tickers
                .get(&chase.options.symbol)
                .map(|ticker| ticker.last);
            if let Some(price) = last.filter(|price| *price != 0.0)
                && chase.should_terminate(price)
            {
                chase.mark_complete("Terminated by length condition");
                return;
            }
        }
        if self.reached_target_position(chase) {
            chase.mark_complete("Completed");
            return;
        }
        let orders = self.tracked_orders(chase);
        if !orders.is_empty() {
            chase.set_pending_update(true);
            if let Err(message) = self.update_orders(chase, orders).await {
                if !chase.is_stopped() {
                    chase.mark_complete("Chaser store update failed");
                }
                self.error(&format!(
                    "Chaser supervisor failed during store update for {}: {message}",
                    chase.options.symbol
                ));
            }
            chase.set_pending_update(false);
        }
    }

    async fn update_orders(&self, chase: &Rc<Chase>, orders: Vec<Order>) -> Result<(), String> {
        let order_price = orders
            .iter()
            .find(|order| order.status == OrderStatus::Open)
            .map(|order| order.price);
        let best = self.best_price(chase)?;
        if chase.options.reduce_only() && self.position_gone(chase) {
            chase.mark_complete("Completed");
            return Ok(());
        }
        let done = self.apply_order_fill_state(chase, &orders);
        let all_canceled = orders
            .iter()
            .all(|order| order.status == OrderStatus::Canceled);
        if chase.take_fresh_order() {
            if all_canceled {
                chase.bump_tick_distance();
            }
        } else if all_canceled && !done {
            let rejected = orders.iter().any(|order| {
                order.extra.as_ref().and_then(|extra| extra.reject_reason)
                    == Some(RejectReason::PostOnly)
            });
            // Not this chase's cancel, or the row would be replaced: Cancel All or the venue.
            if !rejected {
                chase.mark_complete("Its order was cancelled outside the chase");
                self.warn(&format!(
                    "Chaser for {} stopped: its order was cancelled outside the chase",
                    chase.options.symbol
                ));
                return Ok(());
            }
        }
        if done {
            return Ok(());
        }
        if let Some(price) = order_price {
            let favorable = match chase.options.side {
                OrderSide::Buy => best < price,
                OrderSide::Sell => best > price,
            };
            if favorable {
                return Ok(());
            }
        }
        if chase
            .since_last_partial_fill_ms()
            .is_some_and(|ms| ms < HOLD_STEADY_AFTER_PARTIAL_FILL_MS)
        {
            return Ok(());
        }
        if order_price == Some(best) {
            return Ok(());
        }
        chase.increment_attempts();
        if chase.is_stopped() {
            return Ok(());
        }
        let fill_tracked = self.fill_tracked();
        let to_cancel: Vec<String> = orders
            .iter()
            .filter(|order| {
                order.status != OrderStatus::Closed && order.status != OrderStatus::Canceled
            })
            .map(|order| order.id.clone())
            .collect();
        if !to_cancel.is_empty() {
            self.log(chase, "cancelling orders");
            if let Err(error) = self.bounded_cancel(&chase.options.symbol, &to_cancel).await {
                if stop_twap_unknown_cancel(chase, &error) {
                    return Ok(());
                }
                // The stop's own cancel keeps trying; this chase places nothing more.
                if cancel_retryable(&error) {
                    chase.mark_complete(&if signer_offline(&error) {
                        SIGNER_OFFLINE_STOP_REASON.to_owned()
                    } else {
                        format!("Stopped: a cancel attempt failed: {}", error.message)
                    });
                    return Ok(());
                }
                let message = error.message.as_str();
                let filled_or_gone = ["already canceled", "already filled", "never placed"]
                    .iter()
                    .any(|phrase| message.contains(phrase));
                let finished_meanwhile = to_cancel.iter().all(|id| self.known_closed(id));
                if !(fill_tracked && filled_or_gone) && !finished_meanwhile {
                    self.error("Chaser failed to cancel orders, stopping");
                    chase.mark_complete("Failed to cancel orders");
                    return Ok(());
                }
                // Before any replacement, so a fill not yet applied cannot overfill.
                if finished_meanwhile {
                    self.apply_order_fill_state(chase, &self.tracked_orders(chase));
                }
            }
        }
        let resync = chase.needs_reconcile.replace(false);
        if resync && !fill_tracked {
            // Order rows carry cumulative fills and the rejoin snapshot
            // refreshes them, so REST only tops up; a venue without fill
            // lookup by order id falls back to the rows.
            match self.filled_quantities(chase, &chase.ever()).await {
                Ok(filled) => {
                    for (id, amount) in filled {
                        chase.set_filled_amount(&id, amount);
                    }
                }
                Err(error) => self.log(
                    chase,
                    &format!("resync fill reconcile unavailable: {error}"),
                ),
            }
            if chase.is_stopped() {
                return Ok(());
            }
        }
        // A cancel the venue answers as done (Bybit 110001) may be an order that
        // filled unseen: only the venue's fills tell, and none, no replacement.
        let hides_fills = cancel_answer_hides_fills(&self.store().venue);
        if !hides_fills && !fill_tracked {
            let mut waited = 0;
            while waited < ROW_CLOSE_WAIT_MS
                && !chase.is_stopped()
                && !to_cancel.iter().all(|id| self.known_closed(id))
            {
                self.cx.sleep_ms(ROW_CLOSE_POLL_MS).await;
                waited += ROW_CLOSE_POLL_MS;
            }
        }
        let unconfirmed: Vec<String> = to_cancel
            .iter()
            .filter(|id| hides_fills || !self.known_closed(id))
            .cloned()
            .collect();
        if !fill_tracked && !unconfirmed.is_empty() {
            if let Err(error) = self.confirm_fills(chase, &unconfirmed).await {
                chase.mark_complete("Could not confirm the order before replacing it");
                // Its cancel succeeded: a second one could only read as a failure.
                chase.take_current();
                self.error(&format!(
                    "Chaser for {} stopped before replacing order {}: whether it filled could not be read ({error}). Check Positions and Open Orders",
                    chase.options.symbol,
                    unconfirmed.join(", ")
                ));
                return Ok(());
            }
            if chase.is_stopped() {
                return Ok(());
            }
        }
        if fill_tracked {
            // A cancel can have won behind an ambiguous answer: an unknown fill
            // would overfill, so a failed reconciliation stops the chase. After
            // a rejoin every order this chase placed is reconciled.
            let ids = if resync {
                chase.ever()
            } else {
                chase.current()
            };
            match self.filled_quantities(chase, &ids).await {
                Ok(filled) => {
                    for (id, amount) in filled {
                        chase.set_filled_amount(&id, amount);
                    }
                    if chase.is_stopped() {
                        return Ok(());
                    }
                }
                Err(error) => {
                    let message = format!(
                        "Chaser reconciliation failed after cancellation; stopped before replacement: {error}"
                    );
                    chase.mark_complete("Failed to reconcile fills after cancellation");
                    // Its cancel succeeded: a second one could only read as a failure.
                    chase.take_current();
                    self.error(&message);
                    return Ok(());
                }
            }
        }
        if chase.is_stopped() {
            return Ok(());
        }
        let ids = self.try_place_orders(chase).await;
        chase.set_current(ids);
        if chase.is_stopped() {
            return Ok(());
        }
        self.cx.sleep_ms(SETTLE_AFTER_PLACEMENT_MS).await;
        Ok(())
    }

    async fn filled_quantities(
        &self,
        chase: &Chase,
        ids: &[String],
    ) -> Result<Vec<(String, f64)>, String> {
        self.bounded_fills_read(|| self.read_filled_quantities(chase, ids))
            .await
    }

    /// One answer inside `ANSWER_BOUND_MS`. The order is already cancelled, so a
    /// read that failed for a passing reason can wait; a refusal ends it at once.
    async fn bounded_fills_read<T, F: std::future::Future<Output = Result<T, VenueError>>>(
        &self,
        read: impl Fn() -> F,
    ) -> Result<T, String> {
        let last_failure = RefCell::new(None);
        let attempts = async {
            let mut wait = FILLS_RETRY_MIN_MS;
            loop {
                match read().await {
                    Err(error) if fills_read_retryable(&error) => {
                        *last_failure.borrow_mut() = Some(error.message);
                        self.cx.sleep_ms(wait).await;
                        wait *= 2;
                    }
                    answer => return answer.map_err(|error| error.message),
                }
            }
        };
        safe_cex_algos::executor::timeout(ANSWER_BOUND_MS, attempts)
            .await
            .unwrap_or_else(|_| {
                let seconds = ANSWER_BOUND_MS / 1000;
                Err(match last_failure.borrow_mut().take() {
                    Some(failure) => format!("fills read kept failing for {seconds} s: {failure}"),
                    None => format!("fills read got no answer in {seconds} s"),
                })
            })
    }

    /// Every order of the chase in one read: its orders can number hundreds.
    async fn chase_fills(
        &self,
        chase: &Chase,
        ids: &[String],
    ) -> Result<Vec<(String, f64)>, String> {
        let read = || async {
            let query = Query::Fills {
                symbol: Some(chase.options.symbol.clone()),
                order_id: None,
                from_ms: Some(chase.started_ms - FILLS_FROM_MARGIN_MS),
                to_ms: None,
                limit: Some(1000),
            };
            let result = self
                .command(|command| Command::Query { command, query })
                .await?;
            let rows: Vec<Execution> = serde_json::from_str(&result)
                .map_err(|error| VenueError::new(error.to_string()))?;
            let unit = self.base_per_placement_unit(chase);
            Ok(ids
                .iter()
                .map(|id| {
                    let total: f64 = rows
                        .iter()
                        .filter(|row| &row.order_id == id)
                        .map(|row| row.qty)
                        .sum();
                    (id.clone(), total / unit)
                })
                .collect())
        };
        self.bounded_fills_read(read).await
    }

    async fn read_filled_quantities(
        &self,
        chase: &Chase,
        ids: &[String],
    ) -> Result<Vec<(String, f64)>, VenueError> {
        let mut filled = Vec::new();
        for id in ids {
            let query = Query::Fills {
                symbol: Some(chase.options.symbol.clone()),
                order_id: Some(id.clone()),
                from_ms: (!cancel_answer_hides_fills(&self.store().venue))
                    .then_some(chase.started_ms - FILLS_FROM_MARGIN_MS),
                to_ms: None,
                limit: Some(1000),
            };
            let result = self
                .command(|command| Command::Query { command, query })
                .await?;
            let rows: Vec<Execution> = serde_json::from_str(&result)
                .map_err(|error| VenueError::new(error.to_string()))?;
            let total = rows
                .iter()
                .filter(|row| &row.order_id == id)
                .map(|row| row.qty)
                .sum::<f64>()
                / self.base_per_placement_unit(chase);
            filled.push((id.clone(), total));
        }
        Ok(filled)
    }

    fn base_per_placement_unit(&self, chase: &Chase) -> f64 {
        let store = self.store();
        if !amounts_in_lots(&store.venue) {
            return 1.0;
        }
        store
            .markets
            .get(&chase.options.symbol)
            .and_then(|market| market.contract_size)
            .filter(|size| *size > 0.0)
            .unwrap_or(1.0)
    }

    /// What landed, even after a STOP: the caller makes it current for cleanup to cancel.
    async fn try_place_orders(&self, chase: &Rc<Chase>) -> Vec<String> {
        chase.set_placing(true);
        let amount = chase.remaining();
        let ids = self.place_until_resting(chase).await;
        // A split order's parts are not known apart.
        if let [id] = ids.as_slice() {
            chase.note_placed(id, amount);
        }
        chase.set_placing(false);
        ids
    }

    async fn place_until_resting(&self, chase: &Rc<Chase>) -> Vec<String> {
        let max_tries = if self.store().venue == "binance" {
            20
        } else {
            10
        };
        let mut tries = 1;
        let mut last_refusal: Option<String> = None;
        loop {
            if chase.is_stopped() {
                return Vec::new();
            }
            if chase.remaining() == 0.0 {
                chase.mark_complete("No remaining size. User likely cancelled order");
                self.error("Chaser has no remaining size. User likely cancelled order");
                return Vec::new();
            }
            if tries >= max_tries {
                self.error(&format!(
                    "Chaser failed to place orders after {max_tries} tries{}",
                    last_refusal
                        .as_deref()
                        .map_or_else(String::new, |refusal| format!(": {refusal}"))
                ));
                chase.mark_complete(&format!("Failed to place orders after {max_tries} tries"));
                self.check_and_mark_if_complete(chase);
                return Vec::new();
            }
            chase.set_fresh_order(tries == 1);
            let price = match self.best_price(chase) {
                Ok(price) => price,
                Err(message) => {
                    chase.mark_complete(&message);
                    self.error(&format!("Chaser: {message} for {}", chase.options.symbol));
                    return Vec::new();
                }
            };
            let client_order_id = self.mint_client_order_id(chase);
            let order = match place_opts(chase, price, client_order_id.clone()) {
                Ok(order) => order,
                Err(error) => {
                    chase.mark_complete("Invalid placement");
                    self.error(&format!(
                        "Chaser for {} stopped: invalid placement ({error})",
                        chase.options.symbol
                    ));
                    return Vec::new();
                }
            };
            let placed = self
                .command(|command| Command::PlaceOrders {
                    command,
                    orders: vec![order],
                })
                .await;
            // An accepted placement whose ids cannot be read may be live:
            // treat it as an unknown outcome, never as a refusal to retry.
            let (mut ids, lost) = match &placed {
                Ok(result) => match serde_json::from_str::<Vec<String>>(result) {
                    Ok(ids) => (ids, false),
                    Err(_) => (Vec::new(), true),
                },
                // A split order refused part way: the parts that landed rest.
                Err(error) => match &error.partial {
                    Some(partial) if !partial.order_ids.is_empty() => {
                        (partial.order_ids.clone(), false)
                    }
                    _ => (Vec::new(), has_unknown_outcome(error)),
                },
            };
            chase.set_placement_unknown(lost || placed.as_ref().is_err_and(has_unknown_outcome));
            if let Err(error) = &placed {
                last_refusal = Some(error.message.clone());
                if let Some((reason, refused)) = settled_refusal(error)
                    && ids.is_empty()
                    && !lost
                {
                    self.error(&format!(
                        "Chaser for {} stopped: {refused}: {}",
                        chase.options.symbol, error.message
                    ));
                    chase.mark_complete(reason);
                    self.check_and_mark_if_complete(chase);
                    return Vec::new();
                }
            }
            let mut unconfirmed = lost;
            if let Some(client_order_id) = client_order_id.as_deref()
                && ids.is_empty()
                && (lost || placed.is_ok())
            {
                // A lost answer is looked up at 1/2/4/8 s; an empty answer once,
                // since most venues return it for an ordinary refusal.
                let delays: &[u64] = if lost {
                    &LOST_PLACEMENT_CHECKS_MS
                } else {
                    &[0]
                };
                match self
                    .find_lost_placement(chase, client_order_id, delays)
                    .await
                {
                    Some(order)
                        if !matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) =>
                    {
                        ids = vec![order.id];
                        unconfirmed = false;
                        chase.set_placement_unknown(false);
                    }
                    Some(order) => {
                        chase.set_placement_unknown(false);
                        // Filled, or a post-only reject after a partial fill:
                        // count it, then retry what is left.
                        chase.set_filled_amount(&order.id, order.placement_filled());
                        unconfirmed = false;
                        if chase.is_stopped() {
                            return Vec::new();
                        }
                    }
                    None => {}
                }
            }
            if unconfirmed {
                chase.note_order_may_be_open();
                // Not found, or no client id to look it up by: placing again
                // could double it, and it may still land.
                let message = format!(
                    "Chaser for {} stopped: could not confirm whether its order was placed — check open orders",
                    chase.options.symbol
                );
                self.error(&message);
                if !chase.is_stopped() {
                    chase.mark_complete(UNCONFIRMED_ORDER_STOP_REASON);
                }
                return Vec::new();
            }
            if chase.is_stopped() || !ids.is_empty() {
                return ids;
            }
            chase.bump_tick_distance();
            let delay = if self.store().venue == "hyperliquid" {
                500
            } else {
                100
            };
            self.cx.sleep_ms(delay).await;
            tries += 1;
        }
    }

    fn mint_client_order_id(&self, chase: &Chase) -> Option<String> {
        let store = self.store();
        let market = store.markets.get(&chase.options.symbol)?;
        // An order the venue would split cannot carry one caller id.
        if chase.remaining() > market.limits.amount.max && market.limits.amount.max > 0.0 {
            return None;
        }
        self.client_ids.mint(&store.venue)
    }

    /// Looks the order up after each delay; a failed lookup is a miss. A STOP
    /// ends the wait with one last look, so a landed order is still found.
    async fn find_lost_placement(
        &self,
        chase: &Chase,
        client_order_id: &str,
        delays_ms: &[u64],
    ) -> Option<LookedUpOrder> {
        for &delay in delays_ms {
            if chase.is_stopped() {
                return self.look_up(chase, client_order_id).await;
            }
            if delay > 0 {
                self.cx.sleep_ms(delay).await;
            }
            if let Some(order) = self.look_up(chase, client_order_id).await {
                return Some(order);
            }
        }
        None
    }

    async fn look_up(&self, chase: &Chase, client_order_id: &str) -> Option<LookedUpOrder> {
        let query = Query::OrderByClientId {
            symbol: chase.options.symbol.clone(),
            client_order_id: client_order_id.into(),
        };
        match self
            .command(|command| Command::Query { command, query })
            .await
        {
            Ok(result) => serde_json::from_str::<Lookup>(&result)
                .ok()
                .and_then(|lookup| lookup.order.filter(|_| lookup.found)),
            Err(error) => {
                self.log(
                    chase,
                    &format!("client order id lookup failed: {}", error.message),
                );
                None
            }
        }
    }

    fn spawn_cleanup(self: &Rc<Self>, chase: Rc<Chase>) {
        let book = self.clone();
        self.cx.spawn(async move { book.cleanup(chase).await });
    }

    /// A stop that keeps cancelling through a retryable failure: the row stays,
    /// stopped, until each order is settled, then `chaser:complete` once.
    async fn cleanup(&self, chase: Rc<Chase>) {
        chase.watch_scope.dispose();
        chase.wake.notify_one();
        // The supervisor cleans up itself once the placement answers, landed or not.
        if chase.is_placing() {
            return;
        }
        for id in chase.current() {
            self.cancel_until_settled(&chase, &id).await;
        }
        // A cancel another task still retries finishes the chase from there.
        if chase.current().is_empty() {
            self.finish(chase).await;
        }
    }

    /// The account is going away or being nuked, on a bound of its own: one try each.
    async fn settle_once(&self, chase: Rc<Chase>) {
        chase.watch_scope.dispose();
        chase.wake.notify_one();
        chase.placement_settled().await;
        for id in chase.take_current() {
            if !self.known_closed(&id) {
                let symbol = self.order_symbol(&chase, &id);
                if let Err(error) = self.cancel(&symbol, std::slice::from_ref(&id)).await
                    && self.cancel_failed(&symbol, std::slice::from_ref(&id), &error)
                {
                    chase.note_order_may_be_open();
                }
            }
        }
        self.finish(chase).await;
    }

    async fn finish(&self, chase: Rc<Chase>) {
        // The row stays until the completion is out: the page reads a row gone
        // alone as a chase that died, and the final fills read can take seconds.
        if chase.claim_complete_emission() {
            self.emit_complete(&chase).await;
            self.detach(&chase);
        }
    }

    fn order_symbol(&self, chase: &Chase, id: &str) -> String {
        self.store().order(id).map_or_else(
            || chase.options.symbol.clone(),
            |order| order.symbol.clone(),
        )
    }

    /// One order of a stopped chase, until the venue answers or the bound passes.
    /// A nuke or disposal that took the id over ends this.
    async fn cancel_until_settled(&self, chase: &Chase, id: &str) {
        if !chase.claim_cancel(id) {
            return;
        }
        let symbol = self.order_symbol(chase, id);
        let reason = chase.stop_reason();
        let mut changes = self.cx.port.subscribe();
        let started = Instant::now();
        let mut delay = CANCEL_RETRY_MIN_MS;
        let mut retrying = false;
        // How the order ended, for the stop reason once the retry is over.
        let mut ended = None;
        while chase.current().iter().any(|current| current == id) {
            if self.known_closed(id) {
                if retrying {
                    let end = self.store().order(id).map(|order| {
                        if order.status == OrderStatus::Closed {
                            Settled::Filled(placement_filled(order))
                        } else {
                            Settled::Cancelled(placement_filled(order))
                        }
                    });
                    let end = end.unwrap_or(Settled::Cancelled(0.0));
                    ended = Some(self.tell_settled(chase, &symbol, id, end));
                }
                break;
            }
            let error = match self
                .bounded_cancel(&symbol, std::slice::from_ref(&id.to_owned()))
                .await
            {
                Ok(_) => {
                    if retrying {
                        ended = self.tell_retried_end(chase, &symbol, id, None).await;
                    }
                    break;
                }
                Err(error) => error,
            };
            if stop_twap_unknown_cancel(chase, &error) {
                break;
            }
            let elapsed = started.elapsed().as_millis() as u64;
            if !cancel_retryable(&error) || elapsed >= CANCEL_RETRY_BOUND_MS {
                // A refusal is often the order already filling: completion, with its fills, says which.
                if cancel_retryable(&error) {
                    chase.note_order_may_be_open();
                    self.cancel_failed(&symbol, std::slice::from_ref(&id.to_owned()), &error);
                    if retrying {
                        chase.note_outcome_told();
                    }
                } else if retrying {
                    ended = self
                        .tell_retried_end(chase, &symbol, id, Some(&error))
                        .await;
                } else {
                    chase.note_cancel_refused(id, &symbol, error);
                }
                break;
            }
            if !retrying && !self.known_closed(id) {
                retrying = true;
                chase.mark_complete(&format!(
                    "{reason}; retrying the cancel of order {id}. {CANCEL_WAY_OUT}"
                ));
                self.warn(&format!(
                    "{RETRYING}{symbol} order {id} is still open: {}. Retrying the cancel; it can still fill until then. {CANCEL_WAY_OUT}",
                    if signer_offline(&error) {
                        "the browser that signs for this account is offline"
                    } else {
                        error.message.as_str()
                    }
                ));
            }
            // Traffic returning is the reconnect: try again at once after the minimum.
            self.cx.sleep_ms(CANCEL_RETRY_MIN_MS).await;
            let wait = delay.saturating_sub(CANCEL_RETRY_MIN_MS);
            if wait > 0 {
                let _ = safe_cex_algos::executor::timeout(wait, changes.recv()).await;
            }
            delay = (delay * 2).min(CANCEL_RETRY_MAX_MS);
        }
        chase.drop_current(id);
        if retrying {
            chase.mark_complete(&match ended {
                Some(end) => format!("{reason}; {end}"),
                None => reason,
            });
        }
    }

    /// The one final message for an order whose cancel had to be retried, and
    /// the same outcome for the stop reason.
    fn tell_settled(&self, chase: &Chase, symbol: &str, id: &str, end: Settled) -> String {
        let (message, outcome) = match end {
            Settled::Filled(filled) => (
                format!(
                    "Chaser stopped: {symbol} order {id} filled {filled} before its cancel got through"
                ),
                format!("order {id} filled {filled}"),
            ),
            Settled::Cancelled(filled) if filled > 0.0 => (
                format!("Chaser stopped: {symbol} order {id} is cancelled after filling {filled}"),
                format!("order {id} is cancelled after filling {filled}"),
            ),
            Settled::Cancelled(_) => (
                format!("Chaser stopped: {symbol} order {id} is cancelled; nothing filled"),
                format!("order {id} is cancelled, nothing filled"),
            ),
            Settled::FillsUnread => (
                format!(
                    "Chaser stopped: {symbol} order {id} is no longer open, but its fills could not be read: check Positions"
                ),
                format!("order {id} is no longer open, its fills unread"),
            ),
        };
        self.warn(&message);
        chase.note_outcome_told();
        outcome
    }

    /// How a stop cancel that had to wait out an outage ended. The store can
    /// lag the reconnect, and a venue may answer a cancel of a filled order as
    /// done, so the order's own fills say whether it filled first.
    async fn tell_retried_end(
        &self,
        chase: &Chase,
        symbol: &str,
        id: &str,
        refusal: Option<&VenueError>,
    ) -> Option<String> {
        let filled = self
            .filled_quantities(chase, std::slice::from_ref(&id.to_owned()))
            .await
            .ok()
            .and_then(|rows| rows.first().map(|(_, amount)| *amount));
        // A refusal while the store still shows it open is said as such.
        if let Some(error) = refusal
            && !filled.is_some_and(|filled| filled > 0.0)
            && self.cancel_failed(symbol, std::slice::from_ref(&id.to_owned()), error)
        {
            chase.note_order_may_be_open();
            chase.note_outcome_told();
            return None;
        }
        let end = match filled {
            Some(filled) if filled > 0.0 => {
                chase.set_filled_amount(id, filled);
                Settled::Filled(filled)
            }
            Some(_) => Settled::Cancelled(0.0),
            None => Settled::FillsUnread,
        };
        Some(self.tell_settled(chase, symbol, id, end))
    }

    async fn emit_complete(&self, chase: &Chase) {
        let ids = chase.ever();
        let rows: Vec<(String, f64)> = {
            let store = self.store();
            ids.iter()
                .filter_map(|id| {
                    store
                        .order(id)
                        .map(|order| (order.id.clone(), chase.row_filled(order)))
                })
                .collect()
        };
        for (id, filled) in rows {
            chase.set_filled_amount(&id, filled);
        }
        // A fill during the final cancel, or a row gone from the store, shows only at the venue.
        let mut reconciled = false;
        if !ids.is_empty() && chase.remaining() > 0.0 {
            let read = if self.fill_tracked() {
                self.filled_quantities(chase, &ids).await
            } else {
                self.chase_fills(chase, &ids).await
            };
            match read {
                Ok(filled) => {
                    for (id, amount) in filled {
                        chase.set_filled_amount(&id, amount);
                    }
                    reconciled = true;
                }
                Err(error) => self.log(chase, &format!("final fills read failed: {error}")),
            }
        }
        let filled = if reconciled && self.fill_tracked() {
            chase.filled_size()
        } else {
            chase.filled_size().max(self.position_fill(chase))
        };
        for (id, symbol, error) in chase.take_cancel_refused() {
            let order_filled = chase.filled_by(&id);
            if order_filled > 0.0 {
                self.warn(&format!(
                    "Chaser stopped: {symbol} order {id} filled {order_filled} before its cancel got through"
                ));
            } else if self.cancel_failed(&symbol, std::slice::from_ref(&id), &error) {
                chase.note_order_may_be_open();
            }
        }
        let event = ChaserComplete {
            id: chase.id.clone(),
            client_id: chase.options.client_id.clone(),
            symbol: chase.options.symbol.clone(),
            side: chase.options.side,
            reduce_only: chase.options.reduce_only(),
            size: chase.options.size,
            filled,
            stop_reason: chase.stop_reason(),
            outcome_told: chase.outcome_told(),
        };
        self.cx.port.emit(
            "chaser:complete",
            serde_json::to_string(&event).unwrap_or_default(),
        );
        let mut ended = self.ended.borrow_mut();
        if ended.len() == ENDED_KEPT {
            ended.pop_front();
        }
        ended.push_back((
            chase.id.clone(),
            Progress {
                remaining_size: chase.remaining(),
                stopped: true,
                ended: true,
                order_may_be_open: chase.order_may_be_open(),
            },
        ));
    }

    /// What the position grew by since the chase started, as the web's fallback.
    /// Lot venues are left out: their positions and placements differ in unit.
    fn position_fill(&self, chase: &Chase) -> f64 {
        let store = self.store();
        let Some(target) = chase.target_position_contracts else {
            return 0.0;
        };
        if amounts_in_lots(&store.venue) {
            return 0.0;
        }
        let side = chase.options.entry_side();
        let contracts = store
            .positions
            .iter()
            .find(|position| position.symbol == chase.options.symbol && position.side == side)
            .map_or(0.0, |position| position.contracts.abs());
        (contracts - (target - chase.options.size)).clamp(0.0, chase.options.size)
    }
}

/// One row of a fills read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    order_id: String,
    qty: f64,
}

/// The `ClientOrderLookup` answer: `{found: true, order}` or `{found: false, definitive}`.
#[derive(Deserialize)]
struct Lookup {
    found: bool,
    #[serde(default)]
    order: Option<LookedUpOrder>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookedUpOrder {
    id: String,
    status: OrderStatus,
    filled: f64,
    #[serde(default)]
    extra: Option<LookedUpExtra>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookedUpExtra {
    #[serde(default)]
    filled_in_placement_units: Option<f64>,
}

impl LookedUpOrder {
    fn placement_filled(&self) -> f64 {
        match self
            .extra
            .as_ref()
            .and_then(|extra| extra.filled_in_placement_units)
        {
            Some(fill) if fill.is_finite() && fill >= 0.0 => fill,
            _ => self.filled,
        }
    }
}

fn place_opts(
    chase: &Chase,
    price: f64,
    client_order_id: Option<String>,
) -> Result<PlaceOrderOpts, String> {
    let amount = chase.remaining();
    if !(price.is_finite() && price > 0.0 && amount.is_finite() && amount > 0.0) {
        return Err(format!("price {price}, amount {amount}"));
    }
    let value = serde_json::json!({
        "symbol": chase.options.symbol,
        "type": "limit",
        "side": chase.options.side,
        "amount": amount,
        "price": price,
        "reduceOnly": chase.options.reduce_only(),
        "timeInForce": "PostOnly",
    });
    let mut order: PlaceOrderOpts =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    order.client_order_id = client_order_id;
    Ok(order)
}

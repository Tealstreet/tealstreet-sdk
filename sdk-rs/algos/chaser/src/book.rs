use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use safe_cex_algos::{AlgoCx, AlgoStore, Change};
use safe_cex_engine::engine::{Command, Query};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::model::{
    CancelOrderRef, Order, OrderSide, OrderStatus, PlaceOrderOpts, RejectReason,
};
use serde::Deserialize;
use tokio::time::Instant;

use crate::chase::{
    ActiveChaser, Chase, ChaserComplete, ChaserOptions, Dirty, adjust, placement_filled,
};
use crate::client_id::ClientIdMinter;

pub const KEPT_ORDER_STOP_REASON: &str = "User cancelled, kept order in book";
pub const UNCONFIRMED_ORDER_STOP_REASON: &str = "Could not confirm order placement";

const PRICE_CHECK_MIN_INTERVAL_MS: u64 = 50;
/// After a send whose outcome is unknown.
const LOST_PLACEMENT_CHECKS_MS: [u64; 4] = [1_000, 2_000, 4_000, 8_000];
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

/// Progress of one chase, for TWAP slices.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Progress {
    pub remaining_size: f64,
    pub stopped: bool,
}

/// The chases of one account: the TS `ChaserOrderManager`.
pub struct ChaserBook {
    cx: AlgoCx,
    chases: RefCell<Vec<Rc<Chase>>>,
    client_ids: ClientIdMinter,
    next_id: Cell<u64>,
}

impl ChaserBook {
    pub fn new(cx: AlgoCx) -> Rc<Self> {
        Rc::new(Self {
            cx,
            chases: RefCell::new(Vec::new()),
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
        ));
        self.chases.borrow_mut().push(chase.clone());
        let book = self.clone();
        chase
            .scope
            .clone()
            .spawn(async move { book.run(chase).await });
        Ok(())
    }

    pub fn progress(&self, id: &str) -> Option<Progress> {
        self.find(id).map(|chase| Progress {
            remaining_size: chase.remaining(),
            stopped: chase.is_stopped(),
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
        self.detach(&chase);
        if chase.claim_complete_emission() {
            let book = self.clone();
            self.cx
                .spawn(async move { book.emit_complete(&chase).await });
        }
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
            self.cleanup(chase).await;
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
            self.cleanup(chase).await;
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

    fn log(&self, chase: &Chase, message: &str) {
        self.emit_json("log", &format!("[CHASER{}] {message}", chase.id));
    }

    fn fill_tracked(&self) -> bool {
        FILL_TRACKED_VENUES.contains(&self.store().venue.as_str())
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
                && position.notional != 0.0
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
        self.command(|command| Command::CancelOrdersWithSymbols { command, orders })
            .await
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
                    tokio::time::sleep(Duration::from_millis(
                        PRICE_CHECK_MIN_INTERVAL_MS - elapsed,
                    ))
                    .await;
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
        let book = self.clone();
        let chase = chase.clone();
        let fill_tracked = self.fill_tracked();
        chase.watch_scope.clone().spawn(async move {
            let mut seen = false;
            let mut missing = 0;
            while let Some(change) = changes.recv().await {
                if chase.is_stopped() {
                    chase.wake.notify_one();
                    return;
                }
                if change.resync {
                    chase.needs_reconcile.set(true);
                }
                if fill_tracked {
                    let current = chase.current();
                    for fill in change.fills.iter().filter(|fill| {
                        current.contains(&fill.order_id) && fill.symbol == chase.options.symbol
                    }) {
                        chase.add_fill_amount(&fill.order_id, fill.amount);
                    }
                }
                if change.orders {
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
        if chase.take_fresh_order()
            && orders
                .iter()
                .all(|order| order.status == OrderStatus::Canceled)
        {
            chase.bump_tick_distance();
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
            if let Err(error) = self.cancel(&chase.options.symbol, &to_cancel).await {
                let message = error.message.as_str();
                let filled_or_gone = ["already canceled", "already filled", "never placed"]
                    .iter()
                    .any(|phrase| message.contains(phrase));
                if !(fill_tracked && filled_or_gone) {
                    self.error("Chaser failed to cancel orders, stopping");
                    chase.mark_complete("Failed to cancel orders");
                    return Ok(());
                }
            }
        }
        let resync = chase.needs_reconcile.replace(false);
        if resync && !fill_tracked {
            // Order rows carry cumulative fills and the rejoin snapshot
            // refreshes them, so REST only tops up; a venue without fill
            // lookup by order id falls back to the rows.
            match self.filled_quantities(&chase.ever()).await {
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
        if fill_tracked {
            // A cancel can have won behind an ambiguous answer: an unknown fill
            // would overfill, so a failed reconciliation stops the chase. After
            // a rejoin every order this chase placed is reconciled.
            let ids = if resync {
                chase.ever()
            } else {
                chase.current()
            };
            match self.filled_quantities(&ids).await {
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
        self.cx.sleep_ms(SETTLE_AFTER_PLACEMENT_MS).await;
        Ok(())
    }

    async fn filled_quantities(&self, ids: &[String]) -> Result<Vec<(String, f64)>, String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Execution {
            order_id: String,
            qty: f64,
        }
        let mut filled = Vec::new();
        for id in ids {
            let query = Query::Fills {
                symbol: None,
                order_id: Some(id.clone()),
                from_ms: Some(0.0),
                to_ms: None,
                limit: Some(1000),
            };
            let result = self
                .command(|command| Command::Query { command, query })
                .await
                .map_err(|error| error.message)?;
            let rows: Vec<Execution> =
                serde_json::from_str(&result).map_err(|error| error.to_string())?;
            let total = rows
                .iter()
                .filter(|row| &row.order_id == id)
                .map(|row| row.qty)
                .sum();
            filled.push((id.clone(), total));
        }
        Ok(filled)
    }

    async fn try_place_orders(&self, chase: &Rc<Chase>) -> Vec<String> {
        let max_tries = if self.store().venue == "binance" {
            20
        } else {
            10
        };
        let mut tries = 1;
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
                    "Chaser failed to place orders after {max_tries} tries"
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
                Err(error) => (Vec::new(), error.outcome == Some(Outcome::Unknown)),
            };
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
                    }
                    Some(order) => {
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
            if chase.is_stopped() {
                if !ids.is_empty() {
                    // STOP raced the placement: cancel what landed ourselves.
                    let _ = self.cancel(&chase.options.symbol, &ids).await;
                }
                return Vec::new();
            }
            if !ids.is_empty() {
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

    /// Detach, cancel every current id, then report `chaser:complete` once.
    async fn cleanup(&self, chase: Rc<Chase>) {
        let owns_completion = chase.claim_complete_emission();
        chase.watch_scope.dispose();
        chase.wake.notify_one();
        self.detach(&chase);
        let ids = chase.take_current();
        for id in &ids {
            let symbol = self.store().order(id).map_or_else(
                || chase.options.symbol.clone(),
                |order| order.symbol.clone(),
            );
            let _ = self.cancel(&symbol, std::slice::from_ref(id)).await;
        }
        if owns_completion {
            self.emit_complete(&chase).await;
        }
    }

    async fn emit_complete(&self, chase: &Chase) {
        let ids = chase.ever();
        let rows: Vec<(String, f64)> = {
            let store = self.store();
            ids.iter()
                .filter_map(|id| {
                    store
                        .order(id)
                        .map(|order| (order.id.clone(), placement_filled(order)))
                })
                .collect()
        };
        for (id, filled) in rows {
            chase.set_filled_amount(&id, filled);
        }
        let mut reconciled = false;
        if chase.options.client_id.is_some()
            && !ids.is_empty()
            && self.fill_tracked()
            && let Ok(filled) = self.filled_quantities(&ids).await
        {
            for (id, amount) in filled {
                chase.set_filled_amount(&id, amount);
            }
            reconciled = true;
        }
        let filled = if reconciled {
            chase.filled_size()
        } else {
            chase
                .filled_size()
                .max(self.bybit_demo_position_fill(chase))
        };
        let event = ChaserComplete {
            id: chase.id.clone(),
            client_id: chase.options.client_id.clone(),
            symbol: chase.options.symbol.clone(),
            side: chase.options.side,
            reduce_only: chase.options.reduce_only(),
            size: chase.options.size,
            filled,
            stop_reason: chase.stop_reason(),
        };
        self.cx.port.emit(
            "chaser:complete",
            serde_json::to_string(&event).unwrap_or_default(),
        );
        chase.scope.dispose();
    }

    fn bybit_demo_position_fill(&self, chase: &Chase) -> f64 {
        let store = self.store();
        let Some(target) = chase.target_position_contracts else {
            return 0.0;
        };
        if store.venue != "bybitdemo" {
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

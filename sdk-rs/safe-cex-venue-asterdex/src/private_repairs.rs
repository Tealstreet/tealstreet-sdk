//! Position, balance and order repair methods extracted from the private socket.
//! The full socket/constructor remain separate work. The supplied BaseExchange
//! owns timers and detached operations; its disposal cancels work rather than
//! leaving JavaScript promises alive. Parent RPCs dispatch on invocation.
use safe_cex_exchange::{
    OwnedOperation,
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::{Order, Position};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub trait RepairParent {
    fn position_state_version(&self) -> f64;
    fn balance_state_version(&self) -> f64;
    fn fetch_positions(&self) -> ExchangeFuture<'static, Vec<Position>>;
    fn reconcile_balance_after_private_update(&self, version: f64)
    -> ExchangeFuture<'static, bool>;
    fn fetch_orders(&self) -> ExchangeFuture<'static, Vec<Order>>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairState {
    pub position_timer: bool,
    pub position_in_flight: bool,
    pub position_queued: bool,
    pub balance_timer: bool,
    pub balance_in_flight: bool,
    pub balance_queued: bool,
    pub order_timer: bool,
}
pub struct PrivateRepairs {
    owner: Rc<BaseExchange>,
    parent: Rc<dyn RepairParent>,
    position_timer: RefCell<Option<OwnedOperation>>,
    position_in_flight: Cell<bool>,
    position_queued: Cell<bool>,
    balance_timer: RefCell<Option<OwnedOperation>>,
    balance_in_flight: Cell<bool>,
    balance_queued: Cell<bool>,
    order_timer: RefCell<Option<OwnedOperation>>,
}
enum Flight {
    Position,
    Balance,
}
struct ResetFlight {
    repairs: Rc<PrivateRepairs>,
    flight: Flight,
}
impl Drop for ResetFlight {
    fn drop(&mut self) {
        match self.flight {
            Flight::Position => self.repairs.position_in_flight.set(false),
            Flight::Balance => self.repairs.balance_in_flight.set(false),
        }
    }
}
fn error_message(error: &ExchangeError) -> String {
    if error.message.is_empty() {
        error.js_error_string()
    } else {
        error.message.clone()
    }
}
impl PrivateRepairs {
    pub fn new(owner: Rc<BaseExchange>, parent: Rc<dyn RepairParent>) -> Rc<Self> {
        Rc::new(Self {
            owner,
            parent,
            position_timer: RefCell::new(None),
            position_in_flight: Cell::new(false),
            position_queued: Cell::new(false),
            balance_timer: RefCell::new(None),
            balance_in_flight: Cell::new(false),
            balance_queued: Cell::new(false),
            order_timer: RefCell::new(None),
        })
    }
    pub fn state(&self) -> RepairState {
        RepairState {
            position_timer: self.position_timer.borrow().is_some(),
            position_in_flight: self.position_in_flight.get(),
            position_queued: self.position_queued.get(),
            balance_timer: self.balance_timer.borrow().is_some(),
            balance_in_flight: self.balance_in_flight.get(),
            balance_queued: self.balance_queued.get(),
            order_timer: self.order_timer.borrow().is_some(),
        }
    }
    fn launch(&self, operation: &'static str, work: ExchangeFuture<'static, ()>) -> OwnedOperation {
        let runtime = self.owner.runtime.clone();
        self.owner.start_owned_operation(async move {
            if let Err(error) = work.await {
                runtime
                    .report_exception(&error, &serde_json::json!({"operation": operation}).into());
            }
        })
    }
    // ts: AsterDEXPrivateWebsocket.schedulePositionRepair
    pub fn schedule_position_repair(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() {
            return Ok(());
        }
        self.position_queued.set(true);
        if self.position_timer.borrow().is_some() || self.position_in_flight.get() {
            return Ok(());
        }
        self.owner
            .log("Scheduling AsterDEX position repair (1s)...", "debug")?;
        let repairs = self.clone();
        let timer = self.owner.start_timeout(1_000, move || {
            repairs.position_timer.borrow_mut().take();
            repairs.run_position_repair();
            Ok(())
        })?;
        *self.position_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.runPositionRepair
    pub fn run_position_repair(self: &Rc<Self>) -> Option<OwnedOperation> {
        if self.owner.is_disposed() || self.position_in_flight.get() {
            return None;
        }
        self.position_queued.set(false);
        self.position_in_flight.set(true);
        let guard = ResetFlight {
            repairs: self.clone(),
            flight: Flight::Position,
        };
        let version = self.parent.position_state_version();
        let fetching = self.parent.fetch_positions();
        let repairs = self.clone();
        Some(self.launch("AsterDEX position repair", Box::pin(async move {
            let result = async {
                let positions = fetching.await?;
                if repairs.owner.is_disposed() { return Ok(()); }
                if version == repairs.parent.position_state_version() {
                    repairs.owner.log("AsterDEX position repair completed successfully", "debug")?;
                    repairs.owner.store.add_or_update_positions_full(positions)?;
                    let mut loaded = repairs.owner.store.loaded()?.clone();
                    loaded.positions = true;
                    repairs.owner.store.update(safe_cex_store::StoreUpdate { loaded: Some(loaded), ..Default::default() }, false)?;
                } else {
                    repairs.owner.log("Skipping stale AsterDEX position repair because a newer websocket position update arrived", "debug")?;
                }
                Ok::<(), ExchangeError>(())
            }.await;
            let handled = match result {
                Ok(()) => Ok(()),
                Err(error) => repairs.owner.log(&format!("Failed to repair AsterDEX positions after account update: {}", error_message(&error)), "warning"),
            };
            drop(guard);
            if repairs.position_queued.get() && !repairs.owner.is_disposed() { repairs.schedule_position_repair()?; }
            handled
        })))
    }
    // ts: AsterDEXPrivateWebsocket.clearPositionRepairTimer
    pub fn clear_position_repair_timer(&self) {
        let timer = self.position_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
        self.position_queued.set(false);
    }
    // ts: AsterDEXPrivateWebsocket.scheduleBalanceReconcile
    pub fn schedule_balance_reconcile(self: &Rc<Self>, version: f64) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() {
            return Ok(());
        }
        self.balance_queued.set(true);
        if self.balance_timer.borrow().is_some() || self.balance_in_flight.get() {
            return Ok(());
        }
        let repairs = self.clone();
        let timer = self.owner.start_timeout(1_000, move || {
            repairs.balance_timer.borrow_mut().take();
            repairs.run_balance_reconcile(version);
            Ok(())
        })?;
        *self.balance_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.runBalanceReconcile
    pub fn run_balance_reconcile(self: &Rc<Self>, version: f64) -> Option<OwnedOperation> {
        if self.owner.is_disposed() || self.balance_in_flight.get() {
            return None;
        }
        self.balance_queued.set(false);
        self.balance_in_flight.set(true);
        let guard = ResetFlight {
            repairs: self.clone(),
            flight: Flight::Balance,
        };
        let reconciling = self.parent.reconcile_balance_after_private_update(version);
        let repairs = self.clone();
        Some(self.launch(
            "AsterDEX balance reconcile",
            Box::pin(async move {
                let result = async {
                    let applied = reconciling.await?;
                    if !applied && !repairs.owner.is_disposed() {
                        repairs.balance_queued.set(true);
                    }
                    Ok(())
                }
                .await;
                drop(guard);
                if repairs.balance_queued.get() && !repairs.owner.is_disposed() {
                    repairs.schedule_balance_reconcile(repairs.parent.balance_state_version())?;
                }
                result
            }),
        ))
    }
    // ts: AsterDEXPrivateWebsocket.clearBalanceReconcileTimer
    pub fn clear_balance_reconcile_timer(&self) {
        let timer = self.balance_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
        self.balance_queued.set(false);
    }
    // ts: AsterDEXPrivateWebsocket.scheduleOrderReconcile
    pub fn schedule_order_reconcile(self: &Rc<Self>) -> Result<(), ExchangeError> {
        if self.owner.is_disposed() {
            return Ok(());
        }
        self.clear_order_reconcile_timer();
        self.owner.log(
            "Scheduling debounced AsterDEX order reconcile (10s)...",
            "debug",
        )?;
        let repairs = self.clone();
        let timer = self.owner.start_timeout(10_000, move || {
            repairs.order_timer.borrow_mut().take();
            repairs.run_order_reconcile();
            Ok(())
        })?;
        *self.order_timer.borrow_mut() = Some(timer);
        Ok(())
    }
    // ts: AsterDEXPrivateWebsocket.runOrderReconcile
    pub fn run_order_reconcile(self: &Rc<Self>) -> Option<OwnedOperation> {
        if self.owner.is_disposed() {
            return None;
        }
        let fetching = self.parent.fetch_orders();
        let repairs = self.clone();
        Some(self.launch(
            "AsterDEX order reconcile",
            Box::pin(async move {
                let result = async {
                    let orders = fetching.await?;
                    if repairs.owner.is_disposed() {
                        return Ok(());
                    }
                    repairs
                        .owner
                        .log("AsterDEX order reconcile completed successfully", "debug")?;
                    let mut loaded = repairs.owner.store.loaded()?.clone();
                    loaded.orders = true;
                    repairs.owner.store.update(
                        safe_cex_store::StoreUpdate {
                            orders: Some(orders),
                            loaded: Some(loaded),
                            ..Default::default()
                        },
                        false,
                    )?;
                    Ok::<(), ExchangeError>(())
                }
                .await;
                if let Err(error) = result {
                    repairs.owner.log(
                        &format!(
                            "Failed to reconcile AsterDEX orders after order event: {}",
                            error_message(&error)
                        ),
                        "warning",
                    )?;
                }
                Ok(())
            }),
        ))
    }
    // ts: AsterDEXPrivateWebsocket.clearOrderReconcileTimer
    pub fn clear_order_reconcile_timer(&self) {
        let timer = self.order_timer.borrow_mut().take();
        if let Some(timer) = timer {
            timer.cancel();
        }
    }
}

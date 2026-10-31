//! TWAP for the native exchange server: a port of the TS worker's
//! `TwapHandler`, `TwapInstance`, `TwapManager` and `TwapPostprocessor`.
//!
//! The crate owns no clock, timer, socket or engine handle. A host drives one
//! [`TwapBook`] per account: it passes `twap:*` ops to [`TwapBook::handle`],
//! wakes the book at [`TwapBook::wake_at`], reports slice placements back with
//! [`TwapBook::placement_settled`], and executes the returned [`TwapEffect`]s
//! against the engine and the hosted protocol.

pub mod factory;
mod instance;
mod math;
mod types;
mod validate;

pub use validate::{MAX_ORDERS_COUNT, MAX_TWAPS_PER_ACCOUNT, RETIRED_IDS_KEPT, SCHEDULE_GRACE_MS};

pub use instance::TwapInstance;
pub use types::{
    InFlightSlice, OrderSide, PersistedTwap, SliceAction, TwapOptions, TwapOrigin, TwapState,
    TwapStatus,
};

use std::collections::{HashSet, VecDeque};

use serde_json::{Map, Value};

/// Clock, entropy and ids the host provides; tests substitute fixed values.
pub trait TwapEnv {
    /// Epoch milliseconds (`Date.now()`).
    fn now_ms(&self) -> f64;
    /// A uniform draw in `[0, 1)` (`Math.random()`).
    fn random_unit(&mut self) -> f64;
    /// A fresh TWAP or chaser id (`nanoid()`).
    fn new_id(&mut self) -> String;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TickerQuote {
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub last: Option<f64>,
}

/// A chaser's progress as the chaser runtime reports it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChaserProgress {
    pub remaining_size: f64,
    pub stopped: bool,
}

/// Read-only views of the engine store and the chaser runtime.
pub trait TwapMarketView {
    fn ticker(&self, symbol: &str) -> Option<TickerQuote>;
    /// `None` once the chaser runtime has dropped the chaser.
    fn chaser(&self, chaser_id: &str) -> Option<ChaserProgress>;
    /// Start a slice's chaser. Asked, not an effect: the slice's count and the
    /// state that reports it depend on a refusal.
    fn start_chaser(&self, request: &ChaserRequest) -> Result<(), String>;
    /// A caller id the venue honors on `request` (`mintClientOrderId`), or `None`.
    fn client_order_id(&self, _request: &TwapOrderRequest) -> Option<String> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderKind {
    Limit,
    Market,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TwapOrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub kind: OrderKind,
    pub price: Option<f64>,
    pub amount: f64,
    pub reduce_only: bool,
    /// The caller id a direct slice carries, so a restore can look it up.
    pub client_order_id: Option<String>,
}

/// Why a slice placement failed, and whether that proves no order exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SliceFailure {
    pub message: String,
    /// False when an order may be live: the slice still counts as executed.
    pub nothing_placed: bool,
}

/// The chaser a `useChaserOrders` slice starts, with the id TWAP tracks it by.
#[derive(Clone, Debug, PartialEq)]
pub struct ChaserRequest {
    pub id: String,
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    pub max_attempts: u32,
    pub reduce_only: bool,
    pub delay: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TwapEffect {
    /// Place through the engine. With a `settle_token`, report the outcome to
    /// [`TwapBook::placement_settled`]; without one the outcome is ignored.
    PlaceOrder {
        twap_id: String,
        settle_token: Option<u64>,
        request: TwapOrderRequest,
    },
    /// Stop the chaser and cancel its open order.
    RemoveChaser { twap_id: String, chaser_id: String },
    /// A user-facing error log (`LogSeverity.Error`).
    Log { message: String },
    /// Send `{ op: 'twap:stateUpdate', data }` to the account's clients.
    StateUpdate { data: Value },
    /// Host-side diagnostics the TS worker wrote to `console.error`.
    Diagnostic { message: String },
}

/// One account's TWAPs, in insertion order like the TS `Map`.
#[derive(Debug)]
pub struct TwapBook {
    /// The `accountName` every TWAP here must carry; `activeTWAPs` lists by it.
    account_name: String,
    entries: Vec<Entry>,
    /// Completed or stopped ids, oldest first, that a restore must not revive.
    retired: VecDeque<String>,
    retired_set: HashSet<String>,
}

#[derive(Debug)]
struct Entry {
    instance: TwapInstance,
    /// False once removed from the TS manager; the entry lingers only while
    /// in-flight placements or slice timeouts can still act.
    managed: bool,
}

impl TwapBook {
    pub fn new(account_name: impl Into<String>) -> Self {
        Self {
            account_name: account_name.into(),
            entries: Vec::new(),
            retired: VecDeque::new(),
            retired_set: HashSet::new(),
        }
    }

    /// Apply one `twap:*` op from the hosted protocol.
    pub fn handle(
        &mut self,
        op: &str,
        data: &Value,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
    ) -> Vec<TwapEffect> {
        let mut effects = Vec::new();
        match op {
            "twap:start" => match serde_json::from_value::<TwapOptions>(data.clone()) {
                Ok(options) => {
                    if let Err(message) = self.start_with(options, env, view, &mut effects) {
                        effects.push(TwapEffect::Log { message });
                    }
                }
                Err(error) => effects.push(TwapEffect::Diagnostic {
                    message: format!("twap:start options rejected: {error}"),
                }),
            },
            "twap:resume" => self.with_id(data, |instance| instance.resume(env)),
            "twap:cleanup" => self.with_id(data, TwapInstance::cleanup),
            "twap:pause" => self.with_id(data, TwapInstance::pause),
            "twap:stop" => self.with_id(data, TwapInstance::terminate),
            "twap:state" => self.restore(data, env, &mut effects),
            _ => {}
        }
        self.settle(&mut effects);
        effects
    }

    /// `twap:start` for a typed caller (the exec `twap` command): the new id,
    /// or the refusal that `twap:start` would log.
    pub fn start(
        &mut self,
        options: TwapOptions,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
    ) -> (Result<String, String>, Vec<TwapEffect>) {
        let mut effects = Vec::new();
        let started = self.start_with(options, env, view, &mut effects);
        self.settle(&mut effects);
        (started, effects)
    }

    /// `twap:stop` for a typed caller; false when no running TWAP has `id`.
    pub fn stop(&mut self, id: &str) -> (bool, Vec<TwapEffect>) {
        let mut effects = Vec::new();
        let found = match self.managed_mut(id) {
            Some(instance) => {
                instance.terminate();
                true
            }
            None => false,
        };
        self.settle(&mut effects);
        (found, effects)
    }

    fn start_with(
        &mut self,
        options: TwapOptions,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) -> Result<String, String> {
        validate::options(&options)
            .and_then(|()| self.owns(&options))
            .and_then(|()| self.has_capacity())
            .map_err(|reason| format!("TWAP {} rejected: {reason}", options.symbol()))?;
        let id = env.new_id();
        let instance = TwapInstance::new(id.clone(), options, None, env);
        self.add(instance);
        if let Some(instance) = self.managed_mut(&id) {
            instance.start(env, view, effects);
        }
        Ok(id)
    }

    /// The managed TWAPs that have not completed.
    pub fn running_ids(&self) -> Vec<String> {
        self.managed_entries()
            .filter(|entry| entry.instance.state.status != TwapStatus::Completed)
            .map(|entry| entry.instance.id.clone())
            .collect()
    }

    /// `twap:stop` for every TWAP, as on account disposal.
    pub fn terminate_all(&mut self) -> Vec<TwapEffect> {
        let mut effects = Vec::new();
        for entry in self.entries.iter_mut().filter(|entry| entry.managed) {
            entry.instance.terminate();
        }
        self.settle(&mut effects);
        effects
    }

    /// Fire every timer due now across the account's TWAPs.
    pub fn wake(&mut self, env: &mut dyn TwapEnv, view: &dyn TwapMarketView) -> Vec<TwapEffect> {
        let mut effects = Vec::new();
        let now = env.now_ms();
        for entry in &mut self.entries {
            if entry.instance.wake_at().is_some_and(|due_at| due_at <= now) {
                entry.instance.wake(env, view, &mut effects);
            }
        }
        self.settle(&mut effects);
        effects
    }

    pub fn placement_settled(
        &mut self,
        twap_id: &str,
        token: u64,
        outcome: Result<Vec<String>, SliceFailure>,
        env: &mut dyn TwapEnv,
    ) -> Vec<TwapEffect> {
        let mut effects = Vec::new();
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.instance.id == twap_id)
        {
            entry
                .instance
                .placement_settled(token, outcome, env, &mut effects);
        }
        self.settle(&mut effects);
        effects
    }

    /// The earliest timer deadline across the account's TWAPs.
    pub fn wake_at(&self) -> Option<f64> {
        self.entries
            .iter()
            .filter_map(|entry| entry.instance.wake_at())
            .min_by(f64::total_cmp)
    }

    pub fn get(&self, id: &str) -> Option<&TwapInstance> {
        self.entries
            .iter()
            .find(|entry| entry.managed && entry.instance.id == id)
            .map(|entry| &entry.instance)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|entry| entry.managed)
            .map(|entry| entry.instance.id.as_str())
    }

    /// `TwapManager.serializeState`: the `twap:stateUpdate` payload clients persist.
    pub fn serialize_state(&self) -> Value {
        let mut result = Map::new();
        for entry in self.managed_entries() {
            let instance = &entry.instance;
            if instance.state.status != TwapStatus::Completed {
                result.insert(
                    instance.id.clone(),
                    serde_json::json!({ "options": instance.options, "state": instance.state }),
                );
            }
        }
        Value::Object(result)
    }

    /// `TwapPostprocessor`: the store's `activeTWAPs` for `account_name`.
    pub fn active_twaps(&self, account_name: &str) -> Vec<Value> {
        self.managed_entries()
            .map(|entry| &entry.instance)
            .filter(|instance| {
                !instance.options.active_market.is_null()
                    && instance.options.account_name == account_name
                    && instance.state.status != TwapStatus::Completed
            })
            .map(active_twap)
            .collect()
    }

    /// `TwapOrderEnrichmentPostprocessor`: the TWAP owning a direct limit order.
    pub fn order_owner(&self, account_name: &str, order_id: &str) -> Option<&str> {
        self.managed_entries()
            .map(|entry| &entry.instance)
            .filter(|instance| {
                !instance.options.active_market.is_null()
                    && instance.options.account_name == account_name
                    && instance.state.status != TwapStatus::Completed
            })
            .filter(|instance| instance.current_order_ids().iter().any(|id| id == order_id))
            .last()
            .map(|instance| instance.id.as_str())
    }

    /// A TWAP naming another account would trade here yet never be listed.
    fn owns(&self, options: &TwapOptions) -> Result<(), String> {
        if options.account_name != self.account_name {
            return Err(format!(
                "it names account {:?}, not {:?}",
                options.account_name, self.account_name
            ));
        }
        Ok(())
    }

    fn has_capacity(&self) -> Result<(), String> {
        let running = self
            .managed_entries()
            .filter(|entry| entry.instance.state.status != TwapStatus::Completed)
            .count();
        if running >= validate::MAX_TWAPS_PER_ACCOUNT {
            return Err(format!(
                "at most {} TWAPs can run per account",
                validate::MAX_TWAPS_PER_ACCOUNT
            ));
        }
        Ok(())
    }

    fn retire(&mut self, id: String) {
        if !self.retired_set.insert(id.clone()) {
            return;
        }
        self.retired.push_back(id);
        if self.retired.len() > validate::RETIRED_IDS_KEPT
            && let Some(oldest) = self.retired.pop_front()
        {
            self.retired_set.remove(&oldest);
        }
    }

    fn managed_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|entry| entry.managed)
    }

    fn managed_mut(&mut self, id: &str) -> Option<&mut TwapInstance> {
        self.entries
            .iter_mut()
            .find(|entry| entry.managed && entry.instance.id == id)
            .map(|entry| &mut entry.instance)
    }

    fn with_id(&mut self, data: &Value, apply: impl FnOnce(&mut TwapInstance)) {
        if let Some(instance) = data.as_str().and_then(|id| self.managed_mut(id)) {
            apply(instance);
        }
    }

    /// `Map.set`: a known id keeps its position and takes the new instance.
    fn add(&mut self, instance: TwapInstance) {
        let entry = Entry {
            instance,
            managed: true,
        };
        match self
            .entries
            .iter_mut()
            .find(|existing| existing.instance.id == entry.instance.id)
        {
            Some(existing) => *existing = entry,
            None => self.entries.push(entry),
        }
    }

    fn restore(&mut self, data: &Value, env: &mut dyn TwapEnv, effects: &mut Vec<TwapEffect>) {
        let Some(persisted) = data.as_array() else {
            effects.push(TwapEffect::Diagnostic {
                message: "twap:state expects an array".to_owned(),
            });
            return;
        };
        for value in persisted {
            // TS restores inside one try: the first bad entry ends the restore.
            let entry = match serde_json::from_value::<PersistedTwap>(value.clone()) {
                Ok(entry) => entry,
                Err(error) => {
                    effects.push(TwapEffect::Diagnostic {
                        message: format!("twap:state entry rejected: {error}"),
                    });
                    return;
                }
            };
            // Another holder already restored or started it: one TWAP per id.
            if self
                .entries
                .iter()
                .any(|known| known.instance.id == entry.id)
            {
                continue;
            }
            if self.retired_set.contains(&entry.id) {
                effects.push(TwapEffect::Diagnostic {
                    message: format!("TWAP {} not restored: it already finished", entry.id),
                });
                continue;
            }
            if let Err(reason) = validate::restored(&entry.options, &entry.state, env.now_ms())
                .and_then(|()| self.owns(&entry.options))
                .and_then(|()| self.has_capacity())
            {
                effects.push(TwapEffect::Log {
                    message: format!("TWAP {} not restored: {reason}", entry.options.symbol()),
                });
                continue;
            }
            let running = entry.state.status == TwapStatus::Running;
            let id = entry.id.clone();
            self.add(TwapInstance::new(
                entry.id,
                entry.options,
                Some(entry.state),
                env,
            ));
            if let Some(instance) = self.managed_mut(&id) {
                // Persisted at once, so the host sees which rows it adopted.
                instance.mark_state_changed();
                // A host that looks the slice up settles it before restoring.
                instance.count_unconfirmed_in_flight(effects);
                if running {
                    instance.pause();
                    instance.resume(env);
                }
            }
        }
    }

    /// Drop completed TWAPs from the manager, retire drained entries, and emit
    /// one state update when any state changed.
    fn settle(&mut self, effects: &mut Vec<TwapEffect>) {
        let mut changed = false;
        let mut finished = Vec::new();
        for entry in &mut self.entries {
            changed |= entry.instance.take_state_changed();
            if entry.managed && entry.instance.state.status == TwapStatus::Completed {
                entry.managed = false;
                finished.push(entry.instance.id.clone());
            }
        }
        for id in finished {
            self.retire(id);
        }
        self.entries
            .retain(|entry| entry.managed || entry.instance.has_outstanding_work());
        if changed {
            effects.push(TwapEffect::StateUpdate {
                data: self.serialize_state(),
            });
        }
    }
}

fn active_twap(instance: &TwapInstance) -> Value {
    let mut object = Map::new();
    object.insert("id".to_owned(), Value::String(instance.id.clone()));
    for source in [
        serde_json::to_value(&instance.options),
        serde_json::to_value(&instance.state),
    ] {
        if let Ok(Value::Object(fields)) = source {
            object.extend(fields);
        }
    }
    object.insert(
        "currentOrderIds".to_owned(),
        Value::from(instance.current_order_ids().to_vec()),
    );
    let size = match instance.options.side {
        OrderSide::Sell => -instance.options.size,
        OrderSide::Buy => instance.options.size,
    };
    object.insert(
        "size".to_owned(),
        serde_json::to_value(size).unwrap_or(Value::Null),
    );
    let symbol = match instance.options.symbol() {
        "" => "Unknown",
        symbol => symbol,
    };
    object.insert("symbol".to_owned(), Value::String(symbol.to_owned()));
    Value::Object(object)
}

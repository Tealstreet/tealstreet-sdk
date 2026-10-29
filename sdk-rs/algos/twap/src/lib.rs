//! TWAP for the native exchange server: a port of the TS worker's
//! `TwapHandler`, `TwapInstance`, `TwapManager` and `TwapPostprocessor`.
//!
//! The crate owns no clock, timer, socket or engine handle. A host drives one
//! [`TwapBook`] per account: it passes `twap:*` ops to [`TwapBook::handle`],
//! wakes the book at [`TwapBook::wake_at`], reports slice placements back with
//! [`TwapBook::placement_settled`], and executes the returned [`TwapEffect`]s
//! against the engine and the hosted protocol.

mod instance;
mod math;
mod types;

pub use instance::TwapInstance;
pub use types::{
    OrderSide, PersistedTwap, SliceAction, TwapOptions, TwapOrigin, TwapState, TwapStatus,
};

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
    StartChaser {
        twap_id: String,
        request: ChaserRequest,
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
#[derive(Debug, Default)]
pub struct TwapBook {
    entries: Vec<Entry>,
}

#[derive(Debug)]
struct Entry {
    instance: TwapInstance,
    /// False once removed from the TS manager; the entry lingers only while
    /// in-flight placements or slice timeouts can still act.
    managed: bool,
}

impl TwapBook {
    pub fn new() -> Self {
        Self::default()
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
                    let id = env.new_id();
                    let instance = TwapInstance::new(id.clone(), options, None, env);
                    self.add(instance);
                    if let Some(instance) = self.managed_mut(&id) {
                        instance.start(env, view, &mut effects);
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
        outcome: Result<Vec<String>, String>,
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
            if self.get(&entry.id).is_some() {
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
            if running && let Some(instance) = self.managed_mut(&id) {
                instance.pause();
                instance.resume(env);
            }
        }
    }

    /// Drop completed TWAPs from the manager, retire drained entries, and emit
    /// one state update when any state changed.
    fn settle(&mut self, effects: &mut Vec<TwapEffect>) {
        let mut changed = false;
        for entry in &mut self.entries {
            changed |= entry.instance.take_state_changed();
            if entry.instance.state.status == TwapStatus::Completed {
                entry.managed = false;
            }
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

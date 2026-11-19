//! AGT-37 notifications raised by a DELEGATED account's algo host: fills,
//! algo-child order rejections and algo stops. Each is built through
//! `host_notify`'s typed fields; see `DELEGATED_RETENTION.md`.
use crate::host_notify::{
    Algo, Cause, Draft, Fields, HostNotification, HostNotifier, Kind, Scope, Severity, Side, Verb,
};
use safe_cex_algos::{AccountPort, AlgoStore, Change, CommandFuture};
use safe_cex_engine::engine::{Command, CommandId, Fill};
use safe_cex_engine::error::Outcome;
use safe_cex_engine::model::OrderSide;
use serde_json::Value;
use std::cell::{Cell, Ref, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::Instant;

/// Fills of one order within this quiet window become one notification.
pub const FILL_DEBOUNCE: Duration = Duration::from_secs(2);
/// One `orderRejected` per algo, symbol and side in this window: a retrying
/// chase must not page the user once per attempt.
pub const REJECT_REPEAT: Duration = Duration::from_secs(60);
/// Past this many orders waiting out the debounce, the earliest is sent now.
const MAX_PENDING_FILLS: usize = 512;

/// Chase stop reasons that are an explicit client stop, never an `algoStopped`.
/// They are the literal reasons `safe_cex_chaser` and exec's nuke record.
const CLIENT_CHASE_STOPS: [&str; 4] = [
    "User cancelled",
    safe_cex_chaser::KEPT_ORDER_STOP_REASON,
    "Client disconnected while chase was active",
    "Stopped by nuke",
];
const FINISHED_CHASE_STOPS: [&str; 2] = ["Completed", "Terminated by length condition"];

/// Where one account's notifications go; built only when the lane is on.
#[derive(Clone)]
pub struct AlgoNotify {
    notifier: HostNotifier,
    scope: Scope,
    /// Rechecked per event: a COSIGNED resident can become DELEGATED.
    delegated: Arc<dyn Fn() -> bool + Send + Sync>,
    held: Arc<AtomicBool>,
}

impl AlgoNotify {
    pub fn new(
        notifier: HostNotifier,
        scope: Scope,
        delegated: Arc<dyn Fn() -> bool + Send + Sync>,
        held: Arc<AtomicBool>,
    ) -> Self {
        Self {
            notifier,
            scope,
            delegated,
            held,
        }
    }

    fn emit(&self, draft: Draft) {
        self.notifier.push(HostNotification::build(
            &self.scope,
            draft,
            self.held.load(Ordering::Relaxed),
            crate::accounts::now_ms(),
        ));
    }
}

struct PendingFill {
    symbol: String,
    side: OrderSide,
    filled: f64,
    /// The order was still open right after its latest fill applied; leaving
    /// the store later, without a fill, is a cancel. Unit-free by design.
    open_after: bool,
    due: Instant,
}

type TwapChild = Box<dyn Fn(&str) -> bool>;

struct TwapSeen {
    symbol: String,
    side: Option<Side>,
    errored: bool,
}

/// The algo host's observer, on its engine thread.
pub(crate) struct Tap {
    notify: AlgoNotify,
    /// The main loop ended: disposal's stops are covered by `delegated.shutdown`.
    ended: Cell<bool>,
    fills: RefCell<HashMap<String, PendingFill>>,
    pub(crate) wake: tokio::sync::Notify,
    twaps: RefCell<HashMap<String, TwapSeen>>,
    client_stopped: RefCell<HashSet<String>>,
    rejected: RefCell<HashMap<(Algo, String, Side), Instant>>,
    /// True for a chase that is a TWAP slice; the TWAP reports its own end.
    twap_child: RefCell<Option<TwapChild>>,
}

fn open<'a>(store: &'a AlgoStore, order_id: &str) -> Option<&'a safe_cex_engine::model::Order> {
    store
        .orders
        .iter()
        .find(|order| order.id == order_id && order.remaining > 0.0)
}

fn side(side: OrderSide) -> Side {
    match side {
        OrderSide::Buy => Side::Buy,
        OrderSide::Sell => Side::Sell,
    }
}

fn side_of(value: &Value) -> Option<Side> {
    match value.as_str()? {
        "buy" => Some(Side::Buy),
        "sell" => Some(Side::Sell),
        _ => None,
    }
}

fn algo_name(algo: Algo) -> &'static str {
    match algo {
        Algo::Chase => "chase",
        Algo::Twap => "twap",
        Algo::Exec => "exec",
    }
}

fn stopped(algo: Algo, symbol: &str, side: Option<Side>, finished: bool) -> (Fields, Severity) {
    let mut fields = Fields::default().symbol(symbol).algo(algo);
    if let Some(side) = side {
        fields = fields.side(side);
    }
    if finished {
        (
            fields.verb(Verb::Finished).cause(Cause::AlgoFinished),
            Severity::Info,
        )
    } else {
        (
            fields.verb(Verb::Stopped).cause(Cause::AlgoError),
            Severity::Warning,
        )
    }
}

impl Tap {
    pub(crate) fn new(notify: AlgoNotify) -> Self {
        Self {
            notify,
            ended: Cell::new(false),
            fills: RefCell::default(),
            wake: tokio::sync::Notify::new(),
            twaps: RefCell::default(),
            client_stopped: RefCell::default(),
            rejected: RefCell::default(),
            twap_child: RefCell::default(),
        }
    }

    fn delegated(&self) -> bool {
        (self.notify.delegated)()
    }

    pub(crate) fn set_twap_child(&self, check: TwapChild) {
        *self.twap_child.borrow_mut() = Some(check);
    }

    pub(crate) fn end(&self) {
        self.ended.set(true);
    }

    /// Before dispatch: a client `twap:stop`/`twap:cleanup` or nuke is no `algoStopped`.
    pub(crate) fn client_stop(&self, twap_id: &str) {
        self.client_stopped.borrow_mut().insert(twap_id.to_owned());
    }

    /// Live fills from flush frames only; a rejoin snapshot never reaches here.
    pub(crate) fn record_fills(&self, fills: &[Fill], store: &AlgoStore) {
        if fills.is_empty() || !self.delegated() {
            return;
        }
        let due = Instant::now() + FILL_DEBOUNCE;
        let mut pending = self.fills.borrow_mut();
        for fill in fills {
            let entry = pending
                .entry(fill.order_id.clone())
                .or_insert_with(|| PendingFill {
                    symbol: fill.symbol.clone(),
                    side: fill.side,
                    filled: 0.0,
                    open_after: false,
                    due,
                });
            if fill.amount.is_finite() && fill.amount > 0.0 {
                entry.filled += fill.amount;
            }
            entry.open_after = open(store, &fill.order_id).is_some();
            entry.due = due;
        }
        drop(pending);
        self.wake.notify_one();
    }

    pub(crate) fn next_fill_due(&self) -> Option<Instant> {
        self.fills.borrow().values().map(|fill| fill.due).min()
    }

    /// Sends every order whose debounce has elapsed, or all of them.
    pub(crate) fn flush_fills(&self, store: &AlgoStore, all: bool) {
        let now = Instant::now();
        let mut ready: Vec<(String, PendingFill)> = {
            let mut pending = self.fills.borrow_mut();
            let mut ids: Vec<_> = pending
                .iter()
                .filter(|(_, fill)| all || fill.due <= now)
                .map(|(id, _)| id.clone())
                .collect();
            // Bounded: the earliest orders past the cap go now.
            if pending.len() - ids.len() > MAX_PENDING_FILLS {
                let mut rest: Vec<_> = pending
                    .iter()
                    .filter(|(id, _)| !ids.contains(id))
                    .map(|(id, fill)| (fill.due, id.clone()))
                    .collect();
                rest.sort();
                let excess = pending.len() - ids.len() - MAX_PENDING_FILLS;
                ids.extend(rest.into_iter().take(excess).map(|(_, id)| id));
            }
            ids.into_iter()
                .filter_map(|id| pending.remove(&id).map(|fill| (id, fill)))
                .collect()
        };
        ready.sort_by_key(|(_, fill)| fill.due);
        for (order_id, fill) in ready {
            let (verb, natural_key) = match open(store, &order_id) {
                Some(order) => (
                    Verb::PartiallyFilled,
                    format!("{order_id}\0partiallyFilled\0{}", order.filled),
                ),
                None if fill.open_after => (
                    Verb::PartiallyFilled,
                    format!("{order_id}\0partiallyFilled\0{}", fill.filled),
                ),
                None => (Verb::Filled, format!("{order_id}\0filled")),
            };
            self.notify.emit(Draft {
                kind: Kind::Fill,
                severity: Severity::Info,
                natural_key,
                fields: Fields::default()
                    .symbol(&fill.symbol)
                    .side(side(fill.side))
                    .verb(verb),
                deadline: None,
                collapse: Some(crate::host_notify::opaque_key(
                    &self.notify.scope.account_digest,
                    &order_id,
                )),
            });
        }
    }

    /// A server-initiated placement the venue refused outright.
    fn rejected(&self, algo: Algo, symbol: &str, order_side: OrderSide) {
        if self.ended.get() || !self.delegated() {
            return;
        }
        let now = Instant::now();
        let key = (algo, symbol.to_owned(), side(order_side));
        {
            let mut rejected = self.rejected.borrow_mut();
            rejected.retain(|_, at| now.duration_since(*at) < REJECT_REPEAT);
            if rejected.contains_key(&key) {
                return;
            }
            rejected.insert(key, now);
        }
        self.notify.emit(Draft {
            kind: Kind::OrderRejected,
            severity: Severity::Warning,
            natural_key: format!(
                "{}\0{symbol}\0{:?}\0{}",
                algo_name(algo),
                order_side,
                crate::accounts::now_ms()
            ),
            fields: Fields::default()
                .symbol(symbol)
                .side(side(order_side))
                .algo(algo)
                .verb(Verb::Rejected)
                .cause(Cause::VenueRejected),
            deadline: None,
            collapse: None,
        });
    }

    /// Sees every algo reply before it reaches the page.
    pub(crate) fn observe(&self, op: &str, data: &str) {
        match op {
            "chaser:complete" => self.chase_ended(data),
            "twap:stateUpdate" => self.twaps_changed(data),
            "error" => self.twap_error(data),
            _ => {}
        }
    }

    fn chase_ended(&self, data: &str) {
        let Ok(end) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let (Some(id), Some(symbol), Some(reason)) = (
            end["id"].as_str(),
            end["symbol"].as_str(),
            end["stopReason"].as_str(),
        ) else {
            return;
        };
        if self.ended.get()
            || CLIENT_CHASE_STOPS.contains(&reason)
            || self
                .twap_child
                .borrow()
                .as_ref()
                .is_some_and(|check| check(id))
            || !self.delegated()
        {
            return;
        }
        let (fields, severity) = stopped(
            Algo::Chase,
            symbol,
            side_of(&end["side"]),
            FINISHED_CHASE_STOPS.contains(&reason),
        );
        self.notify.emit(Draft {
            kind: Kind::AlgoStopped,
            severity,
            natural_key: format!("chase\0{id}"),
            fields,
            deadline: None,
            collapse: Some(crate::host_notify::opaque_key(
                &self.notify.scope.account_digest,
                &format!("chase:{id}"),
            )),
        });
    }

    /// TWAP's slice-failure and shortfall reports (`report` in the TWAP factory)
    /// begin `TWAP <symbol> `; its end is then an error, not a finish. An owner
    /// rebind reports `TWAPs stopped: ...` and empties the map: all errored.
    fn twap_error(&self, data: &str) {
        let text = serde_json::from_str::<String>(data).unwrap_or_else(|_| data.to_owned());
        if text.starts_with("TWAPs ") {
            for seen in self.twaps.borrow_mut().values_mut() {
                seen.errored = true;
            }
            return;
        }
        let Some(rest) = text.strip_prefix("TWAP ") else {
            return;
        };
        for seen in self.twaps.borrow_mut().values_mut() {
            if rest
                .strip_prefix(seen.symbol.as_str())
                .is_some_and(|after| after.starts_with(' '))
            {
                seen.errored = true;
            }
        }
    }

    /// A completed TWAP leaves the state map; it is never shown as completed.
    fn twaps_changed(&self, data: &str) {
        let Ok(Value::Object(rows)) = serde_json::from_str::<Value>(data) else {
            return;
        };
        let mut twaps = self.twaps.borrow_mut();
        for (id, row) in &rows {
            let options = &row["options"];
            twaps.entry(id.clone()).or_insert_with(|| TwapSeen {
                symbol: options["activeMarket"]["symbol"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
                side: side_of(&options["side"]),
                errored: false,
            });
        }
        let gone: Vec<_> = twaps
            .keys()
            .filter(|id| !rows.contains_key(*id))
            .cloned()
            .collect();
        let mut ended = Vec::new();
        for id in gone {
            if let Some(seen) = twaps.remove(&id) {
                let client = self.client_stopped.borrow_mut().remove(&id);
                if !client {
                    ended.push((id, seen));
                }
            }
        }
        drop(twaps);
        if self.ended.get() || ended.is_empty() || !self.delegated() {
            return;
        }
        for (id, seen) in ended {
            let (fields, severity) = stopped(Algo::Twap, &seen.symbol, seen.side, !seen.errored);
            self.notify.emit(Draft {
                kind: Kind::AlgoStopped,
                severity,
                natural_key: format!("twap\0{id}"),
                fields,
                deadline: None,
                collapse: Some(crate::host_notify::opaque_key(
                    &self.notify.scope.account_digest,
                    &format!("twap:{id}"),
                )),
            });
        }
    }
}

/// Chase and TWAP contexts: placements they send are server-initiated, so a
/// venue refusal becomes `orderRejected`. Exec and nuke use the untapped port.
pub(crate) struct RejectTap<P> {
    pub port: Rc<P>,
    pub tap: Rc<Tap>,
    pub algo: Algo,
}

impl<P: AccountPort + 'static> AccountPort for RejectTap<P> {
    fn store(&self) -> Ref<'_, AlgoStore> {
        self.port.store()
    }
    fn subscribe(&self) -> tokio::sync::mpsc::UnboundedReceiver<Change> {
        self.port.subscribe()
    }
    fn next_command_id(&self) -> CommandId {
        self.port.next_command_id()
    }
    fn command(&self, command: Command) -> CommandFuture {
        let target = match &command {
            Command::PlaceOrders { orders, .. } => orders
                .first()
                .map(|order| (order.symbol.clone(), order.side)),
            _ => None,
        };
        let result = self.port.command(command);
        let Some((symbol, order_side)) = target else {
            return result;
        };
        let (tap, algo) = (self.tap.clone(), self.algo);
        Box::pin(async move {
            let result = result.await;
            if let Err(error) = &result
                && error.outcome == Some(Outcome::Rejected)
            {
                tap.rejected(algo, &symbol, order_side);
            }
            result
        })
    }
    fn capabilities(&self) -> safe_cex_engine::Capabilities {
        self.port.capabilities()
    }
    fn emit(&self, op: &str, data: String) {
        self.port.emit(op, data);
    }
}

#[cfg(test)]
#[path = "algo_notify_tests.rs"]
mod tests;

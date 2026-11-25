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
use safe_cex_native::own_placements::Placements;
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
/// They are the literal reasons `safe_cex_chaser` and exec's nuke record; a
/// retried cancel appends `; ...` to them (`cancel_until_settled`), so callers
/// compare `base_reason`.
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
        self.notifier.push(self.build(draft));
    }

    fn build(&self, draft: Draft) -> HostNotification {
        HostNotification::build(
            &self.scope,
            draft,
            self.held.load(Ordering::Relaxed),
            crate::accounts::now_ms(),
        )
    }

    /// A fill carries its claim: the user-independent order key and whether
    /// this account placed the order (AGT-196, decided by the gateway).
    fn emit_fill(&self, draft: Draft, order_id: &str, placed_here: bool) {
        let mut notification = self.build(draft);
        notification.fill_key = Some(crate::host_notify::fill_key(&self.scope.venue, order_id));
        notification.placed_here = Some(placed_here);
        self.notifier.push(notification);
    }
}

struct PendingFill {
    symbol: String,
    side: OrderSide,
    filled: f64,
    /// The order row (amount, filled) right after its latest fill applied, if
    /// the store still held it. Only the order's own fields share a unit.
    row: Option<(f64, f64)>,
    /// The row's identities (client order id, parent, triggering algo), kept
    /// for the placement claim after the venue may have removed a filled row.
    identities: Identities,
    due: Instant,
}

/// The order's own record proves its size was met; fill amounts never do.
fn size_met((amount, filled): (f64, f64)) -> bool {
    amount > 0.0 && filled >= amount - amount * 1e-9
}

/// The chaser book's live rows, read weakly; TWAP slices carry `twap_id`.
type ActiveChases = Box<dyn Fn() -> Vec<safe_cex_chaser::ActiveChaser>>;

/// The recorded reason before any `; <retry/settlement detail>` suffix.
fn base_reason(reason: &str) -> &str {
    reason.split_once(';').map_or(reason, |(base, _)| base)
}

struct TwapSeen {
    symbol: String,
    side: Option<Side>,
    errored: bool,
}

/// The algo host's observer, on its engine thread.
pub(crate) struct Tap {
    notify: AlgoNotify,
    /// Orders this account's own clients placed (AGT-196). Another Tealstreet
    /// user connected to the same venue account sees the same fills; this is
    /// how a fill says which resident placed it.
    placements: Placements,
    /// The main loop ended: disposal's stops are not ordinary ends. Only a box
    /// stop reports them (`server_stopped`).
    ended: Cell<bool>,
    fills: RefCell<HashMap<String, PendingFill>>,
    pub(crate) wake: tokio::sync::Notify,
    twaps: RefCell<HashMap<String, TwapSeen>>,
    /// TWAPs a client stop was dispatched for; removed when it is refused.
    client_stopped: RefCell<HashSet<String>>,
    rejected: RefCell<HashMap<(Algo, String, Side), Instant>>,
    /// A TWAP slice chase reports through its TWAP, and its rejections as `twap`.
    chases: RefCell<Option<ActiveChases>>,
}

/// The other ids a stored order answers to: its client order id, its parent,
/// and the venue algo order that triggered it (OKX, BloFin).
#[derive(Default)]
struct Identities(Vec<String>);

fn identities(store: &AlgoStore, order_id: &str) -> Option<Identities> {
    let order = store.orders.iter().find(|order| order.id == order_id)?;
    let triggered_by = order
        .extra
        .as_ref()
        .and_then(|extra| extra.triggered_by.clone());
    Some(Identities(
        [
            order.client_order_id.clone(),
            order.parent_id.clone(),
            triggered_by,
        ]
        .into_iter()
        .flatten()
        .collect(),
    ))
}

fn row(store: &AlgoStore, order_id: &str) -> Option<(f64, f64)> {
    store
        .orders
        .iter()
        .find(|order| order.id == order_id)
        .map(|order| (order.amount, order.filled))
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

/// How an algo ended, as its `algoStopped` verb, cause and severity.
#[derive(Clone, Copy)]
enum End {
    Finished,
    Errored,
    /// The box itself stopped it: a drain, SIGTERM or a forced user drain.
    ServerStopped,
}

fn stopped(algo: Algo, symbol: &str, side: Option<Side>, end: End) -> (Fields, Severity) {
    let mut fields = Fields::default().symbol(symbol).algo(algo);
    if let Some(side) = side {
        fields = fields.side(side);
    }
    let (verb, cause, severity) = match end {
        End::Finished => (Verb::Finished, Cause::AlgoFinished, Severity::Info),
        End::Errored => (Verb::Stopped, Cause::AlgoError, Severity::Warning),
        End::ServerStopped => (Verb::Stopped, Cause::ServerRestart, Severity::Warning),
    };
    (fields.verb(verb).cause(cause), severity)
}

impl Tap {
    pub(crate) fn new(notify: AlgoNotify, placements: Placements) -> Self {
        Self {
            notify,
            placements,
            ended: Cell::new(false),
            fills: RefCell::default(),
            wake: tokio::sync::Notify::new(),
            twaps: RefCell::default(),
            client_stopped: RefCell::default(),
            rejected: RefCell::default(),
            chases: RefCell::default(),
        }
    }

    fn delegated(&self) -> bool {
        (self.notify.delegated)()
    }

    pub(crate) fn set_chases(&self, chases: ActiveChases) {
        *self.chases.borrow_mut() = Some(chases);
    }

    fn active_chases(&self) -> Vec<safe_cex_chaser::ActiveChaser> {
        self.chases
            .borrow()
            .as_ref()
            .map_or_else(Vec::new, |chases| chases())
    }

    pub(crate) fn end(&self) {
        self.ended.set(true);
    }

    /// Before dispatch: a client `twap:stop`/`twap:cleanup` or nuke is no `algoStopped`.
    /// Only a TWAP this tap has seen running is recorded.
    pub(crate) fn client_stop(&self, twap_id: &str) {
        if self.twaps.borrow().contains_key(twap_id) {
            self.client_stopped.borrow_mut().insert(twap_id.to_owned());
        }
    }

    /// A refused stop must not suppress that TWAP's later genuine end.
    pub(crate) fn client_stop_refused(&self, twap_id: &str) {
        self.client_stopped.borrow_mut().remove(twap_id);
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
                    row: None,
                    identities: Identities::default(),
                    due,
                });
            if fill.amount.is_finite() && fill.amount > 0.0 {
                entry.filled += fill.amount;
            }
            entry.row = row(store, &fill.order_id);
            if let Some(identities) = identities(store, &fill.order_id) {
                entry.identities = identities;
            }
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
        for (order_id, mut fill) in ready {
            if let Some(identities) = identities(store, &order_id) {
                fill.identities = identities;
            }
            let placed_here = self.placed_here(&order_id, &fill.identities);
            // `filled` only on proof: an IOC/capped market order whose remainder
            // was cancelled in the fill's own batch must not read as filled.
            let (verb, natural_key) = match row(store, &order_id).or(fill.row) {
                Some(row) if size_met(row) => (Verb::Filled, format!("{order_id}\0filled")),
                Some((_, filled)) => (
                    Verb::PartiallyFilled,
                    format!("{order_id}\0partiallyFilled\0{filled}"),
                ),
                None => (
                    Verb::PartiallyFilled,
                    format!("{order_id}\0partiallyFilled\0{}", fill.filled),
                ),
            };
            self.notify.emit_fill(
                Draft {
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
                },
                &order_id,
                placed_here,
            );
        }
    }

    /// AGT-196: whether this account placed the order, a parent of it or the
    /// algo order that triggered it. False is "not known here", never "foreign".
    fn placed_here(&self, order_id: &str, identities: &Identities) -> bool {
        let placements = safe_cex_native::own_placements::lock(&self.placements);
        placements.placed(order_id) || identities.0.iter().any(|id| placements.placed(id))
    }

    /// A server-initiated placement the venue refused outright.
    fn rejected(&self, algo: Algo, symbol: &str, order_side: OrderSide) {
        if self.ended.get() || !self.delegated() {
            return;
        }
        // A chase placement on a market whose live chases are all TWAP slices is the TWAP's.
        let algo = if algo == Algo::Chase {
            let matching: Vec<_> = self
                .active_chases()
                .into_iter()
                .filter(|chase| chase.symbol == symbol && chase.side == order_side)
                .collect();
            if !matching.is_empty() && matching.iter().all(|chase| chase.twap_id.is_some()) {
                Algo::Twap
            } else {
                Algo::Chase
            }
        } else {
            algo
        };
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
        let reason = base_reason(reason);
        if self.ended.get()
            || CLIENT_CHASE_STOPS.contains(&reason)
            || self
                .active_chases()
                .iter()
                .any(|chase| chase.id == id && chase.twap_id.is_some())
            || !self.delegated()
        {
            return;
        }
        let how = if FINISHED_CHASE_STOPS.contains(&reason) {
            End::Finished
        } else {
            End::Errored
        };
        self.algo_stopped(Algo::Chase, id, symbol, side_of(&end["side"]), how);
    }

    /// One `algoStopped`, keyed per algo end. A TWAP's box stop gets its own key:
    /// the TWAP is restored under its id, and sharing the key would make the
    /// gateway drop its later real end as a duplicate (it keys rows by id).
    fn algo_stopped(&self, algo: Algo, id: &str, symbol: &str, side: Option<Side>, end: End) {
        let (fields, severity) = stopped(algo, symbol, side, end);
        let name = algo_name(algo);
        let natural_key = match (algo, end) {
            (Algo::Twap, End::ServerStopped) => {
                format!("{name}\0{id}\0serverRestart\0{}", crate::accounts::now_ms())
            }
            _ => format!("{name}\0{id}"),
        };
        self.notify.emit(Draft {
            kind: Kind::AlgoStopped,
            severity,
            natural_key,
            fields,
            deadline: None,
            collapse: Some(crate::host_notify::opaque_key(
                &self.notify.scope.account_digest,
                &format!("{name}:{id}"),
            )),
        });
    }

    /// A box stop (drain, SIGTERM, forced user drain): one `serverRestart` stop per
    /// chase and TWAP still running, as chases are never restored. Stopping chases,
    /// slices, paused TWAPs and TWAPs with a client stop in flight are skipped.
    pub(crate) fn server_stopped(&self, chases: &[safe_cex_chaser::ActiveChaser], twaps: &[Value]) {
        if !self.delegated() {
            return;
        }
        for chase in chases
            .iter()
            .filter(|chase| !chase.is_stopped && chase.twap_id.is_none())
        {
            self.algo_stopped(
                Algo::Chase,
                &chase.id,
                &chase.symbol,
                Some(side(chase.side)),
                End::ServerStopped,
            );
        }
        let client_stopped = self.client_stopped.borrow();
        for twap in twaps {
            let Some(id) = twap["id"].as_str() else {
                continue;
            };
            if twap["status"] == "paused" || client_stopped.contains(id) {
                continue;
            }
            self.algo_stopped(
                Algo::Twap,
                id,
                twap["activeMarket"]["symbol"].as_str().unwrap_or_default(),
                side_of(&twap["side"]),
                End::ServerStopped,
            );
        }
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
            let end = if seen.errored {
                End::Errored
            } else {
                End::Finished
            };
            self.algo_stopped(Algo::Twap, &id, &seen.symbol, seen.side, end);
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

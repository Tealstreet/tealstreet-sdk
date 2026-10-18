//! The material Store graph. Every mutation uses the shared native properties.
use crate::js_runtime::{
    for_in::ForInRecord,
    iterator::IteratorRecord,
    ops,
    value::{Js, Thrown, arg, utf16},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(super) const TICKER_FIELDS: [&str; 11] = [
    "bid",
    "ask",
    "last",
    "mark",
    "index",
    "percentage",
    "openInterest",
    "fundingRate",
    "fundingTime",
    "volume",
    "quoteVolume",
];

#[derive(Clone)]
pub(super) enum Subscriber {
    Native(Js),
    Typed(
        std::sync::Arc<
            dyn Fn(
                &safe_cex_types::StoreData,
                bool,
            ) -> Result<(), safe_cex_types::StoreSubscriberError>,
        >,
    ),
}

#[derive(Default)]
pub(super) struct Listeners {
    pub next: u64,
    pub slots: Vec<(u64, Option<Subscriber>)>,
}

pub(super) struct Graph {
    state: RefCell<Js>,
    // Separate source dictionary generations, sharing the actual array rows.
    maps: [RefCell<Js>; 4],
    ticker_index: Js,
    market_index: Js,
    terminal_orders: Js,
    pub listeners: Rc<RefCell<Listeners>>,
    pub market_version: Cell<f64>,
    pub ticker_version: Cell<f64>,
    pub now: RefCell<std::sync::Arc<dyn Fn() -> f64 + Send + Sync>>,
}

fn initial_state() -> Js {
    let zero = || Js::Number(0.0);
    let object = |keys: &[&str], value: Js| {
        Js::object(keys.iter().map(|key| (*key, value.clone())).collect())
    };
    Js::object(vec![
        ("latency", zero()),
        (
            "balance",
            object(&["used", "free", "total", "upnl"], zero()),
        ),
        ("markets", Js::array(vec![])),
        ("tickers", Js::array(vec![])),
        ("positions", Js::array(vec![])),
        ("displayedPositions", Js::array(vec![])),
        (
            "displayedMargin",
            object(&["longExposure", "shortExposure", "totalNotional"], zero()),
        ),
        ("ordersRisk", Js::object(vec![])),
        ("activeChasers", Js::array(vec![])),
        ("activeTWAPs", Js::array(vec![])),
        ("orders", Js::array(vec![])),
        ("messageHandlerStats", Js::object(vec![])),
        (
            "requestStats",
            object(&["ok", "rateLimit", "auth", "network", "other"], zero()),
        ),
        ("fetchedLeverage", Js::object(vec![])),
        (
            "loaded",
            object(
                &["balance", "orders", "markets", "tickers", "positions"],
                Js::Bool(false),
            ),
        ),
        ("options", object(&["isHedged"], Js::Bool(false))),
        ("updateIds", object(&["positions", "orders"], zero())),
        (
            "socketHealth",
            object(
                &[
                    "publicHealthy",
                    "privateHealthy",
                    "publicInitialized",
                    "privateInitialized",
                ],
                Js::Bool(false),
            ),
        ),
    ])
}

// Object.assign must alternate each source getter and target setter. Collecting
// own_entries first would hide partial writes and change reentrant ordering.
fn assign(target: &Js, source: &Js) -> Result<(), Thrown> {
    if !source.is_nullish() {
        for key in source.own_keys() {
            if source.own_enumerability(&key)? == Some(true) {
                target.set_key(&key, source.get_key(&key)?)?;
            }
        }
    }
    Ok(())
}

fn patch(target: &Js, source: &Js) -> Result<bool, Thrown> {
    let mut keys = ForInRecord::get(source);
    let mut changed = false;
    while let Some(key) = keys.next_key()? {
        target.set_key(&key, source.get_key(&key)?)?;
        changed = true;
    }
    Ok(changed)
}

fn length(value: &Js) -> Result<usize, Thrown> {
    Ok(value.get("length")?.to_number()? as usize)
}

fn row(value: &Js, index: usize) -> Result<Js, Thrown> {
    value.get(&index.to_string())
}

fn id_index(items: &Js) -> Result<Js, Thrown> {
    let out = Js::object(vec![]);
    out.set_prototype(Js::Null)?;
    let mut next = 0;
    while next < length(items)? {
        let index = next;
        next += 1;
        let key = row(items, index)?.get("id")?;
        out.set_key(&key.to_js_string()?, Js::Number(index as f64))?;
    }
    Ok(out)
}

fn id_lookup(items: &Js) -> Result<Js, Thrown> {
    let out = Js::object(vec![]);
    out.set_prototype(Js::Null)?;
    let mut next = 0;
    while next < length(items)? {
        let index = next;
        next += 1;
        let key = row(items, index)?.get("id")?;
        out.set_key(&key.to_js_string()?, Js::Bool(true))?;
    }
    Ok(out)
}

fn find(items: &Js, id: &Js, index: Option<&Js>) -> Result<Option<usize>, Thrown> {
    if let Some(index) = index {
        let hit = index.get_key(&id.to_js_string()?)?;
        return Ok(hit.as_f64().filter(|n| *n >= 0.0).map(|n| n as usize));
    }
    // findIndex visits holes, unlike forEach/filter.
    let count = length(items)?;
    for at in 0..count {
        if row(items, at)?.get("id")?.strict_equals(id) {
            return Ok(Some(at));
        }
    }
    Ok(None)
}

fn find_selector(items: &Js, selector: &Js) -> Result<Option<usize>, Thrown> {
    let count = length(items)?;
    for at in 0..count {
        if row(items, at)?
            .get("id")?
            .strict_equals(&selector.get("id")?)
        {
            return Ok(Some(at));
        }
    }
    Ok(None)
}

fn pair(value: &Js) -> Result<(Js, Js), Thrown> {
    let iterator = IteratorRecord::get(value)?;
    let first = iterator.step_value()?.unwrap_or(Js::Undefined);
    let second = iterator.step_value()?.unwrap_or(Js::Undefined);
    iterator.close()?;
    Ok((first, second))
}

fn strip_info(value: &Js, test_truthy: bool) -> Result<Js, Thrown> {
    let strip = if test_truthy {
        value.get("info")?.truthy()
    } else {
        crate::js_runtime::library::in_operator(value, "info")?
    };
    if !strip {
        return Ok(value.clone());
    }
    // Both source destructurings bind info before object rest excludes it.
    value.get("info")?;
    // Object rest excludes info before reading values, including its getter.
    let out = Js::object(vec![]);
    for key in value.own_keys() {
        if key.as_ref() != utf16("info").as_ref() && value.own_enumerability(&key)? == Some(true) {
            out.define_key(&key, value.get_key(&key)?)?;
        }
    }
    Ok(out)
}

impl Graph {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(initial_state()),
            maps: std::array::from_fn(|_| RefCell::new(Js::object(vec![]))),
            ticker_index: Js::new_map(),
            market_index: Js::new_map(),
            terminal_orders: Js::new_map(),
            listeners: Rc::default(),
            market_version: Cell::new(0.0),
            ticker_version: Cell::new(0.0),
            now: RefCell::new(std::sync::Arc::new(|| {
                <&'static safe_cex_store::virtual_clock::VirtualClock>::default()
                    .get_current_time_ms()
            })),
        }
    }

    pub fn state(&self) -> Js {
        self.state.borrow().clone()
    }

    pub fn subscribe_typed(
        &self,
        callback: std::sync::Arc<
            dyn Fn(
                &safe_cex_types::StoreData,
                bool,
            ) -> Result<(), safe_cex_types::StoreSubscriberError>,
        >,
    ) -> Box<dyn Fn()> {
        let mut listeners = self.listeners.borrow_mut();
        if !listeners.slots.iter().any(|(_, entry)| {
            matches!(entry,
            Some(Subscriber::Typed(old)) if std::sync::Arc::ptr_eq(old, &callback))
        }) {
            let id = listeners.next;
            listeners.next += 1;
            listeners
                .slots
                .push((id, Some(Subscriber::Typed(callback.clone()))));
        }
        drop(listeners);
        let listeners = self.listeners.clone();
        Box::new(move || {
            for (_, entry) in &mut listeners.borrow_mut().slots {
                if matches!(entry, Some(Subscriber::Typed(old)) if std::sync::Arc::ptr_eq(old, &callback))
                {
                    *entry = None;
                }
            }
        })
    }

    fn subscribe_native(&self, callback: Js) -> Js {
        let mut listeners = self.listeners.borrow_mut();
        if !listeners.slots.iter().any(|(_, entry)| {
            matches!(entry,
            Some(Subscriber::Native(old)) if old.strict_equals(&callback))
        }) {
            let id = listeners.next;
            listeners.next += 1;
            listeners
                .slots
                .push((id, Some(Subscriber::Native(callback.clone()))));
        }
        drop(listeners);
        let listeners = self.listeners.clone();
        Js::function("unsubscribe", move |_, _| {
            let mut removed = false;
            for (_, entry) in &mut listeners.borrow_mut().slots {
                if matches!(entry, Some(Subscriber::Native(old)) if old.strict_equals(&callback)) {
                    *entry = None;
                    removed = true;
                }
            }
            Ok(Js::Bool(removed))
        })
    }

    pub fn facade(self: &Rc<Self>) -> Result<Js, Thrown> {
        let out = Js::host_object(self.clone());
        for field in [
            "latency",
            "balance",
            "markets",
            "tickers",
            "orders",
            "ordersRisk",
            "positions",
            "displayedPositions",
            "displayedMargin",
            "activeChasers",
            "activeTWAPs",
            "loaded",
            "updateIds",
            "options",
            "messageHandlerStats",
            "starkConfig",
            "zkLinkConfig",
            "userData",
            "fetchedLeverage",
            "socketHealth",
            "requestStats",
            "tickersById",
            "tickersBySymbol",
            "marketsById",
            "marketsBySymbol",
        ] {
            let graph = self.clone();
            out.define_accessor(field, Some(Rc::new(move |_| graph.get(field))), None)?;
        }
        for method in [
            "update",
            "reset",
            "subscribe",
            "serialize",
            "getMarketsVersion",
            "getTickersVersion",
            "updateTicker",
            "updateMarket",
            "updateOrder",
            "updateOrderStatus",
            "updateOrdersStatus",
            "markOrdersTerminal",
            "updatePosition",
            "updatePositions",
            "addOrder",
            "addOrUpdateOrder",
            "addOrUpdateOrders",
            "addOrUpdateOrdersFull",
            "addOrUpdatePosition",
            "addOrUpdatePositions",
            "addOrUpdatePositionsFull",
            "setOrdersForSymbol",
            "removeOrder",
            "removeOrders",
            "setSetting",
            "updateStarkConfig",
            "updateZkLinkConfig",
            "updateUserData",
            "updateSocketHealth",
            "recordRequestOutcome",
            "auditPositions",
            "createEmptyPositionFrom",
            "applyTickerDeltas",
            "applyPackedTickerDeltas",
            "consumeTickerDeltas",
        ] {
            let graph = self.clone();
            out.define(
                method,
                Js::function(method, move |_, args| graph.invoke(method, &args)),
            )?;
        }
        Ok(out)
    }

    pub fn invoke(&self, method: &str, args: &[Js]) -> Result<Js, Thrown> {
        let a = arg(args, 0);
        let b = arg(args, 1);
        match method {
            "update" => self.update(&a, b.truthy())?,
            "reset" => self.reset()?,
            "subscribe" => return Ok(self.subscribe_native(a)),
            "serialize" => return self.serialize(),
            "getMarketsVersion" => return Ok(Js::Number(self.market_version.get())),
            "getTickersVersion" => return Ok(Js::Number(self.ticker_version.get())),
            "updateTicker" => self.update_ticker(&a, &b)?,
            "updateMarket" => self.update_market(&a, &b)?,
            "updateOrder" => self.update_one("orders", &a, &b)?,
            "updateOrderStatus" => {
                self.update_one("orders", &a, &Js::object(vec![("status", b)]))?
            }
            "updateOrdersStatus" => self.update_statuses(&a, &b)?,
            "markOrdersTerminal" => self.mark_terminal(&a, &b)?,
            "updatePosition" => self.update_one("positions", &a, &b)?,
            "updatePositions" => self.update_positions(&a)?,
            "addOrder" => self.add_one("orders", &a, false)?,
            "addOrUpdateOrder" => self.add_one("orders", &a, true)?,
            "addOrUpdatePosition" => self.add_one("positions", &a, true)?,
            "addOrUpdateOrders" => self.upsert_many("orders", &a, false)?,
            "addOrUpdatePositions" => self.upsert_many("positions", &a, false)?,
            "addOrUpdateOrdersFull" => self.upsert_many("orders", &a, true)?,
            "addOrUpdatePositionsFull" => self.upsert_many("positions", &a, true)?,
            "setOrdersForSymbol" => self.set_orders_for_symbol(&a, &b)?,
            "removeOrder" => self.remove_one(&a)?,
            "removeOrders" => self.remove_many(&a)?,
            "setSetting" => self.set_setting(&a, b)?,
            "updateStarkConfig" => self.replace_field("starkConfig", a)?,
            "updateZkLinkConfig" => self.replace_field("zkLinkConfig", a)?,
            "updateUserData" => self.replace_field("userData", a)?,
            "updateSocketHealth" => self.update_socket_health(&a)?,
            "recordRequestOutcome" => {
                let stats = self.get("requestStats")?;
                if stats.truthy() {
                    let key = a.to_js_string()?;
                    stats.set_key(&key, ops::add(&stats.get_key(&key)?, &Js::Number(1.0))?)?;
                }
            }
            "auditPositions" => self.audit_positions(a.truthy())?,
            "createEmptyPositionFrom" => return Self::empty_position(&a),
            "applyTickerDeltas" => self.apply_deltas(&a, b.truthy())?,
            "applyPackedTickerDeltas" => self.apply_packed(&a, b.truthy())?,
            "consumeTickerDeltas" => return Ok(Js::array(vec![])),
            _ => return Err(Thrown::type_error("Unknown native Store method")),
        }
        Ok(Js::Undefined)
    }

    pub fn get(&self, key: &str) -> Result<Js, Thrown> {
        let map = match key {
            "tickersById" => Some(0),
            "tickersBySymbol" => Some(1),
            "marketsById" => Some(2),
            "marketsBySymbol" => Some(3),
            _ => None,
        };
        if let Some(at) = map {
            return Ok(self.maps[at].borrow().clone());
        }
        self.state().get(key)
    }

    pub fn market_for_id(&self, id: &Js) -> Result<Js, Thrown> {
        let index = self.market_index.call_method("get", vec![id.clone()])?;
        if index.is_undefined() {
            Ok(Js::Undefined)
        } else {
            self.get("markets")?.get_property(&index)
        }
    }

    pub fn find_row(&self, key: &str, id: &Js) -> Result<Js, Thrown> {
        let items = self.get(key)?;
        match find(&items, id, None)? {
            Some(index) => row(&items, index),
            None => Ok(Js::Undefined),
        }
    }

    fn set(&self, key: &str, value: Js) -> Result<(), Thrown> {
        self.state().set(key, value)
    }

    fn bump(&self, key: &str) -> Result<(), Thrown> {
        let ids = self.get("updateIds")?;
        let next = ops::add(&ids.get(key)?, &Js::Number(1.0))?;
        ids.set(key, next)
    }

    fn map(&self, at: usize) -> Js {
        self.maps[at].borrow().clone()
    }

    fn refresh(&self, key: &str, item: &Js) -> Result<(), Thrown> {
        let base = if key == "tickers" { 0 } else { 2 };
        // Capture each computed assignment's base before invoking id/symbol.
        let by_id = self.map(base);
        let id = item.get("id")?;
        by_id.set_key(&id.to_js_string()?, item.clone())?;
        let by_symbol = self.map(base + 1);
        let symbol = item.get("symbol")?;
        by_symbol.set_key(&symbol.to_js_string()?, item.clone())
    }

    fn update_in_array(&self, key: &str, index: Option<usize>, changes: &Js) -> Result<(), Thrown> {
        if let Some(index) = index {
            let state = self.state();
            if state.has_property(&utf16(key)) {
                let item = row(&state.get(key)?, index)?;
                patch(&item, changes)?;
                if matches!(key, "tickers" | "markets") {
                    self.refresh(key, &row(&self.get(key)?, index)?)?;
                }
            }
        }
        Ok(())
    }

    pub fn update_ticker(&self, selector: &Js, changes: &Js) -> Result<(), Thrown> {
        let map = self.map(0);
        let id = selector.get("id")?;
        let item = map.get_key(&id.to_js_string()?)?;
        if !item.truthy() {
            return Ok(());
        }
        let changes = strip_info(changes, false)?;
        if !patch(&item, &changes)? {
            return Ok(());
        }
        self.refresh("tickers", &item)?;
        self.notify(false)
    }

    pub fn update_market(&self, selector: &Js, changes: &Js) -> Result<(), Thrown> {
        let id = selector.get("id")?;
        let index = self.market_index.call_method("get", vec![id])?;
        if let Some(index) = index.as_f64() {
            self.update_in_array("markets", Some(index as usize), changes)?;
            self.market_version.set(self.market_version.get() + 1.0);
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn update_one(&self, key: &str, selector: &Js, changes: &Js) -> Result<(), Thrown> {
        let items = self.get(key)?;
        if let Some(index) = find_selector(&items, selector)? {
            self.update_in_array(key, Some(index), changes)?;
            self.bump(key)?;
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn set_setting(&self, key: &Js, value: Js) -> Result<(), Thrown> {
        let options = self.get("options")?;
        let key = key.to_js_string()?;
        if !options.get_key(&key)?.strict_equals(&value) {
            self.get("options")?.set_key(&key, value)?;
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn replace_field(&self, key: &str, value: Js) -> Result<(), Thrown> {
        self.set(key, value)?;
        self.notify(false)
    }

    pub fn update_socket_health(&self, value: &Js) -> Result<(), Thrown> {
        let target = self.get("socketHealth")?.spread_object()?;
        // Spread defines own data properties rather than invoking inherited setters.
        for (key, value) in value.own_entries()? {
            target.define_key(&key, value)?;
        }
        self.set("socketHealth", target)?;
        self.notify(false)
    }

    pub fn record_request_outcome(&self, key: &str) -> Result<(), Thrown> {
        let stats = self.get("requestStats")?;
        if stats.truthy() {
            stats.set(key, ops::add(&stats.get(key)?, &Js::Number(1.0))?)?;
        }
        Ok(())
    }

    pub fn reset(&self) -> Result<(), Thrown> {
        *self.state.borrow_mut() = initial_state();
        self.terminal_orders.call_method("clear", vec![])?;
        self.ticker_index.call_method("clear", vec![])?;
        self.market_index.call_method("clear", vec![])?;
        self.market_version.set(0.0);
        self.ticker_version.set(0.0);
        for index in [0, 2, 3, 1] {
            *self.maps[index].borrow_mut() = Js::object(vec![]);
        }
        self.notify(false)
    }

    pub fn serialize(&self) -> Result<Js, Thrown> {
        let out = Js::object(vec![]);
        out.define("latency", self.get("latency")?)?;
        out.define("balance", self.get("balance")?.spread_object()?)?;
        for key in ["markets", "tickers", "positions", "displayedPositions"] {
            out.define(key, Js::array(self.get(key)?.iterable_values()?))?;
        }
        let margin = Js::object(vec![]);
        for key in ["longExposure", "shortExposure", "totalNotional"] {
            let object = self.get("displayedMargin")?;
            let field = if object.is_nullish() {
                Js::Undefined
            } else {
                object.get(key)?
            };
            margin.define(
                key,
                if field.is_nullish() {
                    Js::Number(0.0)
                } else {
                    field
                },
            )?;
        }
        out.define("displayedMargin", margin)?;
        out.define("ordersRisk", self.get("ordersRisk")?.spread_object()?)?;
        for key in ["activeChasers", "activeTWAPs", "orders"] {
            out.define(key, Js::array(self.get(key)?.iterable_values()?))?;
        }
        for key in ["starkConfig", "messageHandlerStats", "requestStats"] {
            let value = if self.get(key)?.truthy() {
                self.get(key)?.spread_object()?
            } else {
                Js::Undefined
            };
            out.define(key, value)?;
        }
        out.define(
            "fetchedLeverage",
            self.get("fetchedLeverage")?.spread_object()?,
        )?;
        for (parent, fields) in [
            (
                "loaded",
                &["balance", "orders", "markets", "tickers", "positions"][..],
            ),
            ("options", &["isHedged"][..]),
            ("updateIds", &["positions", "orders"][..]),
        ] {
            let value = Js::object(vec![]);
            for field in fields {
                value.define(field, self.get(parent)?.get(field)?)?;
            }
            out.define(parent, value)?;
        }
        let health = if self.get("socketHealth")?.truthy() {
            self.get("socketHealth")?.spread_object()?
        } else {
            Js::Undefined
        };
        out.define("socketHealth", health)?;
        Ok(out)
    }

    fn rebuild(&self, key: &str, values: &Js) -> Result<(), Thrown> {
        let (base, index) = if key == "tickers" {
            (0, &self.ticker_index)
        } else {
            (2, &self.market_index)
        };
        if key == "tickers" {
            self.ticker_version.set(self.ticker_version.get() + 1.0);
        } else {
            self.market_version.set(self.market_version.get() + 1.0);
        }
        index.call_method("clear", vec![])?;
        *self.maps[base].borrow_mut() = Js::object(vec![]);
        *self.maps[base + 1].borrow_mut() = Js::object(vec![]);
        let count = length(values)?;
        for at in 0..count {
            if !values.has_property(&utf16(&at.to_string())) {
                continue;
            }
            let item = row(values, at)?;
            index.call_method("set", vec![item.get("id")?, Js::Number(at as f64)])?;
            self.refresh(key, &item)?;
        }
        Ok(())
    }

    fn worker_patch(&self, key: &str, incoming: &Js) -> Result<Option<bool>, Thrown> {
        if length(incoming)? == 0 || length(incoming)? != length(&self.get(key)?)? {
            return Ok(None);
        }
        let mut next = 0;
        while next < length(incoming)? {
            let at = next;
            next += 1;
            let current = row(&self.get(key)?, at)?;
            let incoming = row(incoming, at)?;
            if !current.get("id")?.strict_equals(&incoming.get("id")?)
                || !current
                    .get("symbol")?
                    .strict_equals(&incoming.get("symbol")?)
            {
                return Ok(None);
            }
        }
        let mut changed = false;
        let mut next = 0;
        while next < length(incoming)? {
            let at = next;
            next += 1;
            let current = row(&self.get(key)?, at)?;
            let incoming = row(incoming, at)?;
            if key == "tickers" {
                for field in TICKER_FIELDS {
                    if !current.get(field)?.strict_equals(&incoming.get(field)?) {
                        current.set(field, incoming.get(field)?)?;
                        changed = true;
                    }
                }
            } else {
                let mut keys = ForInRecord::get(&incoming);
                while let Some(key) = keys.next_key()? {
                    let next = incoming.get_key(&key)?;
                    if !current.get_key(&key)?.strict_equals(&next) {
                        current.set_key(&key, next)?;
                        changed = true;
                    }
                }
            }
        }
        Ok(Some(changed))
    }

    fn packed_in_place(&self, values: &Js) -> Result<bool, Thrown> {
        if length(values)? == 0 {
            return Ok(false);
        }
        let mut changed = false;
        let mut next = 0;
        while next < length(values)? {
            let at = next;
            next += 1;
            let delta = row(values, at)?;
            let index = delta.get("0")?;
            let mask = delta.get("1")?;
            if ops::less_than(&index, &Js::Number(0.0))?
                || ops::greater_equal(&index, &self.get("tickers")?.get("length")?)?
                || mask.strict_equals(&Js::Number(0.0))
            {
                continue;
            }
            let ticker = self.get("tickers")?.get_property(&index)?;
            let mask = mask.to_number()?.trunc().rem_euclid(4_294_967_296.0) as u32;
            let mut cursor = 2;
            for (bit, field) in TICKER_FIELDS.iter().enumerate() {
                if mask & (1 << bit) != 0 {
                    ticker.set(field, delta.get(&cursor.to_string())?)?;
                    cursor += 1;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }

    pub fn apply_packed(&self, values: &Js, worker: bool) -> Result<(), Thrown> {
        if self.packed_in_place(values)? {
            self.notify(worker)?;
        }
        Ok(())
    }

    pub fn apply_deltas(&self, values: &Js, worker: bool) -> Result<(), Thrown> {
        if length(values)? == 0 {
            return Ok(());
        }
        let mut changed = false;
        let mut next = 0;
        while next < length(values)? {
            let at = next;
            next += 1;
            let delta = row(values, at)?;
            let index = delta.get("0")?;
            if ops::less_equal(&index, &Js::Number(-1.0))?
                || ops::greater_equal(&index, &self.get("tickers")?.get("length")?)?
            {
                continue;
            }
            let changes = strip_info(&delta.get("1")?, false)?;
            if ForInRecord::get(&changes).next_key()?.is_none() {
                continue;
            }
            // Existing legacy delta indices retain their typed finite-index admission.
            let index = index.to_number()?;
            if index.fract() != 0.0 || !index.is_finite() || index < 0.0 {
                return Err(Thrown::type_error("ticker delta index must address a row"));
            }
            self.update_in_array("tickers", Some(index as usize), &changes)?;
            changed = true;
        }
        if changed {
            self.notify(worker)?;
        }
        Ok(())
    }

    pub fn update(&self, changes: &Js, worker: bool) -> Result<(), Thrown> {
        let mut normalized = changes.clone();
        let mutable = |normalized: &mut Js| -> Result<(), Thrown> {
            if normalized.same_reference(changes) {
                *normalized = changes.spread_object()?;
            }
            Ok(())
        };
        let mut patched_tickers = false;
        let mut patched_markets = false;
        let packed = changes.get("tickerDeltas")?;
        if !packed.is_undefined() {
            mutable(&mut normalized)?;
            normalized.delete(&utf16("tickers"))?;
            mutable(&mut normalized)?;
            normalized.delete(&utf16("tickerDeltas"))?;
        }
        if changes.get("tickers")?.truthy() && packed.is_undefined() {
            let patched = if worker {
                self.worker_patch("tickers", &changes.get("tickers")?)?
            } else {
                None
            };
            if let Some(changed) = patched {
                mutable(&mut normalized)?;
                normalized.delete(&utf16("tickers"))?;
                patched_tickers = changed;
            } else {
                // Object literal spread is evaluated before the explicit RHS.
                normalized = changes.spread_object()?;
                let input = changes.get("tickers")?;
                let mapped = input.call_method(
                    "map",
                    vec![Js::function("stripTickerInfo", |_, args| {
                        strip_info(&arg(&args, 0), true)
                    })],
                )?;
                normalized.define("tickers", mapped)?;
            }
        }
        if worker
            && normalized.get("markets")?.truthy()
            && let Some(changed) = self.worker_patch("markets", &normalized.get("markets")?)?
        {
            mutable(&mut normalized)?;
            normalized.delete(&utf16("markets"))?;
            patched_markets = changed;
        }
        let raw_ids = changes.get("updateIds")?;
        let optional = |value: &Js, key: &str| -> Result<Js, Thrown> {
            if value.is_nullish() {
                Ok(Js::Undefined)
            } else {
                value.get(key)
            }
        };
        let worker_order_id = worker && !optional(&raw_ids, "orders")?.is_undefined();
        let worker_position_id = worker && !optional(&raw_ids, "positions")?.is_undefined();
        if raw_ids.truthy() {
            mutable(&mut normalized)?;
            let merged = self.get("updateIds")?.spread_object()?;
            for (key, value) in raw_ids.own_entries()? {
                merged.define_key(&key, value)?;
            }
            normalized.set("updateIds", merged)?;
        }
        let incoming_ids = normalized.get("updateIds")?;
        let order_id = optional(&incoming_ids, "orders")?;
        let position_id = optional(&incoming_ids, "positions")?;
        if worker_order_id && order_id.strict_equals(&self.get("updateIds")?.get("orders")?) {
            mutable(&mut normalized)?;
            normalized.delete(&utf16("orders"))?;
        }
        if worker_position_id
            && position_id.strict_equals(&self.get("updateIds")?.get("positions")?)
        {
            mutable(&mut normalized)?;
            normalized.delete(&utf16("positions"))?;
        }
        if worker && incoming_ids.truthy() {
            let order = if order_id.is_nullish() {
                self.get("updateIds")?.get("orders")?
            } else {
                order_id
            };
            if order.strict_equals(&self.get("updateIds")?.get("orders")?) {
                let position = if position_id.is_nullish() {
                    self.get("updateIds")?.get("positions")?
                } else {
                    position_id
                };
                if position.strict_equals(&self.get("updateIds")?.get("positions")?) {
                    mutable(&mut normalized)?;
                    normalized.delete(&utf16("updateIds"))?;
                }
            }
        }
        let patched_packed = if packed.truthy() {
            self.packed_in_place(&packed)?
        } else {
            false
        };
        if !patched_tickers
            && !patched_markets
            && !patched_packed
            && ForInRecord::get(&normalized).next_key()?.is_none()
        {
            return Ok(());
        }
        if !worker && normalized.get("orders")?.truthy() {
            let orders = self.keep_terminal(&normalized.get("orders")?)?;
            if !orders.same_reference(&normalized.get("orders")?) {
                mutable(&mut normalized)?;
                normalized.set("orders", orders)?;
            }
        }
        assign(&self.state(), &normalized)?;
        if normalized.get("positions")?.truthy() {
            self.audit_positions(true)?;
        }
        if !normalized.get("orders")?.is_undefined() && !worker_order_id {
            self.bump("orders")?;
        }
        if !normalized.get("positions")?.is_undefined() && !worker_position_id {
            self.bump("positions")?;
        }
        for key in ["tickers", "markets"] {
            if normalized.get(key)?.truthy() {
                self.rebuild(key, &normalized.get(key)?)?;
            }
        }
        self.notify(worker)
    }

    pub fn audit_positions(&self, remove_dupes: bool) -> Result<(), Thrown> {
        let positions = self.get("positions")?;
        let unique =
            crate::js_runtime::library::uniq_by(positions.iterable_values()?, &Js::str("id"))?;
        if length(&unique)? != length(&self.get("positions")?)? {
            eprintln!("Duplicate positions detected...");
            if remove_dupes {
                eprintln!("Removing duplicate positions");
                self.set("positions", unique.clone())?;
            }
        }
        let mut negative = false;
        for at in 0..length(&unique)? {
            if ops::less_than(&row(&unique, at)?.get("contracts")?, &Js::Number(0.0))? {
                negative = true;
            }
        }
        if negative {
            eprintln!("Negative contracts detected");
        }
        Ok(())
    }

    fn terminal(status: &Js) -> bool {
        status.strict_equals(&Js::str("closed")) || status.strict_equals(&Js::str("canceled"))
    }

    fn remembered(&self, id: Js, now: f64) -> Result<bool, Thrown> {
        let marked = self.terminal_orders.call_method("get", vec![id.clone()])?;
        if marked.is_undefined() {
            return Ok(false);
        }
        if now - marked.to_number()? < 60_000.0 {
            return Ok(true);
        }
        self.terminal_orders.call_method("delete", vec![id])?;
        Ok(false)
    }

    fn keep_terminal(&self, incoming: &Js) -> Result<Js, Thrown> {
        if self.terminal_orders.get("size")?.to_number()? == 0.0 {
            return Ok(incoming.clone());
        }
        let now = {
            let clock = self.now.borrow().clone();
            clock()
        };
        let mut kept: Option<Js> = None;
        for at in 0..length(incoming)? {
            if !incoming.has_property(&utf16(&at.to_string())) {
                continue;
            }
            let order = row(incoming, at)?;
            let reopens =
                !Self::terminal(&order.get("status")?) && self.remembered(order.get("id")?, now)?;
            if reopens && kept.is_none() {
                kept = Some(
                    incoming.call_method("slice", vec![Js::Number(0.0), Js::Number(at as f64)])?,
                );
            }
            let Some(ref out) = kept else {
                continue;
            };
            if !reopens {
                out.call_method("push", vec![order])?;
                continue;
            }
            let current = self.get("orders")?;
            for index in 0..length(&current)? {
                let candidate = row(&current, index)?;
                if candidate.get("id")?.strict_equals(&order.get("id")?) {
                    if Self::terminal(&candidate.get("status")?) {
                        out.call_method("push", vec![candidate])?;
                    }
                    break;
                }
            }
        }
        Ok(kept.unwrap_or_else(|| incoming.clone()))
    }

    pub fn mark_terminal(&self, selectors: &Js, status: &Js) -> Result<(), Thrown> {
        let now = {
            let clock = self.now.borrow().clone();
            clock()
        };
        for at in 0..length(selectors)? {
            if !selectors.has_property(&utf16(&at.to_string())) {
                continue;
            }
            let id = row(selectors, at)?.get("id")?;
            self.terminal_orders
                .call_method("delete", vec![id.clone()])?;
            self.terminal_orders
                .call_method("set", vec![id, Js::Number(now)])?;
        }
        let ids = self
            .terminal_orders
            .call_method("keys", vec![])?
            .iterable_values()?;
        for id in ids {
            if self.terminal_orders.get("size")?.to_number()? <= 1_000.0 {
                break;
            }
            self.terminal_orders.call_method("delete", vec![id])?;
        }
        self.update_statuses(selectors, status)
    }

    pub fn update_statuses(&self, selectors: &Js, status: &Js) -> Result<(), Thrown> {
        if length(selectors)? == 0 || length(&self.get("orders")?)? == 0 {
            return Ok(());
        }
        let ids = id_lookup(selectors)?;
        let mut updated = false;
        let mut at = 0;
        while at < length(&self.get("orders")?)? {
            let item = row(&self.get("orders")?, at)?;
            if ids
                .get_property(&item.get("id")?)?
                .strict_equals(&Js::Bool(true))
            {
                self.update_in_array(
                    "orders",
                    Some(at),
                    &Js::object(vec![("status", status.clone())]),
                )?;
                updated = true;
            }
            at += 1;
        }
        if updated {
            self.bump("orders")?;
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn remove_one(&self, selector: &Js) -> Result<(), Thrown> {
        let items = self.get("orders")?;
        for at in 0..length(&items)? {
            if row(&items, at)?
                .get("id")?
                .strict_equals(&selector.get("id")?)
            {
                self.get("orders")?
                    .call_method("splice", vec![Js::Number(at as f64), Js::Number(1.0)])?;
                self.bump("orders")?;
                self.notify(false)?;
                break;
            }
        }
        Ok(())
    }

    pub fn remove_many(&self, selectors: &Js) -> Result<(), Thrown> {
        if length(selectors)? == 0 || length(&self.get("orders")?)? == 0 {
            return Ok(());
        }
        let ids = id_lookup(selectors)?;
        let previous = length(&self.get("orders")?)?;
        let orders = self.get("orders")?;
        let filtered = orders.call_method(
            "filter",
            vec![Js::function("keepOrder", move |_, args| {
                Ok(Js::Bool(
                    !ids.get_property(&arg(&args, 0).get("id")?)?
                        .strict_equals(&Js::Bool(true)),
                ))
            })],
        )?;
        self.set("orders", filtered)?;
        if length(&self.get("orders")?)? != previous {
            self.bump("orders")?;
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn add_one(&self, key: &str, value: &Js, upsert: bool) -> Result<(), Thrown> {
        if key == "orders" {
            let kept = self.keep_terminal(&Js::array(vec![value.clone()]))?;
            if !kept.get("0")?.strict_equals(value) {
                return Ok(());
            }
        }
        let index = if upsert {
            find_selector(&self.get(key)?, value)?
        } else {
            None
        };
        if let Some(index) = index {
            self.update_in_array(key, Some(index), value)?;
        } else {
            let value = if key == "positions" {
                value.spread_object()?
            } else {
                value.clone()
            };
            self.get(key)?.call_method("push", vec![value])?;
        }
        if key == "positions" {
            self.audit_positions(false)?;
        }
        self.bump(key)?;
        self.notify(false)
    }

    pub fn upsert_many(&self, key: &str, incoming: &Js, full: bool) -> Result<(), Thrown> {
        let values = if key == "orders" {
            self.keep_terminal(incoming)?
        } else {
            incoming.clone()
        };
        if !full && length(&values)? == 0 {
            return Ok(());
        }
        if full && key == "orders" {
            self.set(key, values)?;
        } else {
            let index = if length(&values)? > 1 && length(&self.get(key)?)? > 1 {
                Some(id_index(&self.get(key)?)?)
            } else {
                None
            };
            let used = Js::new_set();
            for at in 0..length(&values)? {
                if !values.has_property(&utf16(&at.to_string())) {
                    continue;
                }
                let value = row(&values, at)?;
                if full {
                    used.call_method("add", vec![value.get("id")?])?;
                }
                let hit = find(&self.get(key)?, &value.get("id")?, index.as_ref())?;
                if let Some(hit) = hit {
                    self.update_in_array(key, Some(hit), &value)?;
                } else {
                    if let Some(ref index) = index {
                        index.set_key(
                            &value.get("id")?.to_js_string()?,
                            self.get(key)?.get("length")?,
                        )?;
                    }
                    let value = if key == "positions" {
                        value.spread_object()?
                    } else {
                        value
                    };
                    self.get(key)?.call_method("push", vec![value])?;
                }
            }
            if full {
                let positions = self.get("positions")?.call_method(
                    "map",
                    vec![Js::function("emptyUnannounced", move |_, args| {
                        let value = arg(&args, 0);
                        if used.call_method("has", vec![value.get("id")?])?.truthy() {
                            Ok(value)
                        } else {
                            Self::empty_position(&value)
                        }
                    })],
                )?;
                self.set("positions", positions)?;
            } else if key == "positions" {
                self.audit_positions(false)?;
            }
        }
        self.bump(key)?;
        self.notify(false)
    }

    pub fn empty_position(value: &Js) -> Result<Js, Thrown> {
        let out = value.spread_object()?;
        for field in [
            "entryPrice",
            "notional",
            "unrealizedPnl",
            "contracts",
            "liquidationPrice",
            "marginSize",
        ] {
            out.define(field, Js::Number(0.0))?;
        }
        out.define("info", Js::Undefined)?;
        out.define("extra", Js::Undefined)?;
        Ok(out)
    }

    pub fn update_positions(&self, updates: &Js) -> Result<(), Thrown> {
        if length(updates)? == 0 || length(&self.get("positions")?)? == 0 {
            return Ok(());
        }
        let index = if length(updates)? > 1 && length(&self.get("positions")?)? > 1 {
            Some(id_index(&self.get("positions")?)?)
        } else {
            None
        };
        let mut selected = vec![];
        for at in 0..length(updates)? {
            if !updates.has_property(&utf16(&at.to_string())) {
                continue;
            }
            let (selector, changes) = pair(&row(updates, at)?)?;
            selected.push((
                find(
                    &self.get("positions")?,
                    &selector.get("id")?,
                    index.as_ref(),
                )?,
                changes,
            ));
        }
        if selected.iter().any(|(index, _)| index.is_some()) {
            for (index, changes) in selected {
                self.update_in_array("positions", index, &changes)?;
            }
            self.bump("positions")?;
            self.notify(false)?;
        }
        Ok(())
    }

    pub fn set_orders_for_symbol(&self, symbol: &Js, incoming: &Js) -> Result<(), Thrown> {
        let kept = self.keep_terminal(incoming)?;
        let symbol = symbol.clone();
        let orders = self.get("orders")?.call_method(
            "filter",
            vec![Js::function("otherSymbol", move |_, args| {
                Ok(Js::Bool(
                    !arg(&args, 0).get("symbol")?.strict_equals(&symbol),
                ))
            })],
        )?;
        self.set("orders", orders)?;
        self.get("orders")?
            .call_method("push", kept.iterable_values()?)?;
        self.bump("orders")?;
        self.notify(false)
    }

    pub fn notify(&self, from_worker: bool) -> Result<(), Thrown> {
        let mut cursor = 0;
        loop {
            let next = {
                let listeners = self.listeners.borrow();
                listeners.slots.get(cursor).cloned()
            };
            let Some((id, callback)) = next else {
                break;
            };
            cursor += 1;
            match callback {
                Some(Subscriber::Native(callback)) => {
                    callback.call(Js::Undefined, vec![self.state(), Js::Bool(from_worker)])?;
                }
                Some(Subscriber::Typed(callback)) => {
                    let data = super::project(&self.state()).map_err(super::read_thrown)?;
                    callback(&data, from_worker).map_err(|cause| {
                        super::mutation_thrown(
                            safe_cex_store::StoreNotificationError::from_subscriber(id, cause)
                                .into(),
                        )
                    })?;
                }
                None => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "legacy_owner_tests.rs"]
mod legacy_owner_tests;
#[cfg(test)]
#[path = "legacy_request_tests.rs"]
mod legacy_request_tests;

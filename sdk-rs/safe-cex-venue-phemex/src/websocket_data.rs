//! Websocket reducers and retained subscription continuations over opaque hosts.
use safe_cex_num::subtract;
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};

pub trait WsHost: Clone + 'static {
    type V: Clone + 'static;
    type E: 'static;
    fn get(&mut self, value: &Self::V, key: &[u16]) -> Result<Self::V, Self::E>;
    fn optional(&mut self, value: &Self::V, key: &[u16]) -> Result<Self::V, Self::E>;
    fn set(&mut self, value: &Self::V, key: &[u16], new: Self::V) -> Result<(), Self::E>;
    fn remove(&mut self, value: &Self::V, key: &[u16]) -> Result<(), Self::E>;
    fn truthy(&mut self, value: &Self::V) -> bool;
    fn nullish(&mut self, value: &Self::V) -> bool;
    fn same(&mut self, a: &Self::V, b: &Self::V) -> bool;
    fn set_same(&mut self, a: &Self::V, b: &Self::V) -> bool;
    fn number(&mut self, value: Self::V) -> Result<f64, Self::E>;
    fn parse(&mut self, value: Self::V) -> Result<f64, Self::E>;
    fn text(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E>;
    fn lower(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E>;
    fn num(&mut self, value: f64) -> Self::V;
    fn string(&mut self, value: Vec<u16>) -> Self::V;
    fn undefined(&mut self) -> Self::V;
    fn boolean(&mut self, value: bool) -> Self::V;
    fn array(&mut self, values: Vec<Self::V>) -> Self::V;
    fn object(&mut self, values: Vec<(&'static str, Self::V)>) -> Self::V;
    fn spread(&mut self, value: &Self::V) -> Result<Self::V, Self::E>;
    fn rows(&mut self, value: Self::V) -> Result<Vec<Self::V>, Self::E>;
    fn values(&mut self, value: &Self::V) -> Result<Vec<Self::V>, Self::E>;
    fn at(&mut self, value: &Self::V, index: usize) -> Result<Self::V, Self::E>;
    fn is_array(&mut self, value: &Self::V) -> bool;
    fn store(&mut self, name: &'static str) -> Result<Self::V, Self::E>;
    fn field(&mut self, name: &'static str) -> Result<Self::V, Self::E>;
    fn call(&mut self, member: &'static str, args: Vec<Self::V>) -> Result<Self::V, Self::E>;
    fn read(
        &mut self,
        endpoint: &'static str,
        params: Self::V,
    ) -> Result<Pin<Box<dyn Future<Output = Result<Self::V, Self::E>>>>, Self::E>;
    fn spawn(
        &mut self,
        task: Pin<Box<dyn Future<Output = Result<(), Self::E>>>>,
    ) -> Result<(), Self::E>;
    fn timer(
        &mut self,
        delay: f64,
        task: Box<dyn FnOnce() -> Result<(), Self::E>>,
    ) -> Result<Self::V, Self::E>;
    fn error_text(&mut self, error: Self::E) -> Result<Vec<u16>, Self::E>;
}

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn get<H: WsHost>(h: &mut H, v: &H::V, key: &str) -> Result<H::V, H::E> {
    h.get(v, &u(key))
}
fn opt<H: WsHost>(h: &mut H, v: &H::V, key: &str) -> Result<H::V, H::E> {
    h.optional(v, &u(key))
}
fn is<H: WsHost>(h: &mut H, v: &H::V, s: &str) -> bool {
    let expected = h.string(u(s));
    h.same(v, &expected)
}
fn parse_field<H: WsHost>(h: &mut H, v: &H::V, name: &str) -> Result<f64, H::E> {
    let v = get(h, v, name)?;
    h.parse(v)
}
fn empty_object<H: WsHost>(h: &mut H) -> H::V {
    h.object(vec![])
}

pub struct PublicSocket<V> {
    pub has_warned_subscription_limit: bool,
    pub snapshot_error_log_at: V,
    pub message_handlers: V,
    pub topics: V,
    pub request_id: f64,
    pub kline_interval_by_resolution: Option<V>,
}
impl<V> PublicSocket<V> {
    pub const MAX_SUBSCRIPTIONS: f64 = 20.0;
    pub const SUBSCRIPTION_WARNING_THRESHOLD: f64 = 18.0;
    pub const SNAPSHOT_ERROR_LOG_INTERVAL_MS: f64 = 30_000.0;
    pub fn new<H: WsHost<V = V>>(h: &mut H) -> Result<Self, H::E> {
        let snapshot_error_log_at = empty_object(h);
        let callback = h.call("boundTicker", vec![])?;
        let callbacks = h.array(vec![callback]);
        let message_handlers = h.object(vec![("perp_market24h_pack_p.update", callbacks)]);
        Ok(Self {
            has_warned_subscription_limit: false,
            snapshot_error_log_at,
            message_handlers,
            topics: empty_object(h),
            request_id: 1200.0,
            kline_interval_by_resolution: None,
        })
    }
}
pub struct PrivateSocket<V> {
    pub request_id: f64,
    pub sync_timeout: Option<V>,
    pub sync_symbols: Vec<V>,
    pub sync_debounce_ms: f64,
}
impl<V> Default for PrivateSocket<V> {
    fn default() -> Self {
        Self {
            request_id: 1200.0,
            sync_timeout: None,
            sync_symbols: vec![],
            sync_debounce_ms: 1000.0,
        }
    }
}

pub fn map_packed_ticker_changes<H: WsHost>(h: &mut H, input: H::V) -> Result<H::V, H::E> {
    let last = parse_field(h, &input, "lastRp")?;
    let volume = parse_field(h, &input, "volumeRq")?;
    let open = parse_field(h, &input, "openRp")?;
    let mut fields = vec![
        ("last", h.num(last)),
        ("volume", h.num(volume)),
        ("quoteVolume", h.num(volume * last)),
        ("percentage", h.num(((last - open) / open) * 100.0)),
    ];
    for (name, raw) in [
        ("fundingRate", "fundingRateRr"),
        ("mark", "markRp"),
        ("index", "indexRp"),
        ("openInterest", "openInterestRv"),
    ] {
        let n = parse_field(h, &input, raw)?;
        fields.push((name, h.num(n)));
    }
    for name in ["bid", "ask"] {
        let v = get(h, &input, name)?;
        if h.truthy(&v) {
            let n = h.parse(v)?;
            fields.push((name, h.num(n)));
        }
    }
    Ok(h.object(fields))
}

pub fn handle_ticker_stream_events<H: WsHost>(h: &mut H, events: H::V) -> Result<(), H::E> {
    for event in h.rows(events)? {
        let data = opt(h, &event, "data")?;
        let length = opt(h, &data, "length")?;
        if !h.truthy(&length) {
            continue;
        }
        let data = opt(h, &event, "data")?;
        for row in h.rows(data)? {
            let raw = h.at(&row, 0)?;
            let symbol = h.call("formatIn", vec![raw])?;
            let tickers = h.store("tickersBySymbol")?;
            let key = h.text(symbol.clone())?;
            let ticker = h.get(&tickers, &key)?;
            let mut fields = vec![];
            for (name, index) in [
                ("ask", 13),
                ("bid", 12),
                ("fundingRateRr", 10),
                ("indexRp", 8),
                ("lastRp", 4),
                ("markRp", 9),
                ("openInterestRv", 7),
                ("openRp", 1),
                ("volumeRq", 5),
            ] {
                fields.push((name, h.at(&row, index)?));
            }
            let input = h.object(fields);
            let changes = h.call("mapPacked", vec![input])?;
            if h.truthy(&ticker) {
                h.call("updateTicker", vec![ticker, changes])?;
            } else {
                let markets = h.store("marketsBySymbol")?;
                let market = h.get(&markets, &key)?;
                if !h.truthy(&market) {
                    continue;
                }
                let tickers = h.store("tickers")?;
                let mut keep = vec![];
                for t in h.rows(tickers)? {
                    let old_symbol = get(h, &t, "symbol")?;
                    if !h.same(&old_symbol, &symbol) {
                        keep.push(t);
                    }
                }
                let mut fields = vec![("id", get(h, &market, "id")?), ("symbol", symbol)];
                for name in ["bid", "ask"] {
                    let v = get(h, &changes, name)?;
                    let v = if h.nullish(&v) {
                        get(h, &changes, "last")?
                    } else {
                        v
                    };
                    fields.push((name, v));
                }
                for name in [
                    "last",
                    "mark",
                    "index",
                    "percentage",
                    "openInterest",
                    "fundingRate",
                ] {
                    fields.push((name, get(h, &changes, name)?));
                }
                fields.push(("fundingTime", h.num(0.0)));
                for name in ["volume", "quoteVolume"] {
                    fields.push((name, get(h, &changes, name)?));
                }
                keep.push(h.object(fields));
                let keep = h.array(keep);
                let update = h.object(vec![("tickers", keep)]);
                h.call("update", vec![update])?;
            }
        }
    }
    Ok(())
}

fn status<H: WsHost>(h: &mut H, order: &H::V, optional: bool) -> Result<Vec<u16>, H::E> {
    let v = if optional {
        opt(h, order, "ordStatus")?
    } else {
        get(h, order, "ordStatus")?
    };
    let v = if optional {
        let v = if h.truthy(&v) { v } else { h.string(vec![]) };
        let text = h.text(v)?;
        h.string(text)
    } else {
        v
    };
    h.lower(v)
}
fn order_status<H: WsHost>(h: &mut H, raw: Vec<u16>) -> Result<H::V, H::E> {
    let raw = h.string(raw);
    let value = h.call("orderStatus", vec![raw])?;
    Ok(if h.truthy(&value) {
        value
    } else {
        h.string(u("canceled"))
    })
}
fn unique<H: WsHost>(h: &mut H, values: Vec<H::V>) -> Vec<H::V> {
    let mut out = vec![];
    for v in values {
        if !out.iter().any(|old| h.set_same(old, &v)) {
            out.push(v);
        }
    }
    out
}
pub fn private_on_message<H: WsHost>(h: &mut H, data: H::V) -> Result<(), H::E> {
    let disposed = h.field("disposed")?;
    if h.truthy(&disposed) {
        return Ok(());
    }
    h.call("received", vec![])?;
    h.call("diagnostic", vec![data.clone()])?;
    let json = h.call("parseFrame", vec![data])?;
    let id = opt(h, &json, "id")?;
    let auth_id = h.num(999.0);
    if h.same(&id, &auth_id) {
        let error = opt(h, &json, "error")?;
        if !h.truthy(&error) {
            let id = h.call("nextId", vec![])?;
            let method = h.string(u("aop_p.subscribe"));
            let params = h.array(vec![]);
            let request = h.object(vec![("method", method), ("params", params), ("id", id)]);
            h.call("sendWithLog", vec![request])?;
        } else {
            let message = h.string(u("Phemex websocket error - wrong api key or secret"));
            h.call("error", vec![message])?;
        }
    }
    let method = opt(h, &json, "method")?;
    if is(h, &method, "server.ping") {
        h.call("pong", vec![])?;
        return Ok(());
    }
    let result = opt(h, &json, "result")?;
    if is(h, &result, "pong") {
        h.call("pong", vec![])?;
        return Ok(());
    }
    let accounts = opt(h, &json, "accounts_p")?;
    let outer = if h.truthy(&accounts) {
        true
    } else {
        let orders = opt(h, &json, "orders_p")?;
        if h.truthy(&orders) {
            true
        } else {
            let p = opt(h, &json, "positions_p")?;
            h.truthy(&p)
        }
    };
    if !outer {
        return Ok(());
    }
    let orders = opt(h, &json, "orders_p")?;
    if h.truthy(&orders) {
        let orders = opt(h, &json, "orders_p")?;
        let type_ = opt(h, &json, "type")?;
        let snapshot = is(h, &type_, "snapshot");
        let mut terminal = vec![];
        if !snapshot {
            let mut selected = vec![];
            for order in h.rows(orders.clone())? {
                let s = status(h, &order, true)?;
                if ["filled", "triggered", "canceled", "rejected"]
                    .iter()
                    .any(|v| s == u(v))
                {
                    selected.push(order);
                }
            }
            for order in selected {
                let raw = get(h, &order, "symbol")?;
                let v = h.call("formatIn", vec![raw])?;
                terminal.push(v);
            }
            terminal.retain(|v| h.truthy(v));
            terminal = unique(h, terminal);
        }
        let kind = h.string(u("order"));
        h.call("stats", vec![kind])?;
        let mut partial_rows = vec![];
        for order in h.rows(orders.clone())? {
            let s = status(h, &order, true)?;
            if s == u("partiallyfilled") || s == u("partially_filled") {
                partial_rows.push(order);
            }
        }
        let mut partial_ids = vec![];
        for order in partial_rows {
            partial_ids.push(get(h, &order, "orderID")?);
        }
        partial_ids.retain(|v| h.truthy(v));
        let partial_ids = unique(h, partial_ids);
        let mut formatted = vec![];
        for order in h.rows(orders.clone())? {
            let raw = status(h, &order, false)?;
            let status_ = order_status(h, raw)?;
            if snapshot && !is(h, &status_, "open") {
                continue;
            }
            let mut mapping = order.clone();
            if is(h, &status_, "open") {
                for other in h.rows(orders.clone())? {
                    let a = get(h, &other, "orderID")?;
                    let b = get(h, &order, "orderID")?;
                    if h.same(&a, &b) {
                        let raw = status(h, &other, false)?;
                        let s = order_status(h, raw)?;
                        if !is(h, &s, "open") {
                            let raw = status(h, &other, false)?;
                            let _ = order_status(h, raw)?;
                            mapping = h.spread(&order)?;
                            let v = get(h, &other, "ordStatus")?;
                            h.set(&mapping, &u("ordStatus"), v)?;
                            break;
                        }
                    }
                }
            }
            let mapped = h.call("mapOrder", vec![mapping])?;
            if !snapshot {
                let mut candidates = vec![];
                for order in h.rows(mapped.clone())? {
                    let filled = get(h, &order, "filled")?;
                    if h.number(filled)? > 0.0 {
                        let s = get(h, &order, "status")?;
                        let include = if is(h, &s, "closed") {
                            true
                        } else {
                            let id = get(h, &order, "id")?;
                            partial_ids.iter().any(|p| h.set_same(p, &id))
                        };
                        if include {
                            candidates.push(order);
                        }
                    }
                }
                let mut seen = vec![];
                for order in candidates {
                    let id = get(h, &order, "id")?;
                    if seen.iter().any(|p| h.set_same(p, &id)) {
                        continue;
                    }
                    seen.push(id.clone());
                    let side = get(h, &order, "side")?;
                    let raw = get(h, &order, "symbol")?;
                    let symbol = h.call("formatIn", vec![raw])?;
                    let info = get(h, &order, "info")?;
                    let exec = opt(h, &info, "execPriceRp")?;
                    let raw = if h.truthy(&exec) {
                        exec
                    } else {
                        get(h, &order, "price")?
                    };
                    let price = h.parse(raw)?;
                    let price = h.num(price);
                    let amount = get(h, &order, "filled")?;
                    let fill = h.object(vec![
                        ("id", id),
                        ("side", side),
                        ("symbol", symbol),
                        ("price", price),
                        ("amount", amount),
                    ]);
                    h.call("fill", vec![fill])?;
                }
            }
            formatted.extend(h.rows(mapped)?);
        }
        let formatted = h.array(formatted);
        if snapshot {
            let loaded = h.store("loaded")?;
            let loaded = h.spread(&loaded)?;
            let yes = h.boolean(true);
            h.set(&loaded, &u("orders"), yes)?;
            let update = h.object(vec![("orders", formatted), ("loaded", loaded)]);
            h.call("update", vec![update])?;
        } else {
            h.call("upsertOrders", vec![formatted])?;
            if !terminal.is_empty() {
                let terminal = h.array(terminal);
                h.call("sync", vec![terminal])?;
            }
        }
    }
    let positions = opt(h, &json, "positions_p")?;
    if h.truthy(&positions) {
        let positions = opt(h, &json, "positions_p")?;
        let kind = h.string(u("position"));
        h.call("stats", vec![kind])?;
        let mut supported = vec![];
        for p in h.rows(positions)? {
            let markets = h.store("markets")?;
            for market in h.rows(markets)? {
                let symbol = get(h, &market, "symbol")?;
                let raw = get(h, &p, "symbol")?;
                let normalized = h.call("formatIn", vec![raw])?;
                if h.same(&symbol, &normalized) {
                    supported.push(p);
                    break;
                }
            }
        }
        let mut parsed = vec![];
        for p in supported {
            parsed.push(h.call("mapPosition", vec![p])?);
        }
        parsed.retain(|p| h.truthy(p));
        let mut upsert = vec![];
        for p in parsed {
            let contracts = get(h, &p, "contracts")?;
            let known = if h.number(contracts)? > 0.0 {
                true
            } else {
                let stored = h.store("positions")?;
                let mut found = false;
                for existing in h.rows(stored)? {
                    let a = get(h, &existing, "id")?;
                    let b = get(h, &p, "id")?;
                    if h.same(&a, &b) {
                        found = true;
                        break;
                    }
                }
                found
            };
            if known {
                upsert.push(p);
            }
        }
        if !upsert.is_empty() {
            let upsert = h.array(upsert);
            h.call("upsertPositions", vec![upsert])?;
        }
    }
    let accounts = opt(h, &json, "accounts_p")?;
    if h.truthy(&accounts) {
        let kind = h.string(u("account"));
        h.call("stats", vec![kind])?;
        let accounts = get(h, &json, "accounts_p")?;
        if !h.is_array(&accounts) {
            return Ok(());
        }
        let balance = h.store("balance")?;
        let old = get(h, &balance, "currencies")?;
        let old = if h.truthy(&old) { old } else { empty_object(h) };
        let currencies = h.spread(&old)?;
        let balance = h.store("balance")?;
        let old_info = get(h, &balance, "info")?;
        let infos = if h.truthy(&old_info) && !h.is_array(&old_info) {
            h.spread(&old_info)?
        } else {
            empty_object(h)
        };
        let accounts = get(h, &json, "accounts_p")?;
        for account in h.rows(accounts)? {
            let currency = opt(h, &account, "currency")?;
            if !h.truthy(&currency) {
                continue;
            }
            let positions = h.store("positions")?;
            let mut selected = vec![];
            for p in h.rows(positions)? {
                let markets = h.store("marketsBySymbol")?;
                let symbol = get(h, &p, "symbol")?;
                let key = h.text(symbol)?;
                let market = h.get(&markets, &key)?;
                let quote = opt(h, &market, "quote")?;
                let matched = if h.same(&quote, &currency) {
                    true
                } else {
                    let info = opt(h, &market, "info")?;
                    let settle = opt(h, &info, "settleCurrency")?;
                    if h.same(&settle, &currency) {
                        true
                    } else {
                        let info = get(h, &p, "info")?;
                        let c = opt(h, &info, "currency")?;
                        h.same(&c, &currency)
                    }
                };
                if matched {
                    selected.push(p);
                }
            }
            let total = parse_field(h, &account, "accountBalanceRv")?;
            let used = parse_field(h, &account, "totalUsedBalanceRv")?;
            let mut upnl = 0.0;
            for p in selected {
                let value = get(h, &p, "unrealizedPnl")?;
                let n = h.number(value)?;
                let n = h.num(n);
                let s = h.text(n)?;
                let s = h.string(s);
                upnl += h.parse(s)?;
            }
            let n = h.num(upnl);
            let rounded = h.call("roundUSD", vec![n])?;
            let upnl = h.number(rounded.clone())?;
            let fields = vec![
                ("used", h.num(used)),
                ("free", h.num(subtract(total, used))),
                ("total", h.num(subtract(total, upnl))),
                ("upnl", rounded),
                ("info", account.clone()),
            ];
            let slice = h.object(fields);
            let key = h.text(currency)?;
            h.set(&currencies, &key, slice)?;
            h.set(&infos, &key, account)?;
        }
        if h.values(&currencies)?.is_empty() {
            return Ok(());
        }
        let mut fields = vec![];
        for name in ["used", "free", "total", "upnl"] {
            let mut sum = h.undefined();
            for c in h.values(&currencies)? {
                let n = get(h, &c, name)?;
                sum = h.call("lodashAdd", vec![sum, n])?;
            }
            let undefined = h.undefined();
            if h.same(&sum, &undefined) {
                sum = h.num(0.0);
            }
            if name == "upnl" {
                sum = h.call("roundUSD", vec![sum])?;
            }
            fields.push((name, sum));
        }
        fields.push(("info", infos));
        fields.push(("currencies", currencies));
        let balance = h.object(fields);
        let update = h.object(vec![("balance", balance)]);
        h.call("update", vec![update])?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct CandleSubscription<V> {
    pub opts: V,
    pub callback: V,
    pub parser: V,
    pub topic: Vec<u16>,
    pub handler: Vec<u16>,
}
fn resolution<H: WsHost>(h: &mut H, opts: &H::V) -> Result<f64, H::E> {
    let frames = h.field("timeframes")?;
    let interval = get(h, opts, "interval")?;
    let key = h.text(interval)?;
    let v = h.get(&frames, &key)?;
    h.number(v)
}
fn name<H: WsHost>(h: &mut H, v: &H::V, field: &str) -> Result<Vec<u16>, H::E> {
    let v = get(h, v, field)?;
    h.text(v)
}
fn connected<H: WsHost>(h: &mut H) -> Result<bool, H::E> {
    let v = h.field("connected")?;
    Ok(h.truthy(&v))
}
fn send<H: WsHost>(h: &mut H, method: &str, params: Vec<H::V>, id: f64) -> Result<(), H::E> {
    let method = h.string(u(method));
    let params = h.array(params);
    let id = h.num(id);
    let request = h.object(vec![("method", method), ("params", params), ("id", id)]);
    h.call("send", vec![request])?;
    Ok(())
}
fn subscribe<H: WsHost>(
    h: &mut H,
    handler: &[u16],
    topic: &[u16],
    callback: H::V,
) -> Result<(), H::E> {
    let handler = h.string(handler.to_vec());
    h.call("addHandler", vec![handler, callback])?;
    let topics = h.field("topics")?;
    let old = h.get(&topics, topic)?;
    let n = if h.truthy(&old) { h.number(old)? } else { 0.0 };
    let n = h.num(n + 1.0);
    h.set(&topics, topic, n)?;
    h.call("warning", vec![])?;
    Ok(())
}
fn unsubscribe<H: WsHost>(
    h: &mut H,
    handler: &[u16],
    topic: &[u16],
    callback: H::V,
) -> Result<bool, H::E> {
    let key = h.string(handler.to_vec());
    h.call("removeHandler", vec![key, callback])?;
    let topics = h.field("topics")?;
    let old = h.get(&topics, topic)?;
    let n = h.number(old)? - 1.0;
    let v = h.num(n);
    h.set(&topics, topic, v)?;
    if connected(h)? {
        let key = h.string(handler.to_vec());
        let count = h.call("handlerCount", vec![key])?;
        if h.number(count)? == 0.0 {
            return Ok(true);
        }
    }
    Ok(false)
}
fn finish_unsubscribe<H: WsHost>(h: &mut H, topic: &[u16]) -> Result<(), H::E> {
    let topics = h.field("topics")?;
    let n = h.get(&topics, topic)?;
    if h.number(n)? <= 0.0 {
        h.remove(&topics, topic)?;
    }
    h.call("warning", vec![])?;
    Ok(())
}
pub fn listen_ohlcv<H: WsHost>(
    h: &mut H,
    opts: H::V,
    callback: H::V,
) -> Result<CandleSubscription<H::V>, H::E> {
    let symbol = name(h, &opts, "symbol")?;
    let res = resolution(h, &opts)?;
    let n = h.num(res);
    let res = h.text(n)?;
    let topic = u("kline_p.subscribe|||")
        .into_iter()
        .chain(symbol.clone())
        .chain(u("|||"))
        .chain(res)
        .collect();
    let interval = name(h, &opts, "interval")?;
    let handler = u("kline_p.")
        .into_iter()
        .chain(interval)
        .chain(u("."))
        .chain(symbol)
        .collect();
    let parser = h.call("newParser", vec![])?;
    let subscription = CandleSubscription {
        opts,
        callback,
        parser,
        topic,
        handler,
    };
    subscription.wait(h)?;
    Ok(subscription)
}
impl<V: Clone + 'static> CandleSubscription<V> {
    pub fn wait<H: WsHost<V = V>>(&self, h: &mut H) -> Result<(), H::E> {
        if connected(h)? {
            subscribe(h, &self.handler, &self.topic, self.parser.clone())?;
            let symbol = get(h, &self.opts, "symbol")?;
            let res = resolution(h, &self.opts)?;
            let res = h.num(res);
            send(h, "kline_p.subscribe", vec![symbol, res], 2.0)
        } else {
            let subscription = self.clone();
            let mut host = h.clone();
            h.timer(100.0, Box::new(move || subscription.wait(&mut host)))?;
            Ok(())
        }
    }
    pub fn parse<H: WsHost<V = V>>(&self, h: &mut H, all: V) -> Result<(), H::E> {
        let mut rows = vec![];
        for index in [1, 0] {
            let row = if h.nullish(&all) {
                h.undefined()
            } else {
                h.at(&all, index)?
            };
            if h.truthy(&row) {
                rows.push(row);
            }
        }
        for row in rows {
            let interval = h.at(&row, 1)?;
            let res = resolution(h, &self.opts)?;
            let res = h.num(res);
            if h.same(&interval, &res) {
                let mut fields = vec![];
                for (field, index) in [
                    ("timestamp", 0),
                    ("open", 3),
                    ("high", 4),
                    ("low", 5),
                    ("close", 6),
                    ("volume", 7),
                ] {
                    fields.push((field, h.at(&row, index)?));
                }
                let candle = h.object(fields);
                h.call("callback", vec![self.callback.clone(), candle])?;
            }
        }
        Ok(())
    }
    pub fn dispose<H: WsHost<V = V>>(&self, h: &mut H) -> Result<(), H::E> {
        if unsubscribe(h, &self.handler, &self.topic, self.parser.clone())? {
            let symbol = get(h, &self.opts, "symbol")?;
            let res = resolution(h, &self.opts)?;
            let res = h.num(res);
            let handler = h.string(self.handler.clone());
            h.call("cleanup", vec![handler])?;
            send(h, "kline_p.unsubscribe", vec![symbol, res], 2.0)?;
        }
        finish_unsubscribe(h, &self.topic)
    }
}

pub fn process_order_book_update<H: WsHost>(
    h: &mut H,
    book: H::V,
    update: H::V,
) -> Result<(), H::E> {
    for side in ["bids", "asks"] {
        let rows = get(h, &update, side)?;
        let rows = if h.truthy(&rows) {
            rows
        } else {
            h.array(vec![])
        };
        let mut i = 0;
        loop {
            let length = get(h, &rows, "length")?;
            // JS `!(i < length)`: h.number coerces, so a NaN length must take
            // the break branch. clippy's `i >= length` inverts exactly that.
            #[allow(clippy::neg_cmp_op_on_partial_ord)]
            if !((i as f64) < h.number(length)?) {
                break;
            }
            let row = h.at(&rows, i)?;
            let price = h.at(&row, 0)?;
            let price = h.parse(price)?;
            let amount = h.at(&row, 1)?;
            let amount = h.parse(amount)?;
            let side = h.string(u(side));
            let price = h.num(price);
            let amount = h.num(amount);
            h.call("updateByPrice", vec![book.clone(), side, price, amount])?;
            i += 1;
        }
    }
    Ok(())
}

struct BookState<V> {
    timer: Option<V>,
    active: bool,
    request_id: f64,
    book: V,
    inner: V,
}
pub struct BookSubscription<V> {
    state: Rc<RefCell<BookState<V>>>,
    symbol: V,
    callback: V,
    parser: V,
    topic: Vec<u16>,
    handler: Vec<u16>,
}
impl<V: Clone> Clone for BookSubscription<V> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            symbol: self.symbol.clone(),
            callback: self.callback.clone(),
            parser: self.parser.clone(),
            topic: self.topic.clone(),
            handler: self.handler.clone(),
        }
    }
}
pub fn listen_order_book<H: WsHost>(
    h: &mut H,
    symbol: H::V,
    callback: H::V,
) -> Result<BookSubscription<H::V>, H::E> {
    let text = h.text(symbol.clone())?;
    let topic = u("orderbook_p.subscribe|||")
        .into_iter()
        .chain(text.clone())
        .collect();
    let handler = u("orderbook_p.").into_iter().chain(text).collect();
    let bids = h.array(vec![]);
    let asks = h.array(vec![]);
    let book = h.object(vec![("bids", bids), ("asks", asks)]);
    let inner_id = h.call("nanoid", vec![])?;
    let key = h.text(inner_id)?;
    let states = h.field("orderBookInnerState")?;
    let inner = h.get(&states, &key)?;
    if !h.truthy(&inner) {
        let updates = h.array(vec![]);
        let loaded = h.boolean(false);
        let inner = h.object(vec![("updates", updates), ("isSnapshotLoaded", loaded)]);
        h.set(&states, &key, inner)?;
    }
    let inner = h.get(&states, &key)?;
    let parser = h.call("newParser", vec![])?;
    let subscription = BookSubscription {
        state: Rc::new(RefCell::new(BookState {
            timer: None,
            active: true,
            request_id: 0.0,
            book,
            inner,
        })),
        symbol,
        callback,
        parser,
        topic,
        handler,
    };
    subscription.wait(h)?;
    Ok(subscription)
}
impl<V: Clone + 'static> BookSubscription<V> {
    pub fn book(&self) -> V {
        self.state.borrow().book.clone()
    }
    pub fn wait<H: WsHost<V = V>>(&self, h: &mut H) -> Result<(), H::E> {
        if connected(h)? {
            subscribe(h, &self.handler, &self.topic, self.parser.clone())?;
            send(h, "orderbook_p.subscribe", vec![self.symbol.clone()], 3.0)
        } else {
            let subscription = self.clone();
            let mut host = h.clone();
            let timer = h.timer(100.0, Box::new(move || subscription.wait(&mut host)))?;
            self.state.borrow_mut().timer = Some(timer);
            Ok(())
        }
    }
    pub fn parse<H: WsHost<V = V>>(&self, h: &mut H, data: V) -> Result<(), H::E> {
        let (inner, book) = {
            let state = self.state.borrow();
            (state.inner.clone(), state.book.clone())
        };
        let loaded = get(h, &inner, "isSnapshotLoaded")?;
        if !h.truthy(&loaded) {
            let updates = get(h, &inner, "updates")?;
            if h.rows(updates.clone())?.is_empty() {
                let subscription = self.clone();
                let mut host = h.clone();
                h.spawn(Box::pin(
                    async move { subscription.snapshot(&mut host).await },
                ))?;
                let data = h.array(vec![data]);
                h.set(&inner, &u("updates"), data)?;
            } else {
                h.call("push", vec![updates, data])?;
            }
            return Ok(());
        }
        h.call("processBook", vec![book.clone(), data])?;
        h.call("calcTotal", vec![book.clone()])?;
        h.call("callback", vec![self.callback.clone(), book])?;
        Ok(())
    }
    fn current<H: WsHost<V = V>>(&self, h: &mut H, id: f64) -> Result<bool, H::E> {
        if !self.state.borrow().active {
            return Ok(false);
        }
        let disposed = h.field("disposed")?;
        if h.truthy(&disposed) {
            return Ok(false);
        }
        Ok(self.state.borrow().request_id == id)
    }
    async fn snapshot<H: WsHost<V = V>>(&self, h: &mut H) -> Result<(), H::E> {
        let id = {
            let mut state = self.state.borrow_mut();
            state.request_id += 1.0;
            state.request_id
        };
        let result = async {
            let limit = h.num(1000.0);
            let params = h.object(vec![("symbol", self.symbol.clone()), ("limit", limit)]);
            let response = h.read(crate::constants::ENDPOINTS[9].1, params)?.await?;
            let data = get(h, &response, "data")?;
            let result = get(h, &data, "result")?;
            let raw = get(h, &result, "orderbook_p")?;
            let sequence = get(h, &result, "sequence")?;
            if !self.current(h, id)? {
                return Ok(());
            }
            let (book, inner) = {
                let state = self.state.borrow();
                (state.book.clone(), state.inner.clone())
            };
            for side in ["bids", "asks"] {
                let rows = get(h, &raw, side)?;
                let mut levels = vec![];
                for row in h.rows(rows)? {
                    let p = h.at(&row, 0)?;
                    let p = h.parse(p)?;
                    let a = h.at(&row, 1)?;
                    let a = h.parse(a)?;
                    let p = h.num(p);
                    let a = h.num(a);
                    let zero = h.num(0.0);
                    levels.push(h.object(vec![("price", p), ("amount", a), ("total", zero)]));
                }
                let levels = h.array(levels);
                h.set(&book, &u(side), levels)?;
            }
            let updates = get(h, &inner, "updates")?;
            let mut newer = vec![];
            for update in h.rows(updates)? {
                let seq = get(h, &update, "sequence")?;
                let yes = h.call("greater", vec![seq, sequence.clone()])?;
                if h.truthy(&yes) {
                    newer.push(update);
                }
            }
            let updates = h.array(newer);
            h.set(&inner, &u("updates"), updates.clone())?;
            for update in h.rows(updates)? {
                h.call("processBook", vec![book.clone(), update])?;
            }
            h.call("calcTotal", vec![book.clone()])?;
            let yes = h.boolean(true);
            h.set(&inner, &u("isSnapshotLoaded"), yes)?;
            let updates = h.array(vec![]);
            h.set(&inner, &u("updates"), updates)?;
            h.call("callback", vec![self.callback.clone(), book])?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            if !self.current(h, id)? {
                return Ok(());
            }
            let inner = self.state.borrow().inner.clone();
            let updates = h.array(vec![]);
            h.set(&inner, &u("updates"), updates)?;
            let no = h.boolean(false);
            h.set(&inner, &u("isSnapshotLoaded"), no)?;
            let now = h.call("dateNow", vec![])?;
            let now = h.number(now)?;
            let key = h.text(self.symbol.clone())?;
            let logs = h.field("snapshotLogs")?;
            let last = h.get(&logs, &key)?;
            let undefined = h.undefined();
            if !h.same(&last, &undefined) && now - h.number(last)? < 30_000.0 {
                return Ok(());
            }
            let value = h.num(now);
            h.set(&logs, &key, value)?;
            let message = h.error_text(error)?;
            let text = u("[Phemex] Orderbook snapshot request failed for ")
                .into_iter()
                .chain(key)
                .chain(u("; waiting for next delta to retry: "))
                .chain(message)
                .collect();
            let message = h.string(text);
            let severity = h.string(u("warning"));
            h.call("log", vec![message, severity])?;
        }
        Ok(())
    }
    pub fn dispose<H: WsHost<V = V>>(&self, h: &mut H) -> Result<(), H::E> {
        let (book, inner, timer) = {
            let mut state = self.state.borrow_mut();
            state.active = false;
            state.request_id += 1.0;
            (state.book.clone(), state.inner.clone(), state.timer.clone())
        };
        let key = h.string(self.handler.clone());
        h.call("removeHandler", vec![key, self.parser.clone()])?;
        for side in ["asks", "bids"] {
            let empty = h.array(vec![]);
            h.set(&book, &u(side), empty)?;
        }
        let empty = h.array(vec![]);
        h.set(&inner, &u("updates"), empty)?;
        let no = h.boolean(false);
        h.set(&inner, &u("isSnapshotLoaded"), no)?;
        if let Some(timer) = timer.filter(|v| h.truthy(v)) {
            h.call("clearTimer", vec![timer])?;
            self.state.borrow_mut().timer = None;
        }
        let topics = h.field("topics")?;
        let old = h.get(&topics, &self.topic)?;
        let n = h.number(old)? - 1.0;
        let n = h.num(n);
        h.set(&topics, &self.topic, n)?;
        if connected(h)? {
            let key = h.string(self.handler.clone());
            let count = h.call("handlerCount", vec![key])?;
            if h.number(count)? == 0.0 {
                let handler = h.string(self.handler.clone());
                h.call("cleanup", vec![handler])?;
                send(h, "orderbook_p.unsubscribe", vec![self.symbol.clone()], 3.0)?;
            }
        }
        finish_unsubscribe(h, &self.topic)
    }
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn utf16_encoder_preserves_native_units() {
        assert_eq!(u(""), Vec::<u16>::new());
        assert_eq!(u("A🚀\0"), vec![65, 55357, 56960, 0]);
        assert_eq!(u("é雪"), vec![233, 38634]);
    }
}

#[cfg(test)]
mod accepted_outbound_contract {
    #[allow(dead_code, unused_imports)]
    mod fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/accepted_host_fixture.rs"
        ));
    }
    #[test]
    fn send_builds_one_native_frame_and_keeps_failure_identity() {
        use safe_cex_exchange::js_runtime::value::{Js, Thrown};
        let mut host = fixture::Host::new(&serde_json::Value::Null);
        super::send(
            &mut host,
            "trade.subscribe",
            vec![Js::str("BTCUSDT"), Js::Number(17.5)],
            -0.0,
        )
        .unwrap();
        let frames = host.sent.borrow();
        assert_eq!(frames.len(), 1, "one outbound frame");
        let frame = &frames[0];
        assert_eq!(frame.own_keys().len(), 3, "exact outbound field count");
        assert!(
            frame
                .get("method")
                .unwrap()
                .strict_equals(&Js::str("trade.subscribe"))
        );
        let params = frame.get("params").unwrap().array_values().unwrap();
        assert_eq!(params.len(), 2);
        assert!(params[0].strict_equals(&Js::str("BTCUSDT")));
        let Js::Number(n) = params[1] else {
            panic!("native numeric second param")
        };
        assert_eq!(n.to_bits(), 0x4031800000000000);
        let Js::Number(id) = frame.get("id").unwrap() else {
            panic!("native numeric request ID")
        };
        assert_eq!(
            id.to_bits(),
            0x8000000000000000,
            "negative-zero request ID bits"
        );
        drop(frames);
        host.fail_send = true;
        let result = super::send(&mut host, "unsubscribe", vec![], 1200.0);
        let Err(Thrown { value, .. }) = result else {
            panic!("send failure must propagate")
        };
        assert!(
            value.strict_equals(&host.sentinel),
            "unchanged host rejection identity"
        );
        assert_eq!(host.sent.borrow().len(), 2);
    }
}

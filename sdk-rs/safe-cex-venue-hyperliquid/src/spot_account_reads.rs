//! Spot account/read lifecycle bodies; native operations and virtual callees bind explicitly.
use crate::{
    helper_tasks::{self, HelperObserver, HelperTaskHost},
    spot_json::JsString,
    ws_ping_lifecycle::WsPingHost,
};
use std::{future::Future, pin::Pin};
pub type AccountFuture<V, E> = Pin<Box<dyn Future<Output = Result<V, E>>>>;
#[derive(Clone, Debug)]
pub enum AccountCallback<V> {
    FakePosition { exchange: V },
    PositionFilter { symbols: V },
    Candle,
    Tick { exchange: V },
}
pub trait SpotAccountHost: WsPingHost + HelperTaskHost<Self::Value, Self::Error> {
    fn null(&self) -> Self::Value;
    fn boolean(&self, v: bool) -> Self::Value;
    fn number(&self, v: f64) -> Self::Value;
    fn is_nullish(&self, v: &Self::Value) -> bool;
    fn is_array(&mut self, v: &Self::Value) -> Result<bool, Self::Error>;
    fn candle_binding(&mut self, v: &Self::Value) -> Result<[Self::Value; 6], Self::Error>;
    fn is_undefined(&self, v: &Self::Value) -> bool;
    fn strict_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn imported(&mut self, name: &str) -> Result<Self::Value, Self::Error>;
    fn array(&mut self, values: &[Self::Value]) -> Result<Self::Value, Self::Error>;
    fn spread_array(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn spread_into(
        &mut self,
        target: &Self::Value,
        source: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn binary(
        &mut self,
        op: &str,
        a: &Self::Value,
        b: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Native three-slot array binding with the first slot elided and IteratorClose.
    fn interval_binding(&mut self, value: &Self::Value) -> Result<[Self::Value; 3], Self::Error>;
    fn pair_binding(&mut self, value: &Self::Value) -> Result<[Self::Value; 2], Self::Error>;
    fn index(&mut self, value: &Self::Value, key: &Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn digit_regex(&mut self) -> Result<Self::Value, Self::Error>;
    /// Native `{data}` binding after await, including null/undefined diagnostics.
    fn response_data(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn construct(
        &mut self,
        constructor: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn throw_value(&self, value: Self::Value) -> Self::Error;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn callback(
        &mut self,
        capture: AccountCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn await_value(&mut self, value: Self::Value) -> AccountFuture<Self::Value, Self::Error>;
    /// Allocate a native promise, enter its body now, retain it beyond observers,
    /// and adopt returned promises before publishing the settled result.
    fn spawn_native(&self, body: AccountFuture<Self::Value, Self::Error>) -> Self::Value;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&text(s))
}
fn method<H: WsPingHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let a = args(h)?;
    h.call(&f, o, &a)
}
fn free<H: SpotAccountHost>(
    h: &mut H,
    name: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(name)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn optional<H: SpotAccountHost>(h: &mut H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.is_nullish(v) {
        Ok(h.undefined())
    } else {
        h.get(v, key)
    }
}
fn path<H: SpotAccountHost>(h: &mut H, v: &H::Value, keys: &[&str]) -> Result<H::Value, H::Error> {
    let mut v = v.clone();
    for k in keys {
        v = optional(h, &v, k)?
    }
    Ok(v)
}
fn enums<H: SpotAccountHost>(h: &mut H, name: &str, key: &str) -> Result<H::Value, H::Error> {
    let table = h.imported(name)?;
    h.get(&table, key)
}
fn field<H: WsPingHost>(
    h: &mut H,
    v: &H::Value,
    key: &str,
    value: &H::Value,
) -> Result<(), H::Error> {
    h.define_data(v, key, value)
}
fn headers<H: SpotAccountHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let c = h.object()?;
    let hdr = h.object()?;
    field(h, &hdr, "Content-Type", &lit(h, "application/json"))?;
    field(h, &c, "headers", &hdr)?;
    Ok(c)
}
fn global_method<H: SpotAccountHost>(
    h: &mut H,
    object: &str,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let o = h.global(object)?;
    method(h, &o, key, args)
}
async fn validate_body<H: SpotAccountHost>(mut h: H, e: H::Value) -> Result<H::Value, H::Error> {
    let result = (|| {
        let valid = free(&mut h, "validateWalletAndPrivateKey", |h| {
            let opts = h.get(&e, "options")?;
            let secret = h.get(&opts, "secret")?;
            let opts = h.get(&e, "options")?;
            let key = h.get(&opts, "key")?;
            Ok(vec![secret, key])
        })?;
        if !h.truthy(&valid) {
            let ctor = h.global("Error")?;
            let error = h.construct(
                &ctor,
                &[lit(&h, "Invalid private key & wallet address pair")],
            )?;
            return Err(h.throw_value(error));
        }
        let result = h.object()?;
        let empty = lit(&h, "");
        field(&mut h, &result, "error", &empty)?;
        Ok(result)
    })();
    match result {
        Ok(r) => Ok(r),
        Err(err) => {
            let err = h.error_value(&err);
            method(&mut h, &e, "emitAndLogError", |h| {
                let msg = path(h, &err, &["response", "data", "message"])?;
                let msg = if h.truthy(&msg) {
                    msg
                } else {
                    optional(h, &err, "message")?
                };
                Ok(vec![msg])
            })?;
            let result = h.object()?;
            let msg = path(&mut h, &err, &["response", "data", "msg"])?;
            let msg = if h.truthy(&msg) {
                msg
            } else {
                optional(&mut h, &err, "message")?
            };
            let msg = if h.truthy(&msg) {
                msg
            } else {
                lit(&h, "Invalid API key or secret")
            };
            field(&mut h, &result, "error", &msg)?;
            Ok(result)
        }
    }
}
async fn clearinghouse_body<H: SpotAccountHost>(
    mut h: H,
    e: H::Value,
) -> Result<H::Value, H::Error> {
    let opts = h.get(&e, "options")?;
    let vault = h.get(&opts, "vault")?;
    let user = if h.truthy(&vault) {
        vault
    } else {
        let opts = h.get(&e, "options")?;
        h.get(&opts, "applicationId")?
    };
    if !h.truthy(&user) {
        return Ok(h.null());
    }
    let xhr = h.get(&e, "xhr")?;
    let response = method(&mut h, &xhr, "post", |h| {
        let url = enums(h, "ENDPOINTS", "INFO")?;
        let body = h.object()?;
        field(h, &body, "type", &lit(h, "spotClearinghouseState"))?;
        field(h, &body, "user", &user)?;
        let config = headers(h)?;
        Ok(vec![url, body, config])
    })?;
    let response = h.await_value(response).await?;
    let data = h.response_data(&response)?;
    Ok(if h.truthy(&data) { data } else { h.null() })
}
async fn refresh_body<H>(mut h: H, e: H::Value, force: H::Value) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let now = global_method(&mut h, "Date", "now", |_| Ok(vec![]))?;
    if h.truthy(&force) {
        let last = h.get(&e, "lastForcedSpotBalanceRefreshAt")?;
        let diff = h.binary("-", &now, &last)?;
        let limit = enums(
            &mut h,
            "HyperliquidSpotExchange",
            "SPOT_BALANCE_FORCE_REFRESH_COOLDOWN_MS",
        )?;
        let less = h.binary("<", &diff, &limit)?;
        if h.truthy(&less) {
            return Ok(h.undefined());
        }
        h.set(&e, "lastForcedSpotBalanceRefreshAt", &now)?;
    } else {
        let last = h.get(&e, "lastSpotBalanceRefreshAt")?;
        let diff = h.binary("-", &now, &last)?;
        let limit = enums(&mut h, "HyperliquidSpotExchange", "SPOT_BALANCE_REFRESH_MS")?;
        let less = h.binary("<", &diff, &limit)?;
        if h.truthy(&less) {
            return Ok(h.undefined());
        }
    }
    let flight = h.get(&e, "spotBalanceRefreshInFlight")?;
    if h.truthy(&flight) {
        return h.get(&e, "spotBalanceRefreshInFlight");
    }
    h.set(&e, "lastSpotBalanceRefreshAt", &now)?;
    let mut inner = h.clone();
    let owner = e.clone();
    let promise = h.spawn_native(Box::pin(async move {
        let result = async {
            let state = method(&mut inner, &owner, "fetchSpotClearinghouseState", |_| {
                Ok(vec![])
            })?;
            let state = inner.await_value(state).await?;
            let disposed = inner.get(&owner, "isDisposed")?;
            if inner.truthy(&disposed) {
                return Ok(inner.undefined());
            }
            let balances = optional(&mut inner, &state, "balances")?;
            if !inner.truthy(&balances) {
                return Ok(inner.undefined());
            }
            method(&mut inner, &owner, "applySpotBalances", |h| {
                Ok(vec![h.get(&state, "balances")?])
            })?;
            Ok(inner.undefined())
        }
        .await;
        let caught = (|| match result {
            Ok(v) => Ok(v),
            Err(error) => {
                let err = inner.error_value(&error);
                method(&mut inner, &owner, "log", |h| {
                    let mut t = text("Failed to refresh spot balance: ").0;
                    t.extend(h.template_string(&err)?.0);
                    let msg = h.literal(&JsString(t));
                    let severity = enums(h, "LogSeverity", "Warning")?;
                    Ok(vec![msg, severity])
                })?;
                Ok(inner.undefined())
            }
        })();
        inner.set(&owner, "spotBalanceRefreshInFlight", &inner.null())?;
        caught
    }));
    h.set(&e, "spotBalanceRefreshInFlight", &promise)?;
    h.get(&e, "spotBalanceRefreshInFlight")
}
async fn tick_body<H>(mut h: H, e: H::Value) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let disposed = h.get(&e, "isDisposed")?;
    if !h.truthy(&disposed) {
        let only = h.get(&e, "isPublicDataOnly")?;
        if h.truthy(&only) {
            return Ok(h.undefined());
        }
        let refresh = method(&mut h, &e, "refreshSpotBalance", |_| Ok(vec![]))?;
        h.await_value(refresh).await?;
        free(&mut h, "loop", |h| {
            let cb = SpotAccountHost::callback(
                h,
                AccountCallback::Tick {
                    exchange: e.clone(),
                },
            )?;
            let interval = method(h, &e, "getTickInterval", |_| Ok(vec![]))?;
            Ok(vec![cb, interval])
        })?;
    }
    Ok(h.undefined())
}
async fn start_body<H>(mut h: H, e: H::Value) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let init = method(&mut h, &e, "initExternalPublicData", |_| Ok(vec![]))?;
    let external = h.await_value(init).await?;
    let only = h.get(&e, "isPublicDataOnly")?;
    let _initial_markets = h.array(&[])?;
    let _initial_tickers = h.array(&[])?;
    let (markets, tickers) = if !h.truthy(&external) {
        let fetched = method(&mut h, &e, "fetchMarketsAndTickers", |_| Ok(vec![]))?;
        let fetched = h.await_value(fetched).await?;
        let [markets, tickers] = h.pair_binding(&fetched)?;
        let disposed = h.get(&e, "isDisposed")?;
        if h.truthy(&disposed) {
            return Ok(h.undefined());
        }
        let ws = h.get(&e, "publicWebsocket")?;
        method(&mut h, &ws, "connectAndSubscribe", |_| Ok(vec![]))?;
        (markets, tickers)
    } else {
        let store = h.get(&e, "store")?;
        let markets = h.get(&store, "markets")?;
        let store = h.get(&e, "store")?;
        let tickers = h.get(&store, "tickers")?;
        (markets, tickers)
    };
    if !h.truthy(&only) {
        let ws = h.get(&e, "privateWebsocket")?;
        method(&mut h, &ws, "connectAndSubscribe", |_| Ok(vec![]))?;
    }
    let positions = method(&mut h, &e, "generateFakePositionsFromMarkets", |_| {
        Ok(vec![markets.clone()])
    })?;
    let update = h.object()?;
    field(&mut h, &update, "positions", &positions)?;
    let orders = h.array(&[])?;
    field(&mut h, &update, "orders", &orders)?;
    let balance = h.object()?;
    for key in ["used", "free", "total", "upnl"] {
        let zero = h.number(0.);
        field(&mut h, &balance, key, &zero)?
    }
    field(&mut h, &update, "balance", &balance)?;
    let loaded = h.object()?;
    let store = h.get(&e, "store")?;
    let source = h.get(&store, "loaded")?;
    h.spread_into(&loaded, &source)?;
    for key in ["orders", "balance", "positions"] {
        let yes = h.boolean(true);
        field(&mut h, &loaded, key, &yes)?
    }
    field(&mut h, &update, "loaded", &loaded)?;
    if !h.truthy(&external) {
        h.set(&update, "markets", &markets)?;
        h.set(&update, "tickers", &tickers)?;
        let loaded = h.get(&update, "loaded")?;
        h.set(&loaded, "markets", &h.boolean(true))?;
        let loaded = h.get(&update, "loaded")?;
        h.set(&loaded, "tickers", &h.boolean(true))?;
    }
    let store = h.get(&e, "store")?;
    method(&mut h, &store, "update", |_| Ok(vec![update]))?;
    if h.truthy(&only) {
        method(&mut h, &e, "log", |h| {
            Ok(vec![lit(
                h,
                "Ready to watch public data on Hyperliquid Spot",
            )])
        })?;
        return Ok(h.undefined());
    }
    let refresh = method(&mut h, &e, "refreshSpotBalance", |h| {
        Ok(vec![h.boolean(true)])
    })?;
    h.await_value(refresh).await?;
    method(&mut h, &e, "log", |h| {
        Ok(vec![lit(
            h,
            "Ready to watch public data on Hyperliquid Spot",
        )])
    })?;
    let tick = method(&mut h, &e, "tick", |_| Ok(vec![]))?;
    h.await_value(tick).await?;
    let disposed = h.get(&e, "isDisposed")?;
    if h.truthy(&disposed) {
        return Ok(h.undefined());
    }
    Ok(h.undefined())
}
pub fn generate_fake_positions_from_markets<H: SpotAccountHost>(
    h: &mut H,
    e: &H::Value,
    markets: &H::Value,
) -> Result<H::Value, H::Error> {
    let mapped = method(h, markets, "map", |h| {
        Ok(vec![SpotAccountHost::callback(
            h,
            AccountCallback::FakePosition {
                exchange: e.clone(),
            },
        )?])
    })?;
    let flat = method(h, &mapped, "flat", |_| Ok(vec![]))?;
    h.spread_array(&flat)
}
fn fake_position<H: SpotAccountHost>(
    h: &mut H,
    market: &H::Value,
    config: &H::Value,
    id_side: &str,
) -> Result<H::Value, H::Error> {
    let p = h.object()?;
    let id = free(h, "createPositionId", |h| {
        let symbol = h.get(market, "symbol")?;
        let side = enums(h, "PositionSide", id_side)?;
        let mode = enums(h, "PositionMode", "OneWay")?;
        Ok(vec![symbol, side, mode])
    })?;
    field(h, &p, "id", &id)?;
    let symbol = h.get(market, "symbol")?;
    field(h, &p, "symbol", &symbol)?;
    let side = enums(h, "PositionSide", "Short")?;
    field(h, &p, "side", &side)?;
    for k in [
        "entryPrice",
        "notional",
        "unrealizedPnl",
        "contracts",
        "liquidationPrice",
    ] {
        field(h, &p, k, &h.number(0.))?
    }
    h.spread_into(&p, config)?;
    Ok(p)
}
pub fn run_account_callback<H>(
    h: &mut H,
    capture: &AccountCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match capture {
        AccountCallback::FakePosition { exchange } => {
            let config = method(h, exchange, "_getSymbolConfigurationFromAssetData", |h| {
                Ok(vec![h.get(&first, "id")?])
            })?;
            let mode = h.get(&config, "positionMode")?;
            let one = enums(h, "PositionMode", "OneWay")?;
            let is_one = h.strict_equal(&mode, &one);
            let first_pos = fake_position(h, &first, &config, "Long")?;
            let positions = if is_one {
                vec![first_pos]
            } else {
                vec![first_pos, fake_position(h, &first, &config, "Short")?]
            };
            h.array(&positions)
        }
        AccountCallback::PositionFilter { symbols } => {
            if !h.truthy(symbols) {
                Ok(first)
            } else {
                method(h, symbols, "includes", |h| {
                    Ok(vec![h.get(&first, "symbol")?])
                })
            }
        }
        AccountCallback::Candle => {
            let [t, o, high, low, c, v] = h.candle_binding(&first)?;
            let candle = h.object()?;
            let timestamp = h.binary("/", &t, &h.number(1000.))?;
            field(h, &candle, "timestamp", &timestamp)?;
            for (k, value) in [
                ("open", o),
                ("high", high),
                ("low", low),
                ("close", c),
                ("volume", v),
            ] {
                let f = h.global("parseFloat")?;
                let number = h.call(&f, &h.undefined(), &[value])?;
                field(h, &candle, k, &number)?
            }
            Ok(candle)
        }
        AccountCallback::Tick { exchange } => method(h, exchange, "tick", |_| Ok(vec![])),
    }
}
async fn positions_body<H>(mut h: H, e: H::Value, symbols: H::Value) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let store = h.get(&e, "store")?;
    let positions = h.get(&store, "positions")?;
    let length = h.get(&positions, "length")?;
    let gt = h.binary(">", &length, &h.number(0.))?;
    if h.truthy(&gt) {
        let store = h.get(&e, "store")?;
        let positions = h.get(&store, "positions")?;
        return h.spread_array(&positions);
    }
    let positions = method(&mut h, &e, "getFakePositions", |_| Ok(vec![]))?;
    method(&mut h, &positions, "filter", |h| {
        Ok(vec![SpotAccountHost::callback(
            h,
            AccountCallback::PositionFilter { symbols },
        )?])
    })
}
async fn ohlcv_body<H>(mut h: H, e: H::Value, opts: H::Value) -> Result<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: 'static,
{
    let interval = h.get(&opts, "interval")?;
    let limit = global_method(&mut h, "Math", "min", |h| {
        let v = h.get(&opts, "limit")?;
        Ok(vec![
            if h.truthy(&v) { v } else { h.number(500.) },
            h.number(1500.),
        ])
    })?;
    let interval_again = h.get(&opts, "interval")?;
    let split = method(&mut h, &interval_again, "split", |h| {
        Ok(vec![h.digit_regex()?])
    })?;
    let [_, amount, unit] = h.interval_binding(&split)?;
    let to = h.get(&opts, "to")?;
    let end = if h.truthy(&to) {
        free(&mut h, "dayjs", |h| Ok(vec![h.get(&opts, "to")?]))?
    } else {
        free(&mut h, "dayjs", |_| Ok(vec![]))?
    };
    let limit_again = h.get(&opts, "limit")?;
    let from = if !h.truthy(&limit_again) {
        h.get(&opts, "from")?
    } else {
        h.undefined()
    };
    let start = if !h.truthy(&limit_again) && h.truthy(&from) {
        free(&mut h, "dayjs", |h| Ok(vec![h.get(&opts, "from")?]))?
    } else {
        method(&mut h, &end, "subtract", |h| {
            let f = h.global("parseFloat")?;
            let a = h.call(&f, &h.undefined(), &[amount])?;
            let mult = h.binary("*", &a, &limit)?;
            Ok(vec![mult, unit])
        })?
    };
    let xhr = h.get(&e, "xhr")?;
    let response = method(&mut h, &xhr, "post", |h| {
        let url = enums(h, "ENDPOINTS", "INFO")?;
        let body = h.object()?;
        field(h, &body, "type", &lit(h, "candleSnapshot"))?;
        let req = h.object()?;
        let store = h.get(&e, "store")?;
        let bysymbol = h.get(&store, "marketsBySymbol")?;
        let symbol = h.get(&opts, "symbol")?;
        let market = h.index(&bysymbol, &symbol)?;
        let coin = optional(h, &market, "id")?;
        field(h, &req, "coin", &coin)?;
        field(h, &req, "interval", &interval)?;
        let starttime = method(h, &start, "valueOf", |_| Ok(vec![]))?;
        field(h, &req, "startTime", &starttime)?;
        let endtime = method(h, &end, "valueOf", |_| Ok(vec![]))?;
        field(h, &req, "endTime", &endtime)?;
        field(h, &body, "req", &req)?;
        let config = headers(h)?;
        Ok(vec![url, body, config])
    })?;
    let response = h.await_value(response).await?;
    let data = h.response_data(&response)?;
    let candledata = if h.is_array(&data)? {
        data.clone()
    } else {
        let inner = optional(&mut h, &data, "data")?;
        if h.truthy(&inner) {
            inner
        } else {
            let inner = optional(&mut h, &data, "response")?;
            if h.truthy(&inner) {
                inner
            } else {
                h.array(&[])?
            }
        }
    };
    if !h.is_array(&candledata)? {
        method(&mut h, &e, "log", |h| {
            let json = h.global("JSON")?;
            let wire = method(h, &json, "stringify", |_| Ok(vec![data]))?;
            let mut msg = text("Unexpected fetchOHLCV response format: ").0;
            msg.extend(h.template_string(&wire)?.0);
            Ok(vec![
                h.literal(&JsString(msg)),
                enums(h, "LogSeverity", "Error")?,
            ])
        })?;
        return h.array(&[]);
    }
    method(&mut h, &candledata, "map", |h| {
        Ok(vec![SpotAccountHost::callback(h, AccountCallback::Candle)?])
    })
}
pub fn validate_account<H>(host: H, exchange: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, validate_body(host, exchange))
}
pub fn fetch_spot_clearinghouse_state<H>(
    host: H,
    exchange: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, clearinghouse_body(host, exchange))
}
pub fn start<H>(host: H, exchange: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, start_body(host, exchange))
}
pub fn tick<H>(host: H, exchange: H::Value) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, tick_body(host, exchange))
}
pub fn refresh_spot_balance<H>(
    host: H,
    exchange: H::Value,
    force: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    let force = if host.is_undefined(&force) {
        host.boolean(false)
    } else {
        force
    };
    helper_tasks::start(&runtime, refresh_body(host, exchange, force))
}
pub fn fetch_orders<H>(
    host: H,
    _exchange: H::Value,
    _symbols: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, async move {
        let mut host = host;
        host.array(&[])
    })
}
pub fn fetch_positions<H>(
    host: H,
    exchange: H::Value,
    symbols: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, positions_body(host, exchange, symbols))
}
pub fn fetch_ohlcv<H>(
    host: H,
    exchange: H::Value,
    opts: H::Value,
) -> HelperObserver<H::Value, H::Error>
where
    H: SpotAccountHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let runtime = host.clone();
    helper_tasks::start(&runtime, ohlcv_body(host, exchange, opts))
}

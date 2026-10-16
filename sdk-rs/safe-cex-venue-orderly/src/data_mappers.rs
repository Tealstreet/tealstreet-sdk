//! Market, ticker, position, order and candle data bodies.
use crate::{
    account_order_reads::AccountReadHost,
    ws_controls::{Result, Value},
};
#[derive(Clone, Copy, Debug)]
pub enum Op {
    Sub,
    Mul,
    Div,
    Min,
    Max,
}
pub trait DataHost: AccountReadHost {
    fn numeric(&self, value: Value<Self>) -> Result<Self, f64>;
    fn arithmetic(
        &self,
        op: Op,
        left: Value<Self>,
        right: Value<Self>,
    ) -> Result<Self, Value<Self>>;
    fn compare(&self, left: Value<Self>, right: Value<Self>, less: bool) -> Result<Self, bool>;
    fn round(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn split(&self, value: Value<Self>) -> Result<Self, Vec<Value<Self>>>;
    fn starts_with(&self, value: Value<Self>, prefix: &str) -> Result<Self, bool>;
    fn filter(
        &self,
        rows: Value<Self>,
        predicate: impl FnMut(Value<Self>) -> Result<Self, bool>,
    ) -> Result<Self, Value<Self>>;
    fn map(
        &self,
        rows: Value<Self>,
        callback: impl FnMut(Value<Self>) -> Result<Self, Value<Self>>,
    ) -> Result<Self, Value<Self>>;
    fn clone_deep(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn merge(
        &self,
        value: Value<Self>,
        fields: Vec<(&'static str, Value<Self>)>,
    ) -> Result<Self, Value<Self>>;
    fn uniq_id(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn reverse(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn push(&self, value: Value<Self>, items: Vec<Value<Self>>) -> Result<Self, ()>;
    fn reverse_symbol(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn order_type(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn order_side(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn subtract(&self, left: Value<Self>, right: Value<Self>) -> Result<Self, Value<Self>>;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn boolean(&self, value: bool) -> Value<Self>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: DataHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn get<H: DataHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &units(key))
}
fn key<H: DataHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get_key(v.clone(), key)
}
fn call<H: DataHost>(h: &H, r: Value<H>, method: &str, args: Vec<Value<H>>) -> Result<H, Value<H>> {
    let f = get(h, &r, method)?;
    h.call(f, Some(r), args)
}
fn optional<H: DataHost>(h: &H, v: &Value<H>, name: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, name)
    }
}
fn truthy_or<H: DataHost>(
    h: &H,
    v: Value<H>,
    otherwise: impl FnOnce() -> Result<H, Value<H>>,
) -> Result<H, Value<H>> {
    if h.truthy(&v) { Ok(v) } else { otherwise() }
}
fn nullish_or<H: DataHost>(
    h: &H,
    v: Value<H>,
    otherwise: impl FnOnce() -> Result<H, Value<H>>,
) -> Result<H, Value<H>> {
    if h.nullish(&v) { otherwise() } else { Ok(v) }
}
fn string_value<H: DataHost>(h: &H, v: Value<H>) -> Result<H, Value<H>> {
    Ok(h.text(&h.string(&v)?))
}
fn endpoint<H: DataHost>(h: &H, name: &str) -> Value<H> {
    text(
        h,
        crate::types::ENDPOINTS
            .iter()
            .find(|(k, _)| *k == name)
            .expect("mapped endpoint")
            .1,
    )
}
async fn rows<H: DataHost>(h: &H, r: &Value<H>, name: &str) -> Result<H, Value<H>> {
    let xhr = get(h, r, "xhr")?;
    let f = get(h, &xhr, "get")?;
    let promise = h.call(f, Some(xhr), vec![endpoint(h, name)])?;
    let response = h.await_value(promise).await?;
    get(h, &get(h, &get(h, &response, "data")?, "data")?, "rows")
}
fn log<H: DataHost>(h: &H, r: &Value<H>, e: &H::Error) -> Result<H, ()> {
    let f = get(h, r, "emitAndLogError")?;
    let e = h.error_value(e);
    let data = optional(h, &optional(h, &e, "response")?, "data")?;
    let msg = truthy_or(h, optional(h, &data, "message")?, || {
        optional(h, &e, "message")
    })?;
    h.call(f, Some(r.clone()), vec![msg])?;
    Ok(())
}
pub async fn markets<H: DataHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let rows = rows(h, &r, "MARKETS").await?;
    let filtered = h.filter(rows, |row| h.starts_with(get(h, &row, "symbol")?, "PERP_"))?;
    h.map(filtered, |row| {
        let parts = h.split(get(h, &row, "symbol")?)?;
        let base = parts.get(1).cloned().unwrap_or_else(|| h.undefined());
        let quote = parts.get(2).cloned().unwrap_or_else(|| h.undefined());
        let min = h.numeric(key(h, &row, "min_leverage")?)?;
        let imr = h.numeric(key(h, &row, "base_imr")?)?;
        let legacy = h.numeric(key(h, &row, "max_leverage")?)?;
        let max = if imr.is_finite() && imr > 0.0 {
            h.numeric(h.round(h.number(1.0 / imr))?)?
        } else {
            legacy
        };
        let limits = h.object(vec![
            (
                "min",
                h.number(if min.is_finite() && min > 0.0 {
                    min
                } else {
                    1.0
                }),
            ),
            (
                "max",
                h.number(if max.is_finite() && max > 0.0 {
                    max
                } else {
                    100.0
                }),
            ),
        ])?;
        let id = h.text(
            &[
                h.string(&base)?,
                units("/"),
                h.string(&quote)?,
                units(":USDC"),
            ]
            .concat(),
        );
        let symbol = h.text(&[h.string(&base)?, h.string(&quote)?].concat());
        let precision = h.object(vec![
            ("amount", key(h, &row, "base_tick")?),
            ("price", key(h, &row, "quote_tick")?),
        ])?;
        let amounts = h.object(vec![
            ("min", key(h, &row, "base_min")?),
            ("max", key(h, &row, "base_max")?),
        ])?;
        let limits = h.object(vec![("amount", amounts), ("leverage", limits)])?;
        h.object(vec![
            ("id", id),
            ("symbol", symbol),
            ("base", base),
            ("quote", quote),
            ("active", h.boolean(true)),
            ("precision", precision),
            ("limits", limits),
            ("info", h.clone_deep(row)?),
        ])
    })
}
pub async fn tickers<H: DataHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let result = async {
        let rows = rows(h, &r, "TICKERS").await?;
        h.reduce(rows, h.array(vec![]), |acc, row| {
            let symbol = h.normalize_symbol(get(h, &row, "symbol")?)?;
            let store = get(h, &r, "store")?;
            let market = h.get(&get(h, &store, "marketsBySymbol")?, &h.string(&symbol)?)?;
            if !h.truthy(&market) {
                return Ok(acc);
            }
            let open = key(h, &row, "24h_open")?;
            let close = key(h, &row, "24h_close")?;
            let difference = h.arithmetic(Op::Sub, close.clone(), open.clone())?;
            let ratio = h.arithmetic(Op::Div, difference, open)?;
            let percentage = h.arithmetic(
                Op::Mul,
                h.arithmetic(Op::Mul, ratio, h.number(100.0))?,
                h.number(100.0),
            )?;
            let percentage = h.arithmetic(Op::Div, h.round(percentage)?, h.number(100.0))?;
            let ticker = h.object(vec![
                ("id", get(h, &market, "id")?),
                ("symbol", get(h, &market, "symbol")?),
                ("bid", close.clone()),
                ("ask", close.clone()),
                ("last", close),
                ("index", key(h, &row, "index_price")?),
                ("mark", key(h, &row, "mark_price")?),
                ("percentage", percentage),
                ("fundingRate", key(h, &row, "last_funding_rate")?),
                ("fundingTime", key(h, &row, "next_funding_time")?),
                ("volume", key(h, &row, "24h_volume")?),
                ("quoteVolume", key(h, &row, "24h_amount")?),
                ("openInterest", key(h, &row, "open_interest")?),
                ("info", row),
            ])?;
            let mut values = h.spread(acc)?;
            values.push(ticker);
            Ok(h.array(values))
        })
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            log(h, &r, &e)?;
            get(h, &get(h, &r, "store")?, "tickers")
        }
    }
}
pub fn map_position<H: DataHost>(
    h: &H,
    r: Value<H>,
    p: Value<H>,
    mode: Value<H>,
) -> Result<H, Value<H>> {
    let mode = if h.is_undefined(&mode) {
        text(h, "oneway")
    } else {
        mode
    };
    let symbol = h.normalize_symbol(get(h, &p, "symbol")?)?;
    let ticker = h.get(
        &get(h, &get(h, &r, "store")?, "tickersBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&ticker) {
        return Ok(h.null());
    }
    let holdings = h.numeric(key(h, &p, "position_qty")?)?;
    if !holdings.is_finite() {
        return Ok(h.null());
    }
    let entry = truthy_or(
        h,
        h.number(h.numeric(key(h, &p, "average_open_price")?)?),
        || Ok(h.number(0.0)),
    )?;
    let contracts = h.number(holdings.abs());
    let side = text(h, if holdings >= 0.0 { "long" } else { "short" });
    let mark = truthy_or(h, h.number(h.numeric(key(h, &p, "mark_price")?)?), || {
        get(h, &ticker, "mark")
    })?;
    let notional = h.arithmetic(Op::Mul, contracts.clone(), mark.clone())?;
    let difference = h.arithmetic(
        Op::Sub,
        h.arithmetic(Op::Max, entry.clone(), mark.clone())?,
        h.arithmetic(Op::Min, entry.clone(), mark.clone())?,
    )?;
    let loss = if holdings >= 0.0 {
        h.compare(mark.clone(), entry.clone(), true)?
    } else {
        h.compare(mark.clone(), entry.clone(), false)?
    };
    let absolute = h.arithmetic(Op::Mul, difference, contracts.clone())?;
    let upnl = if holdings.abs() > 0.0 {
        let signed = if loss {
            h.arithmetic(Op::Mul, h.number(-1.0), absolute)?
        } else {
            absolute
        };
        h.arithmetic(
            Op::Div,
            h.round(h.arithmetic(Op::Mul, signed, h.number(100.0))?)?,
            h.number(100.0),
        )?
    } else {
        h.number(0.0)
    };
    let function = get(h, &r, "mapMarginType")?;
    let raw = key(h, &p, "margin_mode")?;
    let margin = h.call(function, Some(r.clone()), vec![raw])?;
    let raw_leverage = h.numeric(key(h, &p, "leverage")?)?;
    let imr = h.numeric(key(h, &p, "imr")?)?;
    let leverage = if raw_leverage.is_finite() && raw_leverage > 0.0 {
        h.number(raw_leverage)
    } else if imr.is_finite() && imr > 0.0 {
        h.number(1.0 / imr)
    } else {
        get(h, &r, "accountLeverage")?
    };
    let raw_margin = h.numeric(key(h, &p, "margin")?)?;
    let id = call(
        h,
        r.clone(),
        "createOrderlyPositionId",
        vec![symbol.clone(), side.clone(), mode.clone(), margin.clone()],
    )?;
    let liquidation = truthy_or(
        h,
        h.number(h.numeric(key(h, &p, "est_liq_price")?)?),
        || Ok(h.number(0.0)),
    )?;
    let size = if raw_margin.is_finite() && raw_margin > 0.0 {
        h.number(raw_margin)
    } else if h.compare(leverage.clone(), h.number(0.0), false)? {
        h.arithmetic(Op::Div, notional.clone(), leverage.clone())?
    } else {
        h.number(0.0)
    };
    let info = h.clone_deep(p)?;
    let now = h.virtual_now()?;
    h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", side),
        ("entryPrice", entry),
        ("notional", notional),
        ("leverage", leverage),
        ("unrealizedPnl", upnl),
        ("contracts", contracts),
        ("liquidationPrice", liquidation),
        ("marginSize", size),
        ("positionMode", mode),
        ("marginType", margin),
        ("info", info),
        ("lastUpdate", now),
    ])
}
pub fn map_limit<H: DataHost>(h: &H, r: Value<H>, o: Value<H>) -> Result<H, Value<H>> {
    let symbol = h.normalize_symbol(get(h, &o, "symbol")?)?;
    let market = h.get(
        &get(h, &get(h, &r, "store")?, "marketsBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&market) {
        return Ok(h.null());
    }
    let quantity = truthy_or(h, get(h, &o, "quantity")?, || get(h, &o, "order_quantity"))?;
    let executed = nullish_or(h, key(h, &o, "totalExecutedQuantity")?, || {
        nullish_or(h, key(h, &o, "executed")?, || Ok(h.number(0.0)))
    })?;
    let id = string_value(
        h,
        truthy_or(h, key(h, &o, "order_id")?, || key(h, &o, "orderId"))?,
    )?;
    let kind = h.order_type(truthy_or(h, get(h, &o, "type")?, || {
        get(h, &o, "order_type")
    })?)?;
    let side = h.order_side(get(h, &o, "side")?)?;
    let price = truthy_or(h, get(h, &o, "price")?, || get(h, &o, "order_price"))?;
    let reduce = nullish_or(h, get(h, &o, "reduce_only")?, || get(h, &o, "reduceOnly"))?;
    let remaining = h.subtract(quantity.clone(), executed.clone())?;
    let now = h.virtual_now()?;
    h.object(vec![
        ("id", id),
        ("status", text(h, "open")),
        ("symbol", symbol),
        ("type", kind),
        ("side", side),
        ("price", price),
        ("amount", quantity),
        ("reduceOnly", reduce),
        ("filled", executed),
        ("remaining", remaining),
        ("lastUpdate", now),
        ("info", o),
    ])
}
pub fn map_algo<H: DataHost>(h: &H, r: Value<H>, o: Value<H>) -> Result<H, Value<H>> {
    let symbol = h.normalize_symbol(get(h, &o, "symbol")?)?;
    let market = h.get(
        &get(h, &get(h, &r, "store")?, "marketsBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&market) {
        return Ok(h.array(vec![]));
    }
    let children = get(h, &o, "childOrders")?;
    let children = if h.truthy(&children) {
        children
    } else {
        get(h, &o, "child_orders")?
    };
    let children = if h.truthy(&children) {
        truthy_or(h, get(h, &o, "childOrders")?, || get(h, &o, "child_orders"))?
    } else {
        h.array(vec![o.clone()])
    };
    let filtered = h.filter(children, |c| {
        Ok(h.truthy(&*key(h, &c, "trigger_price")?)
            || h.truthy(&*key(h, &c, "triggerPrice")?)
            || h.text_eq(&*key(h, &c, "algo_type")?, "TRAILING_STOP")
            || h.text_eq(&*key(h, &c, "algoType")?, "TRAILING_STOP"))
    })?;
    h.map(filtered, |c| {
        let filled = truthy_or(h, key(h, &c, "total_executed_quantity")?, || {
            key(h, &c, "totalExecutedQuantity")
        })?;
        let price = truthy_or(h, key(h, &c, "trigger_price")?, || {
            truthy_or(h, key(h, &c, "triggerPrice")?, || {
                h.subtract(key(h, &c, "extreme_price")?, key(h, &c, "callbackValue")?)
            })
        })?;
        let id = string_value(h, key(h, &c, "algo_order_id")?)?;
        let parent = truthy_or(h, key(h, &o, "root_algo_order_id")?, || {
            truthy_or(h, key(h, &c, "root_algo_order_id")?, || {
                truthy_or(h, key(h, &o, "rootAlgoOrderId")?, || {
                    key(h, &c, "rootAlgoOrderId")
                })
            })
        })?;
        let parent = string_value(h, parent)?;
        let kind = h.order_type(truthy_or(h, key(h, &c, "algo_type")?, || {
            key(h, &c, "algoType")
        })?)?;
        let side = h.order_side(get(h, &c, "side")?)?;
        let quantity = get(h, &c, "quantity")?;
        let reduce = truthy_or(h, key(h, &c, "reduce_only")?, || key(h, &c, "reduceOnly"))?;
        let remaining = h.subtract(get(h, &c, "quantity")?, filled.clone())?;
        let now = h.virtual_now()?;
        let copied = h.clone_deep(c.clone())?;
        let info = h.merge(copied, vec![])?;
        let margin = nullish_or(h, key(h, &c, "margin_mode")?, || key(h, &o, "margin_mode"))?;
        h.set(&info, &units("margin_mode"), margin)?;
        h.object(vec![
            ("id", id),
            ("parentId", parent),
            ("status", text(h, "open")),
            ("symbol", symbol.clone()),
            ("type", kind),
            ("side", side),
            ("price", price),
            ("amount", quantity),
            ("reduceOnly", reduce),
            ("filled", filled),
            ("remaining", remaining),
            ("lastUpdate", now),
            ("info", info),
        ])
    })
}
pub async fn positions<H: DataHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let result = async {
        let xhr = get(h, &r, "xhr")?;
        let f = get(h, &xhr, "get")?;
        let promise = h.call(f, Some(xhr), vec![endpoint(h, "POSITIONS")])?;
        let response = h.await_value(promise).await?;
        let data = get(h, &get(h, &response, "data")?, "data")?;
        let rows = get(h, &data, "rows")?;
        let total = get(h, &data, "total_collateral_value")?;
        let free = get(h, &data, "free_collateral")?;
        let promise = call(h, r.clone(), "getPositionMode", vec![])?;
        let mode = h.await_value(promise).await?;
        let real = h.reduce(rows, h.array(vec![]), |acc, p| {
            if !h.truthy(&*get(h, &p, "position_qty")?) {
                return Ok(acc);
            }
            let position = call(h, r.clone(), "mapPosition", vec![p, mode.clone()])?;
            if !h.truthy(&position) {
                return Ok(acc);
            }
            let mut values = h.spread(acc)?;
            values.push(position);
            Ok(h.array(values))
        })?;
        let markets = get(h, &get(h, &r, "store")?, "markets")?;
        let fake = h.reduce(markets, h.array(vec![]), |acc, m| {
            let f = get(h, &r, "getCurrentMarginType")?;
            let symbol = get(h, &m, "symbol")?;
            let margin = h.call(f, Some(r.clone()), vec![symbol])?;
            let mut values = vec![];
            for side in ["long", "short"] {
                let f = get(h, &r, "createOrderlyPositionId")?;
                let symbol = get(h, &m, "symbol")?;
                let side = text(h, side);
                let id = h.call(
                    f,
                    Some(r.clone()),
                    vec![symbol, side.clone(), mode.clone(), margin.clone()],
                )?;
                let position = h.object(vec![
                    ("id", id),
                    ("symbol", get(h, &m, "symbol")?),
                    ("side", side),
                    ("entryPrice", h.number(0.0)),
                    ("notional", h.number(0.0)),
                    ("leverage", get(h, &r, "accountLeverage")?),
                    ("unrealizedPnl", h.number(0.0)),
                    ("contracts", h.number(0.0)),
                    ("liquidationPrice", h.number(0.0)),
                    ("positionMode", mode.clone()),
                    ("marginType", margin.clone()),
                ])?;
                values.push(position);
            }
            h.push(acc.clone(), values)?;
            Ok(acc)
        })?;
        let store = get(h, &r, "store")?;
        let f = get(h, &store, "update")?;
        let current = get(h, &get(h, &r, "store")?, "balance")?;
        let balance = h.merge(current, vec![])?;
        h.set(&balance, &units("free"), free.clone())?;
        let used = h.arithmetic(Op::Sub, total.clone(), free)?;
        h.set(&balance, &units("used"), used)?;
        h.set(&balance, &units("total"), total)?;
        let patch = h.object(vec![("balance", balance)])?;
        h.call(f, Some(store), vec![patch])?;
        let mut values = h.spread(real)?;
        values.extend(h.spread(fake)?);
        h.uniq_id(h.array(values))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            log(h, &r, &e)?;
            get(h, &get(h, &r, "store")?, "positions")
        }
    }
}
pub async fn candles<H: DataHost>(h: &H, r: Value<H>, options: Value<H>) -> Result<H, Value<H>> {
    let xhr = get(h, &r, "xhr")?;
    let f = get(h, &xhr, "get")?;
    let symbol = h.reverse_symbol(get(h, &options, "symbol")?)?;
    let interval = get(h, &options, "interval")?;
    let limit = truthy_or(h, get(h, &options, "limit")?, || Ok(h.number(500.0)))?;
    let limit = h.arithmetic(Op::Min, limit, h.number(1000.0))?;
    let params = h.object(vec![
        ("symbol", symbol),
        ("type", interval),
        ("limit", limit),
    ])?;
    let config = h.object(vec![("params", params)])?;
    let promise = h.call(f, Some(xhr), vec![endpoint(h, "KLINE"), config])?;
    let response = h.await_value(promise).await?;
    let rows = get(h, &get(h, &get(h, &response, "data")?, "data")?, "rows")?;
    let candles = h.map(rows, |row| {
        h.object(vec![
            (
                "timestamp",
                h.arithmetic(Op::Div, get(h, &row, "start_timestamp")?, h.number(1000.0))?,
            ),
            ("open", get(h, &row, "open")?),
            ("high", get(h, &row, "high")?),
            ("low", get(h, &row, "low")?),
            ("close", get(h, &row, "close")?),
            ("volume", get(h, &row, "volume")?),
        ])
    })?;
    h.reverse(candles)
}
pub fn listen_candles<H: DataHost>(
    h: &H,
    r: Value<H>,
    options: Value<H>,
    callback: Value<H>,
) -> Result<H, Value<H>> {
    call(
        h,
        get(h, &r, "publicWebsocket")?,
        "listenOHLCV",
        vec![options, callback],
    )
}
pub fn listen_book<H: DataHost>(
    h: &H,
    r: Value<H>,
    symbol: Value<H>,
    callback: Value<H>,
) -> Result<H, Value<H>> {
    call(
        h,
        get(h, &r, "publicWebsocket")?,
        "listenOrderBook",
        vec![symbol, callback],
    )
}

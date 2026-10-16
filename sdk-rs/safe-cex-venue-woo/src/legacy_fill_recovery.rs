//! Legacy WOO fill-driven position recovery and private execution reports.
use crate::legacy_metadata::Task;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    Signature,
    Refresh,
    Positions,
    Schedule,
    Optimistic,
    OffFill,
    BaseDispose,
    ClearTimeout,
    ClearInterval,
    SetTimeout,
    MapLimit,
    MapAlgo,
    AddOrder,
    OrderStatus,
    Fill,
    AddPosition,
    UpdatePosition,
    AddPositions,
    ParseTimestamp,
    Normalize,
    PositionId,
    Emit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Map {
    Timers,
    Flight,
    Expected,
}
/// Compatible primitives preserve JS getters/values, original references and captured calls.
/// Sort and map iterators retain source locale-comparator and live Map iteration semantics.
pub trait Host: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    type Prepared;
    type MapIter;
    type Iterator;
    type MapTarget;
    fn prepare(&mut self, b: Binding) -> Result<Self::Prepared, Self::Error>;
    fn invoke(
        &mut self,
        c: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn call(
        &mut self,
        c: Self::Prepared,
        args: Vec<Self::Value>,
    ) -> Result<Task<Self::Value, Self::Error>, Self::Error>;
    fn root(&mut self, k: &str) -> Result<Self::Value, Self::Error>;
    fn set_root(&mut self, k: &str, v: Self::Value) -> Result<(), Self::Error>;
    fn field(&mut self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn key_field(&mut self, v: &Self::Value, k: Vec<u16>) -> Result<Self::Value, Self::Error>;
    fn v(&mut self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn length(&mut self, v: &Self::Value) -> Result<usize, Self::Error>;
    fn index(&mut self, v: &Self::Value, i: usize) -> Result<Self::Value, Self::Error>;
    fn has_index(&mut self, v: &Self::Value, i: usize) -> Result<bool, Self::Error>;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn is_undefined(&self, v: &Self::Value) -> bool;
    fn equals(&self, v: &Self::Value, s: &str) -> bool;
    fn same(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn number(&self, n: f64) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn string(&self, s: Vec<u16>) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn object(&self, f: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, v: Vec<Self::Value>) -> Self::Value;
    fn text(&mut self, v: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn number_coerce(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn operator_number(&mut self, v: &Self::Value) -> Result<f64, Self::Error>;
    fn add(&mut self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn sort_by_id(&mut self, v: Vec<Self::Value>) -> Result<Vec<Self::Value>, Self::Error>;
    fn prepare_map_set(&mut self, m: Map) -> Result<Self::MapTarget, Self::Error>;
    fn map_set_prepared(
        &mut self,
        target: Self::MapTarget,
        k: Self::Value,
        v: Self::Value,
    ) -> Result<(), Self::Error>;
    fn map_get(&mut self, m: Map, k: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn map_has(&mut self, m: Map, k: &Self::Value) -> Result<bool, Self::Error>;
    fn map_set(&mut self, m: Map, k: Self::Value, v: Self::Value) -> Result<(), Self::Error>;
    fn map_delete(&mut self, m: Map, k: &Self::Value) -> Result<(), Self::Error>;
    fn map_clear(&mut self, m: Map) -> Result<(), Self::Error>;
    fn map_iterator(&mut self, m: Map) -> Result<Self::MapIter, Self::Error>;
    fn map_next(&mut self, i: &mut Self::MapIter) -> Result<Option<Self::Value>, Self::Error>;
    fn iterator(&mut self, v: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&mut self, i: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn close_iterator(&mut self, i: &mut Self::Iterator) -> Result<(), Self::Error>;
    fn timeout(
        &mut self,
        c: Self::Prepared,
        delay: f64,
        callback: Rc<dyn Fn() -> Result<Task<Self::Value, Self::Error>, Self::Error>>,
    ) -> Result<Self::Value, Self::Error>;
    fn error_field(&mut self, e: &Self::Error, p: &[&str]) -> Result<Self::Value, Self::Error>;
    fn emit_error(&mut self, c: Self::Prepared, m: Self::Value) -> Result<(), Self::Error>;
    fn order_side(&mut self, v: Self::Value) -> Result<Self::Value, Self::Error>;
}
fn s<H: Host>(h: &H, v: &str) -> H::Value {
    h.string(v.encode_utf16().collect())
}
fn optional<H: Host>(h: &mut H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        h.field(v, k)
    }
}
fn invoke<H: Host>(h: &mut H, b: Binding, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let c = h.prepare(b)?;
    h.invoke(c, args)
}
fn call<H: Host>(
    h: &mut H,
    b: Binding,
    args: Vec<H::Value>,
) -> Result<Task<H::Value, H::Error>, H::Error> {
    let c = h.prepare(b)?;
    h.call(c, args)
}
fn emit<H: Host>(h: &mut H, e: &H::Error) -> Result<(), H::Error> {
    let c = h.prepare(Binding::Emit)?;
    let m = h.error_field(e, &["response", "data", "message"])?;
    let m = if h.truthy(&m) {
        m
    } else {
        h.error_field(e, &["message"])?
    };
    h.emit_error(c, m)
}
fn disposed<H: Host>(h: &mut H) -> Result<bool, H::Error> {
    let v = h.root("isDisposed")?;
    Ok(h.truthy(&v))
}
// ts: WOOXExchange#getPositionSignatureForSymbol
pub fn get_position_signature_for_symbol<H: Host>(
    h: &mut H,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let positions = h.root("positions")?;
    let n = h.length(&positions)?;
    let mut selected = vec![];
    for i in 0..n {
        if h.has_index(&positions, i)? {
            let p = h.index(&positions, i)?;
            let ps = h.field(&p, "symbol")?;
            if h.same(&ps, &symbol) {
                selected.push(p);
            }
        }
    }
    let positions = h.sort_by_id(selected)?;
    let mut signature = vec![];
    for (i, p) in positions.iter().enumerate() {
        if i > 0 {
            signature.push(124);
        }
        for (j, k) in ["id", "side", "contracts", "entryPrice"]
            .into_iter()
            .enumerate()
        {
            if j > 0 {
                signature.push(58);
            }
            let v = h.field(p, k)?;
            signature.extend(h.text(&v)?);
        }
    }
    Ok(h.string(signature))
}
// ts: WOOXExchange#scheduleFillPositionRefresh
pub fn schedule_fill_position_refresh<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: H::Value,
    attempt: f64,
) -> Result<(), H::Error> {
    let mut h = host.borrow_mut();
    if disposed(&mut *h)? {
        return Ok(());
    }
    let timer = h.map_get(Map::Timers, &symbol)?;
    if h.truthy(&timer) {
        invoke(&mut *h, Binding::ClearTimeout, vec![timer])?;
    }
    let delay = if attempt == 0.0 {
        0.0
    } else {
        let base = h.root("FILL_POSITION_REFRESH_BASE_DELAY_MS")?;
        h.operator_number(&base)?
            * safe_cex_num::math_pow(2.0, safe_cex_num::math_max(attempt - 1.0, 0.0))
    };
    let c = h.prepare(Binding::SetTimeout)?;
    let owner = host.clone();
    let key = symbol.clone();
    let timer = h.timeout(
        c,
        delay,
        Rc::new(move || {
            let read = {
                let mut h = owner.borrow_mut();
                h.map_delete(Map::Timers, &key)?;
                let a = h.number(attempt);
                call(&mut *h, Binding::Refresh, vec![key.clone(), a])?
            };
            let owner = owner.clone();
            Ok(Box::pin(async move {
                match read.await {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        let mut h = owner.borrow_mut();
                        emit(&mut *h, &e)?;
                        Ok(h.undefined())
                    }
                }
            }))
        }),
    )?;
    h.map_set(Map::Timers, symbol, timer)
}
fn refresh_finish<H: Host>(
    h: &mut H,
    symbol: H::Value,
    attempt: f64,
    result: Result<Option<bool>, H::Error>,
) -> Result<H::Value, H::Error> {
    let result = match result {
        Ok(v) => Ok(v),
        Err(e) => emit(h, &e).map(|()| Some(true)),
    };
    h.map_delete(Map::Flight, &symbol)?;
    let Some(retry) = result? else {
        return Ok(h.undefined());
    };
    if retry {
        let max = h.root("MAX_FILL_POSITION_REFRESH_ATTEMPTS")?;
        if attempt + 1.0 < h.operator_number(&max)? {
            let next = h.number(attempt + 1.0);
            invoke(h, Binding::Schedule, vec![symbol, next])?;
            return Ok(h.undefined());
        }
    }
    h.map_delete(Map::Expected, &symbol)?;
    Ok(h.undefined())
}
// ts: WOOXExchange#refreshPositionForSymbolFromFill
pub fn refresh_position_for_symbol_from_fill<H: Host>(
    host: Rc<RefCell<H>>,
    symbol: H::Value,
    attempt: f64,
) -> Task<H::Value, H::Error> {
    let guard = (|| {
        let mut h = host.borrow_mut();
        if disposed(&mut *h)? || h.map_has(Map::Flight, &symbol)? {
            return Ok(false);
        }
        let u = h.undefined();
        h.map_set(Map::Flight, symbol.clone(), u)?;
        Ok(true)
    })();
    match guard {
        Ok(false) => return Box::pin(std::future::ready(Ok(host.borrow().undefined()))),
        Err(e) => return Box::pin(std::future::ready(Err(e))),
        Ok(true) => {}
    }
    let first = (|| {
        let mut h = host.borrow_mut();
        let before = invoke(&mut *h, Binding::Signature, vec![symbol.clone()])?;
        let expected = h.map_get(Map::Expected, &symbol)?;
        let symbols = h.array(vec![symbol.clone()]);
        let read = call(&mut *h, Binding::Positions, vec![symbols])?;
        Ok((before, expected, read))
    })();
    let (before, expected, read) = match first {
        Ok(v) => v,
        Err(e) => {
            let result = refresh_finish(&mut *host.borrow_mut(), symbol, attempt, Err(e));
            return Box::pin(std::future::ready(result));
        }
    };
    Box::pin(async move {
        let result = async {
            let positions = read.await?;
            let mut h = host.borrow_mut();
            if disposed(&mut *h)? {
                return Ok(None);
            }
            invoke(&mut *h, Binding::AddPositions, vec![positions])?;
            let after = invoke(&mut *h, Binding::Signature, vec![symbol.clone()])?;
            let retry = if h.truthy(&expected) {
                !h.same(&after, &expected)
            } else {
                h.same(&before, &after)
            };
            if !retry {
                h.map_delete(Map::Expected, &symbol)?;
            }
            Ok(Some(retry))
        }
        .await;
        refresh_finish(&mut *host.borrow_mut(), symbol, attempt, result)
    })
}
fn sign(n: f64) -> f64 {
    if n.is_nan() || n == 0.0 {
        n
    } else if n > 0.0 {
        1.0
    } else {
        -1.0
    }
}
// ts: WOOXExchange#applyOptimisticOneWayPositionFromFill
pub fn apply_optimistic_one_way_position_from_fill<H: Host>(
    h: &mut H,
    fill: H::Value,
) -> Result<bool, H::Error> {
    let mode = h.root("_accountPositionMode")?;
    if !h.equals(&mode, "oneway") {
        return Ok(false);
    }
    let symbol = optional(h, &fill, "symbol")?;
    let side = optional(h, &fill, "side")?;
    let price = optional(h, &fill, "price")?;
    let price = h.number_coerce(&price)?;
    let qty = optional(h, &fill, "amount")?;
    let qty = h.number_coerce(&qty)?;
    if !h.truthy(&symbol)
        || !h.equals(&side, "buy") && !h.equals(&side, "sell")
        || !price.is_finite()
        || price <= 0.0
        || !qty.is_finite()
        || qty <= 0.0
    {
        return Ok(false);
    }
    let positions = h.root("positions")?;
    let n = h.length(&positions)?;
    let mut current = None;
    for i in 0..n {
        let p = h.index(&positions, i)?;
        let ps = h.field(&p, "symbol")?;
        if h.same(&ps, &symbol) {
            let contracts = h.field(&p, "contracts")?;
            if h.operator_number(&contracts)? > 0.0 {
                current = Some(p);
                break;
            }
        }
    }
    let signed_current = if let Some(p) = &current {
        let side = h.field(p, "side")?;
        let contracts = h.field(p, "contracts")?;
        if h.equals(&side, "long") {
            contracts
        } else {
            let n = -h.operator_number(&contracts)?;
            h.number(n)
        }
    } else {
        h.number(0.0)
    };
    let signed_fill = if h.equals(&side, "buy") { qty } else { -qty };
    let fill_contracts = h.number(signed_fill);
    let mut next = h.add(signed_current.clone(), fill_contracts)?;
    if h.operator_number(&next)?.abs() < 1e-10 {
        next = h.number(0.0);
    }
    let contracts = h.operator_number(&next)?.abs();
    let next_side = if h.operator_number(&next)? >= 0.0 {
        "long"
    } else {
        "short"
    };
    if let Some(p) = &current {
        let reset = if contracts == 0.0 {
            true
        } else {
            let old = h.field(p, "side")?;
            !h.equals(&old, next_side)
        };
        if reset {
            let c = h.prepare(Binding::UpdatePosition)?;
            let id = h.field(p, "id")?;
            let selector = h.object(vec![("id", id)]);
            let zero = h.number(0.0);
            let patch = h.object(
                [
                    "entryPrice",
                    "notional",
                    "unrealizedPnl",
                    "contracts",
                    "liquidationPrice",
                ]
                .into_iter()
                .map(|k| (k, zero.clone()))
                .collect(),
            );
            h.invoke(c, vec![selector, patch])?;
        }
    }
    if contracts == 0.0 {
        return Ok(true);
    }
    let tickers = h.root("tickersBySymbol")?;
    let key = h.text(&symbol)?;
    let ticker = h.key_field(&tickers, key)?;
    let mark = optional(h, &ticker, "mark")?;
    let mark = h.number_coerce(&mark)?;
    let mark = if mark.is_finite() && mark > 0.0 {
        mark
    } else {
        price
    };
    let previous = h.operator_number(&signed_current)?.abs();
    let same_direction = sign(h.operator_number(&signed_current)?)
        == sign(h.operator_number(&next)?)
        && sign(h.operator_number(&signed_current)?) != 0.0;
    let mut entry = h.number(price);
    if let Some(p) = &current {
        if same_direction && contracts <= previous {
            entry = h.field(p, "entryPrice")?;
        } else if same_direction && previous > 0.0 {
            let old = h.field(p, "entryPrice")?;
            let old = h.operator_number(&old)?;
            entry = h.number((old * previous + price * signed_fill.abs()) / contracts);
        }
    }
    let mode = if let Some(p) = &current {
        h.field(p, "positionMode")?
    } else {
        h.undefined()
    };
    let mode = if h.truthy(&mode) {
        mode
    } else {
        s(h, "oneway")
    };
    let leverage = if let Some(p) = &current {
        h.field(p, "leverage")?
    } else {
        h.undefined()
    };
    let leverage = if h.truthy(&leverage) {
        leverage
    } else {
        h.number(1.0)
    };
    let margin = if let Some(p) = &current {
        h.field(p, "marginType")?
    } else {
        h.undefined()
    };
    let margin = if h.truthy(&margin) {
        margin
    } else {
        s(h, "cross")
    };
    let next_side = s(h, next_side);
    let id = invoke(
        h,
        Binding::PositionId,
        vec![symbol.clone(), next_side.clone(), mode.clone()],
    )?;
    let upnl = if let Some(p) = &current {
        h.field(p, "unrealizedPnl")?
    } else {
        h.undefined()
    };
    let upnl = if h.truthy(&upnl) { upnl } else { h.number(0.0) };
    let liquidation = if let Some(p) = &current {
        h.field(p, "liquidationPrice")?
    } else {
        h.undefined()
    };
    let liquidation = if h.truthy(&liquidation) {
        liquidation
    } else {
        h.number(0.0)
    };
    let position = h.object(vec![
        ("id", id),
        ("symbol", symbol),
        ("side", next_side),
        ("entryPrice", entry),
        ("notional", h.number(contracts * mark)),
        ("leverage", leverage),
        ("unrealizedPnl", upnl),
        ("contracts", h.number(contracts)),
        ("liquidationPrice", liquidation),
        ("positionMode", mode),
        ("marginType", margin),
    ]);
    invoke(h, Binding::AddPosition, vec![position])?;
    Ok(true)
}
// ts: WOOXExchange#handleFillForPositionSync
pub fn handle_fill_for_position_sync<H: Host>(h: &mut H, fill: H::Value) -> Result<(), H::Error> {
    let symbol = optional(h, &fill, "symbol")?;
    if !h.truthy(&symbol) {
        return Ok(());
    }
    let applied = invoke(h, Binding::Optimistic, vec![fill])?;
    let positions = h.root("positions")?;
    let n = h.length(&positions)?;
    let mut open = false;
    for i in 0..n {
        if h.has_index(&positions, i)? {
            let p = h.index(&positions, i)?;
            let ps = h.field(&p, "symbol")?;
            if h.same(&ps, &symbol) {
                let amount = h.field(&p, "contracts")?;
                if h.operator_number(&amount)? > 0.0 {
                    open = true;
                    break;
                }
            }
        }
    }
    if h.truthy(&applied) && open {
        let target = h.prepare_map_set(Map::Expected)?;
        let signature = invoke(h, Binding::Signature, vec![symbol.clone()])?;
        h.map_set_prepared(target, symbol.clone(), signature)?;
    } else {
        h.map_delete(Map::Expected, &symbol)?;
    }
    let zero = h.number(0.0);
    invoke(h, Binding::Schedule, vec![symbol, zero])?;
    Ok(())
}
// ts: WOOXExchange.dispose
pub fn dispose<H: Host>(h: &mut H) -> Result<(), H::Error> {
    let c = h.prepare(Binding::OffFill)?;
    let handler = h.root("handleFillForPositionSync")?;
    let event = s(h, "fill");
    h.invoke(c, vec![event, handler])?;
    let mut it = h.map_iterator(Map::Timers)?;
    while let Some(timer) = h.map_next(&mut it)? {
        invoke(h, Binding::ClearTimeout, vec![timer])?;
    }
    for map in [Map::Timers, Map::Flight, Map::Expected] {
        h.map_clear(map)?;
    }
    let timer = h.root("_fundingRefreshTimer")?;
    if !h.is_undefined(&timer) {
        let c = h.prepare(Binding::ClearInterval)?;
        let timer = h.root("_fundingRefreshTimer")?;
        h.invoke(c, vec![timer])?;
        let u = h.undefined();
        h.set_root("_fundingRefreshTimer", u)?;
    }
    let no = h.boolean(false);
    h.set_root("_fundingRefreshInFlight", no)?;
    invoke(h, Binding::BaseDispose, vec![])?;
    Ok(())
}
fn first_v<H: Host>(
    h: &mut H,
    v: &H::Value,
    keys: &[&str],
    fallback: bool,
) -> Result<H::Value, H::Error> {
    for (i, k) in keys.iter().enumerate() {
        let value = h.v(v, k)?;
        if h.truthy(&value) || i + 1 == keys.len() && !fallback {
            return Ok(value);
        }
    }
    Ok(s(h, ""))
}
fn timestamp<H: Host>(h: &mut H, v: &H::Value) -> Result<H::Value, H::Error> {
    let c = h.prepare(Binding::ParseTimestamp)?;
    let raw = first_v(
        h,
        v,
        &[
            "executedTimestamp",
            "tradeTime",
            "updatedTime",
            "updated_time",
            "timestamp",
        ],
        false,
    )?;
    h.invoke(c, vec![raw])
}
fn fill_payload<H: Host>(
    h: &H,
    id: H::Value,
    order_id: H::Value,
    side: H::Value,
    symbol: H::Value,
    price: H::Value,
    amount: H::Value,
    time: H::Value,
) -> H::Value {
    let mut f = vec![
        ("id", id),
        ("orderId", order_id),
        ("side", side),
        ("symbol", symbol),
        ("price", price),
        ("amount", amount),
    ];
    if h.truthy(&time) {
        f.push(("timestamp", time));
    }
    h.object(f)
}
// ts: WooPrivateWebscoket.handleExecutionReport
pub fn handle_execution_report<H: Host>(h: &mut H, data: H::Value) -> Result<(), H::Error> {
    let updated = invoke(h, Binding::MapLimit, vec![data.clone()])?;
    let status = h.v(&data, "status")?;
    let partial = h.equals(&status, "PARTIAL_FILLED") || h.equals(&status, "PARTIALLY_FILLED");
    let price = h.v(&data, "executedPrice")?;
    let amount = h.v(&data, "executedQuantity")?;
    let id = first_v(h, &data, &["orderId", "order_id"], true)?;
    let id = h.text(&id)?;
    let id = h.string(id);
    let time = timestamp(h, &data)?;
    if (h.equals(&status, "REPLACED") || h.equals(&status, "NEW") || partial) && h.truthy(&updated)
    {
        invoke(h, Binding::AddOrder, vec![updated.clone()])?;
    }
    if (h.equals(&status, "CANCELLED") || h.equals(&status, "FILLED")) && h.truthy(&id) {
        let selector = h.object(vec![("id", id.clone())]);
        let value = s(
            h,
            if h.equals(&status, "CANCELLED") {
                "canceled"
            } else {
                "closed"
            },
        );
        invoke(h, Binding::OrderStatus, vec![selector, value])?;
    }
    if h.equals(&status, "FILLED") || partial {
        let side = optional(h, &updated, "side")?;
        let side = if h.truthy(&side) {
            side
        } else {
            let raw = h.v(&data, "side")?;
            h.order_side(raw)?
        };
        let symbol = optional(h, &updated, "symbol")?;
        let symbol = if h.truthy(&symbol) {
            symbol
        } else {
            let raw = h.v(&data, "symbol")?;
            if h.truthy(&raw) {
                let c = h.prepare(Binding::Normalize)?;
                let raw = h.v(&data, "symbol")?;
                h.invoke(c, vec![raw])?
            } else {
                h.undefined()
            }
        };
        if !h.truthy(&side) || !h.truthy(&symbol) {
            return Ok(());
        }
        let c = h.prepare(Binding::Fill)?;
        let payload_id = if h.truthy(&id) {
            id.clone()
        } else {
            first_v(h, &data, &["id", "orderId", "order_id"], false)?
        };
        let order_id = if h.truthy(&id) { id } else { h.undefined() };
        let payload = fill_payload(h, payload_id, order_id, side, symbol, price, amount, time);
        h.invoke(c, vec![payload])?;
    }
    Ok(())
}
// ts: WooPrivateWebscoket.handleAlgoExecutionReport
pub fn handle_algo_execution_report<H: Host>(h: &mut H, data: H::Value) -> Result<(), H::Error> {
    let rows = if h.is_array(&data) {
        data
    } else if h.truthy(&data) {
        h.array(vec![data])
    } else {
        h.array(vec![])
    };
    let n = h.length(&rows)?;
    for i in 0..n {
        if !h.has_index(&rows, i)? {
            continue;
        }
        let row = h.index(&rows, i)?;
        let status = h.v(&row, "algoStatus")?;
        if ["NEW", "REPLACED", "PARTIAL_FILLED", "PARTIALLY_FILLED"]
            .into_iter()
            .any(|s| h.equals(&status, s))
        {
            let mapped = invoke(h, Binding::MapAlgo, vec![row.clone()])?;
            let mut it = h.iterator(mapped)?;
            let first = h.next(&mut it)?;
            if first.is_some() {
                h.close_iterator(&mut it)?;
            }
            let updated = first.unwrap_or_else(|| h.undefined());
            if h.truthy(&updated) {
                invoke(h, Binding::AddOrder, vec![updated])?;
            }
        }
        if h.equals(&status, "FILLED") {
            let id = first_v(h, &row, &["orderId", "algoOrderId", "id"], true)?;
            let id = h.text(&id)?;
            let id = h.string(id);
            let time = timestamp(h, &row)?;
            let c = h.prepare(Binding::Fill)?;
            let payload_id = if h.truthy(&id) {
                id.clone()
            } else {
                first_v(h, &row, &["orderId", "algoOrderId", "id"], false)?
            };
            let order_id = if h.truthy(&id) { id } else { h.undefined() };
            let side = h.field(&row, "side")?;
            let side = h.order_side(side)?;
            let norm = h.prepare(Binding::Normalize)?;
            let symbol = h.field(&row, "symbol")?;
            let symbol = h.invoke(norm, vec![symbol])?;
            let price = h.field(&row, "averageExecutedPrice")?;
            let amount = h.field(&row, "totalExecutedQuantity")?;
            let payload = fill_payload(h, payload_id, order_id, side, symbol, price, amount, time);
            h.invoke(c, vec![payload])?;
        }
        if h.equals(&status, "CANCELLED") || h.equals(&status, "FILLED") {
            let id = first_v(h, &row, &["algoOrderId", "orderId", "id"], true)?;
            let id = h.text(&id)?;
            let id = h.string(id);
            if !h.truthy(&id) {
                continue;
            }
            let c = h.prepare(Binding::OrderStatus)?;
            let selector = h.object(vec![("id", id)]);
            let status = s(
                h,
                if h.equals(&status, "CANCELLED") {
                    "canceled"
                } else {
                    "closed"
                },
            );
            h.invoke(c, vec![selector, status])?;
        }
    }
    Ok(())
}

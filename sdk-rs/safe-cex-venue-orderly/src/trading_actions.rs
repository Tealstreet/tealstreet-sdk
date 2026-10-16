//! Cancellation, algo editing, leverage and margin action bodies.
use crate::{
    account_order_reads::AccountReadHost,
    ws_controls::{Result, Value},
};
use std::future::Future;
pub trait ActionHost: AccountReadHost {
    fn has(&self, value: &Value<Self>, key: &str) -> Result<Self, bool>;
    fn strict_eq(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn finite_number(&self, value: &Self::Value) -> bool;
    fn true_value(&self) -> Value<Self>;
    fn numeric(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn parse_int(&self, value: Value<Self>, radix: u32) -> Result<Self, Value<Self>>;
    fn math_min(&self, left: Value<Self>, right: Value<Self>) -> Result<Self, Value<Self>>;
    fn math_max(&self, left: Value<Self>, right: Value<Self>) -> Result<Self, Value<Self>>;
    fn math_round(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn reverse_symbol(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn new_error(&self, message: Value<Self>) -> Self::Error;
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
    fn find(
        &self,
        rows: Value<Self>,
        predicate: impl FnMut(Value<Self>) -> Result<Self, bool>,
    ) -> Result<Self, Value<Self>>;
    fn series<F: Future<Output = Result<Self, ()>>>(
        &self,
        rows: Value<Self>,
        callback: impl FnMut(Value<Self>) -> F,
    ) -> impl Future<Output = Result<Self, ()>>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: ActionHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn get<H: ActionHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &units(key))
}
fn opt<H: ActionHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, key)
    }
}
fn endpoint<H: ActionHost>(h: &H, name: &str) -> Value<H> {
    text(
        h,
        crate::types::ENDPOINTS
            .iter()
            .find(|(k, _)| *k == name)
            .expect("mapped endpoint")
            .1,
    )
}
fn call<H: ActionHost>(
    h: &H,
    r: Value<H>,
    method: &str,
    args: Vec<Value<H>>,
) -> Result<H, Value<H>> {
    let f = get(h, &r, method)?;
    h.call(f, Some(r), args)
}
fn logger<H: ActionHost>(h: &H, r: &Value<H>, error: &H::Error) -> Result<H, ()> {
    let f = get(h, r, "emitAndLogError")?;
    let error = h.error_value(error);
    let response = opt(h, &error, "response")?;
    let data = opt(h, &response, "data")?;
    let message = opt(h, &data, "message")?;
    let message = if h.truthy(&message) {
        message
    } else {
        opt(h, &error, "message")?
    };
    h.call(f, Some(r.clone()), vec![message])?;
    Ok(())
}
fn market_error<H: ActionHost>(h: &H, symbol: &Value<H>) -> Result<H, H::Error> {
    Ok(h.new_error(h.text(&[units("Market "), h.string(symbol)?, units(" not found")].concat())))
}
async fn checked_post<H: ActionHost>(
    h: &H,
    xhr: Value<H>,
    function: Value<H>,
    name: &str,
    payload: Value<H>,
    fallback: &str,
) -> Result<H, ()> {
    let promise = h.call(function, Some(xhr), vec![endpoint(h, name), payload])?;
    let response = h.await_value(promise).await?;
    let data = get(h, &response, "data")?;
    if !h.strict_eq(&*opt(h, &data, "success")?, &*h.true_value()) {
        let message = opt(h, &data, "message")?;
        return Err(h.new_error(if h.truthy(&message) {
            message
        } else {
            text(h, fallback)
        }));
    }
    Ok(())
}
pub async fn update_algo<H: ActionHost>(
    h: &H,
    r: Value<H>,
    options: Value<H>,
) -> Result<H, Value<H>> {
    let order = get(h, &options, "order")?;
    let update = get(h, &options, "update")?;
    let payload = h.object(vec![])?;
    let parent = get(h, &order, "parentId")?;
    let parent = if h.truthy(&parent) {
        parent
    } else {
        h.number(0.0)
    };
    h.set(&payload, &units("order_id"), h.numeric(parent)?)?;
    if h.has(&update, "price")? {
        let child = h.object(vec![
            ("order_id", get(h, &order, "id")?),
            ("trigger_price", h.numeric(get(h, &update, "price")?)?),
        ])?;
        h.set(&payload, &units("child_orders"), h.array(vec![child]))?;
    }
    let operation = async {
        let xhr = get(h, &r, "xhr")?;
        let f = get(h, &xhr, "put")?;
        let promise = h.call(f, Some(xhr), vec![endpoint(h, "ALGO_ORDER"), payload])?;
        h.await_value(promise).await?;
        Ok(h.array(vec![get(h, &order, "id")?]))
    }
    .await;
    match operation {
        Ok(v) => Ok(v),
        Err(e) => {
            logger(h, &r, &e)?;
            Ok(h.array(vec![]))
        }
    }
}
pub async fn cancel_orders<H: ActionHost>(
    h: &H,
    r: Value<H>,
    orders: Value<H>,
) -> Result<H, Value<H>> {
    h.series(orders, |order| {
        let r = r.clone();
        async move {
            let function = get(h, &r, "isAlgoOrder")?;
            let kind = get(h, &order, "type")?;
            let algo = h.call(function, Some(r.clone()), vec![kind])?;
            if h.truthy(&algo) {
                let promise = call(h, r, "cancelAlgoOrder", vec![order])?;
                h.await_value(promise).await?;
            } else {
                let xhr = get(h, &r, "xhr")?;
                let f = get(h, &xhr, "delete")?;
                let url = [
                    h.string(&endpoint(h, "CANCEL_ORDER"))?,
                    units("?order_id="),
                    h.string(&*h.parse_int(get(h, &order, "id")?, 10)?)?,
                    units("&symbol="),
                    h.string(&*h.reverse_symbol(get(h, &order, "symbol")?)?)?,
                ]
                .concat();
                let empty = h.object(vec![])?;
                let config = h.object(vec![("data", empty)])?;
                let promise = h.call(f, Some(xhr), vec![h.text(&url), config])?;
                h.await_value(promise).await?;
            }
            Ok(())
        }
    })
    .await?;
    Ok(h.undefined())
}
pub async fn cancel_algo<H: ActionHost>(
    h: &H,
    r: Value<H>,
    order: Value<H>,
) -> Result<H, Value<H>> {
    let operation = async {
        let orders = get(h, &get(h, &r, "store")?, "orders")?;
        let sibling = h.find(orders, |s| {
            Ok(
                h.strict_eq(&*get(h, &s, "parentId")?, &*get(h, &order, "parentId")?)
                    && !h.strict_eq(&*get(h, &s, "id")?, &*get(h, &order, "id")?),
            )
        })?;
        let id = get(h, &order, "parentId")?;
        let id = if h.truthy(&id) {
            id
        } else {
            get(h, &order, "id")?
        };
        let xhr = get(h, &r, "xhr")?;
        let f = get(h, &xhr, "delete")?;
        let url = [
            h.string(&endpoint(h, "ALGO_ORDER"))?,
            units("?order_id="),
            h.string(&id)?,
            units("&symbol="),
            h.string(&*h.reverse_symbol(get(h, &order, "symbol")?)?)?,
        ]
        .concat();
        let config = h.object(vec![("data", h.object(vec![])?)])?;
        let promise = h.call(f, Some(xhr), vec![h.text(&url), config])?;
        h.await_value(promise).await?;
        if h.truthy(&sibling) {
            let key = if h.text_eq(&*get(h, &sibling, "type")?, "stop_market") {
                "stopLoss"
            } else {
                "takeProfit"
            };
            let function = get(h, &r, "mapMarginType")?;
            let raw = h.get_key(get(h, &sibling, "info")?, "margin_mode")?;
            let margin = h.call(function, Some(r.clone()), vec![raw])?;
            let function = get(h, &r, "placePositionalAlgoOrder")?;
            let symbol = get(h, &sibling, "symbol")?;
            let side = get(h, &sibling, "side")?;
            let price = get(h, &sibling, "price")?;
            let mode = call(h, r.clone(), "toOrderlyMarginMode", vec![margin])?;
            let payload = h.object(vec![
                ("symbol", symbol),
                ("side", side),
                (key, price),
                ("marginMode", mode),
            ])?;
            let promise = h.call(function, Some(r.clone()), vec![payload])?;
            h.await_value(promise).await?;
        }
        Ok(())
    }
    .await;
    if let Err(e) = operation {
        let cause = h.error_value(&e);
        let response = opt(h, &cause, "response")?;
        let data = opt(h, &response, "data")?;
        let message = opt(h, &data, "message")?;
        if !h.nullish(&message) && h.includes(&message, &units("already canceled"))? {
            let store = get(h, &r, "store")?;
            let f = get(h, &store, "updateOrderStatus")?;
            let selector = h.object(vec![("id", get(h, &order, "id")?)])?;
            h.call(f, Some(store), vec![selector, text(h, "canceled")])?;
        } else {
            logger(h, &r, &e)?;
        }
    }
    Ok(h.undefined())
}
async fn cancel_both<H: ActionHost>(
    h: &H,
    r: Value<H>,
    symbol: Option<Value<H>>,
) -> Result<H, Value<H>> {
    let normalized = match symbol {
        Some(v) => Some(h.reverse_symbol(v)?),
        None => None,
    };
    for name in [
        if normalized.is_some() {
            "CANCEL_SYMBOL_ORDERS"
        } else {
            "CANCEL_ORDERS"
        },
        "CANCEL_ALGO_ORDERS",
    ] {
        let operation = async {
            let xhr = get(h, &r, "xhr")?;
            let f = get(h, &xhr, "delete")?;
            let mut args = vec![endpoint(h, name)];
            if let Some(symbol) = &normalized {
                args.push(h.object(vec![(
                    "params",
                    h.object(vec![("symbol", symbol.clone())])?,
                )])?);
            }
            let promise = h.call(f, Some(xhr), args)?;
            h.await_value(promise).await?;
            Ok(())
        }
        .await;
        if let Err(e) = operation {
            logger(h, &r, &e)?;
        }
    }
    Ok(h.undefined())
}
pub async fn cancel_all<H: ActionHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    cancel_both(h, r, None).await
}
pub async fn cancel_symbol<H: ActionHost>(
    h: &H,
    r: Value<H>,
    symbol: Value<H>,
) -> Result<H, Value<H>> {
    cancel_both(h, r, Some(symbol)).await
}
pub async fn set_leverage<H: ActionHost>(
    h: &H,
    r: Value<H>,
    symbol: Value<H>,
    input: Value<H>,
) -> Result<H, Value<H>> {
    let markets = get(h, &get(h, &r, "store")?, "marketsBySymbol")?;
    let market = h.get(&markets, &h.string(&symbol)?)?;
    if !h.truthy(&market) {
        return Err(market_error(h, &symbol)?);
    }
    if !h.finite_number(&input) {
        return Err(h.new_error(text(h, "Orderly leverage must be a finite number")));
    }
    let margin = call(h, r.clone(), "getCurrentMarginType", vec![symbol.clone()])?;
    let maximum = get(h, &get(h, &get(h, &market, "limits")?, "leverage")?, "max")?;
    let minimum = get(h, &get(h, &get(h, &market, "limits")?, "leverage")?, "min")?;
    let clamped = h.math_min(maximum, h.math_max(minimum, input)?)?;
    let leverage = h.math_round(clamped)?;
    let normalized = h.reverse_symbol(symbol.clone())?;
    let mode = call(h, r.clone(), "toOrderlyMarginMode", vec![margin.clone()])?;
    let payload = h.object(vec![
        ("leverage", leverage.clone()),
        ("symbol", normalized),
        ("margin_mode", mode),
    ])?;
    let operation = async {
        let xhr = get(h, &r, "xhr")?;
        let function = get(h, &xhr, "post")?;
        checked_post(
            h,
            xhr,
            function,
            "LEVERAGE",
            payload,
            "Orderly leverage update failed",
        )
        .await?;
        if h.text_eq(&margin, "cross") {
            h.set(&r, &units("accountLeverage"), leverage.clone())?;
        }
        let store = get(h, &r, "store")?;
        let function = get(h, &store, "updatePositions")?;
        let positions = get(h, &get(h, &r, "store")?, "positions")?;
        let positions = h.filter(positions, |p| {
            Ok(h.strict_eq(&*get(h, &p, "symbol")?, &symbol)
                && h.strict_eq(&*get(h, &p, "marginType")?, &margin))
        })?;
        let patches = h.map(positions, |p| {
            Ok(h.array(vec![p, h.object(vec![("leverage", leverage.clone())])?]))
        })?;
        h.call(function, Some(store), vec![patches])?;
        Ok(())
    }
    .await;
    if let Err(e) = operation {
        logger(h, &r, &e)?;
        return Err(e);
    }
    Ok(h.undefined())
}
pub async fn set_all_leverage<H: ActionHost>(
    h: &H,
    r: Value<H>,
    input: Value<H>,
) -> Result<H, Value<H>> {
    if !h.finite_number(&input) {
        return Err(h.new_error(text(h, "Orderly leverage must be a finite number")));
    }
    let leverage = h.math_round(h.math_min(h.number(100.0), h.math_max(h.number(1.0), input)?)?)?;
    if !h.strict_eq(&*get(h, &r, "accountLeverage")?, &leverage)
        && !h.truthy(&*get(h, &r, "isDisposed")?)
    {
        let operation = async {
            let xhr = get(h, &r, "xhr")?;
            let f = get(h, &xhr, "post")?;
            let payload = h.object(vec![
                ("leverage", leverage.clone()),
                ("margin_mode", text(h, "CROSS")),
            ])?;
            checked_post(
                h,
                xhr,
                f,
                "LEVERAGE",
                payload,
                "Orderly leverage update failed",
            )
            .await?;
            h.set(&r, &units("accountLeverage"), leverage)?;
            Ok(())
        }
        .await;
        if let Err(e) = operation {
            logger(h, &r, &e)?;
        }
    }
    Ok(h.undefined())
}
pub async fn change_margin<H: ActionHost>(
    h: &H,
    r: Value<H>,
    isolated: Value<H>,
    symbol: Value<H>,
) -> Result<H, Value<H>> {
    if !h.truthy(&symbol) {
        return Err(h.new_error(text(h, "Orderly margin mode update requires a symbol")));
    }
    let markets = get(h, &get(h, &r, "store")?, "marketsBySymbol")?;
    if !h.truthy(&*h.get(&markets, &h.string(&symbol)?)?) {
        return Err(market_error(h, &symbol)?);
    }
    let margin = text(
        h,
        if h.truthy(&isolated) {
            "isolated"
        } else {
            "cross"
        },
    );
    let operation = async {
        let xhr = get(h, &r, "xhr")?;
        let f = get(h, &xhr, "post")?;
        let normalized = h.reverse_symbol(symbol.clone())?;
        let mode = call(h, r.clone(), "toOrderlyMarginMode", vec![margin.clone()])?;
        let payload = h.object(vec![("symbol", normalized), ("default_margin_mode", mode)])?;
        checked_post(
            h,
            xhr,
            f,
            "MARGIN_MODE",
            payload,
            "Orderly margin mode update failed",
        )
        .await?;
        call(
            h,
            get(h, &r, "marginTypeBySymbol")?,
            "set",
            vec![symbol, margin],
        )?;
        Ok(())
    }
    .await;
    if let Err(e) = operation {
        logger(h, &r, &e)?;
        return Err(e);
    }
    Ok(h.undefined())
}

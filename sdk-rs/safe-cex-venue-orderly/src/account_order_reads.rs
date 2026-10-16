//! Account/order REST bodies retaining runtime values, calls and await boundaries.
use crate::ws_controls::{Result, Value, WsHost};
use std::future::Future;
pub trait AccountReadHost: WsHost {
    fn await_value(&self, value: Value<Self>) -> impl Future<Output = Result<Self, Value<Self>>>;
    fn error_value(&self, error: &Self::Error) -> Value<Self>;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn is_null(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn text_eq(&self, value: &Self::Value, expected: &str) -> bool;
    fn undefined(&self) -> Value<Self>;
    fn null(&self) -> Value<Self>;
    fn number(&self, value: f64) -> Value<Self>;
    fn array(&self, values: Vec<Value<Self>>) -> Value<Self>;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn spread(&self, value: Value<Self>) -> Result<Self, Vec<Value<Self>>>;
    fn reduce(
        &self,
        rows: Value<Self>,
        initial: Value<Self>,
        callback: impl FnMut(Value<Self>, Value<Self>) -> Result<Self, Value<Self>>,
    ) -> Result<Self, Value<Self>>;
    fn normalize_symbol(&self, value: Value<Self>) -> Result<Self, Value<Self>>;
    fn get_key(&self, value: Value<Self>, key: &str) -> Result<Self, Value<Self>>;
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: AccountReadHost>(h: &H, s: &str) -> Value<H> {
    h.text(&units(s))
}
fn get<H: AccountReadHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    h.get(v, &units(key))
}
fn optional<H: AccountReadHost>(h: &H, v: &Value<H>, key: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, key)
    }
}
fn call<H: AccountReadHost>(
    h: &H,
    r: Value<H>,
    method: &str,
    args: Vec<Value<H>>,
) -> Result<H, Value<H>> {
    let f = get(h, &r, method)?;
    h.call(f, Some(r), args)
}
fn endpoint<H: AccountReadHost>(h: &H, name: &str) -> Value<H> {
    text(
        h,
        crate::types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == name)
            .expect("mapped source endpoint")
            .1,
    )
}
async fn request<H: AccountReadHost>(
    h: &H,
    r: &Value<H>,
    name: &str,
    incomplete: bool,
) -> Result<H, Value<H>> {
    let xhr = get(h, r, "xhr")?;
    let f = get(h, &xhr, "get")?;
    let mut args = vec![endpoint(h, name)];
    if incomplete {
        let params = h.object(vec![("status", text(h, "INCOMPLETE"))])?;
        args.push(h.object(vec![("params", params)])?);
    }
    let promise = h.call(f, Some(xhr), args)?;
    h.await_value(promise).await
}
fn nested_data<H: AccountReadHost>(h: &H, response: Value<H>) -> Result<H, Value<H>> {
    get(h, &get(h, &response, "data")?, "data")
}
fn log_message<H: AccountReadHost>(h: &H, r: &Value<H>, error: &H::Error) -> Result<H, ()> {
    let logger = get(h, r, "emitAndLogError")?;
    let cause = h.error_value(error);
    let response = optional(h, &cause, "response")?;
    let data = optional(h, &response, "data")?;
    let message = optional(h, &data, "message")?;
    let message = if h.truthy(&message) {
        message
    } else {
        optional(h, &cause, "message")?
    };
    h.call(logger, Some(r.clone()), vec![message])?;
    Ok(())
}
pub async fn position_mode<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    if !h.truthy(&*get(h, &r, "_accountPositionMode")?) {
        request(h, &r, "ACCOUNT", false).await?;
        h.set(&r, &units("_accountPositionMode"), text(h, "oneway"))?;
    }
    get(h, &r, "_accountPositionMode")
}
pub async fn account<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let data = nested_data(h, request(h, &r, "ACCOUNT", false).await?)?;
    let user_id = get(h, &data, "account_id")?;
    let affiliate = get(h, &data, "account_id")?;
    let affiliate = if h.truthy(&affiliate) {
        affiliate
    } else {
        h.null()
    };
    h.object(vec![("userId", user_id), ("affiliateId", affiliate)])
}
pub async fn validate<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let operation = async {
        request(h, &r, "ACCOUNT", false).await?;
        h.object(vec![("error", text(h, ""))])
    }
    .await;
    match operation {
        Ok(value) => Ok(value),
        Err(error) => {
            log_message(h, &r, &error)?;
            let cause = h.error_value(&error);
            let response = optional(h, &cause, "response")?;
            let data = optional(h, &response, "data")?;
            let mut message = optional(h, &data, "msg")?;
            if !h.truthy(&message) {
                message = optional(h, &cause, "message")?;
            }
            if !h.truthy(&message) {
                message = text(h, "Invalid API key or secret");
            }
            h.object(vec![("error", message)])
        }
    }
}
pub async fn leverage<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let data = nested_data(h, request(h, &r, "ACCOUNT", false).await?)?;
    let value = optional(h, &data, "max_leverage")?;
    let value = if h.truthy(&value) {
        value
    } else {
        h.number(20.0)
    };
    h.set(&r, &units("accountLeverage"), value)?;
    h.set(&r, &units("_accountPositionMode"), text(h, "oneway"))?;
    Ok(h.undefined())
}
pub async fn margin_types<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let response = request(h, &r, "MARGIN_MODES", false).await?;
    let data = optional(h, &get(h, &response, "data")?, "data")?;
    let rows = optional(h, &data, "rows")?;
    if !h.is_array(&rows) {
        return Ok(h.undefined());
    }
    let mut iterator = h.iterator(rows)?;
    while let Some(row) = h.next(&mut iterator)? {
        if !h.is_string(&*optional(h, &row, "symbol")?) {
            continue;
        }
        let margin = h.get_key(row.clone(), "default_margin_mode")?;
        if !h.text_eq(&margin, "CROSS") && !h.text_eq(&margin, "ISOLATED") {
            continue;
        }
        let map = get(h, &r, "marginTypeBySymbol")?;
        let setter = get(h, &map, "set")?;
        let symbol = h.normalize_symbol(get(h, &row, "symbol")?)?;
        let mapped = call(h, r.clone(), "mapMarginType", vec![margin])?;
        h.call(setter, Some(map), vec![symbol, mapped])?;
    }
    Ok(h.undefined())
}
pub async fn limit_orders<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let operation = async {
        let data = nested_data(h, request(h, &r, "ORDERS", true).await?)?;
        let rows = get(h, &data, "rows")?;
        h.reduce(rows, h.array(vec![]), |acc, row| {
            let order = call(h, r.clone(), "mapLimitOrder", vec![row])?;
            if h.is_null(&order) {
                return Ok(acc);
            }
            let mut next = h.spread(acc)?;
            next.push(order);
            Ok(h.array(next))
        })
    }
    .await;
    match operation {
        Ok(value) => Ok(value),
        Err(error) => {
            log_message(h, &r, &error)?;
            Ok(h.array(vec![]))
        }
    }
}
pub async fn algo_orders<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let operation = async {
        let data = nested_data(h, request(h, &r, "ALGO_ORDERS", true).await?)?;
        let rows = get(h, &data, "rows")?;
        h.reduce(rows, h.array(vec![]), |acc, row| {
            let children = call(h, r.clone(), "mapAlgoOrder", vec![row])?;
            let mut next = h.spread(acc)?;
            next.extend(h.spread(children)?);
            Ok(h.array(next))
        })
    }
    .await;
    match operation {
        Ok(value) => Ok(value),
        Err(error) => {
            log_message(h, &r, &error)?;
            Ok(h.array(vec![]))
        }
    }
}
pub async fn orders<H: AccountReadHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let promise = call(h, r.clone(), "fetchLimitOrders", vec![])?;
    let limits = h.await_value(promise).await?;
    let promise = call(h, r, "fetchAlgoOrders", vec![])?;
    let algos = h.await_value(promise).await?;
    let mut all = h.spread(limits)?;
    all.extend(h.spread(algos)?);
    Ok(h.array(all))
}

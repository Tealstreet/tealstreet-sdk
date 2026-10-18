//! Historical fill and private-channel mappings over retained JSON records.
use safe_cex_num::{
    JsCoercionError, JsJsonValue as J, js_date_parse, js_number, js_number_string, js_string,
    js_time_clip, js_truthy, parse_float,
};
use serde_json::Value;
use std::future::Future;
fn j(v: Option<&Value>) -> J {
    v.map_or(J::Undefined, |v| J::from(v.clone()))
}
fn s(v: impl Into<String>) -> J {
    J::String(v.into())
}
fn n(v: f64) -> J {
    J::Number(v)
}
fn o(fields: Vec<(&str, J)>) -> J {
    J::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn put(obj: &mut J, key: &str, value: J) {
    let J::Object(fields) = obj else {
        panic!("object required")
    };
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value
    } else {
        fields.push((key.into(), value))
    }
}
fn truth(v: Option<&Value>) -> bool {
    js_truthy(v)
}
fn string(v: Option<&Value>) -> Result<String, JsCoercionError> {
    js_string(v)
}
fn float(v: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_float(&string(v)?))
}
fn first<'a>(row: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter()
        .find_map(|k| row.get(*k).filter(|v| truth(Some(v))))
}
fn or<'a>(v: Option<&'a Value>, fallback: &'a Value) -> Option<&'a Value> {
    Some(v.filter(|v| truth(Some(v))).unwrap_or(fallback))
}
fn record(v: &Value) -> Result<J, JsCoercionError> {
    if v.is_object() {
        Ok(J::from(v.clone()))
    } else {
        Err(JsCoercionError {
            name: "TypeError",
            message: "JSON record required",
        })
    }
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == name)
        .unwrap()
        .1
}
fn table<T: serde::Serialize>(table: &[(&str, T)], key: Option<&str>, fallback: J) -> J {
    table
        .iter()
        .find(|(k, _)| Some(*k) == key)
        .map_or(fallback, |(_, v)| J::from(serde_json::to_value(v).unwrap()))
}
fn ms(v: Option<&Value>) -> Option<f64> {
    let num = v.and_then(Value::as_f64)?;
    if !num.is_finite() {
        return None;
    }
    let num = num.floor();
    Some(if num >= 1e18 {
        (num / 1e6).floor()
    } else if num >= 1e15 {
        (num / 1e3).floor()
    } else if num >= 1e12 {
        num
    } else {
        num * 1000.0
    })
}
fn seconds(
    v: Option<&Value>,
    local: impl FnOnce(f64) -> f64,
) -> Result<Option<f64>, JsCoercionError> {
    if v.is_none() || v.is_some_and(Value::is_null) {
        return Ok(None);
    }
    let num = js_number(v)?;
    if num.is_finite() && num > 0.0 {
        let num = num.floor();
        return Ok(Some(if num >= 1e18 {
            (num / 1e9).floor()
        } else if num >= 1e15 {
            (num / 1e6).floor()
        } else if num >= 1e12 {
            (num / 1e3).floor()
        } else {
            num
        }));
    }
    let date = js_date_parse(&string(v)?, local);
    Ok((date.is_finite() && date > 0.0).then(|| (date / 1000.0).floor()))
}
fn normalized(v: Option<&Value>) -> Result<String, JsCoercionError> {
    Ok(string(
        v.filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?
    .to_uppercase()
    .chars()
    .filter(char::is_ascii_alphanumeric)
    .collect())
}
pub trait FillHistoryHost {
    type Error: From<JsCoercionError>;
    fn public_only(&mut self) -> Result<bool, Self::Error>;
    fn market(&mut self, symbol: &Value) -> Result<Option<Value>, Self::Error>;
    fn get(
        &mut self,
        endpoint: &'static str,
        config: J,
    ) -> impl Future<Output = Result<Value, Self::Error>>;
    fn local_to_utc(&mut self, value: f64) -> f64;
    fn warning(&mut self, prefix: &'static str, error: &Self::Error) -> Result<(), Self::Error>;
}
pub async fn fetch_historical_fills<H: FillHistoryHost>(
    host: &mut H,
    options: &Value,
) -> Result<Vec<J>, H::Error> {
    if host.public_only()? {
        return Ok(Vec::new());
    }
    let Some(symbol) = options.get("symbol").filter(|v| truth(Some(v))) else {
        return Ok(Vec::new());
    };
    let Some(market) = host.market(symbol)? else {
        return Ok(Vec::new());
    };
    let limit = js_number(options.get("limit").or(Some(&Value::from(100))))?;
    let limit = safe_cex_num::math_min(safe_cex_num::math_max(limit, 1.0), 100.0);
    let from = ms(options.get("from"));
    let to = ms(options.get("to"));
    let key = normalized(market.get("symbol"))?;
    let request_id = options.get("orderId");
    let result = async {
        let mut params = vec![("symbol", j(market.get("symbol")))];
        if request_id.is_some() {
            params.push(("orderId", j(request_id)))
        }
        if let Some(from) = from {
            params.push(("startTime", n(from)))
        }
        if let Some(to) = to {
            params.push(("endTime", n(to)))
        }
        params.push(("limit", n(limit)));
        let config = o(vec![
            ("params", o(params)),
            ("__suppressExchangeError", J::Bool(true)),
            ("__suppressExchangeRateLimitError", J::Bool(true)),
            (
                "headers",
                o(vec![
                    ("x-tealstreet-suppress-exchange-error", s("1")),
                    ("x-tealstreet-suppress-exchange-rate-limit-error", s("1")),
                ]),
            ),
        ]);
        let payload = host.get(endpoint("HISTORY_TRADES"), config).await?;
        let mut rows = &[][..];
        'roots: for root in [
            Some(&payload),
            payload.get("data"),
            payload.get("result"),
            payload.get("data").and_then(|v| v.get("data")),
            payload.get("result").and_then(|v| v.get("data")),
        ]
        .into_iter()
        .flatten()
        .filter(|v| truth(Some(v)))
        {
            for candidate in [
                root.get("tradeList"),
                root.get("list"),
                root.get("rows"),
                root.get("userTrades"),
                root.get("data"),
                Some(root),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(array) = candidate.as_array() {
                    rows = array;
                    break 'roots;
                }
            }
        }
        let mut fills: Vec<(String, f64, J)> = Vec::new();
        for row in rows {
            if row.is_null() {
                return Err(JsCoercionError {
                    name: "TypeError",
                    message: "Cannot read properties of null",
                }
                .into());
            }
            let row_symbol = first(row, &["symbol", "instId", "contract", "instrument"])
                .or_else(|| market.get("symbol").filter(|v| truth(Some(v))));
            if normalized(row_symbol)? != key {
                continue;
            }
            let side = table(
                crate::types::ORDER_SIDE,
                Some(
                    &string(first(row, &["side"]).or(Some(&Value::String(String::new()))))?
                        .to_uppercase(),
                ),
                J::Undefined,
            );
            if matches!(side, J::Undefined) {
                continue;
            }
            let price = float(
                first(row, &["price", "avgPrice", "fillPrice"])
                    .or(Some(&Value::String("0".into()))),
            )?;
            let qty = float(
                first(row, &["qty", "tradeQty", "fillSize"]).or(Some(&Value::String("0".into()))),
            )?;
            if !price.is_finite() || !qty.is_finite() || price <= 0.0 || qty <= 0.0 {
                continue;
            }
            let Some(timestamp) = seconds(
                first(
                    row,
                    &[
                        "ctime",
                        "mtime",
                        "time",
                        "ts",
                        "tradeTime",
                        "createTime",
                        "updatedTime",
                    ],
                ),
                |v| host.local_to_utc(v),
            )?
            else {
                continue;
            };
            if timestamp == 0.0 {
                continue;
            }
            let order_id =
                string(first(row, &["orderId", "ordId"]).or(Some(&Value::String(String::new()))))?;
            if truth(request_id) && !order_id.is_empty() && order_id != string(request_id)? {
                continue;
            }
            let fee = float(first(row, &["fee", "execFee"]).or(Some(&Value::String("0".into()))))?;
            let mut fill = Vec::new();
            if let Some(id) = row
                .get("tradeId")
                .filter(|v| !v.is_null() && !matches!(v,Value::String(s)if s.is_empty()))
            {
                fill.push(("id", s(string(Some(id))?)))
            }
            fill.extend([
                ("timestamp", n(timestamp)),
                ("symbol", j(market.get("symbol"))),
            ]);
            if !order_id.is_empty() {
                fill.push(("orderId", s(&order_id)))
            }
            fill.extend([("price", n(price)), ("qty", n(qty)), ("side", side.clone())]);
            if fee.is_finite() {
                fill.push(("commission", n(fee.abs())))
            }
            let asset = first(row, &["feeCurrency", "feeCoin", "marginCoin"])
                .or_else(|| market.get("quote"));
            if asset.is_some() {
                fill.push(("commissionAsset", j(asset)))
            }
            if from.is_some_and(|v| v != 0.0 && timestamp * 1000.0 < v)
                || to.is_some_and(|v| v != 0.0 && timestamp * 1000.0 > v)
            {
                continue;
            }
            let J::String(side) = side else {
                unreachable!()
            };
            let key = format!(
                "{}:{}:{}:{}:{}",
                if order_id.is_empty() {
                    "no-order"
                } else {
                    &order_id
                },
                js_number_string(timestamp),
                js_number_string(price),
                js_number_string(qty),
                side
            );
            if let Some(old) = fills.iter_mut().find(|(k, _, _)| *k == key) {
                *old = (key, timestamp, o(fill))
            } else {
                fills.push((key, timestamp, o(fill)))
            }
        }
        fills.sort_by(|(_, a, _), (_, b, _)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let count = if limit.is_nan() { 0 } else { limit as usize };
        Ok(fills.into_iter().take(count).map(|(_, _, v)| v).collect())
    }
    .await;
    match result {
        Ok(fills) => Ok(fills),
        Err(error) => {
            host.warning("fetchHistoricalFills failed: ", &error)?;
            Ok(Vec::new())
        }
    }
}
pub trait PrivateMessageHost {
    type Raw;
    type Error: From<JsCoercionError>;
    fn disposed(&mut self) -> Result<bool, Self::Error>;
    fn track(&mut self) -> Result<(), Self::Error>;
    fn count(&mut self) -> Result<f64, Self::Error>;
    fn set_count(&mut self, value: f64) -> Result<(), Self::Error>;
    fn diagnostic(&mut self, raw: &Self::Raw) -> Result<(), Self::Error>;
    fn parse(&mut self, raw: Self::Raw) -> Result<Value, Self::Error>;
    fn balance_notice(&mut self, json: &Value) -> Result<(), Self::Error>;
    fn refresh(&mut self, kind: &'static str) -> Result<(), Self::Error>;
    fn orders(&mut self) -> Result<Vec<Value>, Self::Error>;
    fn positions(&mut self) -> Result<Vec<Value>, Self::Error>;
    fn add_orders(&mut self, orders: Vec<J>) -> Result<(), Self::Error>;
    fn key(&mut self, row: &Value, name: &'static str) -> Result<Option<Value>, Self::Error>;
    fn reduce_only(
        &mut self,
        raw: Option<&Value>,
        client: Option<&Value>,
        fallback: Option<&Value>,
    ) -> Result<bool, Self::Error>;
    fn cache_tif(&mut self, client: &Value, tif: J) -> Result<(), Self::Error>;
    fn cache_position(&mut self, key: String, id: Option<&Value>) -> Result<(), Self::Error>;
    fn update_position_info(&mut self, position: &Value, info: J) -> Result<(), Self::Error>;
    fn fake_full(&mut self) -> Result<bool, Self::Error>;
    fn leg_id(
        &mut self,
        raw: Option<&Value>,
        position: Option<&Value>,
        leg: &'static str,
        disambiguate: bool,
    ) -> Result<String, Self::Error>;
    fn clock(&mut self) -> Result<f64, Self::Error>;
    fn local_to_utc(&mut self, value: f64) -> f64;
    fn handled_pong(&mut self) -> Result<bool, Self::Error>;
    fn last_pong(&mut self) -> Result<f64, Self::Error>;
    fn set_last_pong(&mut self, value: f64) -> Result<(), Self::Error>;
    fn set_handled_pong(&mut self, value: bool) -> Result<(), Self::Error>;
    fn pong(&mut self) -> Result<(), Self::Error>;
    fn login_diagnostic(&mut self, message: String) -> Result<(), Self::Error>;
    fn login_error(&mut self, message: &'static str) -> Result<(), Self::Error>;
    fn spawn_login_retry(&mut self) -> Result<(), Self::Error>;
}
pub trait LoginRetryHost {
    type Error;
    fn sync(&mut self) -> impl Future<Output = Result<(), Self::Error>>;
    fn sleep(&mut self, ms: f64) -> impl Future<Output = Result<(), Self::Error>>;
    fn launch_login(&mut self) -> Result<(), Self::Error>;
}
pub async fn retry_login<H: LoginRetryHost>(host: &mut H) -> Result<(), H::Error> {
    host.sync().await?;
    host.sleep(2000.0).await?;
    host.launch_login()
}
fn upper(v: Option<&Value>) -> Result<Option<String>, JsCoercionError> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(v)) => Ok(Some(v.to_uppercase())),
        _ => Err(JsCoercionError {
            name: "TypeError",
            message: "toUpperCase is not a function",
        }),
    }
}
fn date(v: Option<&Value>, local: impl FnOnce(f64) -> f64) -> Result<f64, JsCoercionError> {
    Ok(match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Number(_) | Value::Bool(_)) => js_time_clip(js_number(v)?),
        _ => js_date_parse(&string(v)?, local),
    })
}
fn value_precision(v: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(match v {
        Some(Value::String(v)) => {
            safe_cex_num::after_decimal(safe_cex_num::PrecisionInput::String(v.clone()))
        }
        None | Some(Value::Null) => 2.0,
        _ => safe_cex_num::after_decimal(safe_cex_num::PrecisionInput::Number(js_number(v)?)),
    })
}
fn subtract(a: Option<&Value>, b: Option<&Value>) -> Result<f64, JsCoercionError> {
    let factor = safe_cex_num::math_pow(
        10.0,
        safe_cex_num::math_max(value_precision(a)?, value_precision(b)?),
    );
    Ok(safe_cex_num::math_round((js_number(a)? - js_number(b)?) * factor) / factor)
}
pub fn on_private_message<H: PrivateMessageHost>(
    host: &mut H,
    raw: H::Raw,
) -> Result<(), H::Error> {
    if host.disposed()? {
        return Ok(());
    }
    host.track()?;
    let count = host.count()?;
    host.set_count(count + 1.0)?;
    host.diagnostic(&raw)?;
    let json = host.parse(raw)?;
    let channel = first(&json, &["op", "ch"]).and_then(Value::as_str);
    let data = json.get("data").unwrap_or(&Value::Null);
    let event = data.get("event").and_then(Value::as_str);
    match channel {
        Some("balance") => {
            host.balance_notice(&json)?;
            host.refresh("balance")?;
        }
        Some("order") => {
            match event {
                Some("CREATE") => {
                    let reduce = host.reduce_only(
                        data.get("reduceOnly")
                            .filter(|v| !v.is_null())
                            .or_else(|| data.get("reductionOnly")),
                        data.get("clientId"),
                        None,
                    )?;
                    let id = host.key(data, "orderId")?;
                    let qty = host.key(data, "qty")?;
                    let dealt = host.key(data, "dealAmount")?;
                    let mtime = host.key(data, "mtime")?;
                    let ctime = if !truth(mtime.as_ref()) {
                        host.key(data, "ctime")?
                    } else {
                        None
                    };
                    let zero = Value::String("0".into());
                    let qty = or(qty.as_ref(), &zero);
                    let dealt = or(dealt.as_ref(), &zero);
                    let mut info = record(data)?;
                    put(&mut info, "reduceOnly", J::Bool(reduce));
                    let mut order = o(vec![
                        ("id", j(id.as_ref())),
                        ("status", s("open")),
                        ("symbol", j(data.get("symbol"))),
                        (
                            "type",
                            table(
                                crate::types::ORDER_TYPE,
                                upper(data.get("type"))?.as_deref(),
                                J::Undefined,
                            ),
                        ),
                        (
                            "side",
                            table(
                                crate::types::ORDER_SIDE,
                                upper(data.get("side"))?.as_deref(),
                                J::Undefined,
                            ),
                        ),
                        ("price", n(float(or(data.get("price"), &zero))?)),
                        ("amount", n(float(qty)?)),
                        ("reduceOnly", J::Bool(reduce)),
                        ("filled", n(float(dealt)?)),
                        (
                            "lastUpdate",
                            n(date(
                                mtime.as_ref().filter(|v| truth(Some(v))).or(ctime.as_ref()),
                                |v| host.local_to_utc(v),
                            )?),
                        ),
                        ("remaining", n(subtract(qty, dealt)?)),
                        (
                            "timeInForce",
                            table(
                                crate::types::TIME_IN_FORCE,
                                upper(data.get("effect"))?.as_deref(),
                                s("GoodTillCancel"),
                            ),
                        ),
                        ("info", info),
                    ]);
                    for (field, target) in [
                        ("tpStopType", "tpTriggerType"),
                        ("slStopType", "slTriggerType"),
                    ] {
                        let val = host.key(data, field)?;
                        if truth(val.as_ref()) {
                            let val = host.key(data, field)?;
                            put(
                                &mut order,
                                target,
                                table(
                                    crate::types::ORDER_TRIGGER_TYPE,
                                    val.as_ref().and_then(Value::as_str),
                                    J::Undefined,
                                ),
                            );
                        }
                    }
                    for (field, target) in
                        [("tpPrice", "takeProfitPrice"), ("slPrice", "stopLossPrice")]
                    {
                        let val = host.key(data, field)?;
                        if truth(val.as_ref()) {
                            let val = host.key(data, field)?;
                            put(&mut order, target, n(float(val.as_ref())?));
                        }
                    }
                    if truth(data.get("clientId")) && truth(data.get("effect")) {
                        let tif = table(
                            crate::types::TIME_IN_FORCE,
                            upper(data.get("effect"))?.as_deref(),
                            J::Undefined,
                        );
                        if !matches!(tif, J::Undefined) {
                            host.cache_tif(&data["clientId"], tif)?;
                        }
                    }
                    host.add_orders(vec![order])?;
                }
                Some("UPDATE" | "CLOSE") => {
                    let existing = host
                        .orders()?
                        .into_iter()
                        .find(|o| o.get("id") == data.get("orderId"));
                    if let Some(existing) = existing {
                        let updating = event == Some("UPDATE");
                        let mut order = record(&existing)?;
                        let reduce = if updating {
                            Some(
                                host.reduce_only(
                                    data.get("reduceOnly")
                                        .filter(|v| !v.is_null())
                                        .or_else(|| data.get("reductionOnly")),
                                    data.get("clientId").filter(|v| !v.is_null()).or_else(|| {
                                        existing.get("info").and_then(|i| i.get("clientId"))
                                    }),
                                    existing.get("reduceOnly"),
                                )?,
                            )
                        } else {
                            None
                        };
                        if !updating {
                            put(
                                &mut order,
                                "status",
                                s(
                                    if data.get("orderStatus").and_then(Value::as_str)
                                        == Some("FILLED")
                                    {
                                        "closed"
                                    } else {
                                        "canceled"
                                    },
                                ),
                            );
                        }
                        let price = existing.get("price").ok_or(JsCoercionError {
                            name: "TypeError",
                            message: "Missing existing price",
                        })?;
                        let price_fallback = Value::String(string(Some(price))?);
                        put(
                            &mut order,
                            "price",
                            n(float(or(data.get("price"), &price_fallback))?),
                        );
                        let qty = if updating {
                            host.key(data, "qty")?
                        } else {
                            None
                        };
                        let dealt = host.key(data, "dealAmount")?;
                        let amount_fallback = Value::String(string(existing.get("amount"))?);
                        let dealt_fallback = Value::String(string(existing.get("filled"))?);
                        let qty = or(qty.as_ref(), &amount_fallback);
                        let dealt = or(dealt.as_ref(), &dealt_fallback);
                        if updating {
                            put(&mut order, "amount", n(float(qty)?));
                        }
                        put(&mut order, "filled", n(float(dealt)?));
                        put(
                            &mut order,
                            "remaining",
                            n(if updating { subtract(qty, dealt)? } else { 0.0 }),
                        );
                        let mtime = host.key(data, "mtime")?;
                        let ctime = if !truth(mtime.as_ref()) {
                            host.key(data, "ctime")?
                        } else {
                            None
                        };
                        let time = date(
                            mtime.as_ref().filter(|v| truth(Some(v))).or(ctime.as_ref()),
                            |v| host.local_to_utc(v),
                        )?;
                        put(
                            &mut order,
                            "lastUpdate",
                            if time == 0.0 || time.is_nan() {
                                j(existing.get("lastUpdate"))
                            } else {
                                n(time)
                            },
                        );
                        if let Some(reduce) = reduce {
                            put(&mut order, "reduceOnly", J::Bool(reduce));
                            let mut info = existing
                                .get("info")
                                .filter(|v| truth(Some(v)))
                                .map_or(o(Vec::new()), |v| J::from(v.clone()));
                            let J::Object(fields) = record(data)? else {
                                unreachable!()
                            };
                            for (k, v) in fields {
                                put(&mut info, &k, v)
                            }
                            put(
                                &mut info,
                                "orderId",
                                j(data
                                    .get("orderId")
                                    .filter(|v| truth(Some(v)))
                                    .or_else(|| existing.get("id"))),
                            );
                            put(&mut info, "reduceOnly", J::Bool(reduce));
                            put(&mut order, "info", info);
                        } else {
                            put(&mut order, "info", j(Some(data)));
                        }
                        host.add_orders(vec![order])?;
                    }
                }
                _ => {}
            }
            host.refresh("orders")?;
        }
        Some("position") => {
            if truth(data.get("positionId")) && truth(data.get("symbol")) && truth(data.get("side"))
            {
                let side = upper(data.get("side"))?;
                let side = if matches!(side.as_deref(), Some("LONG" | "BUY")) {
                    "long"
                } else {
                    "short"
                };
                let key = format!("{}:{side}", string(data.get("symbol"))?);
                host.cache_position(
                    key,
                    if event == Some("CLOSE") {
                        None
                    } else {
                        data.get("positionId")
                    },
                )?;
            }
            if matches!(event, Some("OPEN" | "UPDATE")) {
                let side = upper(data.get("side"))?;
                let side = if matches!(side.as_deref(), Some("LONG" | "BUY")) {
                    "long"
                } else {
                    "short"
                };
                if let Some(pos) = host.positions()?.into_iter().find(|p| {
                    p.get("symbol") == data.get("symbol")
                        && p.get("side").and_then(Value::as_str) == Some(side)
                }) {
                    if truth(data.get("positionId")) {
                        let mut info = pos
                            .get("info")
                            .map_or(o(Vec::new()), |v| J::from(v.clone()));
                        put(&mut info, "positionId", j(data.get("positionId")));
                        host.update_position_info(&pos, info)?;
                    }
                }
            }
            host.refresh("positions")?;
        }
        Some("tpsl") => {
            match event {
                Some("CREATE") => {
                    let position = host.positions()?.into_iter().find(|p| {
                        p.get("info").and_then(|i| i.get("positionId")) == data.get("positionId")
                    });
                    let raw_id = host.key(data, "orderId")?;
                    let tp = truth(data.get("tpPrice")) && !float(data.get("tpPrice"))?.is_nan();
                    let sl = truth(data.get("slPrice")) && !float(data.get("slPrice"))?.is_nan();
                    let mut info = record(data)?;
                    put(&mut info, "id", j(raw_id.as_ref()));
                    put(&mut info, "orderId", j(raw_id.as_ref()));
                    let mut orders = Vec::new();
                    for (leg, present, price_key, qty_key, stop_key, trigger_key, kind) in [
                        (
                            "tp",
                            tp,
                            "tpPrice",
                            "tpQty",
                            "tpStopType",
                            "tpTriggerType",
                            "take_profit_market",
                        ),
                        (
                            "sl",
                            sl,
                            "slPrice",
                            "slQty",
                            "slStopType",
                            "slTriggerType",
                            "stop_market",
                        ),
                    ] {
                        if !present {
                            continue;
                        }
                        let zero = Value::String("0".into());
                        let qty = float(or(data.get(qty_key), &zero))?;
                        let full = position.as_ref().is_some_and(|p| {
                            js_number(p.get("contracts")).is_ok_and(|amount| amount == qty)
                        });
                        let id =
                            host.leg_id(raw_id.as_ref(), data.get("positionId"), leg, tp && sl)?;
                        let side = if let Some(p) = &position {
                            s(if p.get("side").and_then(Value::as_str) == Some("long") {
                                "sell"
                            } else {
                                "buy"
                            })
                        } else {
                            table(
                                crate::types::ORDER_SIDE,
                                upper(data.get("side"))?.as_deref(),
                                J::Undefined,
                            )
                        };
                        let amount = if host.fake_full()? && full { 0.0 } else { qty };
                        let time = host.clock()?;
                        orders.push(o(vec![
                            ("id", s(id)),
                            ("status", s("open")),
                            ("symbol", j(data.get("symbol"))),
                            ("type", s(kind)),
                            ("side", side),
                            ("price", n(float(data.get(price_key))?)),
                            ("amount", n(amount)),
                            ("reduceOnly", J::Bool(true)),
                            ("filled", n(0.0)),
                            ("lastUpdate", n(time)),
                            ("remaining", n(qty)),
                            ("info", info.clone()),
                            (
                                trigger_key,
                                table(
                                    crate::types::ORDER_TRIGGER_TYPE,
                                    data.get(stop_key).and_then(Value::as_str),
                                    s("mark_price"),
                                ),
                            ),
                        ]));
                    }
                    if orders.is_empty() {
                        host.refresh("orders")?
                    } else {
                        host.add_orders(orders)?;
                    }
                }
                Some("UPDATE" | "CLOSE") => {
                    let order_id = string(data.get("orderId"))?;
                    let mut orders = Vec::new();
                    for existing in host.orders()? {
                        if !string(existing.get("id"))?.contains(&order_id) {
                            continue;
                        }
                        let mut order = record(&existing)?;
                        if event == Some("CLOSE") {
                            put(
                                &mut order,
                                "status",
                                s(
                                    if data.get("status").and_then(Value::as_str) == Some("FILLED")
                                    {
                                        "closed"
                                    } else {
                                        "canceled"
                                    },
                                ),
                            );
                        } else {
                            let tp = if truth(data.get("tpPrice")) {
                                float(data.get("tpPrice"))?
                            } else {
                                f64::NAN
                            };
                            let sl = if truth(data.get("slPrice")) {
                                float(data.get("slPrice"))?
                            } else {
                                f64::NAN
                            };
                            let price = if existing.get("type").and_then(Value::as_str)
                                == Some("take_profit_market")
                            {
                                tp
                            } else {
                                sl
                            };
                            if price != 0.0 && !price.is_nan() {
                                put(&mut order, "price", n(price));
                            }
                            for (source, target) in [
                                ("tpStopType", "tpTriggerType"),
                                ("slStopType", "slTriggerType"),
                            ] {
                                if truth(data.get(source)) {
                                    put(
                                        &mut order,
                                        target,
                                        table(
                                            crate::types::ORDER_TRIGGER_TYPE,
                                            data.get(source).and_then(Value::as_str),
                                            J::Undefined,
                                        ),
                                    );
                                }
                            }
                        }
                        orders.push(order);
                    }
                    if event == Some("CLOSE") || !orders.is_empty() {
                        host.add_orders(orders)?;
                    }
                }
                _ => {}
            }
            host.refresh("orders")?;
        }
        Some("login") => {
            if truth(data.get("result")) {
                host.login_diagnostic("[bitunix login] success".into())?
            } else {
                let wire = safe_cex_num::js_json_stringify(&J::from(json.clone()))?
                    .unwrap_or("undefined".into());
                host.login_diagnostic(format!("[bitunix login] failed {wire}"))?;
                host.login_error(
                    "Bitunix private ws login failed, double check your key and secret",
                )?;
                host.spawn_login_retry()?;
            }
        }
        _ => {}
    }
    if !host.handled_pong()? {
        let now = host.clock()?;
        let last = host.last_pong()?;
        if now - last > 5000.0 {
            let now = host.clock()?;
            host.set_last_pong(now)?;
            host.set_handled_pong(true)?;
            host.pong()?;
        }
    }
    Ok(())
}

//! Live order reads, formatting and amendment over retained JSON models.
use crate::order_state::TpslLegSnapshot;
use safe_cex_num::{
    JsCoercionError, PrecisionInput, adjust, after_decimal, js_number_string, js_retained_number,
    js_retained_parse_float, js_retained_string, math_max, math_pow, math_round,
};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::future::Future;

pub trait OrderModelHost {
    type Error: From<JsCoercionError>;
    fn caller_id(&mut self, options: &Input) -> Result<Option<String>, Self::Error>;
    fn market(&mut self, symbol: &Input) -> Result<Option<Input>, Self::Error>;
    fn positions(&mut self) -> Result<Vec<Input>, Self::Error>;
    fn orders(&mut self) -> Result<Vec<Input>, Self::Error>;
    fn cached_position(&mut self, key: &str) -> Result<Option<Input>, Self::Error>;
    fn uuid(&mut self) -> Result<String, Self::Error>;
    fn assert_single(&mut self, caller: Option<&str>, count: usize) -> Result<(), Self::Error>;
    fn cache_tif(&mut self, client_id: &str, tif: &Input) -> Result<(), Self::Error>;
    fn cache_reduce_only(
        &mut self,
        client_id: &str,
        reduce_only: &Input,
    ) -> Result<(), Self::Error>;
    fn emit(&mut self, message: Input) -> Result<(), Self::Error>;
    fn error(&mut self, message: Input) -> Result<Self::Error, Self::Error>;
    fn reason(&mut self, error: &Self::Error, fallback: Option<&str>)
    -> Result<Input, Self::Error>;
    fn warning(&mut self, prefix: &'static str, error: &Self::Error) -> Result<(), Self::Error>;
    fn get(
        &mut self,
        endpoint: &'static str,
        config: Input,
    ) -> impl Future<Output = Result<Input, Self::Error>>;
    fn post(
        &mut self,
        endpoint: &'static str,
        payload: Input,
    ) -> impl Future<Output = Result<Input, Self::Error>>;
    fn post_limited(
        &mut self,
        endpoint: &'static str,
        payload: Input,
    ) -> impl Future<Output = Result<Input, Self::Error>>;
    fn update_status(&mut self, ids: &[String]) -> Result<(), Self::Error>;
    fn place_replacement(
        &mut self,
        options: Input,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>>;
    fn cached_tif(&mut self, client_id: Option<&Input>) -> Result<Option<Input>, Self::Error>;
    fn reduce_only(
        &mut self,
        raw: Option<&Input>,
        client_id: Option<&Input>,
    ) -> Result<bool, Self::Error>;
    fn key(&mut self, row: &Input, key: &'static str) -> Result<Option<Input>, Self::Error>;
    fn raw_id(&mut self, order: &Input) -> Result<String, Self::Error>;
    fn leg_id(
        &mut self,
        raw_id: &str,
        position_id: Option<&Input>,
        leg: &'static str,
        disambiguate: bool,
    ) -> Result<String, Self::Error>;
    fn clock(&mut self) -> Result<f64, Self::Error>;
    fn fake_full_enabled(&mut self) -> Result<bool, Self::Error>;
    fn attached_leg(&mut self, order: &Input) -> Result<Option<&'static str>, Self::Error>;
    fn standalone(&mut self, order: &Input) -> Result<bool, Self::Error>;
    fn position_order(&mut self, order: &Input) -> Result<bool, Self::Error>;
    fn snapshot(
        &mut self,
        order: &Input,
        leg: &'static str,
    ) -> Result<Option<TpslLegSnapshot>, Self::Error>;
    fn trigger(
        &mut self,
        trigger: Option<&Input>,
        raw: Option<&Input>,
    ) -> Result<String, Self::Error>;
    fn update_standalone(
        &mut self,
        order: &Input,
        update: &Input,
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>>;
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == name)
        .unwrap()
        .1
}
fn string(v: Option<&Input>) -> Result<String, JsCoercionError> {
    js_retained_string(v)
}
fn number(v: Option<&Input>) -> Result<f64, JsCoercionError> {
    js_retained_number(v)
}
fn float(v: Option<&Input>) -> Result<f64, JsCoercionError> {
    js_retained_parse_float(v)
}

fn truth(v: Option<&Input>) -> bool {
    v.is_some_and(Input::is_truthy)
}
fn j(v: Option<&Input>) -> Input {
    v.cloned().unwrap_or(Input::Undefined)
}
fn s(v: impl Into<String>) -> Input {
    Input::Text(v.into().encode_utf16().collect())
}
fn n(v: f64) -> Input {
    Input::Float(v)
}
fn object(fields: Vec<(&str, Input)>) -> Input {
    Input::Object(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )
}
fn put(object: &mut Input, key: &str, value: Input) {
    let Input::Object(fields) = object else {
        panic!("native order object required")
    };
    if let Some((_, old)) = fields
        .iter_mut()
        .find(|(k, _)| k.iter().copied().eq(key.encode_utf16()))
    {
        *old = value;
    } else {
        fields.push((key.encode_utf16().collect(), value));
    }
}
fn has(v: &Input, key: &str) -> bool {
    v.get(key).is_some()
}
fn kind(v: &Input, name: &str) -> bool {
    v.get("type").is_some_and(|v| v.text_equals(name))
}
fn side(opts: &Input) -> &'static str {
    if opts.get("side").is_some_and(|v| v.text_equals("buy")) {
        "short"
    } else {
        "long"
    }
}
fn precision(v: Option<&Input>) -> Result<PrecisionInput, JsCoercionError> {
    Ok(match v {
        Some(Input::Text(_)) => PrecisionInput::String(string(v)?),
        _ => PrecisionInput::Number(number(v)?),
    })
}
fn adjusted(value: Option<&Input>, step: Option<&Input>) -> Result<f64, JsCoercionError> {
    Ok(adjust(number(value)?, precision(step)?))
}
fn digits(v: Option<&Input>) -> Result<f64, JsCoercionError> {
    Ok(if v.is_none() || v.is_some_and(Input::is_null) {
        2.0
    } else {
        after_decimal(precision(v)?)
    })
}
fn subtract(a: Option<&Input>, b: Option<&Input>) -> Result<f64, JsCoercionError> {
    let factor = math_pow(10.0, math_max(digits(a)?, digits(b)?));
    Ok(math_round((number(a)? - number(b)?) * factor) / factor)
}

fn stop(v: Option<&Input>) -> &'static str {
    if v.is_some_and(|v| v.text_equals("mark_price")) {
        "MARK_PRICE"
    } else {
        "LAST_PRICE"
    }
}
fn strict(a: Option<&Input>, b: Option<&Input>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Input::Float(_) | Input::Lexeme(_)), Some(Input::Float(_) | Input::Lexeme(_))) => {
            a.and_then(Input::as_f64) == b.and_then(Input::as_f64)
        }
        (Some(Input::Array(_) | Input::Object(_)), Some(Input::Array(_) | Input::Object(_))) => {
            std::ptr::eq(a.unwrap(), b.unwrap())
        }
        _ => a == b,
    }
}
fn enum_value<T: serde::Serialize>(value: &T) -> Input {
    Input::from(serde_json::to_value(value).expect("Bitunix protocol enums serialize as strings"))
}
fn inverse<T: serde::Serialize>(table: &[(&str, T)], value: Option<&Input>) -> Input {
    table
        .iter()
        .rev()
        .find(|(_, v)| Some(&enum_value(v)) == value)
        .map_or(Input::Undefined, |(key, _)| s(*key))
}
fn forward<T: serde::Serialize>(
    table: &[(&str, T)],
    key: Option<&[u16]>,
    fallback: Input,
) -> Input {
    table
        .iter()
        .find(|(k, _)| key.is_some_and(|key| key.iter().copied().eq(k.encode_utf16())))
        .map_or(fallback, |(_, value)| Input::from(enum_value(value)))
}
fn inverse_type(v: Option<&Input>) -> Input {
    inverse(crate::types::ORDER_TYPE, v)
}
fn inverse_side(v: Option<&Input>) -> Input {
    inverse(crate::types::ORDER_SIDE, v)
}
fn inverse_tif(v: Option<&Input>) -> Input {
    inverse(crate::types::TIME_IN_FORCE, v)
}
fn forward_type(v: &str) -> Input {
    forward(
        crate::types::ORDER_TYPE,
        Some(&v.encode_utf16().collect::<Vec<_>>()),
        Input::Undefined,
    )
}
fn forward_side(v: Option<&Input>) -> Input {
    forward(
        crate::types::ORDER_SIDE,
        v.and_then(Input::as_text),
        Input::Undefined,
    )
}
fn forward_tif(v: &str) -> Input {
    forward(
        crate::types::TIME_IN_FORCE,
        Some(&v.encode_utf16().collect::<Vec<_>>()),
        s("GoodTillCancel"),
    )
}
fn forward_trigger(v: Option<&Input>) -> Input {
    forward(
        crate::types::ORDER_TRIGGER_TYPE,
        v.and_then(Input::as_text),
        s("mark_price"),
    )
}
fn finite_trigger(v: Option<&Input>) -> Result<bool, JsCoercionError> {
    Ok(!matches!(v, None | Some(Input::Null))
        && !matches!(v,Some(Input::Text(s)) if s.is_empty())
        && number(v)?.is_finite())
}
fn spread(v: &Input) -> Input {
    if v.is_object() {
        v.clone()
    } else {
        object(vec![])
    }
}

fn position<'a>(
    positions: &'a [Input],
    opts: &Input,
) -> Result<Option<&'a Input>, JsCoercionError> {
    for p in positions {
        if strict(p.get("symbol"), opts.get("symbol"))
            && p.get("side").is_some_and(|v| v.text_equals(side(opts)))
            && number(p.get("contracts"))? > 0.0
        {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

fn pstep<'a>(market: &'a Input, key: &str) -> Option<&'a Input> {
    market.get("precision").and_then(|p| p.get(key))
}
fn order_info<'a>(order: &'a Input, key: &str) -> Option<&'a Input> {
    order.get("info").and_then(|i| i.get(key))
}
fn omit_undefined(value: &mut Input) {
    if let Input::Object(fields) = value {
        fields.retain(|(_, v)| !matches!(v, Input::Undefined));
    }
}
fn row_list(reply: &Input) -> Vec<Input> {
    let data = reply.get("data").and_then(|v| v.get("data"));
    data.and_then(Input::as_array)
        .or_else(|| {
            data.and_then(|d| d.get("orderList"))
                .and_then(Input::as_array)
        })
        .map(<[Input]>::to_vec)
        .unwrap_or_default()
}
fn suppression() -> Input {
    object(vec![
        ("__suppressExchangeError", Input::Bool(true)),
        ("__suppressExchangeRateLimitError", Input::Bool(true)),
    ])
}
fn reject<H: OrderModelHost, T>(host: &mut H, message: String) -> Result<T, H::Error> {
    let message = s(message);
    host.emit(message.clone())?;
    Err(host.error(message)?)
}

pub fn format_create_order<H: OrderModelHost>(
    host: &mut H,
    opts: &Input,
) -> Result<Vec<Input>, H::Error> {
    let caller = host.caller_id(opts)?;
    if kind(opts, "trailing_stop_market") {
        return Err(host.error(s("Trailing stop loss is not supported"))?);
    }
    let symbol = opts.get("symbol").cloned().unwrap_or(Input::Null);
    let Some(market) = host.market(&symbol)? else {
        return Err(host.error(s(format!(
            "Market {} not found",
            string(opts.get("symbol"))?
        )))?);
    };
    if kind(opts, "stop_market") || kind(opts, "take_profit_market") {
        return format_tpsl_order(host, opts);
    }
    let max = number(
        market
            .get("limits")
            .and_then(|l| l.get("amount"))
            .and_then(|a| a.get("max")),
    )?;
    let amount = adjusted(opts.get("amount"), pstep(&market, "amount"))?;
    let price = if truth(opts.get("price")) && !kind(opts, "market") {
        adjusted(opts.get("price"), pstep(&market, "price"))?
    } else {
        f64::NAN
    };
    let reduce = opts
        .get("reduceOnly")
        .filter(|v| truth(Some(v)))
        .cloned()
        .unwrap_or(Input::Bool(false));
    let effect = if truth(opts.get("timeInForce")) {
        inverse_tif(opts.get("timeInForce"))
    } else {
        s("GTC")
    };
    let mut trade_side = "OPEN";
    let mut position_id = None;
    if truth(Some(&reduce)) {
        trade_side = "CLOSE";
        let positions = host.positions()?;
        let Some(p) = position(&positions, opts)? else {
            return reject(
                host,
                format!(
                    "No {} position open for {} - nothing to reduce",
                    side(opts),
                    string(opts.get("symbol"))?
                ),
            );
        };
        position_id = p
            .get("info")
            .and_then(|i| i.get("positionId"))
            .filter(|v| truth(Some(v)))
            .cloned();
        if position_id.is_none() {
            position_id =
                host.cached_position(&format!("{}:{}", string(opts.get("symbol"))?, side(opts)))?;
        }
        if !truth(position_id.as_ref()) {
            return reject(
                host,
                format!(
                    "Missing positionId for {} (side: {}) - cannot place reduce-only order without a positionId",
                    string(opts.get("symbol"))?,
                    side(opts)
                ),
            );
        }
    }
    let client = match &caller {
        Some(c) => c.clone(),
        None => format!("ts_{}", host.uuid()?),
    };
    let mut request = object(vec![
        ("symbol", j(opts.get("symbol"))),
        (
            "qty",
            if amount != 0.0 && !amount.is_nan() {
                s(js_number_string(amount))
            } else {
                Input::Undefined
            },
        ),
        (
            "price",
            if price != 0.0 && !price.is_nan() {
                s(js_number_string(price))
            } else {
                Input::Undefined
            },
        ),
        ("side", inverse_side(opts.get("side"))),
        ("tradeSide", s(trade_side)),
        ("positionId", j(position_id.as_ref())),
        ("orderType", inverse_type(opts.get("type"))),
        ("effect", effect),
        ("clientId", s(client.clone())),
        ("reduceOnly", j(Some(&reduce))),
    ]);
    for (value, prefix, trigger) in [
        ("takeProfit", "tp", "tpTriggerType"),
        ("stopLoss", "sl", "slTriggerType"),
    ] {
        if truth(opts.get(value)) {
            put(
                &mut request,
                &format!("{prefix}Price"),
                s(string(opts.get(value))?),
            );
            put(
                &mut request,
                &format!("{prefix}StopType"),
                s(stop(opts.get(trigger))),
            );
            put(&mut request, &format!("{prefix}OrderType"), s("MARKET"));
        }
    }
    omit_undefined(&mut request);
    let lots = if amount > max {
        (amount / max).ceil()
    } else {
        1.0
    };
    let rest = if amount > max {
        adjust(amount % max, precision(pstep(&market, "amount"))?)
    } else {
        0.0
    };
    let lot_size = adjust((amount - rest) / lots, precision(pstep(&market, "amount"))?);
    let count = if lots.is_finite() && lots > 0.0 && lots <= 9_007_199_254_740_991.0 {
        lots as usize
    } else {
        0
    };
    let mut payloads = Vec::new();
    for _ in 0..count {
        let mut req = request.clone();
        put(&mut req, "qty", s(js_number_string(lot_size)));
        payloads.push(req);
    }
    if rest != 0.0 && !rest.is_nan() {
        let mut req = request;
        put(&mut req, "qty", s(js_number_string(rest)));
        payloads.push(req);
    }
    host.assert_single(caller.as_deref(), payloads.len())?;
    if !client.is_empty() && truth(opts.get("timeInForce")) {
        host.cache_tif(&client, opts.get("timeInForce").unwrap())?;
    }
    if !client.is_empty() {
        host.cache_reduce_only(&client, &reduce)?;
    }
    Ok(payloads)
}

pub fn format_tpsl_order<H: OrderModelHost>(
    host: &mut H,
    opts: &Input,
) -> Result<Vec<Input>, H::Error> {
    let positions = host.positions()?;
    let Some(p) = position(&positions, opts)? else {
        return reject(
            host,
            format!(
                "No open {} position found for {}",
                side(opts),
                string(opts.get("symbol"))?
            ),
        );
    };
    let cached =
        host.cached_position(&format!("{}:{}", string(opts.get("symbol"))?, side(opts)))?;
    let id = cached
        .filter(|v| truth(Some(v)))
        .or_else(|| p.get("info").and_then(|i| i.get("positionId")).cloned());
    if !truth(id.as_ref()) {
        return reject(
            host,
            format!(
                "Missing positionId for {} TP/SL order",
                string(opts.get("symbol"))?
            ),
        );
    }
    let symbol = opts.get("symbol").cloned().unwrap_or(Input::Null);
    let Some(market) = host.market(&symbol)? else {
        return reject(
            host,
            format!("Market {} not found", string(opts.get("symbol"))?),
        );
    };
    let full =
        !truth(opts.get("amount")) || opts.get("amount").is_some_and(|a| a.as_f64() == Some(0.0));
    if !truth(opts.get("price")) {
        return reject(
            host,
            if full {
                "Price is required for Position TP/SL orders"
            } else {
                "Price is required for TP/SL orders"
            }
            .into(),
        );
    }
    let mut payload = object(vec![
        ("symbol", j(opts.get("symbol"))),
        ("positionId", j(id.as_ref())),
    ]);
    if full {
        let price = adjusted(opts.get("price"), pstep(&market, "price"))?;
        let mut tp = None;
        let mut sl = None;
        for o in host.orders()? {
            if (kind(&o, "take_profit_market") || kind(&o, "stop_market"))
                && strict(o.get("symbol"), opts.get("symbol"))
                && strict(order_info(&o, "positionId"), id.as_ref())
            {
                let pair = (
                    string(o.get("price"))?,
                    stop(o.get(if kind(&o, "take_profit_market") {
                        "tpTriggerType"
                    } else {
                        "slTriggerType"
                    }))
                    .to_owned(),
                );
                if kind(&o, "take_profit_market") {
                    tp = Some(pair);
                } else {
                    sl = Some(pair);
                }
            }
        }
        if kind(opts, "take_profit_market") {
            put(&mut payload, "tpPrice", s(js_number_string(price)));
            put(
                &mut payload,
                "tpStopType",
                s(stop(opts.get("tpTriggerType"))),
            );
            if let Some((price, stop_type)) = sl.filter(|(p, _)| !p.is_empty()) {
                put(&mut payload, "slPrice", s(price));
                put(&mut payload, "slStopType", s(stop_type));
            }
        } else if kind(opts, "stop_market") {
            put(&mut payload, "slPrice", s(js_number_string(price)));
            put(
                &mut payload,
                "slStopType",
                s(stop(opts.get("slTriggerType"))),
            );
            if let Some((price, stop_type)) = tp.filter(|(p, _)| !p.is_empty()) {
                put(&mut payload, "tpPrice", s(price));
                put(&mut payload, "tpStopType", s(stop_type));
            }
        }
        put(&mut payload, "isPositionOrder", Input::Bool(true));
    } else {
        let amount = adjusted(opts.get("amount"), pstep(&market, "amount"))?;
        let price = adjusted(opts.get("price"), pstep(&market, "price"))?;
        let prefix = if kind(opts, "take_profit_market") {
            Some(("tp", "tpTriggerType"))
        } else if kind(opts, "stop_market") {
            Some(("sl", "slTriggerType"))
        } else {
            None
        };
        if let Some((prefix, trigger)) = prefix {
            for (suffix, value) in [
                ("Price", js_number_string(price)),
                ("StopType", stop(opts.get(trigger)).into()),
                ("OrderType", "MARKET".into()),
                ("Qty", js_number_string(amount)),
            ] {
                put(&mut payload, &format!("{prefix}{suffix}"), s(value));
            }
        }
    }
    Ok(vec![payload])
}

pub async fn fetch_regular_orders<H: OrderModelHost>(host: &mut H) -> Result<Vec<Input>, H::Error> {
    let result = async {
        let reply = host.get(endpoint("OPEN_ORDERS"), suppression()).await?;
        let mut orders = Vec::new();
        for row in row_list(&reply) {
            let Some(_record) = row.as_object() else {
                return Err(host.error(s("Expected object order row"))?);
            };
            let cached = host.cached_tif(row.get("clientId"))?;
            let effect = match row.get("effect") {
                None | Some(Input::Undefined | Input::Null) => String::new(),
                Some(Input::Text(_)) => string(row.get("effect"))?.to_uppercase(),
                _ => {
                    return Err(JsCoercionError {
                        name: "TypeError",
                        message: "o.effect?.toUpperCase is not a function",
                    }
                    .into());
                }
            };
            let rest_tif = forward_tif(&effect);
            let tif = cached
                .as_ref()
                .filter(|v| truth(Some(v)))
                .map_or(rest_tif, |v| j(Some(v)));
            let raw_type = ["orderType", "type"].into_iter().find_map(|key| {
                row.get(key)
                    .filter(|value| !value.is_nullish() && !value.text_equals(""))
            });
            let raw_type =
                string(raw_type.filter(|v| truth(Some(v))).or(Some(&s(""))))?.to_uppercase();
            let reduce = host.reduce_only(row.get("reduceOnly"), row.get("clientId"))?;
            let mut info = spread(&row);
            put(&mut info, "reduceOnly", Input::Bool(reduce));
            let id = host.key(&row, "orderId")?;
            let raw_side = row
                .get("side")
                .filter(|v| truth(Some(v)))
                .cloned()
                .unwrap_or(s(""));
            let upper_side = s(string(Some(&raw_side))?.to_uppercase());
            let price = float(row.get("price"))?;
            let amount = float(host.key(&row, "qty")?.as_ref())?;
            let filled = float(host.key(&row, "tradeQty")?.as_ref())?;
            let last = host.key(&row, "mtime")?;
            let qty = host.key(&row, "qty")?;
            let traded = host.key(&row, "tradeQty")?;
            let remaining = subtract(qty.as_ref(), traded.as_ref())?;
            let tp_price = float(host.key(&row, "tpOrderPrice")?.as_ref())?;
            let sl_price = float(host.key(&row, "slOrderPrice")?.as_ref())?;
            let tp_trigger = forward_trigger(host.key(&row, "tpStopType")?.as_ref());
            let sl_trigger = forward_trigger(host.key(&row, "slStopType")?.as_ref());
            orders.push(object(vec![
                ("id", j(id.as_ref())),
                ("status", s("open")),
                ("symbol", j(row.get("symbol"))),
                ("type", forward_type(&raw_type)),
                ("side", forward_side(Some(&upper_side))),
                ("price", n(price)),
                ("amount", n(amount)),
                ("reduceOnly", Input::Bool(reduce)),
                ("filled", n(filled)),
                ("lastUpdate", j(last.as_ref())),
                ("remaining", n(remaining)),
                ("takeProfitPrice", n(tp_price)),
                ("stopLossPrice", n(sl_price)),
                ("timeInForce", tif),
                ("info", info.clone()),
                ("tpTriggerType", tp_trigger),
                ("slTriggerType", sl_trigger),
            ]));
            for (leg, field, trigger_key) in [
                ("tp", "tpPrice", "tpStopType"),
                ("sl", "slPrice", "slStopType"),
            ] {
                if finite_trigger(row.get(field))? {
                    let id = host.key(&row, "orderId")?;
                    let reversed = if forward_side(row.get("side")).text_equals("buy") {
                        s("sell")
                    } else {
                        s("buy")
                    };
                    let price = float(row.get(field))?;
                    let amount = float(host.key(&row, "qty")?.as_ref())?;
                    let last = host.key(&row, "mtime")?;
                    let remaining = float(host.key(&row, "qty")?.as_ref())?;
                    let trigger = forward_trigger(host.key(&row, trigger_key)?.as_ref());
                    orders.push(object(vec![
                        ("id", s(format!("{}__{leg}", string(id.as_ref())?))),
                        ("status", s("open")),
                        ("symbol", j(row.get("symbol"))),
                        (
                            "type",
                            s(if leg == "tp" {
                                "take_profit_market"
                            } else {
                                "stop_market"
                            }),
                        ),
                        ("side", reversed),
                        ("price", n(price)),
                        ("amount", n(amount)),
                        ("reduceOnly", Input::Bool(true)),
                        ("filled", n(0.0)),
                        ("lastUpdate", j(last.as_ref())),
                        ("remaining", n(remaining)),
                        ("takeProfitPrice", n(0.0)),
                        ("stopLossPrice", n(0.0)),
                        ("timeInForce", s("GoodTillCancel")),
                        ("info", info.clone()),
                        (
                            "tpTriggerType",
                            if leg == "tp" {
                                trigger.clone()
                            } else {
                                s("mark_price")
                            },
                        ),
                        (
                            "slTriggerType",
                            if leg == "sl" {
                                trigger
                            } else {
                                s("mark_price")
                            },
                        ),
                    ]));
                }
            }
        }
        Ok::<_, H::Error>(orders)
    }
    .await;
    match result {
        Ok(rows) => Ok(rows),
        Err(error) => {
            host.warning("fetchRegularOrders failed: ", &error)?;
            Ok(vec![])
        }
    }
}

pub async fn fetch_tpsl_orders<H: OrderModelHost>(host: &mut H) -> Result<Vec<Input>, H::Error> {
    let reply = host
        .get(endpoint("OPEN_TPSL_ORDERS"), suppression())
        .await?;
    let rows = row_list(&reply);
    if rows.is_empty() {
        return Ok(vec![]);
    }
    let positions = host.positions()?;
    let mut orders = Vec::new();
    for row in rows {
        if !row.is_object() {
            return Err(host.error(s("Expected object TP/SL row"))?);
        }
        let position = positions
            .iter()
            .rev()
            .find(|p| strict(order_info(p, "positionId"), row.get("positionId")));
        let first = host.key(&row, "id")?;
        let id = if truth(first.as_ref()) {
            first
        } else {
            host.key(&row, "orderId")?
        };
        let id = id.filter(|v| truth(Some(v))).unwrap_or(s(""));
        let seed = object(vec![("id", s(string(Some(&id))?)), ("info", row.clone())]);
        let raw = host.raw_id(&seed)?;
        let tp = finite_trigger(row.get("tpPrice"))?;
        let sl = finite_trigger(row.get("slPrice"))?;
        let mut info = spread(&row);
        put(&mut info, "id", s(raw.clone()));
        put(&mut info, "orderId", s(raw.clone()));
        for (leg, has_leg) in [("tp", tp), ("sl", sl)] {
            if !has_leg {
                continue;
            }
            let qty = host.key(&row, if leg == "tp" { "tpQty" } else { "slQty" })?;
            let qty = float(qty.as_ref())?
                .is_finite()
                .then(|| float(qty.as_ref()))
                .transpose()?
                .unwrap_or(0.0);
            let full = if truth(row.get("positionId")) {
                if let Some(p) = position {
                    number(p.get("contracts"))? == qty
                } else {
                    false
                }
            } else {
                false
            };
            let id = host.leg_id(&raw, row.get("positionId"), leg, tp && sl)?;
            let side = position.map_or_else(
                || forward_side(row.get("side")),
                |p| {
                    if p.get("side").is_some_and(|v| v.text_equals("long")) {
                        s("sell")
                    } else {
                        s("buy")
                    }
                },
            );
            let price = float(row.get(if leg == "tp" { "tpPrice" } else { "slPrice" }))?;
            let amount = if host.fake_full_enabled()? && full {
                0.0
            } else {
                qty
            };
            let reduce = host
                .key(&row, "reduceOnly")?
                .filter(|v| truth(Some(v)))
                .unwrap_or(Input::Bool(false));
            let clock = host.clock()?;
            let trigger = forward_trigger(
                host.key(
                    &row,
                    if leg == "tp" {
                        "tpStopType"
                    } else {
                        "slStopType"
                    },
                )?
                .as_ref(),
            );
            let mut order = object(vec![
                ("id", s(id)),
                ("status", s("open")),
                ("symbol", j(row.get("symbol"))),
                (
                    "type",
                    s(if leg == "tp" {
                        "take_profit_market"
                    } else {
                        "stop_market"
                    }),
                ),
                ("side", side),
                ("price", n(price)),
                ("amount", n(amount)),
                ("reduceOnly", j(Some(&reduce))),
                ("filled", n(0.0)),
                ("lastUpdate", n(clock)),
                ("remaining", n(qty)),
                ("info", info.clone()),
            ]);
            put(
                &mut order,
                if leg == "tp" {
                    "tpTriggerType"
                } else {
                    "slTriggerType"
                },
                trigger,
            );
            orders.push(order);
        }
    }
    Ok(orders)
}

pub async fn update_standalone_tpsl_order<H: OrderModelHost>(
    host: &mut H,
    order: &Input,
    update: &Input,
) -> Result<Vec<String>, H::Error> {
    let symbol = order.get("symbol").cloned().unwrap_or(Input::Null);
    let Some(market) = host.market(&symbol)? else {
        return reject(
            host,
            format!("Market {} not found", string(order.get("symbol"))?),
        );
    };
    let leg = if kind(order, "take_profit_market") {
        "tp"
    } else {
        "sl"
    };
    let position_order = host.position_order(order)?;
    let raw = host.raw_id(order)?;
    let position_id = order_info(order, "positionId");
    if position_order && has(update, "amount") {
        return reject(
            host,
            "Bitunix full-position TP/SL quantity cannot be amended".into(),
        );
    }
    let mut payload = if position_order {
        object(vec![
            ("symbol", j(order.get("symbol"))),
            (
                "positionId",
                s(string(
                    position_id.filter(|v| truth(Some(v))).or(Some(&s(""))),
                )?),
            ),
        ])
    } else {
        object(vec![("orderId", s(raw.clone()))])
    };
    if position_order && !truth(position_id) {
        return reject(
            host,
            format!(
                "Bitunix cannot amend {}: missing positionId",
                string(order.get("id"))?
            ),
        );
    }
    append_leg(
        host,
        &mut payload,
        order,
        update,
        &market,
        leg,
        leg,
        position_order,
    )?;
    let sibling = if leg == "tp" { "sl" } else { "tp" };
    let mut sibling_order = None;
    for candidate in host.orders()? {
        if strict(candidate.get("symbol"), order.get("symbol"))
            && kind(
                &candidate,
                if sibling == "tp" {
                    "take_profit_market"
                } else {
                    "stop_market"
                },
            )
            && host.standalone(&candidate)?
        {
            if host.raw_id(&candidate)? == raw
                || position_id
                    .filter(|v| !v.is_nullish())
                    .is_some_and(|id| strict(order_info(&candidate, "positionId"), Some(id)))
            {
                sibling_order = Some(candidate);
                break;
            }
        }
    }
    if let Some(sibling_order) = sibling_order {
        append_leg(
            host,
            &mut payload,
            &sibling_order,
            update,
            &market,
            leg,
            sibling,
            position_order,
        )?;
    }
    let result = async {
        let reply = host
            .post(
                endpoint(if position_order {
                    "MODIFY_POSITION_TP_SL_ORDER"
                } else {
                    "MODIFY_TP_SL_ORDER"
                }),
                payload,
            )
            .await?;
        let body = reply.get("data");
        let code = body.and_then(|d| d.get("code"));
        if number(code)? != 0.0 {
            let message =
                if let Some(msg) = body.and_then(|d| d.get("msg")).filter(|v| truth(Some(v))) {
                    j(Some(msg))
                } else {
                    s(format!(
                        "Bitunix TP/SL modify failed with code {}",
                        string(code)?
                    ))
                };
            return Err(host.error(message)?);
        }
        Ok(vec![string(order.get("id"))?])
    }
    .await;
    match result {
        Ok(ids) => Ok(ids),
        Err(error) => {
            let message = host.reason(&error, Some("Bitunix TP/SL modify failed"))?;
            host.emit(message.clone())?;
            Err(host.error(message)?)
        }
    }
}
fn append_leg<H: OrderModelHost>(
    host: &mut H,
    payload: &mut Input,
    order: &Input,
    update: &Input,
    market: &Input,
    changed_leg: &str,
    leg: &'static str,
    position_order: bool,
) -> Result<(), H::Error> {
    let Some(snapshot) = host.snapshot(order, leg)? else {
        return Ok(());
    };
    let price = if leg == changed_leg && has(update, "price") {
        adjusted(update.get("price"), pstep(market, "price"))?
    } else {
        snapshot.price
    };
    if !price.is_finite() || price <= 0.0 {
        return reject(
            host,
            format!("Bitunix {} trigger price is invalid", leg.to_uppercase()),
        );
    }
    put(payload, &format!("{leg}Price"), s(js_number_string(price)));
    put(payload, &format!("{leg}StopType"), s(snapshot.stop_type));
    if position_order {
        return Ok(());
    }
    let qty = if leg == changed_leg && has(update, "amount") {
        adjusted(update.get("amount"), pstep(market, "amount"))?
    } else {
        snapshot.qty
    };
    if !qty.is_finite() || qty <= 0.0 {
        return reject(
            host,
            format!("Bitunix {} quantity is invalid", leg.to_uppercase()),
        );
    }
    put(payload, &format!("{leg}Qty"), s(js_number_string(qty)));
    put(
        payload,
        &format!("{leg}OrderType"),
        s(if snapshot.order_type.text_equals("LIMIT") {
            "LIMIT"
        } else {
            "MARKET"
        }),
    );
    if snapshot.order_type.text_equals("LIMIT") {
        let price = adjust(snapshot.order_price, precision(pstep(market, "price"))?);
        if !price.is_finite() || price <= 0.0 {
            return reject(
                host,
                format!(
                    "Bitunix {} LIMIT execution price is invalid",
                    leg.to_uppercase()
                ),
            );
        }
        put(
            payload,
            &format!("{leg}OrderPrice"),
            s(js_number_string(price)),
        );
    }
    Ok(())
}

/// Source string classifier; only virtual attached TP/SL amendments use it.
pub fn is_unsupported_order_operation(message: &str) -> bool {
    message
        .to_lowercase()
        .contains("order does not support this operation")
}

pub fn get_internal_trigger_type(raw: Option<&Input>) -> Result<&'static str, JsCoercionError> {
    let normalized = string(raw.filter(|v| truth(Some(v))).or(Some(&s(""))))?.to_uppercase();
    Ok(if matches!(normalized.as_str(), "LAST" | "LAST_PRICE") {
        "last_price"
    } else {
        "mark_price"
    })
}

/// Rebuild from the reconciled amendment payload before canceling its parent.
/// Payload fields are the strings constructed by update_order, not a wire projection.
pub async fn recreate_attached_tpsl_parent_order<H: OrderModelHost>(
    host: &mut H,
    target: &Input,
    virtual_leg: &str,
    payload: &Input,
) -> Result<Vec<String>, H::Error> {
    let field = |key: &str| match payload {
        Input::Object(fields) => fields
            .iter()
            .find(|(k, _)| k.iter().copied().eq(key.encode_utf16()))
            .and_then(|(_, v)| match v {
                Input::Text(text) => Some(Input::Text(text.clone())),
                _ => None,
            }),
        _ => None,
    };
    let price = float(field("price").as_ref())?;
    let amount = float(field("qty").as_ref())?;
    if !price.is_finite() || price <= 0.0 {
        return reject(
            host,
            "Bitunix cancel/recreate fallback requires a valid parent LIMIT price".into(),
        );
    }
    if !amount.is_finite() || amount <= 0.0 {
        return reject(
            host,
            "Bitunix cancel/recreate fallback requires a valid parent quantity".into(),
        );
    }
    let mut replacement = object(vec![]);
    for key in ["symbol", "type", "side", "timeInForce"] {
        if let Some(value) = target.get(key) {
            put(&mut replacement, key, value.clone());
        }
    }
    put(&mut replacement, "price", Input::Float(price));
    put(&mut replacement, "amount", Input::Float(amount));
    put(
        &mut replacement,
        "reduceOnly",
        target
            .get("reduceOnly")
            .filter(|v| truth(Some(v)))
            .cloned()
            .unwrap_or(Input::Bool(false)),
    );
    for (prefix, trigger_key, price_key) in [
        ("tp", "tpTriggerType", "takeProfit"),
        ("sl", "slTriggerType", "stopLoss"),
    ] {
        let trigger = float(field(&format!("{prefix}Price")).as_ref())?;
        if !trigger.is_finite() || trigger <= 0.0 {
            continue;
        }
        let order_type = field(&format!("{prefix}OrderType"));
        let order_type = string(
            order_type
                .as_ref()
                .filter(|v| truth(Some(v)))
                .or(Some(&s("MARKET"))),
        )?
        .to_uppercase();
        if order_type != "MARKET" {
            return reject(
                host,
                format!(
                    "Bitunix cannot cancel/recreate attached {} with LIMIT execution semantics",
                    prefix.to_uppercase()
                ),
            );
        }
        put(&mut replacement, price_key, Input::Float(trigger));
        put(
            &mut replacement,
            trigger_key,
            s(get_internal_trigger_type(
                field(&format!("{prefix}StopType")).as_ref(),
            )?),
        );
    }
    if replacement.get("takeProfit").is_none() && replacement.get("stopLoss").is_none() {
        return reject(
            host,
            "Bitunix cancel/recreate fallback requires at least one attached TP/SL leg".into(),
        );
    }
    let parent_id = string(target.get("id"))?;
    let response = host
        .post_limited(
            endpoint("CANCEL_ORDER"),
            object(vec![
                ("symbol", j(target.get("symbol"))),
                (
                    "orderList",
                    Input::Array(vec![object(vec![("orderId", s(&parent_id))])]),
                ),
            ]),
        )
        .await?;
    let body = response.get("data");
    let code = body.and_then(|d| d.get("code"));
    if number(code)? != 0.0 {
        let message = body
            .and_then(|d| d.get("msg"))
            .filter(|v| truth(Some(v)))
            .map_or_else(
                || {
                    string(code)
                        .map(|code| s(format!("Bitunix cancel order failed with code {code}")))
                },
                |msg| Ok(j(Some(msg))),
            )?;
        return Err(host.error(message)?);
    }
    let prefix = format!("{parent_id}__");
    let ids = host
        .orders()?
        .iter()
        .filter_map(|order| order.get("id").map(|id| string(Some(id))))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|id| *id == parent_id || id.starts_with(&prefix))
        .collect::<Vec<_>>();
    host.update_status(&ids)?;
    let replacement_ids = host.place_replacement(replacement).await?;
    Ok(replacement_ids
        .first()
        .filter(|id| !id.is_empty())
        .map(|id| vec![format!("{id}__{virtual_leg}")])
        .unwrap_or_default())
}

pub async fn update_order<H: OrderModelHost>(
    host: &mut H,
    order: &Input,
    update: &Input,
) -> Result<Vec<String>, H::Error> {
    let virtual_leg = host.attached_leg(order)?;
    let virtual_order = virtual_leg.is_some();
    let id = string(order.get("id"))?;
    let parent = if virtual_order {
        crate::order_ids::get_virtual_tpsl_parent_order_id(&id).to_owned()
    } else {
        id.clone()
    };
    let target = if virtual_order {
        host.orders()?.into_iter().find(|candidate| {
            candidate
                .get("id")
                .is_some_and(|id| id.text_equals(&parent))
        })
    } else {
        Some(order.clone())
    };
    if !virtual_order && host.standalone(order)? {
        return host.update_standalone(order, update).await;
    }
    let Some(target) = target else {
        return reject(
            host,
            format!("Bitunix cannot safely amend {id}: reconciled parent order not found"),
        );
    };
    if virtual_order && has(update, "amount") {
        return reject(
            host,
            "Bitunix attached TP/SL quantity cannot be amended independently of its parent order"
                .into(),
        );
    }
    if virtual_order && !has(update, "price") {
        return reject(
            host,
            "Bitunix attached TP/SL amendment requires a trigger price".into(),
        );
    }
    if !kind(&target, "limit") {
        return reject(
            host,
            format!(
                "Bitunix native amendment supports only reconciled pending LIMIT orders; {} requires an ambiguous TP/SL or order-type route",
                string(order.get("type"))?
            ),
        );
    }
    let symbol = target.get("symbol").cloned().unwrap_or(Input::Null);
    let Some(market) = host.market(&symbol)? else {
        return reject(
            host,
            format!("Market {} not found", string(target.get("symbol"))?),
        );
    };
    let info = target.get("info");
    let record = info.is_some_and(Input::is_object);
    let raw_id = info.filter(|_| record).and_then(|i| i.get("orderId"));
    let raw_type = if record {
        string(
            info.and_then(|i| i.get("orderType"))
                .filter(|v| truth(Some(v)))
                .or_else(|| info.and_then(|i| i.get("type")).filter(|v| truth(Some(v))))
                .or(Some(&s(""))),
        )?
        .to_uppercase()
    } else {
        String::new()
    };
    let target_id = string(target.get("id"))?;
    let identity = if record {
        raw_id.is_some_and(|v| !v.is_null() && v != &s("")) && string(raw_id)? == target_id
    } else {
        target.get("id") != Some(&s(""))
    };
    if !identity
        || if record {
            raw_type != "LIMIT"
        } else {
            !kind(&target, "limit")
        }
    {
        return reject(
            host,
            format!(
                "Bitunix cannot safely amend {target_id}: exact pending LIMIT order identity is not reconciled"
            ),
        );
    }
    if record
        && (!info
            .and_then(|i| i.get("reduceOnly"))
            .is_some_and(|value| value.as_bool().is_some())
            || !strict(
                info.and_then(|i| i.get("reduceOnly")),
                target.get("reduceOnly"),
            ))
    {
        return reject(
            host,
            format!("Bitunix cannot safely amend {target_id}: reduce-only state is not reconciled"),
        );
    }
    let qty = adjusted(
        if has(update, "amount") {
            update.get("amount")
        } else {
            target.get("amount")
        },
        pstep(&market, "amount"),
    )?;
    let price = adjusted(
        if has(update, "price") && !virtual_order {
            update.get("price")
        } else {
            target.get("price")
        },
        pstep(&market, "price"),
    )?;
    if !qty.is_finite() || qty <= 0.0 {
        return reject(
            host,
            "Bitunix amended order quantity must be greater than zero".into(),
        );
    }
    if !price.is_finite() || price <= 0.0 {
        return reject(
            host,
            "Bitunix amended LIMIT price must be greater than zero".into(),
        );
    }
    let filled = number(target.get("filled"))?;
    if filled > 0.0 && qty <= filled {
        return reject(
            host,
            "Bitunix amended order quantity must exceed the already-filled quantity".into(),
        );
    }
    let mut payload = object(vec![
        ("orderId", j(target.get("id"))),
        ("symbol", j(target.get("symbol"))),
        ("qty", s(js_number_string(qty))),
        ("price", s(js_number_string(price))),
    ]);
    for (prefix, trigger_key) in [("tp", "tpTriggerType"), ("sl", "slTriggerType")] {
        let trigger = if virtual_leg == Some(prefix) {
            update.get("price")
        } else {
            info.and_then(|i| i.get(&format!("{prefix}Price")))
        };
        if trigger.is_none() || trigger.is_some_and(|v| v.is_null() || v == &s("")) {
            continue;
        }
        let price = adjusted(trigger, pstep(&market, "price"))?;
        if !price.is_finite() || price <= 0.0 {
            return reject(
                host,
                format!(
                    "Bitunix {} trigger price is not reconciled",
                    prefix.to_uppercase()
                ),
            );
        }
        let stop_key = format!("{prefix}StopType");
        let type_key = format!("{prefix}OrderType");
        let price_key = format!("{prefix}OrderPrice");
        let stop_type =
            host.trigger(target.get(trigger_key), info.and_then(|i| i.get(&stop_key)))?;
        let order_type = string(
            info.and_then(|i| i.get(&type_key))
                .filter(|v| truth(Some(v)))
                .or(Some(&s("MARKET"))),
        )?
        .to_uppercase();
        if order_type != "MARKET" && order_type != "LIMIT" {
            return reject(
                host,
                format!(
                    "Bitunix {} execution type is not reconciled",
                    prefix.to_uppercase()
                ),
            );
        }
        put(
            &mut payload,
            &format!("{prefix}Price"),
            s(js_number_string(price)),
        );
        put(&mut payload, &stop_key, s(stop_type));
        put(&mut payload, &type_key, s(order_type.clone()));
        if order_type == "LIMIT" {
            let price = adjusted(
                info.and_then(|i| i.get(&price_key)),
                pstep(&market, "price"),
            )?;
            if !price.is_finite() || price <= 0.0 {
                return reject(
                    host,
                    format!(
                        "Bitunix {} LIMIT execution price is not reconciled",
                        prefix.to_uppercase()
                    ),
                );
            }
            put(&mut payload, &price_key, s(js_number_string(price)));
        }
    }
    let result = async {
        let reply = host.post(endpoint("MODIFY_ORDER"), payload.clone()).await?;
        let body = reply.get("data");
        let code = body.and_then(|d| d.get("code"));
        if number(code)? != 0.0 {
            let message =
                if let Some(msg) = body.and_then(|d| d.get("msg")).filter(|v| truth(Some(v))) {
                    j(Some(msg))
                } else {
                    s(format!(
                        "Bitunix modify order failed with code {}",
                        string(code)?
                    ))
                };
            return Err(host.error(message)?);
        }
        let response_id = body
            .and_then(|d| d.get("data"))
            .and_then(|d| d.get("orderId"));
        let acknowledged =
            if response_id.is_none() || response_id.is_some_and(|v| v.is_null() || v == &s("")) {
                target_id
            } else {
                string(response_id)?
            };
        Ok(vec![if let Some(leg) = virtual_leg {
            format!("{acknowledged}__{leg}")
        } else {
            acknowledged
        }])
    }
    .await;
    match result {
        Ok(ids) => Ok(ids),
        Err(error) => {
            let message = host.reason(&error, Some("Bitunix modify order failed"))?;
            if let Some(leg) = virtual_leg
                && is_unsupported_order_operation(&string(Some(&message))?)
            {
                return match recreate_attached_tpsl_parent_order(host, &target, leg, &payload).await
                {
                    Ok(ids) => Ok(ids),
                    Err(error) => {
                        let message =
                            host.reason(&error, Some("Bitunix cancel/recreate fallback failed"))?;
                        host.emit(message.clone())?;
                        Err(host.error(message)?)
                    }
                };
            }
            host.emit(message.clone())?;
            Err(host.error(message)?)
        }
    }
}

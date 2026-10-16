//! Trading REST bodies with bound imported helpers, parent methods and HTTP clients.
use crate::rest_account::{Handle, ReadHost, ReadResult};
use std::future::Future;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Client {
    Xhr,
    Unlimited,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Post,
    Put,
}
pub trait ErrorHost: ReadHost {
    fn is_string(&self, value: &Self::Value) -> bool;
    fn lower_method(&self, value: &Self::Value, expression: &str)
    -> ReadResult<Self, Handle<Self>>;
    fn upper_method(&self, value: &Self::Value, expression: &str)
    -> ReadResult<Self, Handle<Self>>;
    fn error_value(&self, error: &Self::Error) -> ReadResult<Self, Handle<Self>>;
    fn error_response(&self, error: &Self::Error) -> ReadResult<Self, Handle<Self>>;
    fn error_details(&self, error: &Self::Error) -> ReadResult<Self, Handle<Self>>;
    fn new_error(
        &self,
        message: Handle<Self>,
        response: Option<Handle<Self>>,
        cause: Option<&Self::Error>,
    ) -> Self::Error;
    fn emit_and_log(&self, message: Handle<Self>, context: Option<&str>) -> ReadResult<Self, ()>;
}
pub trait TradingHost: ErrorHost {
    fn resolve_caller_id(&self, options: &Self::Value) -> ReadResult<Self, ()>;
    fn adjust(&self, value: &Self::Value, precision: &Self::Value) -> ReadResult<Self, f64>;
    fn uuid(&self) -> ReadResult<Self, Vec<u16>>;
    fn spot_client_id(&self) -> ReadResult<Self, Handle<Self>>;
    fn iterable(
        &self,
        value: &Self::Value,
        expression: &str,
    ) -> ReadResult<Self, Vec<Handle<Self>>>;
    fn send(
        &self,
        spot: bool,
        client: Client,
        method: Method,
        endpoint: &str,
        body: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn derive(&self, options: Vec<Handle<Self>>) -> ReadResult<Self, Handle<Self>>;
    fn place_orders(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn cancel_orders(
        &self,
        orders: Vec<Handle<Self>>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, ()>>>;
    fn place_order(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn finite_number(&self, value: &Self::Value) -> bool;
    fn set_leverage(
        &self,
        symbol: Handle<Self>,
        leverage: Option<Handle<Self>>,
    ) -> ReadResult<Self, ()>;
    fn info(&self, message: Handle<Self>) -> ReadResult<Self, ()>;
    fn fetch_positions(
        &self,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>>;
    fn add_positions(&self, positions: Handle<Self>) -> ReadResult<Self, ()>;
}
pub(crate) fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
pub(crate) fn text<H: ReadHost>(h: &H, s: &str) -> Handle<H> {
    h.text(&units(s))
}
pub(crate) fn field<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> ReadResult<H, Handle<H>> {
    h.field(v, &units(k), false)
}
pub(crate) fn opt<H: ReadHost>(h: &H, v: &H::Value, k: &str) -> ReadResult<H, Handle<H>> {
    h.field(v, &units(k), true)
}
pub(crate) fn eq<H: ReadHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.strict_eq(v, &text(h, s))
}
pub(crate) fn obj<H: ReadHost>(h: &H, v: Vec<(&str, Handle<H>)>) -> ReadResult<H, Handle<H>> {
    h.object(v.into_iter().map(|(k, v)| (units(k), v)).collect())
}
pub(crate) fn optional_or<H: ReadHost>(
    h: &H,
    v: &H::Value,
    k: &str,
    fallback: Handle<H>,
) -> ReadResult<H, Handle<H>> {
    let v = opt(h, v, k)?;
    Ok(if h.truthy(&v) { v } else { fallback })
}
fn market<H: ReadHost>(h: &H, symbol: &H::Value) -> ReadResult<H, Handle<H>> {
    h.field(&*h.current("marketsBySymbol")?, &h.string(symbol)?, false)
}
fn adjust<H: TradingHost>(
    h: &H,
    value: &H::Value,
    market: &H::Value,
    key: &str,
) -> ReadResult<H, f64> {
    h.adjust(value, &*field(h, &*field(h, market, "precision")?, key)?)
}
fn put<H: ReadHost>(h: &H, o: &H::Value, k: &str, v: Handle<H>) -> ReadResult<H, ()> {
    h.assign(o, &units(k), v)
}
fn error<H: ErrorHost>(h: &H, s: &str) -> H::Error {
    h.new_error(text(h, s), None, None)
}
fn template<H: ReadHost>(
    h: &H,
    prefix: &str,
    v: &H::Value,
    suffix: &str,
) -> ReadResult<H, Handle<H>> {
    Ok(h.text(
        &units(prefix)
            .into_iter()
            .chain(h.string(v)?)
            .chain(units(suffix))
            .collect::<Vec<_>>(),
    ))
}
fn undefined_or<H: ReadHost>(
    h: &H,
    v: Handle<H>,
    otherwise: impl FnOnce() -> ReadResult<H, Handle<H>>,
) -> ReadResult<H, Handle<H>> {
    if h.undefined(&v) { otherwise() } else { Ok(v) }
}
fn nullish_or<H: ReadHost>(
    h: &H,
    v: Handle<H>,
    otherwise: impl FnOnce() -> ReadResult<H, Handle<H>>,
) -> ReadResult<H, Handle<H>> {
    if h.nullish(&v) { otherwise() } else { Ok(v) }
}
fn trigger<H: ReadHost>(h: &H, v: &H::Value) -> Handle<H> {
    text(
        h,
        if eq(h, v, "mark_price") {
            "mark"
        } else if eq(h, v, "index_price") {
            "spot"
        } else {
            "last"
        },
    )
}
fn parse_trigger<H: ReadHost>(h: &H, v: &H::Value) -> ReadResult<H, Handle<H>> {
    let v = if h.truthy(v) {
        h.lower(&h.string(v)?)
    } else {
        vec![]
    };
    Ok(if v == units("mark") {
        text(h, "mark_price")
    } else if v == units("spot") {
        text(h, "index_price")
    } else if v == units("last") {
        text(h, "last_price")
    } else {
        opt(h, &*obj(h, vec![])?, "missing")?
    })
}
fn endpoint(spot: bool, key: &str) -> &'static str {
    let table = if spot {
        crate::spot_types::ENDPOINTS
    } else {
        crate::types::ENDPOINTS
    };
    table
        .iter()
        .find(|(k, _)| *k == key)
        .expect("source endpoint")
        .1
}
fn response_error<H: ErrorHost>(
    h: &H,
    e: &H::Error,
    fallback: &str,
    override_response: bool,
) -> ReadResult<H, Handle<H>> {
    let details = h.error_details(e)?;
    if override_response {
        let value = opt(h, &details, "error")?;
        if h.truthy(&value) {
            return Ok(value);
        }
    }
    h.error_message(e, fallback)
}
fn throw_report<H: ErrorHost, T>(h: &H, message: Handle<H>, context: &str) -> ReadResult<H, T> {
    h.emit_and_log(message.clone(), Some(context))?;
    Err(h.new_error(message, None, None))
}
fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b && a == 0.0 {
        if a.is_sign_positive() || b.is_sign_positive() {
            0.0
        } else {
            -0.0
        }
    } else {
        a.max(b)
    }
}
fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b && a == 0.0 {
        if a.is_sign_negative() || b.is_sign_negative() {
            -0.0
        } else {
            0.0
        }
    } else {
        a.min(b)
    }
}

pub async fn futures_place<H: TradingHost>(h: &H, opts: Handle<H>) -> ReadResult<H, Handle<H>> {
    h.resolve_caller_id(&opts)?;
    let result = async {
        let mut order_type = if eq(h, &*field(h, &opts, "type")?, "limit") {
            "lmt"
        } else {
            "mkt"
        };
        let typ = field(h, &opts, "type")?;
        if eq(h, &typ, "stop_market") {
            order_type = "stp";
        } else if eq(h, &typ, "take_profit_market") {
            order_type = "take_profit";
        } else if eq(h, &typ, "trailing_stop_market") {
            order_type = "trailing_stop";
        } else if eq(h, &typ, "limit") {
            let tif = field(h, &opts, "timeInForce")?;
            if eq(h, &tif, "PostOnly") {
                order_type = "post";
            } else if eq(h, &tif, "ImmediateOrCancel") {
                order_type = "ioc";
            } else if eq(h, &tif, "FillOrKill") {
                order_type = "fok";
            }
        }
        let symbol = field(h, &opts, "symbol")?;
        let market = market(h, &symbol)?;
        if !h.truthy(&market) {
            return Err(h.new_error(
                template(h, "Market ", &*field(h, &opts, "symbol")?, " not found")?,
                None,
                None,
            ));
        }
        let size = adjust(h, &*field(h, &opts, "amount")?, &market, "amount")?;
        let body = obj(
            h,
            vec![
                ("orderType", text(h, order_type)),
                ("symbol", field(h, &opts, "symbol")?),
                (
                    "side",
                    text(
                        h,
                        if eq(h, &*field(h, &opts, "side")?, "buy") {
                            "buy"
                        } else {
                            "sell"
                        },
                    ),
                ),
                ("size", h.number(size)),
                (
                    "cliOrdId",
                    h.text(
                        &units("ts_")
                            .into_iter()
                            .chain(h.uuid()?)
                            .collect::<Vec<_>>(),
                    ),
                ),
                ("broker", text(h, "AA73 N84G 7YOF J26Y")),
            ],
        )?;
        let typ = field(h, &opts, "type")?;
        if eq(h, &typ, "limit") && h.truthy(&*field(h, &opts, "price")?) {
            put(
                h,
                &body,
                "limitPrice",
                h.number(adjust(h, &*field(h, &opts, "price")?, &market, "price")?),
            )?;
        }
        let typ = field(h, &opts, "type")?;
        if eq(h, &typ, "trailing_stop_market") {
            let symbol = field(h, &opts, "symbol")?;
            let ticker = h.field(&*h.current("tickersBySymbol")?, &h.string(&symbol)?, false)?;
            if !h.truthy(&ticker)
                || !h.finite_number(&*field(h, &ticker, "last")?)
                || h.numeric(&*field(h, &ticker, "last")?)? <= 0.0
            {
                return Err(h.new_error(
                    template(
                        h,
                        "Ticker ",
                        &*field(h, &opts, "symbol")?,
                        " is required for a Kraken trailing stop",
                    )?,
                    None,
                    None,
                ));
            }
            if !h.finite_number(&*field(h, &opts, "price")?)
                || h.numeric(&*field(h, &opts, "price")?)? <= 0.0
            {
                return Err(h.new_error(
                    template(
                        h,
                        "Trailing stop price for ",
                        &*field(h, &opts, "symbol")?,
                        " must be a positive finite value",
                    )?,
                    None,
                    None,
                ));
            }
            let deviation = adjust(
                h,
                &*h.number(
                    (h.numeric(&*field(h, &ticker, "last")?)?
                        - h.numeric(&*field(h, &opts, "price")?)?)
                    .abs(),
                ),
                &market,
                "price",
            )?;
            if !deviation.is_finite() || deviation <= 0.0 {
                return Err(h.new_error(
                    template(
                        h,
                        "Trailing stop price for ",
                        &*field(h, &opts, "symbol")?,
                        " must differ from the last price",
                    )?,
                    None,
                    None,
                ));
            }
            put(h, &body, "trailingStopMaxDeviation", h.number(deviation))?;
            put(
                h,
                &body,
                "trailingStopDeviationUnit",
                text(h, "QUOTE_CURRENCY"),
            )?;
            put(h, &body, "reduceOnly", h.boolean(true))?;
            if h.truthy(&*field(h, &opts, "slTriggerType")?) {
                put(
                    h,
                    &body,
                    "triggerSignal",
                    trigger(h, &*field(h, &opts, "slTriggerType")?),
                )?;
            }
        } else if eq(h, &typ, "stop_market") || eq(h, &typ, "take_profit_market") {
            if h.truthy(&*field(h, &opts, "price")?) {
                put(
                    h,
                    &body,
                    "stopPrice",
                    h.number(adjust(h, &*field(h, &opts, "price")?, &market, "price")?),
                )?;
            }
            put(h, &body, "reduceOnly", h.boolean(true))?;
            let sig = field(
                h,
                &opts,
                if eq(h, &*field(h, &opts, "type")?, "stop_market") {
                    "slTriggerType"
                } else {
                    "tpTriggerType"
                },
            )?;
            if h.truthy(&sig) {
                put(h, &body, "triggerSignal", trigger(h, &sig))?;
            }
        } else if h.truthy(&*field(h, &opts, "reduceOnly")?) {
            put(h, &body, "reduceOnly", h.boolean(true))?;
        }
        let data = h
            .send(
                false,
                Client::Unlimited,
                Method::Post,
                endpoint(false, "ORDER"),
                body,
            )?
            .await?;
        if !eq(h, &*opt(h, &data, "result")?, "success") {
            return Err(h.new_error(
                optional_or(h, &data, "error", text(h, "Order placement failed"))?,
                None,
                None,
            ));
        }
        let status = opt(h, &*opt(h, &data, "sendStatus")?, "status")?;
        if h.truthy(&status) && !eq(h, &status, "placed") && !eq(h, &status, "filled") {
            return Err(h.new_error(template(h, "Order rejected: ", &status, "")?, None, None));
        }
        let id = h.text(&h.string(&*optional_or(
            h,
            &*opt(h, &data, "sendStatus")?,
            "order_id",
            text(h, ""),
        )?)?);
        let mut ids = if h.truthy(&id) { vec![id] } else { vec![] };
        if eq(h, &*field(h, &opts, "type")?, "limit")
            && (h.truthy(&*field(h, &opts, "stopLoss")?)
                || h.truthy(&*field(h, &opts, "takeProfit")?))
        {
            let derived = h.derive(vec![opts.clone()])?;
            let extra = h.place_orders(derived)?.await?;
            ids.extend(h.iterable(&extra, "extraIds")?);
        }
        Ok(h.array(ids))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            let details = h.error_details(&e)?;
            let mut message = h.error_message(&e, "Failed to place order")?;
            if h.truthy(&*opt(h, &details, "error")?) {
                message = field(h, &details, "error")?;
            } else if eq(h, &*opt(h, &details, "result")?, "error") {
                message = optional_or(h, &details, "error", text(h, "Order placement failed"))?;
            }
            let lower = h.lower_method(&message, "errorMessage")?;
            if h.includes(&lower, "invalidsize")? {
                message = template(
                    h,
                    "Invalid order size for ",
                    &*field(h, &opts, "symbol")?,
                    ". Check minimum order size and precision.",
                )?;
            } else if h.includes(&lower, "insufficientavailablefunds")?
                || h.includes(&lower, "insufficient")?
            {
                message = text(h, "Insufficient available funds to place this order.");
            } else if h.includes(&lower, "authentication")? || h.includes(&lower, "apikey")? {
                message = text(
                    h,
                    "Authentication failed. Your API key may lack trading permissions. Please ensure General API is set to \"Full access\".",
                );
            } else if h.includes(&lower, "permission")? {
                message = text(
                    h,
                    "Permission denied. Please ensure General API is set to \"Full access\" for your Kraken Futures API key.",
                );
            } else if h.includes(&lower, "size")? || h.includes(&lower, "quantity")? {
                message = template(h, "Invalid order size. ", &message, "")?;
            } else if h.includes(&lower, "price")? {
                message = template(h, "Invalid order price. ", &message, "")?;
            } else if h.includes(&lower, "margin")? || h.includes(&lower, "balance")? {
                message = template(h, "Insufficient margin or balance. ", &message, "")?;
            }
            throw_report(h, message, "PlaceOrder Error")
        }
    }
}

pub async fn spot_place<H: TradingHost>(h: &H, opts: Handle<H>) -> ReadResult<H, Handle<H>> {
    h.resolve_caller_id(&opts)?;
    let result = async {
        let market = market(h, &*field(h, &opts, "symbol")?)?;
        if !h.truthy(&market) {
            return Err(h.new_error(
                template(h, "Market ", &*field(h, &opts, "symbol")?, " not found")?,
                None,
                None,
            ));
        }
        let pair = opt(h, &*field(h, &market, "info")?, "pairName")?;
        if !h.truthy(&pair) {
            return Err(h.new_error(
                template(
                    h,
                    "No REST pair name for ",
                    &*field(h, &opts, "symbol")?,
                    "",
                )?,
                None,
                None,
            ));
        }
        let typ = field(h, &opts, "type")?;
        let order_type = if eq(h, &typ, "market") {
            "market"
        } else if eq(h, &typ, "stop_market") {
            "stop-loss"
        } else if eq(h, &typ, "take_profit_market") {
            "take-profit"
        } else if eq(h, &typ, "trailing_stop_market") {
            "trailing-stop"
        } else {
            "limit"
        };
        let volume = adjust(h, &*field(h, &opts, "amount")?, &market, "amount")?;
        let id = h.spot_client_id()?;
        let id_text = h.string(&id)?;
        if id_text.len() != 32 || !id_text.iter().all(|c| matches!(c,48..=57|97..=102)) {
            return Err(error(
                h,
                "Failed to generate a valid Kraken Spot client order ID",
            ));
        }
        let body = obj(
            h,
            vec![
                ("pair", pair),
                (
                    "type",
                    text(
                        h,
                        if eq(h, &*field(h, &opts, "side")?, "buy") {
                            "buy"
                        } else {
                            "sell"
                        },
                    ),
                ),
                ("ordertype", text(h, order_type)),
                ("volume", h.text(&h.string(&*h.number(volume))?)),
                ("cl_ord_id", id),
            ],
        )?;
        let typ = field(h, &opts, "type")?;
        if (eq(h, &typ, "limit") || eq(h, &typ, "stop_market") || eq(h, &typ, "take_profit_market"))
            && h.truthy(&*field(h, &opts, "price")?)
        {
            let price = adjust(h, &*field(h, &opts, "price")?, &market, "price")?;
            put(h, &body, "price", h.text(&h.string(&*h.number(price))?))?;
        }
        if eq(h, &*field(h, &opts, "type")?, "trailing_stop_market") {
            let symbol = field(h, &opts, "symbol")?;
            let ticker = h.field(&*h.current("tickersBySymbol")?, &h.string(&symbol)?, false)?;
            if !h.truthy(&ticker)
                || !h.finite_number(&*field(h, &ticker, "last")?)
                || h.numeric(&*field(h, &ticker, "last")?)? <= 0.0
            {
                return Err(h.new_error(
                    template(
                        h,
                        "Ticker ",
                        &symbol,
                        " is required for a Kraken Spot trailing stop",
                    )?,
                    None,
                    None,
                ));
            }
            if !h.finite_number(&*field(h, &opts, "price")?)
                || h.numeric(&*field(h, &opts, "price")?)? <= 0.0
            {
                return Err(h.new_error(
                    template(
                        h,
                        "Trailing stop price for ",
                        &symbol,
                        " must be a positive finite value",
                    )?,
                    None,
                    None,
                ));
            }
            let deviation = adjust(
                h,
                &*h.number(
                    (h.numeric(&*field(h, &ticker, "last")?)?
                        - h.numeric(&*field(h, &opts, "price")?)?)
                    .abs(),
                ),
                &market,
                "price",
            )?;
            if !deviation.is_finite() || deviation <= 0.0 {
                return Err(h.new_error(
                    template(
                        h,
                        "Trailing stop price for ",
                        &symbol,
                        " must differ from the last price",
                    )?,
                    None,
                    None,
                ));
            }
            put(
                h,
                &body,
                "price",
                template(h, "+", &*h.number(deviation), "")?,
            )?;
        }
        if eq(h, &*field(h, &opts, "timeInForce")?, "ImmediateOrCancel") {
            put(h, &body, "timeinforce", text(h, "IOC"))?;
        }
        if eq(h, &*field(h, &opts, "timeInForce")?, "PostOnly") {
            put(h, &body, "oflags", text(h, "post"))?;
        }
        let data = h
            .send(
                true,
                Client::Unlimited,
                Method::Post,
                endpoint(true, "ADD_ORDER"),
                body,
            )?
            .await?;
        let txids = opt(h, &*opt(h, &data, "result")?, "txid")?;
        let mut ids = if h.truthy(&txids) {
            h.iterable(&txids, "txids")?
        } else {
            vec![]
        };
        if eq(h, &*field(h, &opts, "type")?, "limit")
            && (h.truthy(&*field(h, &opts, "stopLoss")?)
                || h.truthy(&*field(h, &opts, "takeProfit")?))
        {
            let derived = h.derive(vec![opts.clone()])?;
            let extra = h.place_orders(derived)?.await?;
            ids.extend(h.iterable(&extra, "extraIds")?);
        }
        Ok(h.array(ids))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => throw_report(
            h,
            h.error_message(&e, "Failed to place order")?,
            "PlaceOrder Error",
        ),
    }
}

pub async fn futures_update<H: TradingHost>(h: &H, opts: &H::Value) -> ReadResult<H, Handle<H>> {
    let result = async {
        let order = field(h, opts, "order")?;
        let update = field(h, opts, "update")?;
        let typ = field(h, &order, "type")?;
        if eq(h, &typ, "trailing_stop_market") {
            return Err(h.new_error(
                template(
                    h,
                    "Cannot safely update Kraken trailing stop order ",
                    &*field(h, &order, "id")?,
                    ": recreating it would reset its trailing state",
                )?,
                None,
                None,
            ));
        }
        if eq(h, &typ, "stop_market") || eq(h, &typ, "take_profit_market") {
            let key = if eq(h, &typ, "stop_market") {
                "slTriggerType"
            } else {
                "tpTriggerType"
            };
            let sig = nullish_or(h, field(h, &order, key)?, || {
                parse_trigger(h, &*h.keyed(&*field(h, &order, "info")?, "triggerSignal")?)
            })?;
            if !h.truthy(&sig) {
                let prefix = template(
                    h,
                    "Cannot safely update Kraken ",
                    &*field(h, &order, "type")?,
                    " order ",
                )?;
                let suffix = template(
                    h,
                    &String::from_utf16_lossy(&h.string(&prefix)?),
                    &*field(h, &order, "id")?,
                    ": triggerSignal provenance is missing or unsupported",
                )?;
                return Err(h.new_error(suffix, None, None));
            }
            h.cancel_orders(vec![order.clone()])?.await?;
            let body = obj(
                h,
                vec![
                    ("symbol", field(h, &order, "symbol")?),
                    ("side", field(h, &order, "side")?),
                    ("type", field(h, &order, "type")?),
                    (
                        "amount",
                        undefined_or(h, field(h, &update, "amount")?, || {
                            field(h, &order, "amount")
                        })?,
                    ),
                    (
                        "price",
                        undefined_or(h, field(h, &update, "price")?, || field(h, &order, "price"))?,
                    ),
                    ("reduceOnly", field(h, &order, "reduceOnly")?),
                    (
                        if eq(h, &*field(h, &order, "type")?, "stop_market") {
                            "slTriggerType"
                        } else {
                            "tpTriggerType"
                        },
                        sig,
                    ),
                ],
            )?;
            return match async { h.place_order(body)?.await }.await {
                Ok(v) => Ok(v),
                Err(e) => {
                    let value = h.error_message(&e, "")?;
                    let message = if h.truthy(&value) {
                        value
                    } else {
                        h.error_value(&e)?
                    };
                    let text_value = h.text(
                        &units("Kraken canceled the original ")
                            .into_iter()
                            .chain(h.string(&*field(h, &order, "type")?)?)
                            .chain(units(" order but failed to place its replacement: "))
                            .chain(h.string(&message)?)
                            .collect::<Vec<_>>(),
                    );
                    Err(h.new_error(text_value, None, Some(&e)))
                }
            };
        }
        let market = market(h, &*field(h, &order, "symbol")?)?;
        let body = obj(h, vec![("orderId", field(h, &order, "id")?)])?;
        for (input, output, precision) in [
            ("amount", "size", "amount"),
            ("price", "limitPrice", "price"),
        ] {
            let value = field(h, &update, input)?;
            if !h.undefined(&value) {
                let value = field(h, &update, input)?;
                let value = if h.truthy(&market) {
                    h.number(adjust(h, &value, &market, precision)?)
                } else {
                    value
                };
                put(h, &body, output, value)?;
            }
        }
        let data = h
            .send(
                false,
                Client::Unlimited,
                Method::Post,
                endpoint(false, "EDIT_ORDER"),
                body,
            )?
            .await?;
        if !eq(h, &*opt(h, &data, "result")?, "success") {
            return Err(h.new_error(
                optional_or(h, &data, "error", text(h, "Order update failed"))?,
                None,
                None,
            ));
        }
        let status = opt(h, &*opt(h, &data, "editStatus")?, "status")?;
        if h.truthy(&status) && !eq(h, &status, "edited") && !eq(h, &status, "filled") {
            return Err(h.new_error(
                template(h, "Order update rejected: ", &status, "")?,
                None,
                None,
            ));
        }
        let status = opt(h, &data, "editStatus")?;
        let first = opt(h, &status, "orderId")?;
        let second = if h.truthy(&first) {
            first
        } else {
            opt(h, &status, "order_id")?
        };
        let id = if h.truthy(&second) {
            second
        } else {
            field(h, &order, "id")?
        };
        let id = h.text(&h.string(&id)?);
        Ok(h.array(if h.truthy(&id) { vec![id] } else { vec![] }))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => throw_report(
            h,
            response_error(h, &e, "Failed to update order", true)?,
            "UpdateOrder Error",
        ),
    }
}
pub async fn spot_update<H: TradingHost>(h: &H, opts: &H::Value) -> ReadResult<H, Handle<H>> {
    let result = async {
        let order = field(h, opts, "order")?;
        let update = field(h, opts, "update")?;
        let market = market(h, &*field(h, &order, "symbol")?)?;
        if !h.truthy(&market) {
            return Err(h.new_error(
                template(h, "Market ", &*field(h, &order, "symbol")?, " not found")?,
                None,
                None,
            ));
        }
        let info = field(h, &order, "info")?;
        let raw = opt(h, &*opt(h, &info, "descr")?, "ordertype")?;
        let raw = if h.truthy(&raw) {
            raw
        } else {
            opt(h, &*field(h, &order, "info")?, "order_type")?
        };
        let expected = if h.truthy(&raw) {
            h.order_type(&raw)?
        } else {
            opt(h, &*obj(h, vec![])?, "missing")?
        };
        let limit = eq(h, &raw, "limit") && eq(h, &*field(h, &order, "type")?, "limit");
        let trigger = (eq(h, &raw, "stop-loss")
            && eq(h, &*field(h, &order, "type")?, "stop_market"))
            || (eq(h, &raw, "take-profit")
                && eq(h, &*field(h, &order, "type")?, "take_profit_market"));
        if (!limit && !trigger) || !h.strict_eq(&expected, &*field(h, &order, "type")?) {
            let value = if h.truthy(&raw) {
                raw
            } else {
                field(h, &order, "type")?
            };
            return Err(h.new_error(
                template(
                    h,
                    "Kraken Spot cannot safely amend ",
                    &value,
                    "; only reconciled limit, stop-loss, and take-profit orders are supported",
                )?,
                None,
                None,
            ));
        }
        if limit && eq(h, &*field(h, &order, "timeInForce")?, "ImmediateOrCancel") {
            return Err(error(
                h,
                "Kraken Spot cannot amend an immediate-or-cancel order",
            ));
        }
        let body = obj(h, vec![("txid", field(h, &order, "id")?)])?;
        if !h.undefined(&*field(h, &update, "amount")?) {
            let amount = adjust(h, &*field(h, &update, "amount")?, &market, "amount")?;
            if !amount.is_finite() || amount <= 0.0 {
                return Err(error(
                    h,
                    "Kraken Spot amend quantity must be a positive finite amount",
                ));
            }
            put(
                h,
                &body,
                "order_qty",
                h.text(&h.string(&*h.number(amount))?),
            )?;
        }
        if !h.undefined(&*field(h, &update, "price")?) {
            let price = adjust(h, &*field(h, &update, "price")?, &market, "price")?;
            if !price.is_finite() || price <= 0.0 {
                return Err(error(
                    h,
                    "Kraken Spot amend price must be a positive finite amount",
                ));
            }
            put(
                h,
                &body,
                if limit {
                    "limit_price"
                } else {
                    "trigger_price"
                },
                h.text(&h.string(&*h.number(price))?),
            )?;
            if limit && eq(h, &*field(h, &order, "timeInForce")?, "PostOnly") {
                put(h, &body, "post_only", h.boolean(true))?;
            }
        }
        if h.undefined(&*field(h, &body, "order_qty")?)
            && h.undefined(&*field(h, &body, "limit_price")?)
            && h.undefined(&*field(h, &body, "trigger_price")?)
        {
            return Err(error(
                h,
                "Kraken Spot amend requires an amount or price change",
            ));
        }
        let data = h
            .send(
                true,
                Client::Unlimited,
                Method::Post,
                endpoint(true, "AMEND_ORDER"),
                body,
            )?
            .await?;
        let id = opt(h, &*opt(h, &data, "result")?, "amend_id")?;
        if !h.is_string(&id) || h.string(&id)?.is_empty() {
            return Err(error(
                h,
                "Kraken Spot AmendOrder response did not include an amend_id",
            ));
        }
        Ok(h.array(vec![field(h, &order, "id")?]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => throw_report(
            h,
            h.error_message(&e, "Failed to update order")?,
            "UpdateOrder Error",
        ),
    }
}
pub async fn futures_cancel<H: TradingHost>(h: &H, orders: &H::Value) -> ReadResult<H, ()> {
    let result = async {
        let orders = h.items(orders)?;
        if orders.len() == 1 {
            let body = obj(h, vec![("order_id", field(h, &orders[0], "id")?)])?;
            let data = h
                .send(
                    false,
                    Client::Unlimited,
                    Method::Post,
                    endpoint(false, "CANCEL_ORDER"),
                    body,
                )?
                .await?;
            if !eq(h, &*opt(h, &data, "result")?, "success") {
                return Err(h.new_error(
                    optional_or(h, &data, "error", text(h, "Order cancellation failed"))?,
                    None,
                    None,
                ));
            }
            let status = opt(h, &*opt(h, &data, "cancelStatus")?, "status")?;
            if h.truthy(&status) && !eq(h, &status, "cancelled") {
                return Err(h.new_error(
                    template(h, "Order cancellation rejected: ", &status, "")?,
                    None,
                    None,
                ));
            }
            return Ok(());
        }
        let mut batch = vec![];
        for o in orders {
            batch.push(obj(
                h,
                vec![
                    ("order", text(h, "cancel")),
                    ("order_id", field(h, &o, "id")?),
                ],
            )?);
        }
        let data = h
            .send(
                false,
                Client::Unlimited,
                Method::Post,
                endpoint(false, "BATCH_ORDERS"),
                obj(h, vec![("batchOrder", h.array(batch))])?,
            )?
            .await?;
        if !eq(h, &*opt(h, &data, "result")?, "success") {
            return Err(h.new_error(
                optional_or(
                    h,
                    &data,
                    "error",
                    text(h, "Batch order cancellation failed"),
                )?,
                None,
                None,
            ));
        }
        let rows = opt(h, &data, "batchStatus")?;
        if h.is_array(&rows) {
            for status in h.items(&rows)? {
                let s = opt(h, &status, "status")?;
                if h.truthy(&s) && h.string(&*field(h, &status, "status")?)? != units("cancelled") {
                    return Err(h.new_error(
                        template(
                            h,
                            "Batch order cancellation rejected: ",
                            &*field(h, &status, "status")?,
                            "",
                        )?,
                        None,
                        None,
                    ));
                }
            }
        }
        Ok(())
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => throw_report(
            h,
            response_error(h, &e, "Failed to cancel orders", true)?,
            "CancelOrders Error",
        ),
    }
}
fn preference_error<H: ErrorHost>(h: &H, data: &H::Value, fallback: &str) -> ReadResult<H, ()> {
    if eq(h, &*opt(h, data, "result")?, "error") || eq(h, &*opt(h, data, "status")?, "BAD_REQUEST")
    {
        let first = opt(h, &*opt(h, data, "errors")?, "0")?;
        return Err(h.new_error(
            optional_or(h, &first, "message", text(h, fallback))?,
            None,
            None,
        ));
    }
    if !eq(h, &*opt(h, data, "result")?, "success") {
        return Err(error(h, fallback));
    }
    Ok(())
}
fn preference_message<H: ErrorHost>(
    h: &H,
    e: &H::Error,
    fallback: &str,
) -> ReadResult<H, Handle<H>> {
    let details = h.error_details(e)?;
    let mut message = h.error_message(e, fallback)?;
    let first = opt(h, &*opt(h, &details, "errors")?, "0")?;
    if h.truthy(&*opt(h, &first, "message")?) {
        message = field(h, &first, "message")?;
    } else if h.truthy(&*opt(h, &details, "error")?) {
        message = field(h, &details, "error")?;
    }
    Ok(message)
}
pub async fn futures_set_leverage<H: TradingHost>(
    h: &H,
    symbol: Handle<H>,
    input: &H::Value,
) -> ReadResult<H, ()> {
    let result = async {
        let market = market(h, &symbol)?;
        if !h.truthy(&market) {
            return Err(h.new_error(template(h, "Market ", &symbol, " not found")?, None, None));
        }
        let limits = field(h, &*field(h, &market, "limits")?, "leverage")?;
        let leverage = min(
            max(h.numeric(input)?, h.numeric(&*field(h, &limits, "min")?)?),
            h.numeric(&*field(h, &limits, "max")?)?,
        );
        let body = obj(
            h,
            vec![
                ("symbol", h.upper_method(&symbol, "symbol")?),
                ("maxLeverage", h.number(leverage)),
            ],
        )?;
        let data = h
            .send(
                false,
                Client::Xhr,
                Method::Put,
                endpoint(false, "LEVERAGE_PREFERENCES"),
                body,
            )?
            .await?;
        preference_error(h, &data, "Failed to set leverage")?;
        h.set_leverage(symbol.clone(), Some(h.number(leverage)))?;
        let message = h.text(
            &units("Leverage set to ")
                .into_iter()
                .chain(h.string(&*h.number(leverage))?)
                .chain(units("x for "))
                .chain(h.string(&symbol)?)
                .collect::<Vec<_>>(),
        );
        h.info(message)
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => throw_report(
            h,
            preference_message(h, &e, "Failed to set leverage")?,
            "SetLeverage Error",
        ),
    }
}
pub async fn futures_margin<H: TradingHost>(
    h: &H,
    isolated: &H::Value,
    symbol: Handle<H>,
) -> ReadResult<H, ()> {
    if !h.truthy(&symbol) {
        let message = text(h, "Symbol is required when changing margin type");
        h.emit_and_log(message.clone(), None)?;
        return Err(h.new_error(message, None, None));
    }
    let result = async {
        let market = market(h, &symbol)?;
        if !h.truthy(&market) {
            return Err(h.new_error(template(h, "Market ", &symbol, " not found")?, None, None));
        }
        if h.truthy(isolated) {
            let mut current = None;
            for p in h.items(&*h.current("positions")?)? {
                if h.strict_eq(&*field(h, &p, "symbol")?, &symbol) {
                    current = Some(p);
                    break;
                }
            }
            let leverage = match current {
                Some(p) => nullish_or(h, field(h, &p, "leverage")?, || {
                    field(
                        h,
                        &*field(h, &*field(h, &market, "limits")?, "leverage")?,
                        "max",
                    )
                })?,
                None => field(
                    h,
                    &*field(h, &*field(h, &market, "limits")?, "leverage")?,
                    "max",
                )?,
            };
            let body = obj(
                h,
                vec![
                    ("symbol", h.upper_method(&symbol, "symbol")?),
                    ("maxLeverage", leverage.clone()),
                ],
            )?;
            let data = h
                .send(
                    false,
                    Client::Xhr,
                    Method::Put,
                    endpoint(false, "LEVERAGE_PREFERENCES"),
                    body,
                )?
                .await?;
            preference_error(h, &data, "Failed to set margin type to isolated")?;
            h.set_leverage(symbol.clone(), Some(leverage.clone()))?;
            h.set_preference(symbol.clone(), text(h, "isolated"))?;
            let message = h.text(
                &units("Margin type set to ISOLATED (")
                    .into_iter()
                    .chain(h.string(&leverage)?)
                    .chain(units("x) for "))
                    .chain(h.string(&symbol)?)
                    .collect::<Vec<_>>(),
            );
            h.info(message)?;
        } else {
            let body = obj(h, vec![("symbol", h.upper_method(&symbol, "symbol")?)])?;
            let data = h
                .send(
                    false,
                    Client::Xhr,
                    Method::Put,
                    endpoint(false, "LEVERAGE_PREFERENCES"),
                    body,
                )?
                .await?;
            preference_error(h, &data, "Failed to set margin type to cross")?;
            h.set_leverage(symbol.clone(), None)?;
            h.set_preference(symbol.clone(), text(h, "cross"))?;
            h.info(template(h, "Margin type set to CROSS for ", &symbol, "")?)?;
        }
        let positions = h.fetch_positions()?.await?;
        h.add_positions(positions)
    }
    .await;
    match result {
        Ok(()) => Ok(()),
        Err(e) => throw_report(
            h,
            preference_message(h, &e, "Failed to change margin type")?,
            "ChangeMarginType Error",
        ),
    }
}

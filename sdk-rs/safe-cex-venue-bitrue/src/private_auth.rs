//! Private websocket and API factory bodies over original source bindings.
use crate::collection_reads::{Callback, Op, ReadHost};
use std::rc::Rc;

// ts: isBitrueSignatureRejection, Binance-style -1022; codes arrive as numbers or strings.
pub use safe_cex_transport::signature_rejection::is_binance_style_signature_rejection as is_bitrue_signature_rejection;
pub trait PrivateHost: ReadHost {
    fn contains(&self, v: Self::Value, needle: &str) -> Result<bool, Self::Error>;
    fn less_equal(&self, a: Self::Value, b: Self::Value) -> Result<bool, Self::Error>;
    fn null_is(&self, v: &Self::Value) -> bool;
    fn add(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn copy(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn find(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn is_string(&self, v: &Self::Value) -> bool;
    fn starts_with(&self, v: Self::Value, prefix: &str) -> Result<bool, Self::Error>;
    fn split(&self, v: Self::Value, separator: &str) -> Result<Self::Value, Self::Error>;
    fn index(&self, v: Self::Value, n: usize) -> Result<Self::Value, Self::Error>;
    fn stringify(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn join(&self, v: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn query(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn now(&self) -> Result<Self::Value, Self::Error>;
    fn virtual_now(&self) -> Result<Self::Value, Self::Error>;
    fn parse_message(&self, data: Self::Value) -> Result<Self::Value, Self::Error>;
    fn error_instance(&self, error: &Self::Value) -> bool;
    fn console_args(&self, args: Vec<Self::Value>) -> Result<(), Self::Error>;
    fn delays(&self) -> Result<Self::Value, Self::Error>;
    fn delay_promise(&self, delay: Self::Value) -> Result<Self::Value, Self::Error>;
    fn reverse_symbol(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn create_incremental(&self) -> Result<Self::Value, Self::Error>;
    fn axios_headers(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn axios_create(
        &self,
        config: Self::Value,
        options: Self::Value,
        exchange: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn transport(
        &self,
        client: Self::Value,
        options: Self::Value,
        name: Self::Value,
        parser: &str,
    ) -> Result<(), Self::Error>;
    /// ts: incrementalFills cleanFloat over a Number.
    fn clean_float(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    /// Binds source `isRetryable`: networkErrorOnRead(LISTEN_KEY) || signatureRejectedResponse.
    fn retry_condition(&self) -> Result<Self::Value, Self::Error>;
    /// Binds source `isBitrueSignatureRejection`, i.e. [`is_bitrue_signature_rejection`].
    fn signature_rejection(&self) -> Result<Self::Value, Self::Error>;
    /// Binds `resendOnSignatureRejection(xhr, response, isRejection)`: the resend
    /// promise, or undefined. Hosts decide through `safe_cex_transport::signature_rejection`.
    fn resend_on_signature_rejection(
        &self,
        client: Self::Value,
        response: Self::Value,
        predicate: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn retry(&self, client: Self::Value, options: Self::Value) -> Result<(), Self::Error>;
    fn create_hmac(&self, algorithm: &str, secret: Self::Value)
    -> Result<Self::Value, Self::Error>;
    fn method_uppercase_optional(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn super_socket(&self, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn define(&self, r: Self::Value, key: &str, v: Self::Value) -> Result<(), Self::Error>;
    // Bind every exact source ABI without effects; retain its name and lexical receiver.
    fn member(&self, name: &'static str, target: &'static str, r: Self::Value) -> Self::Value;
    // Class/module binding owns the persistent static array and prototype disposal body.
    fn class_environment(&self, initial_delays: &[f64], disposal_target: &'static str);
    fn named_function(&self, name: &str, f: Callback<Self>) -> Self::Value;
}
fn get<H: PrivateHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(v, k)
}
fn opt<H: PrivateHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, k)
    }
}
fn method<H: PrivateHost>(h: &H, r: H::Value, k: &str) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, k)?, r))
}
fn invoke<H: PrivateHost>(
    h: &H,
    p: (H::Value, H::Value),
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, a)
}
fn call<H: PrivateHost>(
    h: &H,
    r: H::Value,
    k: &str,
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    invoke(h, method(h, r, k)?, a)
}
fn fallback<H: PrivateHost>(h: &H, v: H::Value, to: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { to }
}
fn endpoint() -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == "LISTEN_KEY")
        .expect("source endpoint")
        .1
}
fn accepted<H: PrivateHost>(h: &H, r: H::Value, data: &H::Value) -> Result<bool, H::Error> {
    let p = method(h, r, "isSuccessfulCode")?;
    Ok(h.truthy(&invoke(h, p, vec![opt(h, data, "code")?])?)
        || h.equal(&opt(h, data, "status")?, &h.text("ok"))
        || h.equal(&opt(h, data, "msg")?, &h.text("succ")))
}
fn error_text<H: PrivateHost>(h: &H, e: &H::Value) -> Result<H::Value, H::Error> {
    if h.error_instance(e) {
        get(h, e, "message")
    } else {
        h.string(e.clone())
    }
}
pub fn incremental_fills<H: PrivateHost>(h: &H) -> Result<H::Value, H::Error> {
    h.create_incremental()
}
pub const MEMBERS: [(&str, &str); 15] = [
    (
        "connectAndSubscribe",
        "safe-cex-venue-bitrue/src/private_auth.rs::connect",
    ),
    ("onOpen", "safe-cex-venue-bitrue/src/ws_private.rs::on_open"),
    (
        "onMessage",
        "safe-cex-venue-bitrue/src/private_auth.rs::message",
    ),
    (
        "parseOrderType",
        "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_type",
    ),
    (
        "parseOrderSide",
        "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_side",
    ),
    (
        "parseOrderStatus",
        "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_status",
    ),
    (
        "handleOrderTradeUpdate",
        "safe-cex-venue-bitrue/src/private_auth.rs::order_update",
    ),
    (
        "handleAccountUpdate",
        "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::handle_account_update",
    ),
    (
        "handlePingEvent",
        "safe-cex-venue-bitrue/src/ws_sync.rs::handle_private_ping",
    ),
    ("ping", "safe-cex-venue-bitrue/src/ws_private.rs::ping"),
    (
        "isSuccessfulCode",
        "safe-cex-venue-bitrue/src/ws_private.rs::is_successful_code",
    ),
    (
        "scheduleListenKeyRefresh",
        "safe-cex-venue-bitrue/src/ws_sync.rs::schedule_listen_key_refresh",
    ),
    (
        "fetchListenKey",
        "safe-cex-venue-bitrue/src/private_auth.rs::fetch_key",
    ),
    (
        "updateListenKey",
        "safe-cex-venue-bitrue/src/private_auth.rs::update_key",
    ),
    (
        "subscribe",
        "safe-cex-venue-bitrue/src/ws_sync.rs::subscribe_private",
    ),
];
pub const RETRY_DELAYS: [f64; 3] = [250.0, 500.0, 1_000.0];
pub fn construct<H: PrivateHost>(h: &H, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    h.class_environment(
        &RETRY_DELAYS,
        "safe-cex-venue-bitrue/src/ws_sync.rs::dispose_private",
    );
    let r = h.super_socket(args)?;
    h.define(r.clone(), "incrementalFills", incremental_fills(h)?)?;
    h.define(r.clone(), "listenKeyRefreshTimeoutId", h.undefined())?;
    for (name, target) in MEMBERS {
        h.define(r.clone(), name, h.member(name, target, r.clone()))?;
    }
    Ok(r)
}
pub async fn connect<H: PrivateHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    if !h.truthy(&get(h, &r, "isDisposed")?) {
        let result = async {
            let p = call(h, r.clone(), "fetchListenKey", vec![])?;
            let key = h.await_value(p).await?;
            if h.truthy(&get(h, &r, "isDisposed")?) {
                return Ok(h.undefined());
            }
            let p = method(h, r.clone(), "createWebsocket")?;
            let url = crate::types::BASE_WS_URL
                .iter()
                .find(|(k, _)| *k == "private")
                .expect("private URL")
                .1;
            let address = h.concat(vec![h.text(url), h.text("?listenKey="), h.string(key)?])?;
            let ws = invoke(h, p, vec![address])?;
            h.set(&r, "ws", ws)?;
            for (event, name) in [
                ("open", "onOpen"),
                ("message", "onMessage"),
                ("close", "onClose"),
            ] {
                let ws = get(h, &r, "ws")?;
                let p = method(h, ws, "addEventListener")?;
                let handler = get(h, &r, name)?;
                invoke(h, p, vec![h.text(event), handler])?;
            }
            Ok(h.undefined())
        }
        .await;
        if let Err(e) = result {
            let p = method(h, r, "retryFailedConnect")?;
            let value = h.error_value(&e);
            let message = h.concat(vec![
                h.text("Bitrue futures private WebSocket was not started: "),
                error_text(h, &value)?,
            ])?;
            invoke(h, p, vec![message])?;
        }
    }
    Ok(h.undefined())
}
pub fn message<H: PrivateHost>(h: &H, r: H::Value, event: H::Value) -> Result<H::Value, H::Error> {
    let data = get(h, &event, "data")?;
    if !h.truthy(&get(h, &r, "isDisposed")?) {
        call(h, r.clone(), "trackLastReceived", vec![])?;
        call(h, r.clone(), "emitWsDiagnosticReceive", vec![data.clone()])?;
        let json = match h.parse_message(data.clone()) {
            Ok(v) => v,
            Err(e) => {
                h.console_args(vec![
                    h.text("Failed to parse Bitrue private ws message"),
                    data,
                    h.error_value(&e),
                ])?;
                return Ok(h.undefined());
            }
        };
        if h.equal(&opt(h, &json, "event")?, &h.text("ping"))
            || !h.equal(&opt(h, &json, "ping")?, &h.undefined())
        {
            call(h, r, "handlePingEvent", vec![json])?;
            return Ok(h.undefined());
        }
        let e = opt(h, &json, "e")?;
        let event = if h.truthy(&e) {
            e
        } else {
            opt(h, &json, "event")?
        };
        if h.truthy(&event) {
            call(
                h,
                r.clone(),
                "recordMessageHandlerStats",
                vec![event.clone()],
            )?;
        }
        if ["BALANCE", "ACCOUNT_UPDATE"]
            .iter()
            .any(|v| h.equal(&event, &h.text(v)))
        {
            call(h, r, "handleAccountUpdate", vec![json])?;
        } else if ["ORDER", "ORDER_TRADE_UPDATE", "CANCEL"]
            .iter()
            .any(|v| h.equal(&event, &h.text(v)))
        {
            call(h, r, "handleOrderTradeUpdate", vec![json])?;
        } else if [
            "user_account_update",
            "user_order_update",
            "user_balance_update",
        ]
        .iter()
        .any(|v| h.equal(&event, &h.text(v)))
        {
            if !(h.equal(&opt(h, &json, "status")?, &h.text("ok"))
                || {
                    let p = method(h, r, "isSuccessfulCode")?;
                    h.truthy(&invoke(h, p, vec![opt(h, &json, "code")?])?)
                }
                || h.equal(&opt(h, &json, "msg")?, &h.text("succ")))
            {
                h.console_args(vec![
                    h.concat(vec![h.text("Failed to subscribe to "), event])?,
                    json,
                ])?;
            }
        } else {
            h.console_args(vec![h.text("Unknown event"), json])?;
        }
    }
    Ok(h.undefined())
}
pub fn order_update<H: PrivateHost>(
    h: &H,
    r: H::Value,
    json: H::Value,
) -> Result<H::Value, H::Error> {
    let nested = opt(h, &json, "o")?;
    let update = if h.truthy(&nested) {
        nested
    } else {
        json.clone()
    };
    if !h.truthy(&update) {
        return Ok(h.undefined());
    }
    let raw = h.uppercase(h.string(fallback(h, get(h, &update, "s")?, h.text("")))?)?;
    if !h.truthy(&raw) {
        return Ok(h.undefined());
    }
    let symbol = if h.contains(raw.clone(), "_")? {
        h.reverse_symbol(raw)?
    } else {
        raw
    };
    let markets = get(h, &get(h, &get(h, &r, "parent")?, "store")?, "markets")?;
    let market = h.find(markets, |m| Ok(h.equal(&get(h, &m, "symbol")?, &symbol)))?;
    let multiplier = h.numeric(fallback(
        h,
        opt(h, &opt(h, &market, "info")?, "multiplier")?,
        h.number(1.0),
    ))?;
    let p = method(h, r.clone(), "parseOrderSide")?;
    let side = invoke(h, p, vec![get(h, &update, "S")?])?;
    let p = method(h, r.clone(), "parseOrderType")?;
    let kind = invoke(h, p, vec![get(h, &update, "o")?])?;
    let p = method(h, r.clone(), "parseOrderStatus")?;
    let status = invoke(h, p, vec![get(h, &update, "x")?, get(h, &update, "X")?])?;
    let number = |names: &[&str]| -> Result<H::Value, H::Error> {
        let mut v = h.undefined();
        for key in names {
            if h.nullish(&v) {
                v = get(h, &update, key)?;
            }
        }
        h.numeric(if h.nullish(&v) { h.number(0.0) } else { v })
    };
    let qty = number(&["q", "origQty"])?;
    let volume = number(&["volume"])?;
    let filled_raw = number(&["z", "executedQty"])?;
    let remaining_raw = number(&["l"])?;
    let from_volume = if h.less(h.number(0.0), volume.clone())? {
        h.arithmetic(Op::Mul, volume, multiplier.clone())?
    } else {
        h.number(0.0)
    };
    let amount = if h.less(h.number(0.0), qty.clone())? {
        qty
    } else {
        from_volume
    };
    let scaled = h.arithmetic(Op::Mul, filled_raw.clone(), multiplier.clone())?;
    let filled = if h.less(h.number(0.0), amount.clone())?
        && h.less_equal(filled_raw.clone(), amount.clone())?
    {
        filled_raw
    } else {
        scaled
    };
    let remaining = if h.less(h.number(0.0), remaining_raw.clone())? {
        h.arithmetic(Op::Mul, remaining_raw, multiplier.clone())?
    } else {
        h.arithmetic(
            Op::Max,
            h.arithmetic(Op::Sub, amount.clone(), filled.clone())?,
            h.number(0.0),
        )?
    };
    let price = number(&["p", "ap", "L"])?;
    let mut id = h.undefined();
    for k in ["i", "orderId", "c", "clientOrderId"] {
        if h.nullish(&id) {
            id = get(h, &update, k)?;
        }
    }
    if h.nullish(&id) {
        h.console_args(vec![h.text("Order update without id"), update])?;
        return Ok(h.undefined());
    }
    let exec = h.uppercase(h.string(fallback(h, get(h, &update, "x")?, h.text("")))?)?;
    if h.equal(&exec, &h.text("TRADE")) {
        let raw = number(&["z"])?;
        let p = method(h, r.clone(), "incrementalFills")?;
        let delta = invoke(h, p, vec![h.string(id.clone())?, raw.clone()])?;
        let executed = get(h, &delta, "qty")?;
        let fill = if h.less(h.number(0.0), amount.clone())? && h.less_equal(raw, amount.clone())? {
            executed
        } else {
            h.clean_float(h.arithmetic(Op::Mul, executed, multiplier)?)?
        };
        if h.less(h.number(0.0), fill.clone())? {
            let p = method(h, get(h, &r, "emitter")?, "emit")?;
            let payload = h.object(vec![
                ("id", h.string(id.clone())?),
                ("side", side.clone()),
                ("symbol", symbol.clone()),
                ("price", price.clone()),
                ("amount", fill),
            ]);
            invoke(h, p, vec![h.text("fill"), payload])?;
        }
    }
    let id = h.string(id)?;
    let reduce = get(h, &update, "R")?;
    let reduce = if h.nullish(&reduce) {
        h.boolean(h.equal(&get(h, &update, "open")?, &h.text("CLOSE")))
    } else {
        reduce
    };
    let timestamp = get(h, &json, "E")?;
    let timestamp = if h.nullish(&timestamp) {
        get(h, &update, "E")?
    } else {
        timestamp
    };
    let timestamp = if h.nullish(&timestamp) {
        h.now()?
    } else {
        timestamp
    };
    let order = h.object(vec![
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", kind),
        ("side", side),
        ("price", price),
        ("amount", amount),
        ("filled", filled),
        ("remaining", remaining),
        ("reduceOnly", h.boolean(h.truthy(&reduce))),
        ("lastUpdate", h.numeric(timestamp)?),
        ("info", update),
    ]);
    call(h, get(h, &r, "store")?, "addOrUpdateOrder", vec![order])?;
    Ok(h.undefined())
}
pub async fn fetch_key<H: PrivateHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let mut last = h.undefined();
    let mut attempt = 0;
    loop {
        if !h.less_equal(h.number(attempt as f64), get(h, &h.delays()?, "length")?)? {
            break;
        }
        let result = async {
            let p = call(
                h,
                get(h, &get(h, &r, "parent")?, "xhr")?,
                "post",
                vec![h.text(endpoint())],
            )?;
            let response = h.await_value(p).await?;
            let data = get(h, &response, "data")?;
            let key = opt(h, &opt(h, &data, "data")?, "listenKey")?;
            let key = if h.truthy(&key) {
                key
            } else {
                opt(h, &data, "listenKey")?
            };
            let ok = accepted(h, r.clone(), &data)?;
            if h.truthy(&key) && (ok || h.equal(&opt(h, &data, "code")?, &h.undefined())) {
                call(h, r.clone(), "scheduleListenKeyRefresh", vec![key.clone()])?;
                return Ok(Some(key));
            }
            let msg = h.concat(vec![
                h.text("Bitrue futures listen-key response was invalid (code: "),
                h.string(opt(h, &data, "code")?)?,
                h.text(")"),
            ])?;
            Ok::<_, H::Error>(None::<H::Value>).and_then(|_| {
                let e = h.error(msg)?;
                last = h.error_value(&e);
                Ok(None)
            })
        }
        .await;
        match result {
            Ok(Some(key)) => return Ok(key),
            Ok(None) => {}
            Err(e) => last = h.error_value(&e),
        }
        let delay = h.index(h.delays()?, attempt)?;
        if !h.equal(&delay, &h.undefined()) {
            let parent = get(h, &r, "parent")?;
            let p = method(h, parent, "log")?;
            let msg = h.concat(vec![
                h.text("Bitrue futures listen-key attempt "),
                h.number((attempt + 1) as f64),
                h.text(" failed; retrying in "),
                delay.clone(),
                h.text("ms."),
            ])?;
            invoke(h, p, vec![msg, h.text("warning")])?;
            let p = h.delay_promise(delay)?;
            h.await_value(p).await?;
        }
        attempt += 1;
    }
    let length = get(h, &h.delays()?, "length")?;
    let count = h.arithmetic(Op::Add, length, h.number(1.0))?;
    let reason = if h.error_instance(&last) {
        get(h, &last, "message")?
    } else {
        h.string(fallback(h, last, h.text("unknown error")))?
    };
    Err(h.error(h.concat(vec![
        h.text("Failed to acquire Bitrue futures listen key after "),
        count,
        h.text(" attempts: "),
        reason,
    ])?)?)
}
pub async fn update_key<H: PrivateHost>(
    h: &H,
    r: H::Value,
    key: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&get(h, &r, "isDisposed")?) {
        let result = async {
            let xhr = get(h, &get(h, &r, "parent")?, "xhr")?;
            let p = method(h, xhr, "put")?;
            let url = h.concat(vec![
                h.text(endpoint()),
                h.text("/"),
                h.string(key.clone())?,
            ])?;
            let p = invoke(h, p, vec![url])?;
            let response = h.await_value(p).await?;
            let data = get(h, &response, "data")?;
            if accepted(h, r.clone(), &data)? {
                call(h, r.clone(), "scheduleListenKeyRefresh", vec![key])?;
                return Ok(true);
            }
            Ok::<_, H::Error>(false)
        }
        .await;
        if matches!(result, Ok(true)) {
            return Ok(h.undefined());
        }
        call(h,r,"emitAndLogError",vec![h.text("Failed to refresh Bitrue futures listen key; private updates may stop when it expires.")])?;
    }
    Ok(h.undefined())
}
pub fn intercept<H: PrivateHost>(
    h: &H,
    config: H::Value,
    options: H::Value,
    recv: H::Value,
) -> Result<H::Value, H::Error> {
    let next = h.copy(config)?;
    if h.truthy(&get(h, &options, "remoteSigner")?)
        || !h.truthy(&get(h, &options, "key")?)
        || !h.truthy(&get(h, &options, "secret")?)
    {
        return Ok(next);
    }
    let url = opt(h, &next, "url")?;
    if !h.nullish(&url) && h.starts_with(url, "http")? {
        let parts = h.split(get(h, &next, "url")?, ".com")?;
        h.set(
            &next,
            "baseURL",
            h.concat(vec![h.index(parts.clone(), 0)?, h.text(".com")])?,
        )?;
        h.set(&next, "url", h.index(parts, 1)?)?;
    }
    let verb = fallback(
        h,
        h.method_uppercase_optional(get(h, &next, "method")?)?,
        h.text("GET"),
    );
    let base = get(h, &next, "url")?;
    let data = get(h, &next, "data")?;
    let body = if h.equal(&data, &h.undefined()) || h.null_is(&get(h, &next, "data")?) {
        h.text("")
    } else if h.is_string(&get(h, &next, "data")?) {
        get(h, &next, "data")?
    } else {
        h.stringify(get(h, &next, "data")?)?
    };
    let suffix = if h.truthy(&get(h, &next, "params")?) {
        h.concat(vec![h.text("?"), h.query(get(h, &next, "params")?)?])?
    } else {
        h.text("")
    };
    let path = h.add(base, suffix)?;
    let timestamp = h.virtual_now()?;
    let hmac = h.create_hmac("sha256", get(h, &options, "secret")?)?;
    let p = method(h, hmac, "update")?;
    let msg = h.join(vec![timestamp.clone(), verb, path, body])?;
    let hmac = invoke(h, p, vec![msg])?;
    let signature = call(h, hmac, "digest", vec![h.text("hex")])?;
    let headers = h.copy(get(h, &next, "headers")?)?;
    for (k, v) in [
        ("Content-Type", h.text("application/json")),
        ("X-CH-APIKEY", get(h, &options, "key")?),
        ("X-CH-SIGN", signature),
        ("X-CH-TS", timestamp),
        ("recvWindow", recv),
    ] {
        h.set(&headers, k, v)?;
    }
    let headers = h.axios_headers(headers)?;
    let out = h.copy(next)?;
    h.set(&out, "headers", headers)?;
    Ok(out)
}
pub fn create_api<H: PrivateHost>(
    h: &H,
    options: H::Value,
    r: H::Value,
) -> Result<H::Value, H::Error> {
    let recv = opt(h, &opt(h, &options, "extra")?, "recvWindow")?;
    let recv = if h.nullish(&recv) {
        h.number(crate::types::RECV_WINDOW)
    } else {
        recv
    };
    let host = h.clone();
    let serializer = h.named_function(
        "serialize",
        Rc::new(move |args| host.query(args.first().cloned().unwrap_or_else(|| host.undefined()))),
    );
    let config = h.object(vec![
        ("baseURL", h.text(crate::types::BASE_URL)),
        ("timeout", recv.clone()),
        (
            "paramsSerializer",
            h.object(vec![("serialize", serializer)]),
        ),
        (
            "headers",
            h.object(vec![
                ("X-Referer", h.text(crate::types::BROKER_ID)),
                ("Content-Type", h.text("application/json")),
            ]),
        ),
    ]);
    let xhr = h.axios_create(config, options.clone(), r.clone())?;
    if h.truthy(&get(h, &options, "tealstreetTunnel")?) {
        h.transport(xhr.clone(), options.clone(), get(h, &r, "name")?, "bigInt")?;
    }
    let condition = h.retry_condition()?;
    h.retry(
        xhr.clone(),
        h.object(vec![
            ("retries", h.number(3.0)),
            ("retryCondition", condition),
        ]),
    )?;
    let request = get(h, &get(h, &xhr, "interceptors")?, "request")?;
    let p = method(h, request, "use")?;
    let host = h.clone();
    let interceptor = h.named_function(
        "",
        Rc::new(move |args| {
            intercept(
                &host,
                args.first().cloned().unwrap_or_else(|| host.undefined()),
                options.clone(),
                recv.clone(),
            )
        }),
    );
    invoke(h, p, vec![interceptor])?;
    // The HOW tunnel resolves a 4xx, so a rejection over it never reaches axios-retry.
    let response = get(h, &get(h, &xhr, "interceptors")?, "response")?;
    let p = method(h, response, "use")?;
    let host = h.clone();
    let client = xhr.clone();
    let resend = h.named_function(
        "",
        Rc::new(move |args| {
            let response = args.first().cloned().unwrap_or_else(|| host.undefined());
            let predicate = host.signature_rejection()?;
            let resent =
                host.resend_on_signature_rejection(client.clone(), response.clone(), predicate)?;
            Ok(if host.nullish(&resent) {
                response
            } else {
                resent
            })
        }),
    );
    invoke(h, p, vec![resend])?;
    Ok(xhr)
}

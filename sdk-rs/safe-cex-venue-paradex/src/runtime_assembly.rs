//! Remaining allocation, interceptor composition and asynchronous feed boundaries.
use std::{future::Future, pin::Pin};
pub type RuntimeFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait RuntimeAssemblyHost {
    type Value: Clone;
    type Error: Clone;
    fn get(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn optional(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn undefined(&self) -> Self::Value;
    fn literal(&self, value: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn template(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn numeric(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn intrinsic(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn global(&self) -> Self::Value;
    fn select(&self, receiver: Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn apply(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn invoke(
        &self,
        selected: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<RuntimeFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn wait(
        &self,
        promise: Self::Value,
    ) -> Result<RuntimeFuture<'_, Self::Value, Self::Error>, Self::Error>;
    fn write(&self, owner: &Self::Value, key: &str, value: Self::Value) -> Result<(), Self::Error>;
    // Allocation supplies the original nominal prototype and derived base construction.
    fn allocate(
        &self,
        class: &'static str,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn define(
        &self,
        owner: &Self::Value,
        key: &'static str,
        value: Self::Value,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Result<(), Self::Error>;
    // Member codes dispatch to owned method bodies with the original lexical receiver.
    fn callback(
        &self,
        owner: Self::Value,
        code: &'static str,
        captures: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    // Detached dispatch observes the native callback Result, including retained failures.
    fn start(&self, callback: Self::Value) -> Result<(), Self::Error>;
    fn observe(&self, promise: Self::Value, rejection: Self::Value) -> Result<(), Self::Error>;
    fn make_error(&self, message: Vec<u16>) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn error_from_value(&self, value: Self::Value) -> Self::Error;
}
macro_rules! sync {
    ($h:expr,$receiver:expr,$name:expr,$args:expr) => {{
        let selected = $h.select($receiver, $name)?;
        $h.apply(selected, $args)?
    }};
}
macro_rules! invoke {
    ($h:expr,$receiver:expr,$name:expr,$args:expr) => {{
        let selected = $h.select($receiver, $name)?;
        $h.invoke(selected, $args)?.await?
    }};
}
fn call<H: RuntimeAssemblyHost>(
    h: &H,
    name: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.intrinsic(name, args)
}
fn cat<H: RuntimeAssemblyHost>(h: &H, values: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let mut units = vec![];
    for value in values {
        units.extend(h.template(&value)?);
    }
    call(
        h,
        "text",
        vec![h.array(units.into_iter().map(|v| h.number(v as f64)).collect())],
    )
}
fn field<H: RuntimeAssemblyHost>(
    h: &H,
    owner: &H::Value,
    name: &'static str,
    value: H::Value,
) -> Result<(), H::Error> {
    h.define(owner, name, value, true, true, true)
}
fn arrows<H: RuntimeAssemblyHost>(
    h: &H,
    owner: &H::Value,
    names: &[&'static str],
) -> Result<(), H::Error> {
    for name in names {
        field(h, owner, name, h.callback(owner.clone(), name, vec![])?)?;
    }
    Ok(())
}
fn maps<H: RuntimeAssemblyHost>(
    h: &H,
    owner: &H::Value,
    names: &[&'static str],
) -> Result<(), H::Error> {
    for name in names {
        field(h, owner, name, call(h, "new Map", vec![])?)?;
    }
    Ok(())
}
fn undefined_fields<H: RuntimeAssemblyHost>(
    h: &H,
    owner: &H::Value,
    names: &[&'static str],
) -> Result<(), H::Error> {
    for name in names {
        field(h, owner, name, h.undefined())?;
    }
    Ok(())
}
pub fn allocate_request_signer<H: RuntimeAssemblyHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.allocate("ParadexRequestSigner", args)?;
    undefined_fields(h, &owner, &["sdkAccount", "options"])?;
    Ok(owner)
}
pub fn allocate_jwt_manager<H: RuntimeAssemblyHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.allocate("ParadexJWTManager", args)?;
    field(h, &owner, "token", call(h, "null", vec![])?)?;
    field(h, &owner, "tokenExpiry", h.number(0.0))?;
    field(h, &owner, "refreshPromise", call(h, "null", vec![])?)?;
    undefined_fields(
        h,
        &owner,
        &[
            "sdkAccount",
            "apiBaseUrl",
            "subkeyAddress",
            "mainAccountAddress",
            "options",
        ],
    )?;
    field(h, &owner, "REFRESH_BUFFER_MS", h.number(120_000.0))?;
    field(h, &owner, "TOKEN_VALIDITY_MS", h.number(300_000.0))?;
    Ok(owner)
}
pub fn allocate_exchange<H: RuntimeAssemblyHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.allocate("ParadexExchange", args)?;
    let rule = sync!(
        h,
        h.global(),
        "patternClientOrderIdRule",
        vec![
            h.literal("Paradex"),
            call(h, "RegExp", vec![h.literal("^.{1,64}$"), h.literal("")])?,
            h.object(vec![
                ("mint", h.callback(owner.clone(), "rule-mint", vec![])?),
                (
                    "isHonorableShape",
                    h.callback(owner.clone(), "rule-shape", vec![])?
                )
            ])
        ]
    );
    field(h, &owner, "clientOrderIdRule", rule)?;
    arrows(h, &owner, &["fetchHistoricalPositions"])?;
    undefined_fields(h, &owner, &["xhr", "publicWebsocket", "privateWebsocket"])?;
    for name in [
        "sdkAccount",
        "sdkInitPromise",
        "requestSigner",
        "jwtManager",
    ] {
        field(h, &owner, name, call(h, "null", vec![])?)?;
    }
    maps(h, &owner, &["leverageSettings"])?;
    arrows(
        h,
        &owner,
        &[
            "dispose",
            "getAccount",
            "validateAccount",
            "start",
            "tick",
            "fetchBalance",
            "fetchMarkets",
            "fetchTickers",
            "fetchOHLCV",
            "fetchOrders",
            "fetchIncomeHistory",
            "fetchIncomeHistoryRange",
            "fetchHistoricalOrders",
            "fetchHistoricalFills",
            "fetchPositions",
            "fetchOrderBook",
            "listenOrderBook",
            "listenOHLCV",
            "placeOrder",
            "isNativeBatchOrder",
            "generateClientId",
            "resolveMarketIocExecutionPrice",
            "buildMainOrderPayload",
            "placeMainOrder",
            "placeNativeBatchChunk",
            "placeTakeProfitStopLoss",
            "validateTpSlPrices",
            "placeOrders",
            "updateOrder",
            "cancelOrder",
            "cancelNativeBatchChunk",
            "cancelOrders",
            "cancelAllOrders",
            "cancelSymbolOrders",
            "fetchLeverageSettings",
            "setLeverage",
            "setMaxSlippage",
            "mapOrder",
            "mapTimeInForce",
            "mapOrderType",
            "reverseMapOrderType",
            "mapOrderStatus",
        ],
    )?;
    Ok(owner)
}
pub fn mint_rule_client_id<H: RuntimeAssemblyHost>(h: &H) -> Result<H::Value, H::Error> {
    cat(
        h,
        vec![h.literal("ts_"), sync!(h, h.global(), "uuid", vec![])],
    )
}
pub fn honorable_rule_shape<H: RuntimeAssemblyHost>(
    h: &H,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let regular = sync!(
        h,
        h.global(),
        "isRegularClientOrderShape",
        vec![opts.clone()]
    );
    Ok(h.boolean(h.truthy(&regular) && !h.equal(&h.get(&opts, "close")?, &h.boolean(true))))
}
pub fn allocate_private_socket<H: RuntimeAssemblyHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.allocate("ParadexPrivateWebSocket", args)?;
    field(h, &owner, "connectUrl", h.literal(""))?;
    field(h, &owner, "authenticated", h.boolean(false))?;
    field(h, &owner, "messageId", h.number(0.0))?;
    undefined_fields(
        h,
        &owner,
        &["authRequestId", "pingInterval", "ordersThrottleTimeout"],
    )?;
    field(h, &owner, "lastPositionFetchTime", h.number(0.0))?;
    field(h, &owner, "positionFetchThrottleMs", h.number(500.0))?;
    field(h, &owner, "positionFetchScheduled", h.boolean(false))?;
    field(h, &owner, "hasPendingPositionEvents", h.boolean(false))?;
    field(h, &owner, "messageHandlers", h.object(vec![]))?;
    arrows(
        h,
        &owner,
        &[
            "connectAndSubscribe",
            "onOpen",
            "onMessage",
            "handleOrderUpdate",
            "formatCancelReason",
            "handlePositionUpdate",
            "handleBalanceUpdate",
            "throttledFetchPositions",
            "executePositionFetch",
            "authenticate",
            "subscribeToPrivateChannels",
            "mapOrderStatus",
            "ping",
            "dispose",
            "throttledFetchOrders",
            "fetchThrottledOrders",
        ],
    )?;
    Ok(owner)
}
pub fn allocate_public_socket<H: RuntimeAssemblyHost>(
    h: &H,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let owner = h.allocate("ParadexPublicWebSocket", args)?;
    field(h, &owner, "connectUrl", h.literal(""))?;
    undefined_fields(h, &owner, &["pingInterval"])?;
    maps(
        h,
        &owner,
        &["orderbookCallbacks", "tradeSubscriptionCounts"],
    )?;
    for name in ["desiredTradeSymbols", "sentTradeSymbols"] {
        field(h, &owner, name, call(h, "new Set", vec![])?)?;
    }
    maps(h, &owner, &["pendingSubscribeRequests"])?;
    field(
        h,
        &owner,
        "subscriptionRetryTimeouts",
        call(h, "new Set", vec![])?,
    )?;
    field(h, &owner, "messageId", h.number(0.0))?;
    maps(
        h,
        &owner,
        &[
            "ohlcvPollingTimeouts",
            "ohlcvPollingActive",
            "ohlcvSubscriptionVersions",
            "lastCandleCache",
            "ohlcvCallbacks",
            "ohlcvIntervals",
            "ohlcvLastTradeAt",
            "ohlcvLastRecoveryAt",
        ],
    )?;
    let mut handlers = vec![];
    for (channel, target) in [
        ("ping", "handlePongEvent"),
        ("pong", "handlePongEvent"),
        ("bbo", "handleBBOUpdate"),
        ("markets_summary", "handleMarketSummaryUpdate"),
        ("order_book", "handleOrderbookUpdate"),
        ("trades", "handleTradesUpdate"),
    ] {
        handlers.push((
            channel,
            h.array(vec![h.callback(
                owner.clone(),
                "dispatch",
                vec![
                    h.literal(target),
                    h.boolean(channel != "ping" && channel != "pong"),
                ],
            )?]),
        ));
    }
    field(h, &owner, "messageHandlers", h.object(handlers))?;
    arrows(
        h,
        &owner,
        &[
            "connectAndSubscribe",
            "onOpen",
            "onMessage",
            "ping",
            "dispose",
            "sendJsonRpcSubscription",
            "retrySubscribeChannel",
            "hasActiveSubscriptionInterest",
            "sendTradeSubscribe",
        ],
    )?;
    for name in [
        "handleBBOUpdate",
        "handleMarketSummaryUpdate",
        "handleOrderbookUpdate",
    ] {
        let wrapped = sync!(
            h,
            h.global(),
            "throttle",
            vec![
                h.callback(owner.clone(), name, vec![])?,
                h.number(100.0),
                h.object(vec![
                    ("leading", h.boolean(true)),
                    ("trailing", h.boolean(true))
                ])
            ]
        );
        field(h, &owner, name, wrapped)?;
    }
    arrows(
        h,
        &owner,
        &[
            "handleTradesUpdate",
            "maybeRecoverOhlcvTradeStream",
            "subscribeToChannels",
            "subscribeToOrderBook",
            "unsubscribeFromOrderBook",
            "subscribeToTrades",
            "unsubscribeFromTrades",
            "listenOrderBook",
            "listenOHLCV",
            "cleanupOhlcvSubscription",
        ],
    )?;
    Ok(owner)
}
pub fn create_api<H: RuntimeAssemblyHost>(
    h: &H,
    opts: H::Value,
    exchange: H::Value,
) -> Result<H::Value, H::Error> {
    let selected = h.select(h.global(), "safeCexAxiosCreate")?;
    let base_url = h.literal(if h.truthy(&h.get(&opts, "testnet")?) {
        "https://api.testnet.paradex.trade/v1"
    } else {
        "https://api.prod.paradex.trade/v1"
    });
    let extra = h.optional(&opts, "extra")?;
    let timeout = h.optional(&extra, "recvWindow")?;
    let timeout =
        if h.equal(&timeout, &h.undefined()) || h.equal(&timeout, &call(h, "null", vec![])?) {
            h.number(20_000.0)
        } else {
            timeout
        };
    let xhr = h.apply(
        selected,
        vec![
            h.object(vec![
                ("baseURL", base_url),
                ("timeout", timeout),
                (
                    "paramsSerializer",
                    h.object(vec![(
                        "serialize",
                        h.callback(exchange.clone(), "api-serialize", vec![])?,
                    )]),
                ),
                (
                    "headers",
                    h.object(vec![
                        ("cache-control", h.literal("no-cache")),
                        ("Content-Type", h.literal("application/json")),
                        ("Accept", h.literal("application/json")),
                    ]),
                ),
            ]),
            opts.clone(),
            exchange.clone(),
        ],
    )?;
    sync!(
        h,
        h.global(),
        "retry",
        vec![
            xhr.clone(),
            h.object(vec![
                ("retries", h.number(3.0)),
                (
                    "retryCondition",
                    sync!(h, h.global(), "networkErrorOnRead", vec![])
                ),
                (
                    "retryDelay",
                    h.callback(exchange.clone(), "retry-delay", vec![])?
                )
            ])
        ]
    );
    if !h.truthy(&h.get(&opts, "publicDataOnly")?) && !h.truthy(&h.get(&opts, "payloadSigner")?) {
        sync!(
            h,
            h.global(),
            "installTransportAdapter",
            vec![xhr.clone(), opts, h.get(&exchange, "name")?]
        );
    }
    let interceptors = h.get(&xhr, "interceptors")?;
    sync!(
        h,
        h.get(&interceptors, "request")?,
        "use",
        vec![h.callback(exchange.clone(), "api-request", vec![])?]
    );
    sync!(
        h,
        h.get(&h.get(&xhr, "interceptors")?, "response")?,
        "use",
        vec![
            h.callback(exchange.clone(), "api-success", vec![])?,
            h.callback(exchange, "api-error", vec![])?
        ]
    );
    Ok(xhr)
}
pub fn serialize_api_params<H: RuntimeAssemblyHost>(
    h: &H,
    params: H::Value,
) -> Result<H::Value, H::Error> {
    Ok(sync!(
        h,
        h.get(&h.global(), "qs")?,
        "stringify",
        vec![params, h.object(vec![("arrayFormat", h.literal("comma"))])]
    ))
}
pub fn api_retry_delay<H: RuntimeAssemblyHost>(
    h: &H,
    count: H::Value,
) -> Result<H::Value, H::Error> {
    call(h, "multiply", vec![count, h.number(1000.0)])
}
fn includes<H: RuntimeAssemblyHost>(
    h: &H,
    value: H::Value,
    needle: &str,
) -> Result<bool, H::Error> {
    Ok(h.truthy(&call(h, "includes", vec![value, h.literal(needle)])?))
}
pub async fn api_request<H: RuntimeAssemblyHost>(
    h: &H,
    exchange: H::Value,
    config: H::Value,
) -> Result<H::Value, H::Error> {
    let url = h.get(&config, "url")?;
    let url = if h.truthy(&url) { url } else { h.literal("") };
    let mut public = false;
    for endpoint in [
        "/trades",
        "/funding",
        "/summary",
        "/orderbook",
        "/tradingview",
    ] {
        if includes(h, url.clone(), endpoint)? {
            public = true;
            break;
        }
    }
    let markets = includes(h, url.clone(), "/markets")? && !includes(h, url.clone(), "/account")?;
    if (public || markets) && !includes(h, url.clone(), "/account")? {
        sync!(
            h,
            exchange,
            "log",
            vec![
                cat(h, vec![h.literal("Public endpoint detected: "), url])?,
                h.literal("debug")
            ]
        );
        return Ok(config);
    }
    if h.equal(&h.get(&config, "method")?, &h.literal("post"))
        && includes(h, url.clone(), "/orders")?
    {
        sync!(
            h,
            h.get(&config, "headers")?,
            "set",
            vec![h.literal("PARADEX-PARTNER"), h.literal("TEALSTREET")]
        );
        sync!(
            h,
            exchange.clone(),
            "log",
            vec![
                h.literal("Added PARADEX-PARTNER header for order request"),
                h.literal("debug")
            ]
        );
    }
    let signed = sync!(
        h,
        h.get(&config, "headers")?,
        "has",
        vec![h.literal("PARADEX-STARKNET-SIGNATURE")]
    );
    if h.truthy(&signed) {
        return Ok(config);
    }
    if h.truthy(&h.get(&exchange, "sdkInitPromise")?) {
        h.wait(h.get(&exchange, "sdkInitPromise")?)?.await?;
    }
    Ok(invoke!(
        h,
        h.global(),
        "authenticateRequest",
        vec![config, exchange]
    ))
}
fn response_message<H: RuntimeAssemblyHost>(h: &H, data: &H::Value) -> Result<H::Value, H::Error> {
    let error = h.get(data, "error")?;
    let both = h.truthy(&error) && h.truthy(&h.get(data, "message")?);
    cat(
        h,
        if both {
            vec![
                h.literal("Paradex Error: "),
                h.get(data, "error")?,
                h.literal(": "),
                h.get(data, "message")?,
            ]
        } else {
            vec![h.literal("Paradex Error: "), h.get(data, "error")?]
        },
    )
}
pub fn api_success<H: RuntimeAssemblyHost>(
    h: &H,
    response: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&h.optional(&h.get(&response, "data")?, "error")?) {
        let message = response_message(h, &h.get(&response, "data")?)?;
        let error = h.make_error(h.template(&message)?)?;
        let value = h.error_value(&error);
        h.write(&value, "isAxiosError", h.boolean(true))?;
        h.write(&value, "response", response.clone())?;
        h.write(&value, "config", h.get(&response, "config")?)?;
        h.write(&value, "request", h.get(&response, "request")?)?;
        Err(error)
    } else {
        Ok(response)
    }
}
pub fn api_error<H: RuntimeAssemblyHost>(h: &H, error: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&h.optional(&h.optional(&h.get(&error, "response")?, "data")?, "error")?) {
        let data = h.get(&h.get(&error, "response")?, "data")?;
        let message = response_message(h, &data)?;
        let enhanced = call(h, "Object.create", vec![error])?;
        h.define(&enhanced, "message", message, false, true, true)?;
        Err(h.error_from_value(enhanced))
    } else {
        Err(h.error_from_value(error))
    }
}
pub async fn execute_position_fetch<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
) -> Result<H::Value, H::Error> {
    let now = sync!(h, h.get(&h.global(), "Date")?, "now", vec![]);
    h.write(&owner, "lastPositionFetchTime", now)?;
    h.write(&owner, "hasPendingPositionEvents", h.boolean(false))?;
    let positions = invoke!(h, h.get(&owner, "parent")?, "fetchPositions", vec![]);
    if h.truthy(&h.get(&owner, "isDisposed")?)
        || h.truthy(&h.get(&h.get(&owner, "parent")?, "isDisposed")?)
    {
        return Ok(h.undefined());
    }
    sync!(
        h,
        h.get(&h.get(&owner, "parent")?, "store")?,
        "addOrUpdatePositionsFull",
        vec![positions]
    );
    Ok(h.undefined())
}
pub async fn fetch_throttled_orders<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
) -> Result<H::Value, H::Error> {
    h.write(&owner, "ordersThrottleTimeout", h.undefined())?;
    if !h.truthy(&h.get(&owner, "isDisposed")?) {
        let orders = invoke!(h, h.get(&owner, "parent")?, "fetchOrders", vec![]);
        let store = h.get(&h.get(&owner, "parent")?, "store")?;
        let loaded = call(
            h,
            "spread",
            vec![h.get(&h.get(&h.get(&owner, "parent")?, "store")?, "loaded")?],
        )?;
        h.write(&loaded, "orders", h.boolean(true))?;
        sync!(
            h,
            store,
            "update",
            vec![h.object(vec![("orders", orders), ("loaded", loaded)])]
        );
    }
    Ok(h.undefined())
}
pub fn listen_ohlcv<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let key = cat(
        h,
        vec![
            h.get(&opts, "symbol")?,
            h.literal(":"),
            h.get(&opts, "interval")?,
        ],
    )?;
    let interval = call(h, "timeframe", vec![h.get(&opts, "interval")?])?;
    let interval = if h.truthy(&interval) {
        interval
    } else {
        h.number(60.0)
    };
    let ms = call(h, "multiply", vec![interval, h.number(1000.0)])?;
    let callbacks = sync!(
        h,
        h.get(&owner, "ohlcvCallbacks")?,
        "get",
        vec![key.clone()]
    );
    if h.truthy(&callbacks) {
        sync!(h, callbacks, "add", vec![callback.clone()]);
        sync!(
            h,
            owner.clone(),
            "subscribeToTrades",
            vec![h.get(&opts, "symbol")?]
        );
        return h.callback(owner, "ohlcv-cleanup", vec![key, opts, callback]);
    }
    let set = call(h, "Set.from", vec![h.array(vec![callback.clone()])])?;
    sync!(
        h,
        h.get(&owner, "ohlcvCallbacks")?,
        "set",
        vec![key.clone(), set]
    );
    sync!(
        h,
        h.get(&owner, "ohlcvIntervals")?,
        "set",
        vec![key.clone(), ms.clone()]
    );
    sync!(
        h,
        owner.clone(),
        "subscribeToTrades",
        vec![h.get(&opts, "symbol")?]
    );
    let old = sync!(
        h,
        h.get(&owner, "ohlcvSubscriptionVersions")?,
        "get",
        vec![key.clone()]
    );
    let old = if h.truthy(&old) { old } else { h.number(0.0) };
    let version = call(h, "add", vec![old, h.number(1.0)])?;
    sync!(
        h,
        h.get(&owner, "ohlcvSubscriptionVersions")?,
        "set",
        vec![key.clone(), version.clone()]
    );
    sync!(
        h,
        h.get(&owner, "ohlcvPollingActive")?,
        "set",
        vec![key.clone(), h.boolean(true)]
    );
    let parent = h.get(&owner, "parent")?;
    if !h.truthy(&h.get(&parent, "isPublicDataOnly")?)
        && h.truthy(&h.get(&h.get(&owner, "parent")?, "jwtManager")?)
    {
        let promise = sync!(
            h,
            h.get(&owner, "parent")?,
            "setMaxSlippage",
            vec![h.literal("0.2"), h.get(&opts, "symbol")?]
        );
        h.observe(
            promise,
            h.callback(owner.clone(), "ohlcv-slippage-error", vec![opts.clone()])?,
        )?;
    }
    let poll = h.callback(
        owner.clone(),
        "ohlcv-poll",
        vec![key.clone(), opts.clone(), ms, version],
    )?;
    h.start(poll)?;
    h.callback(owner, "ohlcv-cleanup", vec![key, opts, callback])
}
pub fn cleanup_ohlcv_listener<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
    key: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let latest = sync!(
        h,
        h.get(&owner, "ohlcvCallbacks")?,
        "get",
        vec![key.clone()]
    );
    if !h.equal(&latest, &h.undefined()) && !h.equal(&latest, &call(h, "null", vec![])?) {
        sync!(h, latest.clone(), "delete", vec![callback]);
    }
    sync!(
        h,
        owner.clone(),
        "unsubscribeFromTrades",
        vec![h.get(&opts, "symbol")?]
    );
    if h.truthy(&latest) && h.numeric(&h.get(&latest, "size")?)? > 0.0 {
        return Ok(h.undefined());
    }
    sync!(h, owner, "cleanupOhlcvSubscription", vec![key]);
    Ok(h.undefined())
}
pub fn ohlcv_slippage_error<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
    opts: H::Value,
    error: H::Error,
) -> Result<H::Value, H::Error> {
    sync!(
        h,
        h.get(&owner, "parent")?,
        "log",
        vec![
            cat(
                h,
                vec![
                    h.literal("Failed to set max slippage for "),
                    h.get(&opts, "symbol")?,
                    h.literal(": "),
                    h.error_value(&error)
                ]
            )?,
            h.literal("warning")
        ]
    );
    Ok(h.undefined())
}
pub async fn poll_ohlcv<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
    key: H::Value,
    opts: H::Value,
    ms: H::Value,
    version: H::Value,
    poll: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&sync!(
        h,
        h.get(&owner, "ohlcvPollingActive")?,
        "get",
        vec![key.clone()]
    )) {
        return Ok(h.undefined());
    }
    let result = async {
        let now = sync!(h, h.get(&h.global(), "Date")?, "now", vec![]);
        let from = call(
            h,
            "subtract",
            vec![now.clone(), call(h, "multiply", vec![ms, h.number(3.0)])?],
        )?;
        let candles = invoke!(
            h,
            h.get(&owner, "parent")?,
            "fetchOHLCV",
            vec![h.object(vec![
                ("symbol", h.get(&opts, "symbol")?),
                ("interval", h.get(&opts, "interval")?),
                ("from", from),
                ("to", now)
            ])]
        );
        if !h.truthy(&sync!(
            h,
            h.get(&owner, "ohlcvPollingActive")?,
            "get",
            vec![key.clone()]
        )) || !h.equal(
            &sync!(
                h,
                h.get(&owner, "ohlcvSubscriptionVersions")?,
                "get",
                vec![key.clone()]
            ),
            &version,
        ) {
            return Ok(false);
        }
        let length = h.get(&candles, "length")?;
        if h.numeric(&length)? > 0.0 {
            let latest = call(
                h,
                "member",
                vec![candles, call(h, "subtract", vec![length, h.number(1.0)])?],
            )?;
            let previous = sync!(
                h,
                h.get(&owner, "lastCandleCache")?,
                "get",
                vec![key.clone()]
            );
            if !h.truthy(&previous)
                || !h.equal(
                    &h.get(&previous, "timestamp")?,
                    &h.get(&latest, "timestamp")?,
                )
                || !h.equal(&h.get(&previous, "close")?, &h.get(&latest, "close")?)
                || !h.equal(&h.get(&previous, "volume")?, &h.get(&latest, "volume")?)
            {
                sync!(
                    h,
                    h.get(&owner, "lastCandleCache")?,
                    "set",
                    vec![key.clone(), latest.clone()]
                );
                let callbacks = sync!(
                    h,
                    h.get(&owner, "ohlcvCallbacks")?,
                    "get",
                    vec![key.clone()]
                );
                if !h.equal(&callbacks, &h.undefined())
                    && !h.equal(&callbacks, &call(h, "null", vec![])?)
                {
                    call(h, "broadcast", vec![callbacks, latest])?;
                }
            }
        }
        sync!(
            h,
            owner.clone(),
            "maybeRecoverOhlcvTradeStream",
            vec![opts, key.clone()]
        );
        Ok::<_, H::Error>(true)
    }
    .await;
    if matches!(result, Ok(false)) {
        return Ok(h.undefined());
    }
    if h.truthy(&sync!(
        h,
        h.get(&owner, "ohlcvPollingActive")?,
        "get",
        vec![key.clone()]
    )) && h.equal(
        &sync!(
            h,
            h.get(&owner, "ohlcvSubscriptionVersions")?,
            "get",
            vec![key.clone()]
        ),
        &version,
    ) {
        let timer = sync!(h, h.global(), "setTimeout", vec![poll, h.number(3000.0)]);
        sync!(
            h,
            h.get(&owner, "ohlcvPollingTimeouts")?,
            "set",
            vec![key, timer]
        );
    }
    result.map(|_| h.undefined())
}

pub fn invoke_bound_handler<H: RuntimeAssemblyHost>(
    h: &H,
    owner: H::Value,
    method: &str,
    data: Option<H::Value>,
) -> Result<H::Value, H::Error> {
    let selected = h.select(owner, method)?;
    h.apply(selected, data.into_iter().collect())
}

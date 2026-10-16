//! Exchange composition and the remaining exchange method bodies. BaseExchange,
//! child constructors, pre-existing method bodies and ECMAScript intrinsics are
//! injected bindings, as in the other venue body modules. No HTTP/SDK serializer
//! is substituted here. Property reads remain live across each await.
use crate::account_reads::{returned, start, wait};
use crate::auth_init::AuthHost;
use std::{cell::Cell, future::Future, rc::Rc};

/// Ordinary runtime operations needed by this file, rather than perp semantics.
pub trait ExchangeHost: AuthHost<Constructor: Clone> {
    /// Bind an independent instance host; calling a second constructor must not
    /// retarget an earlier instance's methods.
    fn bind_receiver(&self, receiver: Self::Value) -> Result<Rc<Self>, Self::Error>;
    /// Create the derived constructor/prototype using these executable method
    /// bindings; name is a getter, the other three are prototype methods.
    fn derive(
        &self,
        base: Self::Constructor,
        members: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Constructor, Self::Error>;
    fn prototype_method(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn property(&self, object: &Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    type Iterator;
    fn iterate(&self, value: Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn greater(&self, left: &Self::Value, right: &Self::Value) -> Result<bool, Self::Error>;
    /// Current exchange receiver, also used by the previously ported body bindings.
    fn receiver(&self) -> Self::Value;
    /// Class-field DefineProperty (not ordinary Set); flags are writable,
    /// enumerable, configurable. It must shadow an inherited setter.
    fn define(
        &self,
        object: &Self::Value,
        key: &'static str,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    /// Allocate an arrow function over this receiver using the named source body's
    /// Rust binding. This is function allocation, not an alternate method body.
    fn method(
        &self,
        receiver: &Self::Value,
        name: &'static str,
    ) -> Result<Self::Value, Self::Error>;
    fn async_method(
        &self,
        body: Rc<dyn Fn(Vec<Self::Value>) -> PinTask<Self::Value, Self::Error>>,
    ) -> Self::Value;
    fn callback(&self, callback: Rc<dyn Fn() -> Result<Self::Value, Self::Error>>) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn strict_true(&self, value: &Self::Value) -> bool;
    fn strict_false(&self, value: &Self::Value) -> bool;
    fn finite(&self, value: &Self::Value) -> bool;
    fn to_number(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn fixed(&self, value: &Self::Value, digits: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn adjust(&self, value: Self::Value, step: Self::Value) -> Result<Self::Value, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn error_from_value(&self, message: Self::Value) -> Self::Error;
    fn error_is_instance(&self, error: &Self::Error) -> bool;
    fn has(&self, object: &Self::Value, key: &'static str) -> Result<bool, Self::Error>;
    fn entries(&self, value: &Self::Value) -> Result<Vec<(Self::Value, Self::Value)>, Self::Error>;
    fn object_assign(&self, target: &Self::Value, source: Self::Value) -> Result<(), Self::Error>;
    fn map_keys(&self, map: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn token_pairs(
        &self,
        market: Self::Value,
    ) -> Result<Vec<(Self::Value, Self::Value)>, Self::Error>;
    fn tick_precision(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn normalize_tick(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn disposed(&self) -> bool;
    fn schedule(&self, callback: Rc<dyn Fn()>, delay: f64) -> Result<(), Self::Error>;
    /// Retain/observe a detached polling promise, including logger rejection.
    fn detach(&self, task: PinTask<Self::Value, Self::Error>);
}
/// Detached promises remain owned by the runtime, even after cleanup.
pub type PinTask<V, E> = std::pin::Pin<Box<dyn Future<Output = Result<V, E>>>>;

/// Evaluated derived class; keep this handle for every construction in the realm.
pub struct PolymarketExchangeClass<H: ExchangeHost> {
    host: Rc<H>,
    constructor: H::Constructor,
}
impl<H: ExchangeHost> PolymarketExchangeClass<H> {
    pub fn new(host: Rc<H>) -> Result<Self, H::Error> {
        let base = host.capture_constructor("BaseExchange")?;
        let mut members = vec![];
        for name in [
            "name",
            "getPositionMode",
            "deriveApiCredentials",
            "mapOrder",
        ] {
            members.push((name, host.prototype_method(name)?));
        }
        let constructor = host.derive(base, members)?;
        Ok(Self { host, constructor })
    }
}
/// Instance state and own arrow functions over the shared class handle.
pub struct PolymarketExchange<H: ExchangeHost> {
    host: Rc<H>,
    receiver: H::Value,
}
impl<H: ExchangeHost> PolymarketExchange<H> {
    pub fn new(
        class: &PolymarketExchangeClass<H>,
        name: H::Value,
        opts: H::Value,
        store: H::Value,
    ) -> Result<Self, H::Error> {
        let host = class.host.clone();
        let receiver =
            host.construct(class.constructor.clone(), vec![name, opts.clone(), store])?;
        let host = host.bind_receiver(receiver.clone())?;
        for (key, value) in [
            ("api", host.undefined()),
            ("wallet", host.null()),
            ("credentials", host.null()),
            ("signerType", host.number(0.)),
            ("funderAddress", host.null()),
            ("publicWebsocket", host.undefined()),
            ("privateWebsocket", host.undefined()),
        ] {
            host.define(&receiver, key, value)?;
        }
        for key in [
            "marketMetadata",
            "tokenToSymbol",
            "symbolToToken",
            "costBasis",
        ] {
            let ctor = host.capture_constructor("Map")?;
            let map = host.construct(ctor, vec![])?;
            host.define(&receiver, key, map)?;
        }
        for key in [
            "dispose",
            "getAccount",
            "validateAccount",
            "start",
            "tick",
            "fetchBalance",
            "fetchMarkets",
            "fetchTickers",
            "fetchPositions",
            "fetchAndSetPositions",
            "fetchOrders",
            "placeOrder",
            "updateOrder",
            "cancelOrders",
            "cancelAllOrders",
            "fetchOrderBook",
            "listenOrderBook",
            "fetchOHLCV",
            "listenOHLCV",
            "setLeverage",
            "changePositionMode",
            "changeMarginType",
            "fetchSymbolMaxLeverage",
        ] {
            let h = host.clone();
            let method = match key {
                "fetchMarkets" => {
                    host.async_method(Rc::new(move |_| Box::pin(fetch_markets(h.clone()))))
                }
                "fetchTickers" => {
                    host.async_method(Rc::new(move |_| Box::pin(fetch_tickers(h.clone()))))
                }
                "placeOrder" => host.async_method(Rc::new(move |args| {
                    Box::pin(place_order(
                        h.clone(),
                        args.first().cloned().unwrap_or_else(|| h.undefined()),
                    ))
                })),
                "updateOrder" => host.async_method(Rc::new(move |args| {
                    Box::pin(update_order(
                        h.clone(),
                        args.first().cloned().unwrap_or_else(|| h.undefined()),
                    ))
                })),
                "fetchOHLCV" => host.async_method(Rc::new(move |args| {
                    Box::pin(fetch_ohlcv(
                        h.clone(),
                        args.first().cloned().unwrap_or_else(|| h.undefined()),
                    ))
                })),
                _ => host.method(&receiver, key)?,
            };
            host.define(&receiver, key, method)?;
        }
        if host.truthy(&host.field(&opts, "key")?) {
            let initialized = (|| {
                let call = host.capture("imports", "createWallet")?;
                let wallet = host.call_sync(call, vec![host.field(&opts, "key")?])?;
                host.assign(&receiver, "wallet", wallet)?;
                let log = host.capture("exchange", "log")?;
                let address = host.field(&host.field(&receiver, "wallet")?, "address")?;
                host.invoke_log(
                    log,
                    &message(host.as_ref(), "Initialized wallet: ", &address)?,
                    "info",
                )
            })();
            if let Err(error) = initialized {
                log_error(host.as_ref(), "Failed to create wallet: ", &error, "error")?;
            }
        }
        let poly = host.field(&opts, "polymarket")?;
        if host.truthy(&poly) {
            if host.truthy(&host.field(&poly, "apiKey")?)
                && host.truthy(&host.field(&poly, "apiSecret")?)
                && host.truthy(&host.field(&poly, "apiPassphrase")?)
            {
                let credentials = host.object(vec![
                    ("apiKey", host.field(&poly, "apiKey")?),
                    ("secret", host.field(&poly, "apiSecret")?),
                    ("passphrase", host.field(&poly, "apiPassphrase")?),
                ])?;
                host.assign(&receiver, "credentials", credentials)?;
            }
            if !host.is_undefined(&host.field(&poly, "signerType")?) {
                host.assign(&receiver, "signerType", host.field(&poly, "signerType")?)?;
            }
            if host.truthy(&host.field(&poly, "funderAddress")?) {
                host.assign(
                    &receiver,
                    "funderAddress",
                    host.field(&poly, "funderAddress")?,
                )?;
            }
        }
        let credentials = {
            let h = host.clone();
            let r = receiver.clone();
            host.callback(Rc::new(move || h.field(&r, "credentials")))
        };
        let wallet_address = {
            let h = host.clone();
            let r = receiver.clone();
            host.callback(Rc::new(move || {
                let w = h.field(&r, "wallet")?;
                let a = if h.nullish(&w) {
                    h.undefined()
                } else {
                    h.field(&w, "address")?
                };
                Ok(if h.nullish(&a) { h.null() } else { a })
            }))
        };
        let ctor = host.capture_constructor("PolymarketAPI")?;
        let api = host.construct(
            ctor,
            vec![opts, receiver.clone(), credentials, wallet_address],
        )?;
        host.assign(&receiver, "api", api)?;
        let ctor = host.capture_constructor("PolymarketPublicWebSocket")?;
        let public = host.construct(ctor, vec![receiver.clone()])?;
        host.assign(&receiver, "publicWebsocket", public)?;
        let ctor = host.capture_constructor("PolymarketPrivateWebSocket")?;
        let credentials = {
            let h = host.clone();
            let r = receiver.clone();
            host.callback(Rc::new(move || h.field(&r, "credentials")))
        };
        let private = host.construct(ctor, vec![receiver.clone(), credentials])?;
        host.assign(&receiver, "privateWebsocket", private)?;
        Ok(Self { host, receiver })
    }
    pub fn receiver(&self) -> H::Value {
        self.receiver.clone()
    }
    pub fn fetch_markets(&self) -> impl Future<Output = Result<H::Value, H::Error>> {
        fetch_markets(self.host.clone())
    }
    pub fn fetch_tickers(&self) -> impl Future<Output = Result<H::Value, H::Error>> {
        fetch_tickers(self.host.clone())
    }
    pub fn place_order(&self, opts: H::Value) -> impl Future<Output = Result<H::Value, H::Error>> {
        place_order(self.host.clone(), opts)
    }
    pub fn update_order(&self, opts: H::Value) -> impl Future<Output = Result<H::Value, H::Error>> {
        update_order(self.host.clone(), opts)
    }
    pub fn fetch_ohlcv(&self, opts: H::Value) -> impl Future<Output = Result<H::Value, H::Error>> {
        fetch_ohlcv(self.host.clone(), opts)
    }
    pub fn listen_ohlcv(
        &self,
        opts: H::Value,
        callback: Rc<dyn Fn(H::Value) -> Result<(), H::Error>>,
    ) -> Rc<dyn Fn()> {
        listen_ohlcv(self.host.clone(), opts, callback)
    }
}
fn message<H: ExchangeHost>(h: &H, prefix: &str, value: &H::Value) -> Result<Vec<u16>, H::Error> {
    let mut t = prefix.encode_utf16().collect::<Vec<_>>();
    t.extend(h.string(value)?);
    Ok(t)
}
fn log_error<H: ExchangeHost>(
    h: &H,
    prefix: &str,
    error: &H::Error,
    severity: &'static str,
) -> Result<(), H::Error> {
    let log = h.capture("exchange", "log")?;
    h.invoke_log(log, &message(h, prefix, &h.error_value(error))?, severity)
}
fn optional<H: ExchangeHost>(h: &H, value: H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.nullish(&value) {
        Ok(h.undefined())
    } else {
        h.field(&value, key)
    }
}
fn map_call<H: ExchangeHost>(
    h: &H,
    map: &'static str,
    method: &'static str,
    args: impl FnOnce() -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let value = h.field(&h.receiver(), map)?;
    let call = h.capture_value(value, method)?;
    h.call_sync(call, args()?)
}
fn push<H: ExchangeHost>(h: &H, array: &H::Value, value: H::Value) -> Result<(), H::Error> {
    h.push_spread(array, &h.array(vec![Some(value)]))
}
fn reject<H: ExchangeHost>(h: &H, message: H::Value) -> Result<H::Value, H::Error> {
    let call = h.capture("exchange", "emitAndLogError")?;
    h.call_sync(call, vec![message.clone()])?;
    Err(h.error_from_value(message))
}

pub fn fetch_markets<H: ExchangeHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let result = async {
                let call = h.capture("api", "getGammaMarkets")?;
                let request = h.object(vec![
                    ("active", h.boolean(true)),
                    ("closed", h.boolean(false)),
                ])?;
                let gamma = wait(h.invoke(call, vec![request])).await?;
                let markets = h.array(vec![]);
                let mut iterator = h.iterate(gamma.clone())?;
                while let Some(gm) = h.next(&mut iterator)? {
                    map_call(h.as_ref(), "marketMetadata", "set", || {
                        Ok(vec![h.field(&gm, "conditionId")?, gm.clone()])
                    })?;
                    for (token_id, outcome) in h.token_pairs(gm.clone())? {
                        let mut symbol = h.string(&h.field(&gm, "slug")?)?;
                        symbol.push(45);
                        symbol.extend(h.string(&outcome)?);
                        let symbol = h.string_value(symbol);
                        map_call(h.as_ref(), "tokenToSymbol", "set", || {
                            Ok(vec![token_id.clone(), symbol.clone()])
                        })?;
                        map_call(h.as_ref(), "symbolToToken", "set", || {
                            Ok(vec![symbol.clone(), token_id.clone()])
                        })?;
                        let base = h.field(&gm, "slug")?;
                        let active = h.field(&gm, "active")?;
                        let active = if h.truthy(&active) {
                            h.boolean(
                                !h.truthy(&h.field(&gm, "closed")?)
                                    && !h.strict_false(&h.field(&gm, "enableOrderBook")?)
                                    && !h.strict_false(&h.field(&gm, "acceptingOrders")?),
                            )
                        } else {
                            active
                        };
                        let precision = h.object(vec![
                            ("amount", h.number(2.)),
                            (
                                "price",
                                h.tick_precision(h.field(&gm, "orderPriceMinTickSize")?)?,
                            ),
                        ])?;
                        let min = h.to_number(&h.field(&gm, "orderMinSize")?)?;
                        let amount = h.object(vec![
                            (
                                "min",
                                h.number(if min == 0. || min.is_nan() { 0. } else { min }),
                            ),
                            ("max", h.number(1000000.)),
                        ])?;
                        let leverage =
                            h.object(vec![("min", h.number(1.)), ("max", h.number(1.))])?;
                        let limits = h.object(vec![("amount", amount), ("leverage", leverage)])?;
                        let condition = h.field(&gm, "conditionId")?;
                        let tick_size = h.field(&gm, "orderPriceMinTickSize")?;
                        let neg = h.field(&gm, "negRisk")?;
                        let neg = if h.nullish(&neg) {
                            h.boolean(false)
                        } else {
                            neg
                        };
                        let info = h.object(vec![
                            ("conditionId", condition),
                            ("tokenId", token_id.clone()),
                            ("outcome", outcome),
                            ("tickSize", tick_size),
                            ("negRisk", neg),
                            ("question", h.field(&gm, "question")?),
                            ("description", h.field(&gm, "description")?),
                            ("endDate", h.field(&gm, "endDate")?),
                            ("slug", h.field(&gm, "slug")?),
                        ])?;
                        push(
                            h.as_ref(),
                            &markets,
                            h.object(vec![
                                ("id", token_id),
                                ("symbol", symbol),
                                ("base", base),
                                ("quote", h.text("pUSD")),
                                ("active", active),
                                ("precision", precision),
                                ("limits", limits),
                                ("contractSize", h.number(1.)),
                                ("info", info),
                            ])?,
                        )?;
                    }
                }
                let log = h.capture("exchange", "log")?;
                let msg = format!(
                    "Loaded {} markets from {} prediction markets",
                    h.length(&markets)?,
                    h.length(&gamma)?
                );
                h.invoke_log(log, &msg.encode_utf16().collect::<Vec<_>>(), "default")?;
                Ok(markets)
            }
            .await;
            let value = match result {
                Ok(value) => value,
                Err(error) => {
                    log_error(h.as_ref(), "Error fetching markets: ", &error, "error")?;
                    h.array(vec![])
                }
            };
            returned(h.as_ref(), value).await
        }
    })
}
pub fn fetch_tickers<H: ExchangeHost>(
    host: Rc<H>,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let result = async {
                let token_ids = h.map_keys(&h.field(&h.receiver(), "tokenToSymbol")?)?;
                let n = h.length(&token_ids)?;
                if n == 0 {
                    return Ok(h.array(vec![]));
                }
                let all = h.object(vec![])?;
                for start in (0..n).step_by(100) {
                    let mut slots = vec![];
                    for i in start..usize::min(n, start + 100) {
                        slots.push(h.at(&token_ids, i)?);
                    }
                    let call = h.capture("api", "getMidpoints")?;
                    let prices = wait(h.invoke(call, vec![h.array(slots)])).await?;
                    h.object_assign(&all, prices)?;
                }
                let tickers = h.array(vec![]);
                for (token_id, price) in h.entries(&all)? {
                    let symbol = map_call(h.as_ref(), "tokenToSymbol", "get", || {
                        Ok(vec![token_id.clone()])
                    })?;
                    if !h.truthy(&symbol) {
                        continue;
                    }
                    let info = h.object(vec![("tokenId", token_id.clone())])?;
                    push(
                        h.as_ref(),
                        &tickers,
                        h.object(vec![
                            ("id", token_id),
                            ("symbol", symbol),
                            ("bid", price.clone()),
                            ("ask", price.clone()),
                            ("last", price.clone()),
                            ("mark", price.clone()),
                            ("index", price),
                            ("percentage", h.number(0.)),
                            ("fundingRate", h.number(0.)),
                            ("fundingTime", h.number(0.)),
                            ("volume", h.number(0.)),
                            ("quoteVolume", h.number(0.)),
                            ("openInterest", h.number(0.)),
                            ("info", info),
                        ])?,
                    )?;
                }
                Ok(tickers)
            }
            .await;
            let value = match result {
                Ok(v) => v,
                Err(e) => {
                    log_error(h.as_ref(), "Error fetching tickers: ", &e, "error")?;
                    h.array(vec![])
                }
            };
            returned(h.as_ref(), value).await
        }
    })
}
pub fn place_order<H: ExchangeHost>(
    host: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let call = h.capture("exchange", "resolveCallerClientOrderId")?;
            h.call_sync(call, vec![opts.clone()])?;
            let kind = h.field(&opts, "type")?;
            if !h.is_text(&kind, "limit") {
                return reject(
                    h.as_ref(),
                    h.string_value({
                        let mut m = "Polymarket does not support "
                            .encode_utf16()
                            .collect::<Vec<_>>();
                        m.extend(h.string(&h.field(&opts, "type")?)?);
                        m.extend(" orders in safe-cex".encode_utf16());
                        m
                    }),
                );
            }
            if !h.is_undefined(&h.field(&opts, "triggerPrice")?)
                || !h.is_undefined(&h.field(&opts, "stopLoss")?)
                || !h.is_undefined(&h.field(&opts, "takeProfit")?)
                || h.strict_true(&h.field(&opts, "reduceOnly")?)
                || h.strict_true(&h.field(&opts, "close")?)
            {
                return reject(h.as_ref(),h.text("Polymarket limit orders do not support trigger, bracket, or close-only options"));
            }
            if !h.is_undefined(&h.field(&opts, "timeInForce")?)
                && !h.is_text(&h.field(&opts, "timeInForce")?, "GoodTillCancel")
            {
                let v = h.field(&opts, "timeInForce")?;
                let mut m = message(h.as_ref(), "Polymarket does not support ", &v)?;
                m.extend(" in safe-cex".encode_utf16());
                return reject(h.as_ref(), h.string_value(m));
            }
            if !h.is_text(&h.field(&opts, "side")?, "buy")
                && !h.is_text(&h.field(&opts, "side")?, "sell")
            {
                return reject(
                    h.as_ref(),
                    h.string_value(message(
                        h.as_ref(),
                        "Unsupported Polymarket order side: ",
                        &h.field(&opts, "side")?,
                    )?),
                );
            }
            if h.is_undefined(&h.field(&opts, "price")?) || !h.finite(&h.field(&opts, "price")?) {
                return reject(
                    h.as_ref(),
                    h.text("Polymarket limit orders require a finite price"),
                );
            }
            if !h.finite(&h.field(&opts, "amount")?)
                || h.to_number(&h.field(&opts, "amount")?)? <= 0.
            {
                return reject(
                    h.as_ref(),
                    h.text("Polymarket limit orders require a positive finite amount"),
                );
            }
            if !h.truthy(&h.wallet()?) || !h.truthy(&h.exchange_credentials()?) {
                return reject(
                    h.as_ref(),
                    h.text("Wallet and credentials required for placing orders"),
                );
            }
            let token = map_call(h.as_ref(), "symbolToToken", "get", || {
                Ok(vec![h.field(&opts, "symbol")?])
            })?;
            if !h.truthy(&token) {
                return reject(
                    h.as_ref(),
                    h.string_value(message(
                        h.as_ref(),
                        "Unknown symbol: ",
                        &h.field(&opts, "symbol")?,
                    )?),
                );
            }
            let store = h.field(&h.receiver(), "store")?;
            let by_symbol = h.field(&store, "marketsBySymbol")?;
            let market = h.property(&by_symbol, h.field(&opts, "symbol")?)?;
            let precision = optional(
                h.as_ref(),
                optional(h.as_ref(), market.clone(), "precision")?,
                "amount",
            )?;
            let precision = if h.nullish(&precision) {
                h.number(2.)
            } else {
                precision
            };
            let normalized = (|| {
                let raw_tick = optional(
                    h.as_ref(),
                    optional(h.as_ref(), market.clone(), "info")?,
                    "tickSize",
                )?;
                h.normalize_tick(if h.nullish(&raw_tick) {
                    h.null()
                } else {
                    raw_tick
                })
            })();
            let tick = match normalized {
                Ok(t) => t,
                Err(e) => {
                    let m = if h.error_is_instance(&e) {
                        h.error_message(&e)?
                    } else {
                        h.string_value(h.string(&h.error_value(&e))?)
                    };
                    return reject(h.as_ref(), m);
                }
            };
            let price = h.adjust(h.field(&opts, "price")?, tick.clone())?;
            let size_string = h.fixed(&h.field(&opts, "amount")?, &precision)?;
            let size = h.number(h.to_number(&size_string)?);
            let side = if h.is_text(&h.field(&opts, "side")?, "buy") {
                h.text("BUY")
            } else {
                h.text("SELL")
            };
            let neg = h.boolean(h.strict_true(&optional(
                h.as_ref(),
                optional(h.as_ref(), market.clone(), "info")?,
                "negRisk",
            )?));
            let t = h.to_number(&tick)?;
            let min = optional(
                h.as_ref(),
                optional(
                    h.as_ref(),
                    optional(h.as_ref(), market, "limits")?,
                    "amount",
                )?,
                "min",
            )?;
            if h.to_number(&price)? < t || h.to_number(&price)? > 1. - t {
                return reject(
                    h.as_ref(),
                    h.string_value(
                        format!(
                            "Polymarket limit price must be between {} and {}",
                            safe_cex_num::js_number_string(t),
                            safe_cex_num::js_number_string(1. - t)
                        )
                        .encode_utf16()
                        .collect(),
                    ),
                );
            }
            if h.to_number(&size)? <= 0. || (!h.is_undefined(&min) && h.less(&size, &min)?) {
                let m = if !h.is_undefined(&min) {
                    h.string_value(message(
                        h.as_ref(),
                        "Polymarket order amount must be at least ",
                        &min,
                    )?)
                } else {
                    h.text("Polymarket order amount is too small for market precision")
                };
                return reject(h.as_ref(), m);
            }
            let result = async {
                let signing = h.capture("imports", "createSignedOrder")?;
                let wallet = h.wallet()?;
                let signature_type = h.signer_type()?;
                let funder = h.field(&h.receiver(), "funderAddress")?;
                let params = h.object(vec![
                    ("tokenId", token),
                    ("price", price.clone()),
                    ("size", size.clone()),
                    ("side", side.clone()),
                    ("tickSize", tick),
                    ("negRisk", neg),
                    ("signatureType", signature_type),
                    (
                        "funderAddress",
                        if h.nullish(&funder) {
                            h.undefined()
                        } else {
                            funder
                        },
                    ),
                ])?;
                let signed = wait(h.invoke(signing, vec![wallet, params])).await?;
                let call = h.capture("api", "postOrder")?;
                wait(h.invoke(call, vec![signed, h.text("GTC")])).await
            }
            .await;
            let response = match result {
                Ok(r) => r,
                Err(e) => {
                    let call = h.capture("exchange", "emitAndLogError")?;
                    h.call_sync(
                        call,
                        vec![h.error_value(&e), h.text("Polymarket place order failed")],
                    )?;
                    return Err(e);
                }
            };
            if !h.truthy(&h.field(&response, "success")?) {
                let msg = h.field(&response, "errorMsg")?;
                return reject(
                    h.as_ref(),
                    if h.truthy(&msg) {
                        msg
                    } else {
                        h.text("Order placement failed")
                    },
                );
            }
            let log = h.capture("exchange", "log")?;
            let mut m = message(h.as_ref(), "Placed ", &side)?;
            m.extend(" order for ".encode_utf16());
            m.extend(h.string(&size)?);
            m.push(32);
            m.extend(h.string(&h.field(&opts, "symbol")?)?);
            m.extend(" @ ".encode_utf16());
            m.extend(h.string(&price)?);
            h.invoke_log(log, &m, "info")?;
            let id = h.field(&response, "orderID")?;
            let ids = if h.truthy(&id) {
                h.array(vec![Some(h.field(&response, "orderID")?)])
            } else {
                h.array(vec![])
            };
            returned(h.as_ref(), ids).await
        }
    })
}
pub fn update_order<H: ExchangeHost>(
    host: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let order = h.field(&opts, "order")?;
            let update = h.field(&opts, "update")?;
            let cancel = h.capture("api", "cancelOrder")?;
            wait(h.invoke(cancel, vec![h.field(&order, "id")?])).await?;
            let amount = if h.has(&update, "amount")? {
                h.field(&update, "amount")?
            } else {
                h.field(&order, "amount")?
            };
            let price = if h.has(&update, "price")? {
                h.field(&update, "price")?
            } else {
                h.field(&order, "price")?
            };
            let call = h.capture("exchange", "placeOrder")?;
            let request = h.object(vec![
                ("symbol", h.field(&order, "symbol")?),
                ("type", h.field(&order, "type")?),
                ("side", h.field(&order, "side")?),
                ("amount", amount),
                ("price", price),
            ])?;
            let operation = h.invoke(call, vec![request])?;
            // Async return adopts the replacement promise without introducing an await.
            operation_future(operation).await
        }
    })
}
async fn operation_future<O: crate::order_adapters::Completion<V, E>, V, E>(
    op: Rc<O>,
) -> Result<V, E> {
    std::future::poll_fn(move |cx| op.poll(cx)).await
}
pub fn fetch_ohlcv<H: ExchangeHost>(
    host: Rc<H>,
    opts: H::Value,
) -> impl Future<Output = Result<H::Value, H::Error>> {
    start(host.as_ref(), {
        let h = host.clone();
        async move {
            let token = map_call(h.as_ref(), "symbolToToken", "get", || {
                Ok(vec![h.field(&opts, "symbol")?])
            })?;
            if !h.truthy(&token) {
                return returned(h.as_ref(), h.array(vec![])).await;
            }
            let result = async {
                let call = h.capture("api", "getPriceHistory")?;
                let rows = wait(h.invoke(
                    call,
                    vec![
                        token,
                        h.field(&opts, "from")?,
                        h.field(&opts, "to")?,
                        h.undefined(),
                        h.number(60.),
                    ],
                ))
                .await?;
                let n = h.length(&rows)?;
                let output = h.map_output(&rows, n)?;
                for index in 0..n {
                    if let Some(row) = h.at(&rows, index)? {
                        let timestamp = h.number(h.to_number(&h.field(&row, "t")?)? * 1000.);
                        let mut fields = vec![("timestamp", timestamp)];
                        for key in ["open", "high", "low", "close"] {
                            let call = h.capture("imports", "parseFloat")?;
                            fields.push((key, h.call_sync(call, vec![h.field(&row, "p")?])?));
                        }
                        fields.push(("volume", h.number(0.)));
                        h.map_set(&output, index, h.object(fields)?)?;
                    }
                }
                Ok(output)
            }
            .await;
            let value = match result {
                Ok(v) => v,
                Err(e) => {
                    log_error(h.as_ref(), "Error fetching OHLCV: ", &e, "warning")?;
                    h.array(vec![])
                }
            };
            returned(h.as_ref(), value).await
        }
    })
}
fn poll_candles<H: ExchangeHost>(
    h: Rc<H>,
    opts: H::Value,
    callback: Rc<dyn Fn(H::Value) -> Result<(), H::Error>>,
    active: Rc<Cell<bool>>,
    last: Rc<std::cell::RefCell<H::Value>>,
) {
    let future = start(h.as_ref(), {
        let h = h.clone();
        async move {
            if !active.get() || h.disposed() {
                return Ok(h.undefined());
            }
            let result = async {
                let call = h.capture("exchange", "fetchOHLCV")?;
                let params = h.spread(&opts)?;
                let from = h.number(((h.date_now()? - 60000.) / 1000.).floor());
                let to = h.number((h.date_now()? / 1000.).floor());
                h.define(&params, "from", from)?;
                h.define(&params, "to", to)?;
                let candles = wait(h.invoke(call, vec![params])).await?;
                let mut iterator = h.iterate(candles)?;
                while let Some(candle) = h.next(&mut iterator)? {
                    if h.greater(&h.field(&candle, "timestamp")?, &last.borrow())? {
                        *last.borrow_mut() = h.field(&candle, "timestamp")?;
                        callback(candle)?;
                    }
                }
                Ok::<_, H::Error>(())
            }
            .await;
            if let Err(e) = result {
                log_error(h.as_ref(), "Error in OHLCV polling: ", &e, "warning")?;
            }
            if active.get() && !h.disposed() {
                let next = h.clone();
                h.schedule(
                    Rc::new(move || {
                        poll_candles(
                            next.clone(),
                            opts.clone(),
                            callback.clone(),
                            active.clone(),
                            last.clone(),
                        )
                    }),
                    5000.,
                )?;
            }
            Ok(h.undefined())
        }
    });
    h.detach(Box::pin(future));
}
pub fn listen_ohlcv<H: ExchangeHost>(
    host: Rc<H>,
    opts: H::Value,
    callback: Rc<dyn Fn(H::Value) -> Result<(), H::Error>>,
) -> Rc<dyn Fn()> {
    let active = Rc::new(Cell::new(true));
    let last = Rc::new(std::cell::RefCell::new(host.number(0.)));
    poll_candles(host, opts, callback, active.clone(), last);
    Rc::new(move || active.set(false))
}

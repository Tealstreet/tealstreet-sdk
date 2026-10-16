//! Evaluated API/socket class handles and the two public message handlers.
//! Existing method bodies, shared bases and lodash throttle are bound services.
use crate::{api_factory::FactoryHost, auth_init::initialize_api};
use std::rc::Rc;
/// Class allocation, callable method bindings and Set iteration intrinsics.
pub trait ClassHost: FactoryHost {
    fn define_class(
        &self,
        name: &'static str,
        base: Option<Self::Constructor>,
        prototype: Vec<(&'static str, Self::Value)>,
    ) -> Result<Self::Constructor, Self::Error>;
    fn class_method(
        &self,
        class: &'static str,
        name: &'static str,
        receiver: Option<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn throttle(
        &self,
        body: Self::Value,
        wait: f64,
        leading: bool,
        trailing: bool,
    ) -> Result<Self::Value, Self::Error>;
    fn for_each_set(
        &self,
        set: Self::Value,
        callback: Rc<dyn Fn(Self::Value) -> Result<(), Self::Error>>,
    ) -> Result<(), Self::Error>;
    fn strict_zero(&self, value: &Self::Value) -> bool;
}
/// Keep one handle per module/class evaluation; instances reuse its prototypes.
pub struct PolymarketClasses<H: ClassHost> {
    host: Rc<H>,
    api: H::Constructor,
    public: H::Constructor,
    private: H::Constructor,
}
impl<H: ClassHost> PolymarketClasses<H> {
    pub fn new(host: Rc<H>) -> Result<Self, H::Error> {
        let mut prototype = vec![];
        for name in [
            "getMarkets",
            "getServerTime",
            "getSimplifiedMarkets",
            "getOrderBook",
            "getOrderBooks",
            "getMidpoint",
            "getMidpoints",
            "getPrice",
            "getPrices",
            "getLastTradePrice",
            "getPriceHistory",
            "getSpread",
            "postOrder",
            "postOrders",
            "getOpenOrders",
            "getOrder",
            "cancelOrder",
            "cancelOrders",
            "cancelMarketOrders",
            "cancelAllOrders",
            "getTrades",
            "getBalanceAllowance",
            "deriveApiKey",
            "deleteApiKey",
            "getGammaMarkets",
            "getPositions",
            "getGammaMarket",
            "getGammaMarketBySlug",
            "searchGammaMarkets",
        ] {
            prototype.push((name, host.class_method("PolymarketAPI", name, None)?));
        }
        let api = host.define_class("PolymarketAPI", None, prototype)?;
        let base = host.capture_constructor("BaseWebSocket")?;
        let public = host.define_class("PolymarketPublicWebSocket", Some(base), vec![])?;
        let base = host.capture_constructor("BaseWebSocket")?;
        let map_order = host.class_method("PolymarketPrivateWebSocket", "mapOrder", None)?;
        let private = host.define_class(
            "PolymarketPrivateWebSocket",
            Some(base),
            vec![("mapOrder", map_order)],
        )?;
        Ok(Self {
            host,
            api,
            public,
            private,
        })
    }
}
/// Callable native storage for the current source receiver's credential field.
pub struct CredentialCallback<V, E> {
    read: Rc<dyn Fn() -> Result<V, E>>,
    invoke: Rc<dyn Fn() -> Result<V, E>>,
}
impl<V, E> CredentialCallback<V, E> {
    pub fn value(&self) -> Result<V, E> {
        (self.read)()
    }
    pub fn call(&self) -> Result<V, E> {
        (self.invoke)()
    }
}
fn credential_callback<H: ClassHost>(
    host: Rc<H>,
    receiver: H::Value,
) -> CredentialCallback<H::Value, H::Error> {
    let read = {
        let h = host.clone();
        let r = receiver.clone();
        Rc::new(move || h.field(&r, "getCredentials"))
    };
    let invoke = Rc::new(move || {
        let call = host.capture_value(receiver.clone(), "getCredentials")?;
        host.call_sync(call, vec![])
    });
    CredentialCallback { read, invoke }
}
pub struct PolymarketAPI<H: ClassHost> {
    host: Rc<H>,
    receiver: H::Value,
    pub get_credentials: CredentialCallback<H::Value, H::Error>,
}
impl<H: ClassHost> PolymarketAPI<H> {
    pub fn new(
        classes: &PolymarketClasses<H>,
        options: H::Value,
        exchange: H::Value,
        credentials: H::Value,
        address: H::Value,
    ) -> Result<Self, H::Error> {
        let receiver = classes.host.construct(classes.api.clone(), vec![])?;
        let host = classes.host.bind_receiver(receiver.clone())?;
        for name in ["getCredentials", "xhr", "gammaXhr", "dataXhr", "exchange"] {
            host.define(&receiver, name, host.undefined())?;
        }
        initialize_api(
            host.clone(),
            receiver.clone(),
            options,
            exchange,
            credentials,
            address,
        )?;
        let callback = credential_callback(host.clone(), receiver.clone());
        Ok(Self {
            host,
            receiver,
            get_credentials: callback,
        })
    }
    pub fn receiver(&self) -> H::Value {
        self.receiver.clone()
    }
    /// Invoke the current prototype member, preserving receiver and return identity.
    pub fn call(
        &self,
        name: &'static str,
        args: Vec<H::Value>,
    ) -> Result<Rc<H::Operation>, H::Error> {
        let call = self.host.capture_value(self.receiver.clone(), name)?;
        self.host.invoke(call, args)
    }
}
pub struct PolymarketPublicWebSocket<H: ClassHost> {
    receiver: H::Value,
    host: Rc<H>,
}
impl<H: ClassHost> PolymarketPublicWebSocket<H> {
    pub fn new(classes: &PolymarketClasses<H>, parent: H::Value) -> Result<Self, H::Error> {
        let receiver = classes
            .host
            .construct(classes.public.clone(), vec![parent])?;
        let host = classes.host.bind_receiver(receiver.clone())?;
        host.define(&receiver, "recoversAfterOffline", host.boolean(false))?;
        host.define(
            &receiver,
            "connectUrl",
            host.field(&host.table("POLYMARKET_CONFIG")?, "WS_MARKET")?,
        )?;
        host.define(&receiver, "pingInterval", host.undefined())?;
        let ctor = host.capture_constructor("Set")?;
        let assets = host.construct(ctor, vec![])?;
        host.define(&receiver, "subscribedAssets", assets)?;
        let ctor = host.capture_constructor("Map")?;
        let callbacks = host.construct(ctor, vec![])?;
        host.define(&receiver, "orderbookCallbacks", callbacks)?;
        let handlers = message_handlers(
            host.clone(),
            &[
                ("ping", "handlePongEvent"),
                ("pong", "handlePongEvent"),
                ("price_change", "handlePriceChange"),
                ("book", "handleOrderBookUpdate"),
                ("last_trade_price", "handleLastTradePrice"),
            ],
        )?;
        host.define(&receiver, "messageHandlers", handlers)?;
        for name in [
            "connectAndSubscribe",
            "onOpen",
            "sendSubscription",
            "onMessage",
            "onClose",
            "ping",
            "clearPingInterval",
            "dispose",
            "subscribeToAsset",
            "unsubscribeFromAsset",
            "subscribeToOrderBook",
        ] {
            let method =
                host.class_method("PolymarketPublicWebSocket", name, Some(receiver.clone()))?;
            host.define(&receiver, name, method)?;
        }
        let body = {
            let h = host.clone();
            host.function(Rc::new(move |args| {
                handle_price_change(
                    h.clone(),
                    args.first().cloned().unwrap_or_else(|| h.undefined()),
                )?;
                Ok(h.undefined())
            }))
        };
        let throttled = host.throttle(body, 500., true, true)?;
        host.define(&receiver, "handlePriceChange", throttled)?;
        let last = host.class_method(
            "PolymarketPublicWebSocket",
            "handleLastTradePrice",
            Some(receiver.clone()),
        )?;
        host.define(&receiver, "handleLastTradePrice", last)?;
        let book = {
            let h = host.clone();
            host.function(Rc::new(move |args| {
                handle_order_book_update(
                    h.clone(),
                    args.first().cloned().unwrap_or_else(|| h.undefined()),
                )?;
                Ok(h.undefined())
            }))
        };
        host.define(&receiver, "handleOrderBookUpdate", book)?;
        Ok(Self { host, receiver })
    }
    pub fn receiver(&self) -> H::Value {
        self.receiver.clone()
    }
    pub fn dispatch(&self, topic: &str, data: H::Value) -> Result<(), H::Error> {
        let handlers = self.host.field(&self.receiver, "messageHandlers")?;
        let handlers = self.host.field(&handlers, topic)?;
        let n = self.host.length(&handlers)?;
        for i in 0..n {
            if let Some(callback) = self.host.at(&handlers, i)? {
                self.host.apply(callback, vec![data.clone()])?;
            }
        }
        Ok(())
    }
}
pub struct PolymarketPrivateWebSocket<H: ClassHost> {
    receiver: H::Value,
    pub get_credentials: CredentialCallback<H::Value, H::Error>,
}
impl<H: ClassHost> PolymarketPrivateWebSocket<H> {
    pub fn new(
        classes: &PolymarketClasses<H>,
        parent: H::Value,
        credentials: H::Value,
    ) -> Result<Self, H::Error> {
        let receiver = classes
            .host
            .construct(classes.private.clone(), vec![parent])?;
        let h = classes.host.bind_receiver(receiver.clone())?;
        h.define(&receiver, "recoversAfterOffline", h.boolean(false))?;
        h.define(
            &receiver,
            "connectUrl",
            h.field(&h.table("POLYMARKET_CONFIG")?, "WS_USER")?,
        )?;
        h.define(&receiver, "pingInterval", h.undefined())?;
        h.define(&receiver, "getCredentials", h.undefined())?;
        let handlers = message_handlers(
            h.clone(),
            &[
                ("ping", "handlePongEvent"),
                ("pong", "handlePongEvent"),
                ("order", "handleOrderUpdate"),
                ("trade", "handleTradeUpdate"),
            ],
        )?;
        h.define(&receiver, "messageHandlers", handlers)?;
        for name in [
            "connectAndSubscribe",
            "onOpen",
            "authenticate",
            "onMessage",
            "onClose",
            "ping",
            "clearPingInterval",
            "dispose",
            "handleOrderUpdate",
            "handleTradeUpdate",
            "refreshPositions",
        ] {
            let method =
                h.class_method("PolymarketPrivateWebSocket", name, Some(receiver.clone()))?;
            h.define(&receiver, name, method)?;
        }
        h.assign(&receiver, "getCredentials", credentials)?;
        let callback = credential_callback(h.clone(), receiver.clone());
        Ok(Self {
            receiver,
            get_credentials: callback,
        })
    }
    pub fn receiver(&self) -> H::Value {
        self.receiver.clone()
    }
}
fn message_handlers<H: ClassHost>(
    host: Rc<H>,
    handlers: &[(&'static str, &'static str)],
) -> Result<H::Value, H::Error> {
    let mut entries = vec![];
    for &(topic, method) in handlers {
        let h = host.clone();
        let callback = host.function(Rc::new(move |args| {
            let call = h.capture_value(h.receiver(), method)?;
            let data = if method == "handlePongEvent" {
                vec![]
            } else {
                vec![args.first().cloned().unwrap_or_else(|| h.undefined())]
            };
            h.call_sync(call, data)
        }));
        entries.push((topic, host.array(vec![Some(callback)])));
    }
    host.object(entries)
}
fn parent_log<H: ClassHost>(h: &H, prefix: &str, error: &H::Error) -> Result<(), H::Error> {
    let parent = h.field(&h.receiver(), "parent")?;
    let log = h.capture_value(parent, "log")?;
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    message.extend(h.string(&h.error_value(error))?);
    h.invoke_log(log, &message, "error")
}
fn parse_field<H: ClassHost>(h: &H, object: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    let parse = h.capture("imports", "parseFloat")?;
    let value = h.field(object, key)?;
    h.call_sync(parse, vec![value])
}

fn token_symbol<H: ClassHost>(h: &H, token: H::Value) -> Result<H::Value, H::Error> {
    let parent = h.field(&h.receiver(), "parent")?;
    let map = h.field(&parent, "tokenToSymbol")?;
    if h.nullish(&map) {
        Ok(h.undefined())
    } else {
        let get = h.capture_value(map, "get")?;
        h.call_sync(get, vec![token])
    }
}
/// Inner body is wrapped in the actual host throttle at class initialization.
pub fn handle_price_change<H: ClassHost>(host: Rc<H>, data: H::Value) -> Result<(), H::Error> {
    let h = host.as_ref();
    let body = (|| {
        let changes = h.field(&data, "price_changes")?;
        let changes = if h.truthy(&changes) {
            changes
        } else {
            h.array(vec![])
        };
        let mut iter = h.iterate(changes)?;
        while let Some(change) = h.next(&mut iter)? {
            let token = h.field(&change, "asset_id")?;
            let symbol = token_symbol(h, token.clone())?;
            if !h.truthy(&symbol) {
                continue;
            }
            let bid = parse_field(h, &change, "best_bid")?;
            let ask = parse_field(h, &change, "best_ask")?;
            let midpoint = if h.finite(&bid) && h.finite(&ask) {
                let added = h.add(&bid, &ask)?;
                Some(h.number(h.to_number(&added)? / 2.))
            } else {
                None
            };
            let mut fields = vec![("id", token.clone()), ("symbol", symbol)];
            if h.finite(&bid) {
                fields.push(("bid", bid));
            }
            if h.finite(&ask) {
                fields.push(("ask", ask));
            }
            if let Some(mid) = midpoint {
                fields.push(("mark", mid.clone()));
                fields.push(("index", mid));
            }
            let update = h.object(fields)?;
            let call = h.capture("store", "updateTicker")?;
            let selector = h.object(vec![("id", token)])?;
            h.call_sync(call, vec![selector, update])?;
        }
        Ok::<_, H::Error>(())
    })();
    if let Err(error) = body {
        parent_log(h, "Error handling price change: ", &error)?;
    }
    Ok(())
}
fn levels<H: ClassHost>(h: &H, input: H::Value) -> Result<H::Value, H::Error> {
    let input = if h.truthy(&input) {
        input
    } else {
        h.array(vec![])
    };
    let n = h.length(&input)?;
    let output = h.map_output(&input, n)?;
    let mut total = h.number(0.);
    for i in 0..n {
        if let Some(row) = h.at(&input, i)? {
            let price = parse_field(h, &row, "price")?;
            let amount = parse_field(h, &row, "size")?;
            total = h.add(&total, &amount)?;
            let level = h.object(vec![
                ("price", price),
                ("amount", amount),
                ("total", total.clone()),
            ])?;
            h.map_set(&output, i, level)?;
        }
    }
    Ok(output)
}
pub fn handle_order_book_update<H: ClassHost>(host: Rc<H>, data: H::Value) -> Result<(), H::Error> {
    let h = host.as_ref();
    let body = (|| {
        let token = h.field(&data, "asset_id")?;
        let map = h.field(&h.receiver(), "orderbookCallbacks")?;
        let get = h.capture_value(map, "get")?;
        let callbacks = h.call_sync(get, vec![token])?;
        if !h.truthy(&callbacks) || h.strict_zero(&h.field(&callbacks, "size")?) {
            return Ok(());
        }
        let bids = levels(h, h.field(&data, "bids")?)?;
        let asks = levels(h, h.field(&data, "asks")?)?;
        let book = h.object(vec![("bids", bids), ("asks", asks)])?;
        let host = host.clone();
        h.for_each_set(
            callbacks,
            Rc::new(move |callback| {
                host.apply(callback, vec![book.clone()])?;
                Ok(())
            }),
        )
    })();
    if let Err(error) = body {
        parent_log(h, "Error handling order book update: ", &error)?;
    }
    Ok(())
}

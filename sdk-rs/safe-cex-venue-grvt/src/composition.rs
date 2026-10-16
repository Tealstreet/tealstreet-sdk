//! Owned GRVT state and constructor sequencing; method bodies remain separately mapped.
use crate::{
    builder_workflow::JsResult,
    private_commands::PrivateCommands,
    public_subscriptions::PublicSubscriptions,
    session_host::session_template,
    signing_runtime::{SigningHost, validate_private_key},
};
use indexmap::IndexMap;
use safe_cex_exchange::runtime::LogSeverity;
use std::{cell::RefCell, hash::Hash, rc::Rc};
pub type Fields<V> = Rc<RefCell<IndexMap<&'static str, V>>>;
pub type NonceIds = Rc<RefCell<IndexMap<String, String>>>;
pub type NonceTimes = Rc<RefCell<IndexMap<String, Option<f64>>>>;

pub struct ClientOrderIdRule;
impl ClientOrderIdRule {
    pub fn is_valid(&self, value: &str) -> bool {
        let bytes = value.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= 19
            && (b'1'..=b'9').contains(&bytes[0])
            && bytes.iter().all(u8::is_ascii_digit)
            && value.parse::<u64>().is_ok_and(|value| value < 1u64 << 63)
    }
}
pub struct GrvtExchange<V> {
    pub base: V,
    pub fields: Fields<V>,
    pub nonce_to_order_id: NonceIds,
    pub nonce_creation_times: NonceTimes,
    pub client_order_id_rule: Rc<ClientOrderIdRule>,
    pub methods: IndexMap<&'static str, V>,
}
pub struct GrvtPublicWebSocket<V, T> {
    pub base: V,
    pub parent: V,
    pub fields: Fields<V>,
    pub state: Rc<PublicSubscriptions<T>>,
    pub methods: IndexMap<&'static str, V>,
}
pub struct GrvtPrivateWebSocket<V, T, F> {
    pub base: V,
    pub parent: V,
    pub fields: Fields<V>,
    pub state: Rc<PrivateCommands<T, F>>,
    pub methods: IndexMap<&'static str, V>,
}
impl<V: Clone> GrvtExchange<V> {
    pub fn member(&self, name: &str) -> Option<V> {
        self.methods.get(name).cloned()
    }
    pub fn record_nonce(&self, nonce: &str, real: Option<&str>, time: Option<f64>) {
        crate::snapshots::record_nonce(
            &mut self.nonce_creation_times.borrow_mut(),
            &mut self.nonce_to_order_id.borrow_mut(),
            nonce,
            real,
            time,
        );
    }
}
impl<V: Clone, T> GrvtPublicWebSocket<V, T> {
    pub fn member(&self, name: &str) -> Option<V> {
        self.methods.get(name).cloned()
    }
}
impl<V: Clone, T, F> GrvtPrivateWebSocket<V, T, F> {
    pub fn member(&self, name: &str) -> Option<V> {
        self.methods.get(name).cloned()
    }
}

/// Field views alias owned caches and inherited disposal/ping cells; no shadow state.
/// member binds the existing mapped Rust body to this owner; no default/no-op body exists.
/// Prototype/reflection and arbitrary untyped cache writes are outside native composition.
pub trait CompositionHost: SigningHost {
    type Timer: Clone + PartialEq + 'static;
    type PrivateTimer: Clone + 'static;
    type FillKey: Clone + Eq + Hash + 'static;
    fn base_exchange(
        &self,
        name: Self::Value,
        opts: Self::Value,
        store: Self::Value,
    ) -> JsResult<Self::Value>;
    fn base_socket(&self, parent: Self::Value) -> JsResult<Self::Value>;
    fn attach_fields(&self, instance: &Self::Value, fields: Fields<Self::Value>);
    fn rule_value(&self, rule: Rc<ClientOrderIdRule>) -> Self::Value;
    fn nonce_ids_view(&self, ids: NonceIds) -> Self::Value;
    fn nonce_times_view(&self, times: NonceTimes) -> Self::Value;
    fn public_field(&self, state: Rc<PublicSubscriptions<Self::Timer>>, key: &str) -> Self::Value;
    fn private_field(
        &self,
        state: Rc<PrivateCommands<Self::PrivateTimer, Self::FillKey>>,
        key: &str,
    ) -> Self::Value;
    fn member(
        &self,
        class: &'static str,
        name: &'static str,
        instance: &Self::Value,
    ) -> Self::Value;
    fn prototype_members(
        &self,
        instance: &Self::Value,
        members: &IndexMap<&'static str, Self::Value>,
    );
    fn grafana(&self, periodic: bool) -> Self::Value;
    fn api(
        &self,
        kind: &'static str,
        opts: Self::Value,
        instance: Self::Value,
    ) -> JsResult<Self::Value>;
    fn rate_limit(&self, instance: Self::Value, config: Self::Value) -> JsResult<Self::Value>;
    fn sdk_env(&self, testnet: bool) -> Self::Value;
    fn sdk_client(&self, config: Self::Value) -> JsResult<Self::Value>;
    fn socket(&self, private: bool, parent: Self::Value) -> JsResult<Self::Value>;
    fn same_string(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn severity(&self, severity: LogSeverity) -> Self::Value;
}
pub const EXCHANGE_MEMBERS: &[&str] = &[
    "dispose",
    "tick",
    "calculatePositionUpnlFromLastPrice",
    "calculateUpnlFromPositions",
    "syncBalanceUpnlFromPositions",
    "authenticate",
    "validateAccount",
    "fetchBuilderAuthorization",
    "getBuilderInfo",
    "getAccount",
    "start",
    "fetchMarkets",
    "refreshMarkets",
    "getInstrumentSigningMetadata",
    "updateMarketLeverageLimits",
    "fetchTickers",
    "fetchBalance",
    "getFakePositions",
    "fetchPositions",
    "fetchOpenOrders",
    "mapOrder",
    "mapOrderStatus",
    "fetchOrders",
    "placeOrder",
    "cancelOrder",
    "cancelOrders",
    "cancelAllOrders",
    "cancelSymbolOrders",
    "placeOrders",
    "updateOrder",
    "fetchOHLCV",
    "listenOHLCV",
    "fetchOrderBook",
    "listenOrderBook",
    "fetchLeverageForSymbol",
    "setLeverage",
    "fetchFillsForOrderIds",
];
pub const EXCHANGE_PROTOTYPE_MEMBERS: &[&str] = &[
    "recordNonce",
    "getOrderCreationTimeMs",
    "getPositionMode",
    "symbolToInstrumentId",
    "mapTimeInForceToEnum",
    "convertInterval",
];
pub const PUBLIC_MEMBERS: &[&str] = &[
    "connectAndSubscribe",
    "onOpen",
    "onMarketsChanged",
    "resubscribe",
    "subscribeToAllTickers",
    "cancelTickerBatches",
    "ensureTickerSubscription",
    "mapIntervalToGrvt",
    "subscribeToKline",
    "unsubscribeFromKline",
    "onMessage",
    "handleKlineUpdate",
    "subscribeToOrderbook",
    "unsubscribeFromOrderbook",
    "handleOrderbookUpdate",
    "subscribeToTicker",
    "unsubscribeFromTicker",
    "handleTickerUpdate",
    "listenOHLCV",
    "listenOrderBook",
    "dispose",
];
pub const PRIVATE_MEMBERS: &[&str] = &[
    "ping",
    "getWatchdogStaleThresholdMs",
    "checkLastReceived",
    "connectAndSubscribe",
    "onOpen",
    "authenticate",
    "resetRetryBudgetAfterHealthySession",
    "refreshSessionAndReconnect",
    "failClosedForAuthentication",
    "subscribe",
    "subscribeToStream",
    "onMessage",
    "handleOrderUpdate",
    "handlePositionUpdate",
    "handleFillUpdate",
    "parseOrder",
    "parsePosition",
    "throttledFetchOrders",
    "fetchThrottledOrders",
    "throttledFetchPositions",
    "executePositionFetch",
    "dispose",
];
fn initialize_member<H: CompositionHost>(
    h: &H,
    class: &'static str,
    base: &H::Value,
    fields: &Fields<H::Value>,
    methods: &mut IndexMap<&'static str, H::Value>,
    name: &'static str,
) {
    let value = h.member(class, name, base);
    fields.borrow_mut().insert(name, value.clone());
    methods.insert(name, value);
}
fn get<H: CompositionHost>(h: &H, instance: &H::Value, key: &str) -> JsResult<H::Value> {
    h.get(instance, key)
}
fn log<H: CompositionHost>(
    h: &H,
    instance: &H::Value,
    make: impl FnOnce() -> JsResult<H::Value>,
    level: LogSeverity,
) -> JsResult<H::Value, ()> {
    let function = h.get(instance, "log")?;
    h.call(function, instance.clone(), vec![make()?, h.severity(level)])?;
    Ok(())
}
fn lower<H: CompositionHost>(h: &H, value: H::Value) -> JsResult<H::Value> {
    let f = h.get(&value, "toLowerCase")?;
    h.call(f, value, vec![])
}
/// ts: GrvtExchange.constructor. Original options/store and handles retain identity.
pub fn new_exchange<H: CompositionHost>(
    h: &H,
    name: H::Value,
    opts: H::Value,
    store: H::Value,
) -> JsResult<H::Value, GrvtExchange<H::Value>> {
    let base = h.base_exchange(name, opts.clone(), store)?;
    let fields = Rc::new(RefCell::new(IndexMap::new()));
    h.attach_fields(&base, fields.clone());
    let rule = Rc::new(ClientOrderIdRule);
    let rule_value = h.rule_value(rule.clone());
    fields.borrow_mut().insert("clientOrderIdRule", rule_value);
    for key in [
        "xhr",
        "edgeXhr",
        "tradingXhr",
        "publicWebsocket",
        "privateWebsocket",
        "wallet",
        "grvtClient",
        "grvtTradingAccountId",
        "apiExtras",
    ] {
        fields.borrow_mut().insert(key, h.undefined());
    }
    fields.borrow_mut().insert("defaultLeverage", h.number(10.));
    let ids = Rc::new(RefCell::new(IndexMap::new()));
    let ids_view = h.nonce_ids_view(ids.clone());
    fields.borrow_mut().insert("_nonceToOrderId", ids_view);
    let times = Rc::new(RefCell::new(IndexMap::new()));
    let times_view = h.nonce_times_view(times.clone());
    fields
        .borrow_mut()
        .insert("_nonceCreationTimes", times_view);
    fields.borrow_mut().insert(
        "_builderAuthState",
        h.object(vec![("status", h.string("unknown"))]),
    );
    let every = h.grafana(false);
    fields.borrow_mut().insert("grafanaEndpointsEvery", every);
    let periodic = h.grafana(true);
    fields
        .borrow_mut()
        .insert("grafanaEndpointsPeriodic", periodic);
    let mut methods = IndexMap::new();
    for name in EXCHANGE_MEMBERS {
        initialize_member(h, "exchange", &base, &fields, &mut methods, name);
    }
    let mut prototype = IndexMap::new();
    for name in EXCHANGE_PROTOTYPE_MEMBERS {
        let body = h.member("exchange", name, &base);
        methods.insert(name, body.clone());
        prototype.insert(*name, body);
    }
    h.prototype_members(&base, &prototype);
    if h.truthy(&h.get(&opts, "subaccount")?) {
        h.set(
            &base,
            "apiExtras",
            h.object(vec![("subAccountId", h.get(&opts, "subaccount")?)]),
        )?;
    }
    if h.truthy(&h.get(&opts, "secret")?) {
        let wallet: JsResult<H::Value, ()> = (|| {
            if validate_private_key(h, h.get(&opts, "secret")?) {
                h.set(&base, "wallet", h.new_wallet(h.get(&opts, "secret")?)?)?;
                log(
                    h,
                    &base,
                    || session_template!(h;h.string("Wallet initialized: "),get(h,&get(h,&base,"wallet")?,"address")?),
                    LogSeverity::Info,
                )?;
                if h.truthy(&h.get(&opts, "grvtSignerAddress")?) {
                    let provided = lower(h, h.get(&opts, "grvtSignerAddress")?)?;
                    let derived = lower(h, get(h, &get(h, &base, "wallet")?, "address")?)?;
                    if !h.same_string(&provided, &derived) {
                        log(
                            h,
                            &base,
                            || {
                                let first = session_template!(h;h.string("⚠️ Warning: Provided signer address ("),h.get(&opts,"grvtSignerAddress")?,h.string(") does not match wallet address ("),get(h,&get(h,&base,"wallet")?,"address")?,h.string("). "))?;
                                let second = session_template!(h;h.string("Make sure "),get(h,&get(h,&base,"wallet")?,"address")?,h.string(" is tagged to your API key in the GRVT UI."))?;
                                h.template(vec![first, second])
                            },
                            LogSeverity::Warning,
                        )?;
                    } else {
                        log(
                            h,
                            &base,
                            || session_template!(h;h.string("Signer address verified: "),get(h,&get(h,&base,"wallet")?,"address")?),
                            LogSeverity::Info,
                        )?;
                    }
                } else {
                    log(
                        h,
                        &base,
                        || session_template!(h;h.string("⚠️ No signer address provided. Make sure "),get(h,&get(h,&base,"wallet")?,"address")?,h.string(" is tagged to your API key in the GRVT UI.")),
                        LogSeverity::Warning,
                    )?;
                }
            } else {
                log(
                    h,
                    &base,
                    || Ok(h.string("Invalid private key provided")),
                    LogSeverity::Warning,
                )?;
            }
            Ok(())
        })();
        if let Err(error) = wallet {
            log(
                h,
                &base,
                || session_template!(h;h.string("Failed to initialize wallet: "),h.get(&error,"message")?),
                LogSeverity::Error,
            )?;
        }
    }
    h.set(
        &base,
        "grvtTradingAccountId",
        h.get(&opts, "grvtTradingAccountId")?,
    )?;
    for (kind, slot) in [
        ("market", "xhr"),
        ("edge", "edgeXhr"),
        ("trading", "tradingXhr"),
    ] {
        let instance = h.api(kind, opts.clone(), base.clone())?;
        let limited = h.rate_limit(instance, h.object(vec![("maxRPS", h.number(10.))]))?;
        h.set(&base, slot, limited)?;
    }
    if h.truthy(&h.get(&opts, "key")?) && h.truthy(&h.get(&opts, "secret")?) {
        let client: JsResult<H::Value, ()> = (|| {
            let config = h.object(vec![
                ("apiKey", h.get(&opts, "key")?),
                ("apiSecret", h.get(&opts, "secret")?),
                ("env", h.sdk_env(h.truthy(&h.get(&opts, "testnet")?))),
                ("customAxios", get(h, &base, "tradingXhr")?),
                ("customEdgeAxios", get(h, &base, "edgeXhr")?),
            ]);
            h.set(&base, "grvtClient", h.sdk_client(config)?)?;
            log(
                h,
                &base,
                || Ok(h.string("GRVT SDK client initialized")),
                LogSeverity::Info,
            )?;
            Ok(())
        })();
        if let Err(error) = client {
            log(
                h,
                &base,
                || session_template!(h;h.string("Failed to initialize GRVT SDK client: "),h.get(&error,"message")?),
                LogSeverity::Warning,
            )?;
        }
    }
    h.set(&base, "publicWebsocket", h.socket(false, base.clone())?)?;
    h.set(&base, "privateWebsocket", h.socket(true, base.clone())?)?;
    Ok(GrvtExchange {
        base,
        fields,
        nonce_to_order_id: ids,
        nonce_creation_times: times,
        client_order_id_rule: rule,
        methods,
    })
}
/// ts: GrvtPublicWebSocket.constructor. Base construction precedes owned defaults.
pub fn new_public_socket<H: CompositionHost>(
    h: &H,
    parent: H::Value,
) -> JsResult<H::Value, GrvtPublicWebSocket<H::Value, H::Timer>> {
    let base = h.base_socket(parent.clone())?;
    let fields = Rc::new(RefCell::new(IndexMap::new()));
    h.attach_fields(&base, fields.clone());
    let state = Rc::new(PublicSubscriptions::default());
    for key in [
        "subscriptions",
        "requestId",
        "topics",
        "tickerSubscriptions",
        "tickerBatchTimers",
        "tickerBatchGeneration",
    ] {
        let value = h.public_field(state.clone(), key);
        fields.borrow_mut().insert(key, value);
    }
    let mut methods = IndexMap::new();
    for name in PUBLIC_MEMBERS {
        initialize_member(h, "public", &base, &fields, &mut methods, name);
    }
    Ok(GrvtPublicWebSocket {
        base,
        parent,
        fields,
        state,
        methods,
    })
}
/// ts: GrvtPrivateWebSocket.constructor. Private overrides share inherited lifecycle state.
pub fn new_private_socket<H: CompositionHost>(
    h: &H,
    parent: H::Value,
) -> JsResult<H::Value, GrvtPrivateWebSocket<H::Value, H::PrivateTimer, H::FillKey>> {
    let base = h.base_socket(parent.clone())?;
    let fields = Rc::new(RefCell::new(IndexMap::new()));
    h.attach_fields(&base, fields.clone());
    let state = Rc::new(PrivateCommands::default());
    for key in [
        "reportedMissingProxyHost",
        "treatAnyMessageAsPong",
        "subscriptions",
        "requestId",
        "authenticated",
        "authenticationFailed",
        "sessionRefreshInFlight",
        "sessionRefreshAttempts",
        "privateStreamHealthyAt",
        "lastPositionFetchTime",
        "positionFetchThrottleMs",
        "positionFetchScheduled",
        "hasPendingPositionEvents",
        "ordersThrottleTimeout",
        "processedFillIds",
    ] {
        let value = h.private_field(state.clone(), key);
        fields.borrow_mut().insert(key, value);
    }
    let mut methods = IndexMap::new();
    for name in PRIVATE_MEMBERS {
        initialize_member(h, "private", &base, &fields, &mut methods, name);
    }
    Ok(GrvtPrivateWebSocket {
        base,
        parent,
        fields,
        state,
        methods,
    })
}

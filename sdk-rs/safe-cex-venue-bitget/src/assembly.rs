//! Native construction/interceptor bodies and Rust family facades.
//! Facades deliberately use explicit supplied mapped-callable bindings rather
//! than JavaScript class prototypes; class rows record that representation choice.
use std::rc::Rc;
pub type NativeCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
pub trait AssemblyHost: Clone + 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn boolean(&self, v: bool) -> Self::Value;
    fn number(&self, v: f64) -> Self::Value;
    fn string(&self, v: &str) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn property(&self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn define(&self, v: &Self::Value, k: &str, field: Self::Value) -> Result<(), Self::Error>;
    fn assign(&self, v: &Self::Value, k: &str, field: Self::Value) -> Result<(), Self::Error>;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn global(&self, k: &str) -> Result<Self::Value, Self::Error>;
    fn member(&self, v: &Self::Value, k: &str) -> Result<Self::Value, Self::Error>;
    fn invoke(&self, f: Self::Value, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn construct(&self, f: Self::Value, args: Vec<Self::Value>)
    -> Result<Self::Value, Self::Error>;
    fn function(&self, body: NativeCallback<Self::Value, Self::Error>) -> Self::Value;
    /// Native async-function wrapper: return adopts a promise; thrown errors reject it.
    fn async_function(&self, body: NativeCallback<Self::Value, Self::Error>) -> Self::Value;
    fn spread(&self, v: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Actual separately ported callable binding, owned by this instance. Class
    /// adapters must resolve the supplied manifest target, never a no-op stub.
    fn bind_mapped(
        &self,
        receiver: &Self::Value,
        owner: &str,
        name: &str,
        targets: &[&str],
    ) -> Result<Self::Value, Self::Error>;
}
fn opt<H: AssemblyHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        h.property(v, k)
    }
}
fn imported<H: AssemblyHost>(h: &H, k: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let f = h.global(k)?;
    h.invoke(f, args)
}
fn member<H: AssemblyHost>(
    h: &H,
    v: &H::Value,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let f = h.member(v, k)?;
    h.invoke(f, args)
}
fn includes<H: AssemblyHost>(h: &H, v: &H::Value, needle: H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&member(h, v, "includes", vec![needle])?))
}
// ts: createAPI
pub fn create_api<H: AssemblyHost>(
    h: &H,
    options: H::Value,
    exchange: H::Value,
) -> Result<H::Value, H::Error> {
    let recv = opt(h, &opt(h, &options, "extra")?, "recvWindow")?;
    let recv = if h.nullish(&recv) {
        h.number(crate::protocol::RECV_WINDOW as f64)
    } else {
        recv
    };
    let serializer_host = h.clone();
    let serialize = h.function(Rc::new(move |a| {
        imported(
            &serializer_host,
            "qs.stringify",
            vec![
                a.first()
                    .cloned()
                    .unwrap_or_else(|| serializer_host.undefined()),
            ],
        )
    }));
    let config = h.object(vec![
        ("baseURL", h.string(crate::protocol::BASE_URL)),
        ("timeout", recv),
        ("paramsSerializer", h.object(vec![("serialize", serialize)])),
        (
            "headers",
            h.object(vec![
                ("ACCESS-KEY", h.property(&options, "key")?),
                ("ACCESS-PASSPHRASE", h.property(&options, "passphrase")?),
                ("X-CHANNEL-API-CODE", h.string(crate::protocol::BROKER_ID)),
                ("Content-Type", h.string("application/json")),
            ]),
        ),
    ]);
    let xhr = imported(
        h,
        "safeCexAxiosCreate",
        vec![config, options.clone(), exchange.clone()],
    )?;
    let retry = h.global("retry")?;
    let condition = imported(h, "networkErrorOnRead", vec![])?;
    h.invoke(
        retry,
        vec![
            xhr.clone(),
            h.object(vec![
                ("retries", h.number(3.)),
                ("retryCondition", condition),
            ]),
        ],
    )?;
    let use_request = h.member(
        &h.property(&h.property(&xhr, "interceptors")?, "request")?,
        "use",
    )?;
    let request_host = h.clone();
    let request_options = options.clone();
    let request = h.function(Rc::new(move |args| {
        let h = &request_host;
        let options = &request_options;
        let config = args.first().cloned().unwrap_or_else(|| h.undefined());
        let next = h.spread(&config)?;
        if h.truthy(&h.property(options, "remoteSigner")?) {
            return Ok(next);
        }
        if !h.truthy(&h.property(options, "key")?) || !h.truthy(&h.property(options, "secret")?) {
            return Ok(next);
        }
        let timestamp = imported(h, "virtualClock.getCurrentTimeMs", vec![])?;
        let method = opt(h, &config, "method")?;
        let method = if h.nullish(&method) {
            h.string("GET")
        } else {
            member(h, &method, "toUpperCase", vec![])?
        };
        let mut url = h.property(&config, "url")?;
        if h.truthy(&h.property(&config, "params")?) {
            let params = h.property(&config, "params")?;
            if h.truthy(&h.property(&params, "symbol")?) && {
                let u = h.property(&config, "url")?;
                h.nullish(&u)
                    || !includes(
                        h,
                        &u,
                        h.string(
                            crate::protocol::ENDPOINTS
                                .iter()
                                .find(|e| e.name == "KLINE")
                                .expect("KLINE")
                                .path,
                        ),
                    )?
            } {
                let params = h.property(&config, "params")?;
                let symbol = h.property(&h.property(&config, "params")?, "symbol")?;
                let symbol = member(h, &symbol, "replace", vec![h.string("$"), h.string("")])?;
                h.assign(&params, "symbol", symbol)?;
            }
            let f = h.global("qs.stringify")?;
            let query = h.invoke(
                f,
                vec![
                    h.property(&config, "params")?,
                    h.object(vec![("encode", h.boolean(false))]),
                ],
            )?;
            url = imported(h, "signingQueryTemplate", vec![url, query])?;
        }
        let body = if h.truthy(&h.property(&config, "data")?) {
            let json = h.global("toJsonString")?;
            let data = h.spread(&h.property(&config, "data")?)?;
            if h.truthy(&h.property(&h.property(&config, "data")?, "symbol")?) {
                let symbol = h.property(&h.property(&config, "data")?, "symbol")?;
                let symbol = member(h, &symbol, "replace", vec![h.string("$"), h.string("")])?;
                h.define(&data, "symbol", symbol)?;
            }
            h.invoke(json, vec![data])?
        } else {
            h.string("")
        };
        let preimage = imported(h, "signingJoin", vec![timestamp.clone(), method, url, body])?;
        let f = h.global("createHmac")?;
        let secret = h.property(options, "secret")?;
        let mac = h.invoke(f, vec![h.string("sha256"), secret])?;
        let updated = member(h, &mac, "update", vec![preimage])?;
        let digest = member(h, &updated, "digest", vec![])?;
        let signature = member(h, &digest, "toString", vec![h.string("base64")])?;
        let header_constructor = h.global("AxiosHeaders")?;
        let headers = h.spread(&h.property(&next, "headers")?)?;
        h.define(&headers, "ACCESS-SIGN", signature)?;
        h.define(&headers, "ACCESS-TIMESTAMP", timestamp)?;
        let headers = h.construct(header_constructor, vec![headers])?;
        let output = h.spread(&next)?;
        h.define(&output, "headers", headers)?;
        Ok(output)
    }));
    h.invoke(use_request, vec![request])?;
    let use_response = h.member(
        &h.property(&h.property(&xhr, "interceptors")?, "response")?,
        "use",
    )?;
    let response_host = h.clone();
    let response_exchange = exchange.clone();
    let response = h.async_function(Rc::new(move |args| {
        let h = &response_host;
        let r = args.first().cloned().unwrap_or_else(|| h.undefined());
        if h.truthy(&h.property(&r, "data")?)
            && !h.truthy(&imported(
                h,
                "v",
                vec![h.property(&r, "data")?, h.string("data")],
            )?)
            && !h.equal(
                &imported(h, "v", vec![h.property(&r, "data")?, h.string("code")])?,
                &h.string("00000"),
            )
        {
            let code = imported(h, "v", vec![h.property(&r, "data")?, h.string("code")])?;
            let code = imported(
                h,
                "String",
                vec![if h.nullish(&code) { h.string("") } else { code }],
            )?;
            let message = imported(h, "v", vec![h.property(&r, "data")?, h.string("msg")])?;
            if h.truthy(&message)
                && !h.truthy(&imported(
                    h,
                    "shouldSuppressValidationErrorEmission",
                    vec![response_exchange.clone()],
                )?)
                && !h.truthy(&imported(
                    h,
                    "shouldSilenceBitgetResponseError",
                    vec![
                        code,
                        message.clone(),
                        opt(h, &h.property(&r, "config")?, "url")?,
                    ],
                )?)
            {
                let f = h.member(&response_exchange, "emitAndLogError")?;
                let append = h.global("appendHttpFailure")?;
                let message = imported(h, "String", vec![message])?;
                let msg = h.invoke(append, vec![message, r.clone()])?;
                h.invoke(f, vec![msg])?;
            }
        }
        Ok(r)
    }));
    h.invoke(use_response, vec![response])?;
    if h.truthy(&h.property(&options, "tealstreetTunnel")?) {
        let f = h.global("installTransportAdapter")?;
        h.invoke(
            f,
            vec![xhr.clone(), options, h.property(&exchange, "name")?],
        )?;
    }
    Ok(xhr)
}

fn initialize_exchange<H: AssemblyHost>(h: &H, r: &H::Value) -> Result<(), H::Error> {
    h.define(
        r,
        "clientOrderIdRule",
        imported(
            h,
            "patternClientOrderIdRule",
            vec![h.string("Bitget"), h.global("BitgetClientOrderIdPattern")?],
        )?,
    )?;
    h.define(
        r,
        "fetchHistoricalPositions",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchHistoricalPositions",
            &["safe-cex-venue-bitget/src/history_reads.rs::fetch_historical_positions"],
        )?,
    )?;
    h.define(r, "xhr", h.undefined())?;
    h.define(r, "unlimitedXHR", h.undefined())?;
    h.define(r, "publicWebsocket", h.undefined())?;
    h.define(r, "secondsTickersWebsocket", h.undefined())?;
    h.define(r, "thirdTickersWebsocket", h.undefined())?;
    h.define(r, "privateWebsocket", h.undefined())?;
    h.define(
        r,
        "grafanaEndpointsEvery",
        h.global("GRAFANA_ENDPOINTS_EVERY")?,
    )?;
    h.define(
        r,
        "grafanaEndpointsPeriodic",
        h.global("GRAFANA_ENDPOINTS_PERIODIC")?,
    )?;
    h.define(r, "isUta", h.boolean(false))?;
    h.define(r, "supportsUtaRouting", h.boolean(true))?;
    h.define(r, "_accountTypeDetected", h.boolean(false))?;
    h.define(r, "_accountTypeDetectionInflight", h.null())?;
    h.define(r, "_positioningCache", h.object(vec![]))?;
    h.define(r, "_positioningInflight", h.object(vec![]))?;
    h.define(r, "_planTypePositionsCache", h.null())?;
    h.define(r, "_planTypePositionsInflight", h.null())?;
    h.define(
        r,
        "_suppressBitgetValidationErrorEmission",
        h.boolean(false),
    )?;
    h.define(r, "privateStreamWriteCheckTimeout", h.undefined())?;
    h.define(r, "privateStreamDegradedReconcileTimeout", h.undefined())?;
    h.define(r, "privateStreamDegradedReconcileRunning", h.boolean(false))?;
    h.define(r, "isPrivateStreamDegraded", h.boolean(false))?;
    h.define(r, "lastPrivateStreamMessageAt", h.number(0.))?;
    h.define(r, "hasWarnedPrivateStreamDegraded", h.boolean(false))?;
    h.define(
        r,
        "calculateAccountUpnl",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "calculateAccountUpnl",
            &["safe-cex-venue-bitget/src/pnl.rs::calculate_account_upnl"],
        )?,
    )?;
    h.define(
        r,
        "getAccount",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getAccount",
            &["safe-cex-venue-bitget/src/account_read.rs::get_account"],
        )?,
    )?;
    h.define(
        r,
        "validateAccount",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "validateAccount",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::validate_account"],
        )?,
    )?;
    h.define(
        r,
        "dispose",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "dispose",
            &["safe-cex-venue-bitget/src/stream_lifecycle.rs::dispose"],
        )?,
    )?;
    h.define(
        r,
        "markPrivateStreamMessageReceived",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "markPrivateStreamMessageReceived",
            &["safe-cex-venue-bitget/src/stream_lifecycle.rs::mark_private_stream_message_received"],
        )?,
    )?;
    h.define(
        r,
        "markPrivateStreamDegraded",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "markPrivateStreamDegraded",
            &["safe-cex-venue-bitget/src/stream_timers.rs::mark_private_stream_degraded"],
        )?,
    )?;
    h.define(r,"startPrivateStreamDegradedReconcile",h.bind_mapped(r,"BitgetExchange","startPrivateStreamDegradedReconcile",&["safe-cex-venue-bitget/src/stream_lifecycle.rs::start_private_stream_degraded_reconcile"])?)?;
    h.define(
        r,
        "stopPrivateStreamDegradedReconcile",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "stopPrivateStreamDegradedReconcile",
            &["safe-cex-venue-bitget/src/stream_lifecycle.rs::stop_private_stream_degraded_reconcile"],
        )?,
    )?;
    h.define(r,"schedulePrivateStreamDegradedReconcile",h.bind_mapped(r,"BitgetExchange","schedulePrivateStreamDegradedReconcile",&["safe-cex-venue-bitget/src/stream_timers.rs::schedule_private_stream_degraded_reconcile"])?)?;
    h.define(
        r,
        "reconcilePrivateStateFromRest",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "reconcilePrivateStateFromRest",
            &["safe-cex-venue-bitget/src/order_reads.rs::reconcile_private_state_from_rest"],
        )?,
    )?;
    h.define(
        r,
        "schedulePrivateStreamWriteCheck",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "schedulePrivateStreamWriteCheck",
            &["safe-cex-venue-bitget/src/stream_timers.rs::schedule_private_stream_write_check"],
        )?,
    )?;
    h.define(
        r,
        "start",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "start",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::start"],
        )?,
    )?;
    h.define(
        r,
        "tick",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "tick",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::tick"],
        )?,
    )?;
    h.define(
        r,
        "_detectAccountType",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "_detectAccountType",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::detect_account_type"],
        )?,
    )?;
    h.define(
        r,
        "ensureAccountTypeDetected",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "ensureAccountTypeDetected",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::ensure_account_type_detected"],
        )?,
    )?;
    h.define(r,"promoteUtaRoutingFromClassicUnsupported",h.bind_mapped(r,"BitgetExchange","promoteUtaRoutingFromClassicUnsupported",&["safe-cex-venue-bitget/src/account_predicates.rs::promote_uta_routing_from_classic_unsupported"])?)?;
    h.define(
        r,
        "fetchBalance",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchBalance",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_balance"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaBalance",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaBalance",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_uta_balance"],
        )?,
    )?;
    h.define(
        r,
        "fetchPositionMetadata",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchPositionMetadata",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_position_metadata"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaPositionMetadata",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaPositionMetadata",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_uta_position_metadata"],
        )?,
    )?;
    h.define(
        r,
        "fetchPositions",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchPositions",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_positions"],
        )?,
    )?;
    h.define(
        r,
        "fetchOpenPositions",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchOpenPositions",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_open_positions"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaOpenPositions",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaOpenPositions",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_uta_open_positions"],
        )?,
    )?;
    h.define(
        r,
        "resolveLiquidationPrice",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "resolveLiquidationPrice",
            &["safe-cex-venue-bitget/src/normalization.rs::resolve_liquidation_price"],
        )?,
    )?;
    h.define(
        r,
        "_fetchBillPage",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "_fetchBillPage",
            &["safe-cex-venue-bitget/src/history_reads.rs::fetch_bill_page"],
        )?,
    )?;
    h.define(
        r,
        "fetchIncomeHistory",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchIncomeHistory",
            &["safe-cex-venue-bitget/src/history_reads.rs::fetch_income_history"],
        )?,
    )?;
    h.define(
        r,
        "fetchIncomeHistoryRange",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchIncomeHistoryRange",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::fetch_income_history_range"],
        )?,
    )?;
    h.define(
        r,
        "_fetchFillsPage",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "_fetchFillsPage",
            &["safe-cex-venue-bitget/src/history_reads.rs::fetch_fills_page"],
        )?,
    )?;
    h.define(
        r,
        "fetchHistoricalFills",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchHistoricalFills",
            &["safe-cex-venue-bitget/src/history_reads.rs::fetch_historical_fills"],
        )?,
    )?;
    h.define(
        r,
        "fetchMarkets",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchMarkets",
            &["safe-cex-venue-bitget/src/public_data.rs::fetch_markets"],
        )?,
    )?;
    h.define(
        r,
        "fetchTickers",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchTickers",
            &["safe-cex-venue-bitget/src/public_data.rs::fetch_tickers"],
        )?,
    )?;
    h.define(
        r,
        "fetchOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchNormalOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchNormalOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_normal_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_uta_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaNormalOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaNormalOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_uta_normal_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchUtaStrategyOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchUtaStrategyOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_uta_strategy_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchAlgoOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchAlgoOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_algo_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchTrailingStopOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchTrailingStopOrders",
            &["safe-cex-venue-bitget/src/order_reads.rs::fetch_trailing_stop_orders"],
        )?,
    )?;
    h.define(
        r,
        "fetchOHLCV",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "fetchOHLCV",
            &["safe-cex-venue-bitget/src/public_data.rs::fetch_ohlcv"],
        )?,
    )?;
    h.define(
        r,
        "listenOHLCV",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "listenOHLCV",
            &["safe-cex-venue-bitget/src/subscriptions.rs::listen_ohlcv"],
        )?,
    )?;
    h.define(
        r,
        "listenOrderBook",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "listenOrderBook",
            &["safe-cex-venue-bitget/src/subscriptions.rs::listen_order_book"],
        )?,
    )?;
    h.define(
        r,
        "cancelAllOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelAllOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_all_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelNormalOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelNormalOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_normal_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelUtaNormalOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelUtaNormalOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_uta_normal_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelUtaStrategyOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelUtaStrategyOrders",
            &["safe-cex-venue-bitget/src/order_placement.rs::cancel_uta_strategy_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelAlgoOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelAlgoOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_algo_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelSymbolOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "cancelSymbolOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::cancel_symbol_orders"],
        )?,
    )?;
    h.define(
        r,
        "placeOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeOrder",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_order"],
        )?,
    )?;
    h.define(
        r,
        "placeOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeOrders",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_orders"],
        )?,
    )?;
    h.define(
        r,
        "formatCreateOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "formatCreateOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::format_create_order"],
        )?,
    )?;
    h.define(
        r,
        "formatUtaCreateOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "formatUtaCreateOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::format_uta_create_order"],
        )?,
    )?;
    h.define(
        r,
        "formatUtaStrategyOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "formatUtaStrategyOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::format_uta_strategy_order"],
        )?,
    )?;
    h.define(
        r,
        "formatAlgoOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "formatAlgoOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::format_algo_order"],
        )?,
    )?;
    h.define(
        r,
        "placeOrderBatch",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeOrderBatch",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_order_batch"],
        )?,
    )?;
    h.define(
        r,
        "placeUtaOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeUtaOrders",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_uta_orders"],
        )?,
    )?;
    h.define(
        r,
        "placeUtaStrategyOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeUtaStrategyOrders",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_uta_strategy_orders"],
        )?,
    )?;
    h.define(
        r,
        "placeAlgoOrders",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "placeAlgoOrders",
            &["safe-cex-venue-bitget/src/order_placement.rs::place_algo_orders"],
        )?,
    )?;
    h.define(
        r,
        "updateVirtualTpSlOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "updateVirtualTpSlOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::update_virtual_tp_sl_order"],
        )?,
    )?;
    h.define(
        r,
        "tryAmendLimitOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "tryAmendLimitOrder",
            &["safe-cex-venue-bitget/src/rest_actions.rs::try_amend_limit_order"],
        )?,
    )?;
    h.define(
        r,
        "updateOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "updateOrder",
            &["safe-cex-venue-bitget/src/rest_actions.rs::update_order"],
        )?,
    )?;
    h.define(
        r,
        "updateUtaStrategyOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "updateUtaStrategyOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::update_uta_strategy_order"],
        )?,
    )?;
    h.define(
        r,
        "buildUtaStrategyPlaceOptsFromOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "buildUtaStrategyPlaceOptsFromOrder",
            &["safe-cex-venue-bitget/src/order_logic.rs::build_uta_strategy_place_opts_from_order"],
        )?,
    )?;
    h.define(
        r,
        "updateAlgoOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "updateAlgoOrder",
            &["safe-cex-venue-bitget/src/rest_actions.rs::update_algo_order"],
        )?,
    )?;
    h.define(
        r,
        "setLeverage",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "setLeverage",
            &["safe-cex-venue-bitget/src/rest_actions.rs::set_leverage"],
        )?,
    )?;
    h.define(
        r,
        "changeMarginType",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "changeMarginType",
            &["safe-cex-venue-bitget/src/rest_actions.rs::change_margin_type"],
        )?,
    )?;
    h.define(
        r,
        "changePositionMode",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "changePositionMode",
            &["safe-cex-venue-bitget/src/rest_actions.rs::change_position_mode"],
        )?,
    )?;
    h.define(
        r,
        "mapOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "mapOrder",
            &["safe-cex-venue-bitget/src/orders.rs::map_order"],
        )?,
    )?;
    h.define(
        r,
        "mapUtaPosition",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "mapUtaPosition",
            &["safe-cex-venue-bitget/src/positions.rs::map_uta_position"],
        )?,
    )?;
    h.define(
        r,
        "mapPosition",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "mapPosition",
            &["safe-cex-venue-bitget/src/positions.rs::map_position"],
        )?,
    )?;
    h.define(
        r,
        "getOrderSideV2",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getOrderSideV2",
            &["safe-cex-venue-bitget/src/order_logic.rs::get_order_side_v2"],
        )?,
    )?;
    h.define(
        r,
        "getOrderSideUta",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getOrderSideUta",
            &[
                "safe-cex-venue-bitget/src/order_logic.rs::get_order_side_uta",
                "safe-cex-venue-bitget/src/order_logic.rs::UtaOrderSide",
            ],
        )?,
    )?;
    h.define(
        r,
        "getUtaStrategyOrderSide",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getUtaStrategyOrderSide",
            &["safe-cex-venue-bitget/src/order_logic.rs::get_uta_strategy_order_side"],
        )?,
    )?;
    h.define(
        r,
        "getUtaStrategyTriggerBy",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getUtaStrategyTriggerBy",
            &["safe-cex-venue-bitget/src/order_logic.rs::get_uta_strategy_trigger_by"],
        )?,
    )?;
    h.define(
        r,
        "getAlgoOrderSide",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getAlgoOrderSide",
            &["safe-cex-venue-bitget/src/order_logic.rs::get_algo_order_side"],
        )?,
    )?;
    h.define(
        r,
        "isVirtualTPSLOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "isVirtualTPSLOrder",
            &["safe-cex-venue-bitget/src/order_logic.rs::is_virtual_tpsl_order"],
        )?,
    )?;
    h.define(
        r,
        "isAlgoOrder",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "isAlgoOrder",
            &["safe-cex-venue-bitget/src/order_logic.rs::is_algo_order"],
        )?,
    )?;
    h.define(
        r,
        "getCancelPlanType",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getCancelPlanType",
            &["safe-cex-venue-bitget/src/order_logic.rs::get_cancel_plan_type"],
        )?,
    )?;
    h.define(
        r,
        "resolvePositionsForPlanType",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "resolvePositionsForPlanType",
            &["safe-cex-venue-bitget/src/order_preparation.rs::resolve_positions_for_plan_type"],
        )?,
    )?;
    h.define(
        r,
        "planTypeComparableAmount",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "planTypeComparableAmount",
            &["safe-cex-venue-bitget/src/order_logic.rs::plan_type_comparable_amount"],
        )?,
    )?;
    h.define(
        r,
        "floorToLot",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "floorToLot",
            &["safe-cex-venue-bitget/src/order_logic.rs::floor_to_lot"],
        )?,
    )?;
    h.define(
        r,
        "selectPlanTypeAndAmount",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "selectPlanTypeAndAmount",
            &["safe-cex-venue-bitget/src/order_logic.rs::select_plan_type_and_amount"],
        )?,
    )?;
    h.define(
        r,
        "getOrderPlanTypeAndAmount",
        h.bind_mapped(
            r,
            "BitgetExchange",
            "getOrderPlanTypeAndAmount",
            &[
                "safe-cex-venue-bitget/src/order_logic.rs::get_order_plan_type_and_amount",
                "safe-cex-venue-bitget/src/order_logic.rs::PlanSelectionOptions",
            ],
        )?,
    )?;
    Ok(())
}

fn initialize_publicwebsocket<H: AssemblyHost>(h: &H, r: &H::Value) -> Result<(), H::Error> {
    h.define(r, "topics", h.object(vec![]))?;
    h.define(r, "messageHandlers", h.object(vec![]))?;
    h.define(r, "messageCount", h.number(0.))?;
    h.define(r, "useFallbackUrl", h.boolean(false))?;
    h.define(r, "lastMessageCount", h.number(0.))?;
    h.define(r, "connectionAttempts", h.number(0.))?;
    h.define(r, "MAX_ATTEMPTS", h.number(2.))?;
    h.define(
        r,
        "toFiniteNumber",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "toFiniteNumber",
            &["safe-cex-venue-bitget/src/ticker.rs::to_finite_number"],
        )?,
    )?;
    h.define(
        r,
        "buildTickerUpdate",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "buildTickerUpdate",
            &["safe-cex-venue-bitget/src/ticker.rs::build_ticker_update"],
        )?,
    )?;
    h.define(
        r,
        "connectAndSubscribe",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "connectAndSubscribe",
            &["safe-cex-venue-bitget/src/public_data.rs::connect_and_subscribe"],
        )?,
    )?;
    h.define(
        r,
        "onOpen",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "onOpen",
            &["safe-cex-venue-bitget/src/public_data.rs::on_open"],
        )?,
    )?;
    h.define(
        r,
        "onMarketsChanged",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "onMarketsChanged",
            &["safe-cex-venue-bitget/src/public_data.rs::on_markets_changed"],
        )?,
    )?;
    h.define(
        r,
        "ping",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "ping",
            &["safe-cex-venue-bitget/src/socket_ping.rs::ping_public"],
        )?,
    )?;
    h.define(
        r,
        "subscribe",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "subscribe",
            &["safe-cex-venue-bitget/src/public_data.rs::subscribe"],
        )?,
    )?;
    h.define(
        r,
        "onMessage",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "onMessage",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::on_public_message"],
        )?,
    )?;
    h.define(
        r,
        "handleTickerTopic",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "handleTickerTopic",
            &[
                "safe-cex-venue-bitget/src/ticker.rs::handle_ticker_topic",
                "safe-cex-venue-bitget/src/ticker.rs::TickerHandlerError",
            ],
        )?,
    )?;
    h.define(
        r,
        "listenOHLCV",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "listenOHLCV",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::listen_ohlcv"],
        )?,
    )?;
    h.define(
        r,
        "listenOrderBook",
        h.bind_mapped(
            r,
            "BitgetPublicWebsocket",
            "listenOrderBook",
            &["safe-cex-venue-bitget/src/runtime_orchestration.rs::listen_order_book"],
        )?,
    )?;
    Ok(())
}

fn initialize_privatewebsocket<H: AssemblyHost>(h: &H, r: &H::Value) -> Result<(), H::Error> {
    h.define(r, "messageCount", h.number(0.))?;
    h.define(r, "useFallbackUrl", h.boolean(false))?;
    h.define(r, "lastMessageCount", h.number(0.))?;
    h.define(r, "connectionAttempts", h.number(0.))?;
    h.define(r, "MAX_ATTEMPTS", h.number(2.))?;
    h.define(r, "lastPrivateStreamRecoveryAt", h.number(0.))?;
    h.define(r, "privateStreamRecoveryCooldownMs", h.number(30000.))?;
    h.define(r, "lastPositionFetchTime", h.number(0.))?;
    h.define(r, "positionFetchThrottleMs", h.number(500.))?;
    h.define(r, "positionFetchScheduled", h.boolean(false))?;
    h.define(r, "hasPendingPositionEvents", h.boolean(false))?;
    h.define(r, "lastOrdersFetchTime", h.number(0.))?;
    h.define(r, "ordersFetchThrottleMs", h.number(500.))?;
    h.define(r, "ordersFetchScheduled", h.boolean(false))?;
    h.define(r, "hasPendingOrderEvents", h.boolean(false))?;
    h.define(r, "ordersFetchRequestSeq", h.number(0.))?;
    h.define(
        r,
        "terminalOrderTombstones",
        h.construct(h.global("Map")?, vec![])?,
    )?;
    h.define(
        r,
        "recentWsOpenOrders",
        h.construct(h.global("Map")?, vec![])?,
    )?;
    h.define(r, "orderTombstoneTtlMs", h.number(10000.))?;
    h.define(r, "wsOpenOrderGraceMs", h.number(5000.))?;
    h.define(r, "lastAlgoOrdersFetchTime", h.number(0.))?;
    h.define(r, "algoOrdersFetchThrottleMs", h.number(500.))?;
    h.define(r, "algoOrdersFetchScheduled", h.boolean(false))?;
    h.define(r, "hasPendingAlgoOrderEvents", h.boolean(false))?;
    h.define(r, "lastBalanceFetchTime", h.number(0.))?;
    h.define(r, "balanceFetchThrottleMs", h.number(300.))?;
    h.define(r, "balanceFetchScheduled", h.boolean(false))?;
    h.define(r, "hasPendingBalanceEvents", h.boolean(false))?;
    h.define(r, "lastWsPositionSnapshotAt", h.number(0.))?;
    h.define(
        r,
        "lastWsPositionSnapshotIds",
        h.construct(h.global("Set")?, vec![])?,
    )?;
    h.define(
        r,
        "wsPositionUpdatedAtById",
        h.construct(h.global("Map")?, vec![])?,
    )?;
    h.define(r, "positionRestStaleGuardMs", h.number(5000.))?;
    h.define(
        r,
        "normalizeOrderStatus",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "normalizeOrderStatus",
            &["safe-cex-venue-bitget/src/stream_predicates.rs::normalize_order_status"],
        )?,
    )?;
    h.define(
        r,
        "getRawOrderStatus",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "getRawOrderStatus",
            &["safe-cex-venue-bitget/src/stream_predicates.rs::get_raw_order_status"],
        )?,
    )?;
    h.define(
        r,
        "normalizeChannel",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "normalizeChannel",
            &["safe-cex-venue-bitget/src/stream_predicates.rs::normalize_channel"],
        )?,
    )?;
    h.define(
        r,
        "isTerminalFillStatus",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "isTerminalFillStatus",
            &["safe-cex-venue-bitget/src/stream_predicates.rs::is_terminal_fill_status"],
        )?,
    )?;
    h.define(
        r,
        "toFiniteNumber",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "toFiniteNumber",
            &["safe-cex-venue-bitget/src/numeric.rs::to_finite_number"],
        )?,
    )?;
    h.define(
        r,
        "getPositionPayloadUpdateTime",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "getPositionPayloadUpdateTime",
            &["safe-cex-venue-bitget/src/stream_positions.rs::get_position_payload_update_time"],
        )?,
    )?;
    h.define(
        r,
        "markWsPositions",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "markWsPositions",
            &["safe-cex-venue-bitget/src/private_topics.rs::mark_ws_positions"],
        )?,
    )?;
    h.define(r,"mergeRestPositionsWithFreshWsState",h.bind_mapped(r,"BitgetPrivateWebsocket","mergeRestPositionsWithFreshWsState",&["safe-cex-venue-bitget/src/stream_positions.rs::merge_rest_positions_with_fresh_ws_state"])?)?;
    h.define(
        r,
        "connectAndSubscribe",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "connectAndSubscribe",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::connect_and_subscribe"],
        )?,
    )?;
    h.define(
        r,
        "onOpen",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "onOpen",
            &["safe-cex-venue-bitget/src/private_recovery.rs::on_open"],
        )?,
    )?;
    h.define(
        r,
        "ping",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "ping",
            &["safe-cex-venue-bitget/src/socket_ping.rs::ping_private"],
        )?,
    )?;
    h.define(
        r,
        "recoverMissingPrivateStream",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "recoverMissingPrivateStream",
            &["safe-cex-venue-bitget/src/private_recovery.rs::recover_missing_private_stream"],
        )?,
    )?;
    h.define(
        r,
        "onMessage",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "onMessage",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::on_private_message"],
        )?,
    )?;
    h.define(
        r,
        "handleOrderTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleOrderTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_order_topic"],
        )?,
    )?;
    h.define(
        r,
        "handleUtaOrderTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleUtaOrderTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_uta_order_topic"],
        )?,
    )?;
    h.define(
        r,
        "handleAlgoOrdersTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleAlgoOrdersTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_algo_orders_topic"],
        )?,
    )?;
    h.define(
        r,
        "handlePositionsTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handlePositionsTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_positions_topic"],
        )?,
    )?;
    h.define(
        r,
        "handleUtaPositionsTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleUtaPositionsTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_uta_positions_topic"],
        )?,
    )?;
    h.define(
        r,
        "handleAccountTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleAccountTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_account_topic"],
        )?,
    )?;
    h.define(
        r,
        "handleUtaAccountTopic",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "handleUtaAccountTopic",
            &["safe-cex-venue-bitget/src/private_topics.rs::handle_uta_account_topic"],
        )?,
    )?;
    h.define(
        r,
        "throttledFetchPositions",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "throttledFetchPositions",
            &["safe-cex-venue-bitget/src/private_throttles.rs::throttled_fetch_positions"],
        )?,
    )?;
    h.define(
        r,
        "executePositionFetch",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "executePositionFetch",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::execute_position_fetch"],
        )?,
    )?;
    h.define(
        r,
        "throttledFetchBalance",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "throttledFetchBalance",
            &["safe-cex-venue-bitget/src/private_throttles.rs::throttled_fetch_balance"],
        )?,
    )?;
    h.define(
        r,
        "executeBalanceFetch",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "executeBalanceFetch",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::execute_balance_fetch"],
        )?,
    )?;
    h.define(
        r,
        "throttledFetchOrders",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "throttledFetchOrders",
            &["safe-cex-venue-bitget/src/private_throttles.rs::throttled_fetch_orders"],
        )?,
    )?;
    h.define(
        r,
        "executeOrdersFetch",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "executeOrdersFetch",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::execute_orders_fetch"],
        )?,
    )?;
    h.define(r,"pruneOrderLifecycleCaches",h.bind_mapped(r,"BitgetPrivateWebsocket","pruneOrderLifecycleCaches",&["safe-cex-venue-bitget/src/order_lifecycle.rs::OrderLifecycleCaches", "safe-cex-venue-bitget/src/order_lifecycle.rs::OrderLifecycleCaches::prune_order_lifecycle_caches"])?)?;
    h.define(r,"markOrderAsTerminal",h.bind_mapped(r,"BitgetPrivateWebsocket","markOrderAsTerminal",&["safe-cex-venue-bitget/src/order_lifecycle.rs::OrderLifecycleCaches::mark_order_as_terminal"])?)?;
    h.define(r,"markOrderAsOpenFromWs",h.bind_mapped(r,"BitgetPrivateWebsocket","markOrderAsOpenFromWs",&["safe-cex-venue-bitget/src/order_lifecycle.rs::OrderLifecycleCaches::mark_order_as_open_from_ws"])?)?;
    h.define(
        r,
        "auth",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "auth",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::auth"],
        )?,
    )?;
    h.define(
        r,
        "subscribe",
        h.bind_mapped(
            r,
            "BitgetPrivateWebsocket",
            "subscribe",
            &["safe-cex-venue-bitget/src/socket_runtime.rs::subscribe"],
        )?,
    )?;
    Ok(())
}

fn initialize_inverseexchange<H: AssemblyHost>(h: &H, r: &H::Value) -> Result<(), H::Error> {
    h.define(r, "supportsUtaRouting", h.boolean(false))?;
    h.define(
        r,
        "fetchMarkets",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "fetchMarkets",
            &["safe-cex-venue-bitget/src/public_data.rs::fetch_inverse_markets"],
        )?,
    )?;
    h.define(
        r,
        "fetchBalance",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "fetchBalance",
            &["safe-cex-venue-bitget/src/account_positions.rs::fetch_inverse_balance"],
        )?,
    )?;
    h.define(
        r,
        "fetchOpenPositions",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "fetchOpenPositions",
            &["safe-cex-venue-bitget/src/inverse_positions.rs::fetch_open_positions"],
        )?,
    )?;
    h.define(
        r,
        "cancelAllOrders",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "cancelAllOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::inverse_cancel_all_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelNormalOrders",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "cancelNormalOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::inverse_cancel_normal_orders"],
        )?,
    )?;
    h.define(
        r,
        "cancelAlgoOrders",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "cancelAlgoOrders",
            &["safe-cex-venue-bitget/src/cancellation.rs::inverse_cancel_algo_orders"],
        )?,
    )?;
    h.define(
        r,
        "formatAlgoOrder",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "formatAlgoOrder",
            &["safe-cex-venue-bitget/src/order_preparation.rs::inverse_format_algo_order"],
        )?,
    )?;
    h.define(
        r,
        "placeOrderBatch",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "placeOrderBatch",
            &["safe-cex-venue-bitget/src/order_placement.rs::inverse_place_order_batch"],
        )?,
    )?;
    h.define(
        r,
        "updateAlgoOrder",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "updateAlgoOrder",
            &["safe-cex-venue-bitget/src/rest_actions.rs::inverse_update_algo_order"],
        )?,
    )?;
    h.define(
        r,
        "setLeverage",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "setLeverage",
            &["safe-cex-venue-bitget/src/rest_actions.rs::inverse_set_leverage"],
        )?,
    )?;
    h.define(
        r,
        "changeMarginType",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "changeMarginType",
            &["safe-cex-venue-bitget/src/rest_actions.rs::inverse_change_margin_type"],
        )?,
    )?;
    h.define(
        r,
        "mapOrder",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "mapOrder",
            &["safe-cex-venue-bitget/src/orders.rs::map_inverse_order"],
        )?,
    )?;
    h.define(
        r,
        "getInverseAlgoOrderSide",
        h.bind_mapped(
            r,
            "BitgetInverseExchange",
            "getInverseAlgoOrderSide",
            &["safe-cex-venue-bitget/src/inverse_logic.rs::get_inverse_algo_order_side"],
        )?,
    )?;
    Ok(())
}

fn initialize_usdcexchange<H: AssemblyHost>(_h: &H, _r: &H::Value) -> Result<(), H::Error> {
    Ok(())
}

// ts: BitgetPublicWebsocket
pub struct BitgetPublicWebsocket<H: AssemblyHost> {
    pub host: H,
    pub receiver: H::Value,
}
impl<H: AssemblyHost> BitgetPublicWebsocket<H> {
    pub fn new(host: H, parent: H::Value) -> Result<Self, H::Error> {
        let receiver = host.construct(host.global("BaseWebSocket")?, vec![parent])?;
        initialize_publicwebsocket(&host, &receiver)?;
        Ok(Self { host, receiver })
    }
}
// ts: BitgetPrivateWebsocket
pub struct BitgetPrivateWebsocket<H: AssemblyHost> {
    pub host: H,
    pub receiver: H::Value,
}
impl<H: AssemblyHost> BitgetPrivateWebsocket<H> {
    pub fn new(host: H, parent: H::Value) -> Result<Self, H::Error> {
        let receiver = host.construct(host.global("BaseWebSocket")?, vec![parent])?;
        initialize_privatewebsocket(&host, &receiver)?;
        Ok(Self { host, receiver })
    }
}
// ts: BitgetExchange
pub struct BitgetExchange<H: AssemblyHost> {
    pub host: H,
    pub receiver: H::Value,
}
impl<H: AssemblyHost> BitgetExchange<H> {
    // ts: BitgetExchange.constructor
    pub fn new(
        host: H,
        name: H::Value,
        options: H::Value,
        store: H::Value,
    ) -> Result<Self, H::Error> {
        let receiver = host.construct(
            host.global("BaseExchange")?,
            vec![name, options.clone(), store],
        )?;
        initialize_exchange(&host, &receiver)?;
        let create = host.member(&receiver, "createXhrWithSecondaryKey")?;
        let api_host = host.clone();
        let api = host.function(Rc::new(move |args| {
            create_api(
                &api_host,
                args.first()
                    .cloned()
                    .unwrap_or_else(|| api_host.undefined()),
                args.get(1).cloned().unwrap_or_else(|| api_host.undefined()),
            )
        }));
        let rate_host = host.clone();
        let limit = host.function(Rc::new(move |args| {
            imported(
                &rate_host,
                "rateLimit",
                vec![
                    args.first()
                        .cloned()
                        .unwrap_or_else(|| rate_host.undefined()),
                    rate_host.object(vec![("maxRPS", rate_host.number(3.))]),
                ],
            )
        }));
        let xhr = host.invoke(
            create,
            vec![
                options.clone(),
                api,
                host.object(vec![("rateLimit", limit)]),
            ],
        )?;
        host.assign(&receiver, "xhr", xhr)?;
        let unlimited = create_api(&host, options, receiver.clone())?;
        host.assign(&receiver, "unlimitedXHR", unlimited)?;
        let public = BitgetPublicWebsocket::new(host.clone(), receiver.clone())?;
        host.assign(&receiver, "publicWebsocket", public.receiver)?;
        let private = BitgetPrivateWebsocket::new(host.clone(), receiver.clone())?;
        host.assign(&receiver, "privateWebsocket", private.receiver)?;
        let secondary = host.global("BitgetSecondaryPublicWebsocket")?;
        let second = host.construct(secondary, vec![receiver.clone(), host.number(1.)])?;
        host.assign(&receiver, "secondsTickersWebsocket", second)?;
        let secondary = host.global("BitgetSecondaryPublicWebsocket")?;
        let third = host.construct(secondary, vec![receiver.clone(), host.number(2.)])?;
        host.assign(&receiver, "thirdTickersWebsocket", third)?;
        Ok(Self { host, receiver })
    }
}
// ts: BitgetInverseExchange
pub struct BitgetInverseExchange<H: AssemblyHost> {
    pub inner: BitgetExchange<H>,
}
impl<H: AssemblyHost> BitgetInverseExchange<H> {
    // ts: BitgetInverseExchange.constructor
    pub fn new(
        host: H,
        name: H::Value,
        options: H::Value,
        store: H::Value,
    ) -> Result<Self, H::Error> {
        let inner = BitgetExchange::new(host, name, options, store)?;
        initialize_inverseexchange(&inner.host, &inner.receiver)?;
        Ok(Self { inner })
    }
    pub fn api_product_type(&self, testnet: bool) -> &'static str {
        crate::profile::inverse_api_product_type(testnet)
    }
    pub fn api_margin_coin(&self) -> &'static str {
        crate::profile::inverse_api_margin_coin()
    }
}
// ts: BitgetUSDCExchange
pub struct BitgetUSDCExchange<H: AssemblyHost> {
    pub inner: BitgetExchange<H>,
}
impl<H: AssemblyHost> BitgetUSDCExchange<H> {
    // ts: BitgetUSDCExchange.constructor
    pub fn new(
        host: H,
        name: H::Value,
        options: H::Value,
        store: H::Value,
    ) -> Result<Self, H::Error> {
        let inner = BitgetExchange::new(host, name, options, store)?;
        initialize_usdcexchange(&inner.host, &inner.receiver)?;
        Ok(Self { inner })
    }
    pub fn api_product_type(&self, testnet: bool) -> &'static str {
        crate::profile::usdc_api_product_type(testnet)
    }
    pub fn api_margin_coin(&self, testnet: bool) -> &'static str {
        crate::profile::usdc_api_margin_coin(testnet)
    }
}

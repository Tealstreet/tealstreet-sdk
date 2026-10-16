//! Production `ExchangeImports` for the native host. Requests run the ported
//! factory interceptors inside the shared `HttpClient` over real HTTP; the
//! private socket is a core `BaseWebSocket` (id 3). Requires a tokio `LocalSet`.
use crate::{
    api_factory::{self, ApiConfig, ApiFuture, ApiRejection, ClientMode, FactoryBindings,
        RemoteSignCall, RequestBindings, RequestInterceptor, RequestReturn, ResponseBindings,
        ResponseInterceptor, ResponseReturn, ResponseTransform, RetryRead},
    class_jobs::NativeFuture,
    exchange_classes::{ExchangeDependencies, ExchangeImports, HttpMethod},
    lifecycle::TickTimer,
    orderbook::*,
    private_classes::{
        Action, CapturedAction, NativePrivateSocket, PrivateBase, PrivateCallback, PrivateParent,
        PrivateTimers,
    },
    private_event_value::*,
    private_rest::{SnapshotCompletion, SnapshotRequestFuture},
    private_session::{Factory, RateHook, Venue},
    protocol_types::BinanceOrderBookApplyResult,
    public_listeners::{self, Book, Job, ListenerHost, ListenerState, Snapshot, SyncState},
    request_weight::{Failure, Fulfilled, Rejected},
    rest_history::PageCall,
};
use safe_cex_exchange::{
    http::{HttpClient, HttpClientOptions, HttpHost, safe_cex_axios_create,
        safe_cex_axios_create_no_proxy},
    native::{NativeHttpHost, NativeRuntime, nanoid},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestRequest, RestResponse, TransportRegistry,
    native::ReqwestAdapter, socket::SocketFrame,
};
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    rc::{Rc, Weak},
    sync::Arc,
};

fn failure(message: impl Into<String>) -> Failure {
    Rc::new(ExchangeError::new(message))
}
fn exchange_failure(error: ExchangeError) -> Failure {
    Rc::new(error)
}
/// JSON view of a native value; undefined members are omitted as JSON.stringify does.
fn to_json(value: &EventValue) -> Result<Value, Failure> {
    let Some(text) = safe_cex_num::js_json_stringify(&native_json(value))
        .map_err(|e| failure(e.message))?
    else {
        return Ok(Value::Null);
    };
    serde_json::from_str(&text).map_err(|e| failure(e.to_string()))
}

/// Production Binance dependencies over the given native runtime.
pub fn native_dependencies(runtime: Rc<NativeRuntime>) -> Result<ExchangeDependencies, ExchangeError> {
    let adapter: Arc<dyn RestAdapter> =
        Arc::new(ReqwestAdapter::new().map_err(|e| ExchangeError::new(e.to_string()))?);
    Ok(native_dependencies_with_adapter(runtime, adapter))
}
/// As `native_dependencies`, with an explicit terminal HTTP adapter.
pub fn native_dependencies_with_adapter(
    runtime: Rc<NativeRuntime>,
    adapter: Arc<dyn RestAdapter>,
) -> ExchangeDependencies {
    let ws: Rc<dyn WsRuntime> = runtime;
    ExchangeDependencies {
        runtime: ws.clone(),
        imports: NativeImports::new(ws, adapter),
        signers: Default::default(),
        remote_signer_token: None,
        payload_signer_token: None,
    }
}

pub struct NativeImports {
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
    host: Rc<dyn HttpHost>,
    registry: TransportRegistry,
    clients: RefCell<Vec<Rc<NativeClient>>>,
    timers: Rc<NativeTimers>,
    /// The exchange's emitAndLogError, bound during construction.
    emitter: Rc<RefCell<Option<Emitter>>>,
    /// publicXhr (Futures/PM) or xhr (Spot): the order-book snapshot client.
    public_client: RefCell<Option<Rc<NativeClient>>>,
    this: Weak<NativeImports>,
}
type Emitter = Rc<dyn Fn(EventValue) -> Result<(), Failure>>;
impl NativeImports {
    pub fn new(runtime: Rc<dyn WsRuntime>, adapter: Arc<dyn RestAdapter>) -> Rc<Self> {
        Rc::new_cyclic(|this| Self {
            timers: NativeTimers::new(runtime.clone()),
            runtime,
            emitter: Rc::new(RefCell::new(None)),
            public_client: RefCell::new(None),
            adapter,
            host: Rc::new(NativeHttpHost),
            registry: TransportRegistry::default(),
            clients: RefCell::new(vec![]),
            this: this.clone(),
        })
    }
    fn register_rc(&self, client: Rc<NativeClient>) -> (Rc<NativeClient>, EventValue) {
        let mut clients = self.clients.borrow_mut();
        clients.push(client.clone());
        (client, record([("nativeClient", number((clients.len() - 1) as f64))]))
    }
    fn client(&self, handle: &EventValue) -> Result<Rc<NativeClient>, Failure> {
        let index = handle.get("nativeClient")?.number()?;
        self.clients
            .borrow()
            .get(index as usize)
            .cloned()
            .ok_or_else(|| failure("unknown native HTTP client handle"))
    }
}

/// An axios instance: defaults, interceptors, retry and rate limit around one HttpClient.
struct NativeClient {
    http: Option<HttpClient>,
    /// createApiWithSecondaryKey's Proxy: `get` goes to the secondary client.
    route: Option<(Rc<NativeClient>, Rc<NativeClient>)>,
    request: RefCell<Vec<RequestInterceptor>>,
    response: RefCell<Vec<ResponseStage>>,
    retry: RefCell<Option<(u32, RetryRead)>>,
    transform: Cell<ResponseTransform>,
    limit: RefCell<Option<RateWindow>>,
    emitter: Rc<RefCell<Option<Emitter>>>,
}
enum ResponseStage {
    Api(ResponseInterceptor, ResponseInterceptor),
    Guard(Fulfilled<EventValue>, Rejected<EventValue, Failure>),
}
/// axios-rate-limit: at most `max` request starts in any `per_ms` window.
struct RateWindow {
    max: usize,
    per_ms: f64,
    starts: VecDeque<f64>,
}
impl NativeClient {
    fn http(&self) -> Result<&HttpClient, Failure> {
        self.http.as_ref().ok_or_else(|| failure("routing client has no transport"))
    }
    async fn wait_for_slot(&self, runtime: &Rc<dyn WsRuntime>) -> Result<(), Failure> {
        loop {
            let delay = {
                let mut limit = self.limit.borrow_mut();
                let Some(window) = limit.as_mut() else { return Ok(()) };
                let now = runtime.monotonic_now_ms();
                while window.starts.front().is_some_and(|t| now - t >= window.per_ms) {
                    window.starts.pop_front();
                }
                if window.starts.len() < window.max {
                    window.starts.push_back(now);
                    return Ok(());
                }
                window.starts.front().map_or(0.0, |t| window.per_ms - (now - t))
            };
            runtime.sleep(delay.max(1.0) as u64).await.map_err(exchange_failure)?;
        }
    }
}
fn method_name(method: HttpMethod) -> &'static str {
    match method {
        HttpMethod::Get => "get",
        HttpMethod::Post => "post",
        HttpMethod::Put => "put",
        HttpMethod::Delete => "delete",
    }
}

async fn run_request(
    imports: Rc<NativeImports>,
    client: Rc<NativeClient>,
    method: HttpMethod,
    path: &'static str,
    args: Option<EventValue>,
) -> Result<EventValue, Failure> {
    if let Some((primary, secondary)) = &client.route {
        let target = if method == HttpMethod::Get { secondary } else { primary };
        return Box::pin(run_request(imports, target.clone(), method, path, args)).await;
    }
    let mut attempt = 0u32;
    loop {
        client.wait_for_slot(&imports.runtime).await?;
        let outcome = dispatch(&client, method, path, args.clone()).await;
        let retry = client.retry.borrow().clone();
        if let (Err(Dispatch::Transport(error, _)), Some((retries, read))) = (&outcome, retry)
            && attempt < retries
            && error.is_retryable_network()
            && read.matches_read(Some(method_name(method)), Some(path))
        {
            attempt += 1;
            continue;
        }
        return respond(&client, outcome).await;
    }
}
enum Dispatch {
    /// Transport failure with the error the request phase raised, if any.
    Transport(RestFailure, Option<Failure>),
}
async fn dispatch(
    client: &Rc<NativeClient>,
    method: HttpMethod,
    path: &'static str,
    args: Option<EventValue>,
) -> Result<(RestResponse, RestRequest), Dispatch> {
    let mut request = RestRequest { method: method_name(method).into(), url: path.into(), ..Default::default() };
    match (method, &args) {
        (HttpMethod::Get | HttpMethod::Delete, Some(config)) if !config.nullish() => {
            let params = config.get("params").ok().filter(|p| !p.nullish());
            request.params = params.map(|p| to_json(&p)).transpose().ok().flatten();
        }
        (HttpMethod::Post | HttpMethod::Put, Some(data)) if !data.nullish() => {
            request.body = to_json(data).ok().map(RestBody::Untransformed);
        }
        _ => {}
    }
    let raised: Rc<RefCell<Option<Failure>>> = Rc::new(RefCell::new(None));
    let http = client.http().map_err(|e| {
        Dispatch::Transport(RestFailure::pre_transmit(e.message.clone(), RestRequest::default()), Some(e))
    })?;
    let interceptors = client.request.borrow().clone();
    let slot = raised.clone();
    let result = http
        .execute_with_interceptor_receipt(request, move |request| async move {
            let original = request.clone();
            let outcome: Result<RestRequest, Failure> = async {
                let mut config = config_value(&request)?;
                let mut bindings = NativeRequestBindings;
                // Axios runs request interceptors last-registered first.
                for interceptor in interceptors.iter().rev() {
                    config = match interceptor(config, &mut bindings)? {
                        RequestReturn::Value(config) => config,
                        RequestReturn::Future(future) => future.await?,
                    };
                }
                rest_request(&config, &request)
            }
            .await;
            outcome.map_err(|error| {
                let message = error.message.clone();
                *slot.borrow_mut() = Some(error);
                RestFailure::pre_transmit(message, original)
            })
        })
        .await;
    match result {
        Ok(execution) => Ok((execution.response, execution.config)),
        Err(error) => Err(Dispatch::Transport(error, raised.borrow_mut().take())),
    }
}
/// The axios config object request interceptors see.
fn config_value(request: &RestRequest) -> Result<EventValue, Failure> {
    let config = record([
        ("url", string(&request.url)),
        ("method", string(&request.method)),
    ]);
    if let Some(base) = &request.base_url {
        config.define("baseURL", string(base));
    }
    if let Some(timeout) = request.timeout_ms {
        config.define("timeout", number(timeout as f64));
    }
    let headers = EventValue::object(Default::default());
    for (name, value) in &request.headers {
        headers.define(name, string(value));
    }
    config.define("headers", headers);
    if let Some(params) = &request.params {
        config.define("params", EventValue::from_json(params.clone()));
    }
    if let Some(body) = &request.body {
        let data = body.untransformed().map_err(|e| failure(e.to_string()))?;
        config.define("data", EventValue::from_json(data.clone()));
    }
    Ok(config)
}
fn rest_request(config: &EventValue, original: &RestRequest) -> Result<RestRequest, Failure> {
    let mut request = original.clone();
    request.url = config.get("url")?.text()?;
    let headers = config.get("headers")?;
    request.headers = Default::default();
    if !headers.nullish() {
        let Value::Object(map) = to_json(&headers)? else {
            return Err(failure("request headers must be an object"));
        };
        for (name, value) in map {
            match value {
                Value::Null | Value::Bool(false) => {}
                Value::String(text) => {
                    request.headers.insert(name, text);
                }
                other => {
                    request.headers.insert(name, other.to_string());
                }
            }
        }
    }
    let params = config.get("params")?;
    request.params = if params.nullish() { None } else { Some(to_json(&params)?) };
    let data = config.get("data")?;
    request.body = if matches!(data, EventValue::Undefined) {
        None
    } else {
        Some(RestBody::Untransformed(to_json(&data)?))
    };
    let timeout = config.get("timeout")?;
    if !timeout.nullish() {
        request.timeout_ms = Some(timeout.number()? as i64);
    }
    request.skip_remote_signing = config.get("safeCexSkipRemoteSigning")?.truthy();
    Ok(request)
}
fn response_data(response: &RestResponse, transform: ResponseTransform) -> Value {
    let text = match &response.body {
        safe_cex_transport::RestResponseBody::Encoded(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        safe_cex_transport::RestResponseBody::Untransformed(value)
        | safe_cex_transport::RestResponseBody::Transformed(value) => return value.clone(),
    };
    let trimmed = text.trim_start_matches('\u{feff}');
    // Axios keeps unparseable text as the string body.
    if serde_json::from_str::<serde::de::IgnoredAny>(trimmed).is_err() {
        return Value::String(text);
    }
    match transform {
        ResponseTransform::BigInt => safe_cex_transport::how::big_int_as_string_json_parse(trimmed),
        ResponseTransform::Default => safe_cex_transport::how::json_parse(trimmed),
    }
}
fn response_value(response: &RestResponse, config: &RestRequest, transform: ResponseTransform) -> Result<EventValue, Failure> {
    let headers = EventValue::object(Default::default());
    for (name, value) in &response.headers {
        headers.define(name, string(value));
    }
    Ok(record([
        ("data", EventValue::from_json(response_data(response, transform))),
        ("status", number(response.status as f64)),
        ("statusText", string(response.reason.clone().unwrap_or_default())),
        ("headers", headers),
        ("config", config_value(config)?),
    ]))
}
fn error_value(error: &Failure) -> EventValue {
    let value = record([
        ("message", string(&error.message)),
        ("name", string(&error.name)),
    ]);
    value.define("code", error.code.clone().map_or(EventValue::Undefined, EventValue::from_json));
    value.define(
        "response",
        error.response.clone().map_or(EventValue::Undefined, EventValue::from_json),
    );
    value
}
async fn respond(
    client: &Rc<NativeClient>,
    outcome: Result<(RestResponse, RestRequest), Dispatch>,
) -> Result<EventValue, Failure> {
    let transform = client.transform.get();
    let mut state: Result<EventValue, Failure> = match outcome {
        Ok((response, config)) => response_value(&response, &config, transform),
        Err(Dispatch::Transport(error, raised)) => Err(raised.unwrap_or_else(|| {
            let mut projected: ExchangeError = error.clone().into();
            if let (Some(response), Some(view)) = (&error.response, projected.response.as_mut()) {
                view["data"] = response_data(response, transform);
            }
            Rc::new(projected)
        })),
    };
    let stages: Vec<_> = client
        .response
        .borrow()
        .iter()
        .map(|stage| match stage {
            ResponseStage::Api(a, b) => ResponseStage::Api(a.clone(), b.clone()),
            ResponseStage::Guard(a, b) => ResponseStage::Guard(a.clone(), b.clone()),
        })
        .collect();
    let mut bindings = NativeResponseBindings { emitter: client.emitter.clone() };
    for stage in stages {
        state = match (stage, state) {
            (ResponseStage::Api(success, _), Ok(response)) => api_outcome(success(response, &mut bindings)?, None),
            (ResponseStage::Api(_, failed), Err(error)) => {
                api_outcome(failed(error_value(&error), &mut bindings)?, Some(error))
            }
            (ResponseStage::Guard(success, _), Ok(response)) => success(response),
            (ResponseStage::Guard(_, rejected), Err(error)) => rejected(error)?.await,
        };
    }
    state.and_then(|response| response.get("data"))
}
fn api_outcome(result: ResponseReturn, original: Option<Failure>) -> Result<EventValue, Failure> {
    match result {
        ResponseReturn::Value(response) => Ok(response),
        ResponseReturn::Rejected(ApiRejection::Original(value)) => Err(original.unwrap_or_else(|| {
            failure(value.get("message").and_then(|m| m.text()).unwrap_or_default())
        })),
        ResponseReturn::Rejected(ApiRejection::New { name, message, code, response }) => {
            let mut error = ExchangeError::new(message);
            error.name = name.into();
            error.code = to_json(&code).ok().filter(|v| !v.is_null());
            error.response = to_json(&response).ok().filter(|v| !v.is_null());
            Err(Rc::new(error))
        }
    }
}

struct NativeRequestBindings;
impl RequestBindings for NativeRequestBindings {
    fn headers_from(&mut self, headers: EventValue) -> Result<EventValue, Failure> {
        Ok(headers)
    }
    fn mark_skip(&mut self, config: EventValue) -> Result<EventValue, Failure> {
        config.define("safeCexSkipRemoteSigning", EventValue::Bool(true));
        Ok(config)
    }
    fn sign(
        &mut self,
        config: EventValue,
        options: EventValue,
        name: EventValue,
        recv_window: EventValue,
    ) -> Result<ApiFuture, Failure> {
        let Value::Object(map) = to_json(&config)? else {
            return Err(failure("request config must be an object"));
        };
        let text = |key: &str| -> Result<String, Failure> {
            let value = options.get(key)?;
            if value.nullish() { Ok(String::new()) } else { value.text() }
        };
        let (key, secret) = (text("key")?, text("secret")?);
        let remote = options.get("remoteSigner")?.truthy();
        let exchange: safe_cex_types::ExchangeName =
            serde_json::from_value(Value::String(name.text()?)).map_err(|e| failure(e.to_string()))?;
        let window = recv_window.number()?;
        Ok(Box::pin(async move {
            let signing = safe_cex_crypto::SigningOptions { key: &key, secret: &secret, payload_signer: None };
            let signed: Map<String, Value> =
                crate::signing::sign_binance_request(&map, &signing, exchange, remote, window, &VIRTUAL_CLOCK)
                    .await
                    .map_err(failure)?;
            Ok(EventValue::from_json(Value::Object(signed)))
        }))
    }
}
struct NativeResponseBindings {
    emitter: Rc<RefCell<Option<Emitter>>>,
}
impl ResponseBindings for NativeResponseBindings {
    fn emit(&mut self, message: EventValue) -> Result<(), Failure> {
        let emit = self.emitter.borrow().clone();
        let emit = emit.ok_or_else(|| failure("response emitted before the exchange bound its emitter"))?;
        emit(message)
    }
}

/// FactoryBindings: each factory call builds one NativeClient.
struct Factorizer<'a> {
    imports: &'a NativeImports,
    pending: Option<NativeClient>,
    handle: EventValue,
}
impl FactoryBindings for Factorizer<'_> {
    fn create(
        &mut self,
        mode: ClientMode,
        config: ApiConfig,
        options: EventValue,
        exchange: EventValue,
    ) -> Result<EventValue, Failure> {
        let typed: safe_cex_types::ExchangeOptions =
            crate::class_decode::decode(options.clone()).map_err(|e| failure(e.to_string()))?;
        let name: safe_cex_types::ExchangeName =
            serde_json::from_value(Value::String(exchange.get("name")?.text()?)).map_err(|e| failure(e.to_string()))?;
        let mut request = RestRequest { base_url: Some(config.base_url.text()?), ..Default::default() };
        if !config.timeout.nullish() {
            request.timeout_ms = Some(config.timeout.number()? as i64);
        }
        if let Value::Object(map) = to_json(&config.headers)? {
            for (k, v) in map {
                if let Value::String(v) = v {
                    request.headers.insert(k, v);
                }
            }
        }
        let runtime: Rc<dyn ExchangeRuntime> = self.imports.runtime.clone();
        let serializer = config.serializer;
        let http_options = HttpClientOptions {
            params_serializer: Some(Arc::new(move |params: &Value| Ok(serializer.serialize(params)))),
            response_transform: None,
            exchange: name,
            options: typed,
            request,
            provider: Some(self.imports.runtime.transport()),
            signer: None,
            account_name: None,
            adapter: self.imports.adapter.clone(),
            host: self.imports.host.clone(),
            runtime: runtime.clone(),
            store: None,
            stats: None,
            logs: None,
            every: vec![],
            periodic: vec![],
            next_id: Arc::new(nanoid),
            signing_now: Arc::new(|| safe_cex_transport::native::wall_clock_ms() as u64),
        };
        let http = match mode {
            ClientMode::Proxy => safe_cex_axios_create(http_options, &self.imports.registry)
                .map_err(|e| failure(e.to_string()))?,
            ClientMode::NoProxy => safe_cex_axios_create_no_proxy(http_options),
        };
        self.pending = Some(NativeClient {
            http: Some(http),
            route: None,
            request: RefCell::new(vec![]),
            response: RefCell::new(vec![]),
            retry: RefCell::new(None),
            transform: Cell::new(config.transform),
            limit: RefCell::new(None),
            emitter: self.imports.emitter.clone(),
        });
        self.handle = record([("pendingClient", EventValue::Bool(true))]);
        Ok(self.handle.clone())
    }
    fn retry(&mut self, _client: EventValue, retries: u32, read: RetryRead) -> Result<(), Failure> {
        let client = self.pending.as_ref().ok_or_else(|| failure("retry before create"))?;
        *client.retry.borrow_mut() = Some((retries, read));
        Ok(())
    }
    fn request_use(&mut self, _client: EventValue, handler: RequestInterceptor) -> Result<(), Failure> {
        let client = self.pending.as_ref().ok_or_else(|| failure("interceptor before create"))?;
        client.request.borrow_mut().push(handler);
        Ok(())
    }
    fn install(
        &mut self,
        _client: EventValue,
        _options: EventValue,
        _name: EventValue,
        _transform: ResponseTransform,
    ) -> Result<(), Failure> {
        Err(failure(
            "tealstreetTunnel needs the Tealstreet transport; the native direct host has no HOW tunnel",
        ))
    }
    fn response_use(
        &mut self,
        _client: EventValue,
        success: ResponseInterceptor,
        failed: ResponseInterceptor,
    ) -> Result<(), Failure> {
        let client = self.pending.as_ref().ok_or_else(|| failure("interceptor before create"))?;
        client.response.borrow_mut().push(ResponseStage::Api(success, failed));
        Ok(())
    }
}

impl ExchangeImports for NativeImports {
    fn factory(&self, venue: Venue, kind: Factory, opts: EventValue, owner: EventValue) -> Result<EventValue, Failure> {
        let mut b = Factorizer { imports: self, pending: None, handle: EventValue::Undefined };
        match (venue, kind) {
            (Venue::Futures, Factory::PublicAPI) => api_factory::create_futures_public_api(opts, owner, &mut b)?,
            (Venue::Futures, _) => api_factory::create_futures_api(opts, owner, &mut b)?,
            (Venue::PM, Factory::PublicAPI) => api_factory::create_pm_public_api(opts, owner, &mut b)?,
            (Venue::PM, Factory::FAPI) => api_factory::create_fapi(opts, owner, &mut b)?,
            (Venue::PM, _) => api_factory::create_papi(opts, owner, &mut b)?,
            (Venue::Spot, _) => api_factory::create_spot_api(opts, owner, &mut b)?,
        };
        let client = b.pending.ok_or_else(|| failure("factory created no client"))?;
        let (client, handle) = self.register_rc(Rc::new(client));
        if kind == Factory::PublicAPI {
            *self.public_client.borrow_mut() = Some(client);
        }
        Ok(handle)
    }
    fn bind_emitter(&self, emit: Rc<dyn Fn(EventValue) -> Result<(), Failure>>) {
        *self.emitter.borrow_mut() = Some(emit);
    }
    fn split(
        &self,
        venue: Venue,
        opts: EventValue,
        _owner: EventValue,
        factory: Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>,
        rate: Option<RateHook>,
    ) -> Result<EventValue, Failure> {
        let mut primary = factory(opts.clone())?;
        if let Some(rate) = &rate {
            primary = rate(primary)?;
        }
        let secondary_key = opts.get("secondary_key")?;
        let triple = |route: EventValue, primary: EventValue, secondary: EventValue| {
            if venue == Venue::Spot {
                route
            } else {
                EventValue::Array(Rc::new(RefCell::new(vec![route, primary, secondary])))
            }
        };
        if !secondary_key.truthy() {
            if venue == Venue::Spot {
                *self.public_client.borrow_mut() = Some(self.client(&primary)?);
            }
            return Ok(triple(primary.clone(), primary.clone(), primary));
        }
        // ts: createApiWithSecondaryKey — secondary_* options replace their unprefixed keys.
        let secondary_options = EventValue::object(opts.spread());
        for (key, value) in opts.spread() {
            if let Some(stripped) = key.strip_prefix("secondary_") {
                secondary_options.define(stripped, value);
            }
        }
        let secondary = factory(secondary_options)?;
        let route = NativeClient {
            http: None,
            route: Some((self.client(&primary)?, self.client(&secondary)?)),
            request: RefCell::new(vec![]),
            response: RefCell::new(vec![]),
            retry: RefCell::new(None),
            transform: Cell::new(ResponseTransform::Default),
            limit: RefCell::new(None),
            emitter: self.emitter.clone(),
        };
        let (route, handle) = self.register_rc(Rc::new(route));
        if venue == Venue::Spot {
            *self.public_client.borrow_mut() = Some(route);
        }
        Ok(triple(handle, primary, secondary))
    }
    fn rate_limit(&self, client: EventValue, max_rps: f64) -> Result<EventValue, Failure> {
        *self.client(&client)?.limit.borrow_mut() =
            Some(RateWindow { max: max_rps.max(1.0) as usize, per_ms: 1000.0, starts: VecDeque::new() });
        Ok(client)
    }
    fn request(
        &self,
        client: EventValue,
        method: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        let target = self.client(&client)?;
        let imports = self.this.upgrade().ok_or_else(|| failure("native imports dropped"))?;
        Ok(Box::pin(async move {
            SnapshotCompletion { reaction_order: 0, result: run_request(imports, target, method, path, args).await }
        }))
    }
    fn register_weight_guard(
        &self,
        client: EventValue,
        fulfilled: Fulfilled<EventValue>,
        rejected: Rejected<EventValue, Failure>,
    ) -> Result<(), Failure> {
        let mut target = self.client(&client)?;
        // Proxy reads fall through to the default (primary) instance's interceptors.
        if let Some((primary, _)) = &target.route {
            target = primary.clone();
        }
        target.response.borrow_mut().push(ResponseStage::Guard(fulfilled, rejected));
        Ok(())
    }
    fn external_provider(
        &self,
        _options: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        Rc::new(UnavailableExternalData(self.runtime.clone()))
    }
    fn private_base(&self, parent: Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure> {
        let ws_parent = parent
            .socket_parent()
            .ok_or_else(|| failure("private parent supplies no socket parent"))?;
        Ok(CorePrivateBase::new(ws_parent, self.runtime.clone()))
    }
    fn listener_operations(&self) -> Rc<dyn ListenerHost> {
        Rc::new(NativeListenerOperations {
            state: ListenerState::default(),
            imports: self.this.clone(),
            timers: self.timers.clone(),
            runtime: self.runtime.clone(),
        })
    }
    fn timers(&self) -> Rc<dyn PrivateTimers> {
        self.timers.clone()
    }
    fn remote_sign(&self, _signer: EventValue) -> Result<RemoteSignCall, Failure> {
        Err(failure("remote signing is not available on the native direct host"))
    }
    fn virtual_now(&self) -> f64 {
        VIRTUAL_CLOCK.get_current_time_ms()
    }
    fn uuid(&self) -> String {
        safe_cex_exchange::native::uuid_v4()
    }
    fn dayjs(&self, value: Option<f64>) -> Result<f64, Failure> {
        Ok(value.unwrap_or_else(|| VIRTUAL_CLOCK.get_current_time_ms()))
    }
    fn subtract(&self, end: f64, amount: f64, unit: char) -> Result<f64, Failure> {
        let ms = match unit {
            's' => 1_000.0,
            'm' => 60_000.0,
            'h' => 3_600_000.0,
            'd' => 86_400_000.0,
            'w' => 604_800_000.0,
            other => return Err(failure(format!("unsupported calendar unit {other}"))),
        };
        Ok(end - amount * ms)
    }
    fn paginate(&self, _fetch: PageCall, _opts: EventValue) -> Result<NativeFuture<EventValue>, Failure> {
        Err(failure("income range paging is not wired on the native host yet"))
    }
    fn iso(&self, time: EventValue) -> Result<String, Failure> {
        iso_string(time.number()?)
    }
    fn fold_closed_positions(&self, _opts: EventValue, _symbol_scoped: bool) -> Result<NativeFuture<EventValue>, Failure> {
        Err(failure("closed-position folding is not wired on the native host yet"))
    }
    fn loop_timer(&self, callback: TickTimer, interval: EventValue) -> Result<(), Failure> {
        let runtime = self.runtime.clone();
        let interval = if interval.nullish() { None } else { Some(interval.number()?) };
        if interval == Some(f64::INFINITY) {
            return Ok(());
        }
        let delay = interval.filter(|v| *v > 0.0).unwrap_or(5_000.0);
        let reporter = runtime.clone();
        self.runtime.spawn_local(Box::pin(async move {
            if runtime.sleep(delay as u64).await.is_err() {
                return;
            }
            // setTimeout(fn, 0): the tick runs on its own scheduler turn.
            let _ = runtime.sleep(0).await;
            let result = match callback() {
                Ok(future) => future.await.map(|_| ()),
                Err(error) => Err(error),
            };
            if let Err(error) = result {
                reporter.report_exception(&error, &serde_json::json!({"operation":"binance tick loop"}));
            }
        }));
        Ok(())
    }
}

/// setTimeout/clearTimeout with numeric tickets.
pub struct NativeTimers {
    runtime: Rc<dyn WsRuntime>,
    next: Cell<f64>,
    live: RefCell<HashMap<u64, Box<dyn ExchangeTask>>>,
    this: Weak<NativeTimers>,
}
impl NativeTimers {
    fn new(runtime: Rc<dyn WsRuntime>) -> Rc<Self> {
        Rc::new_cyclic(|this| Self { runtime, next: Cell::new(0.0), live: RefCell::new(HashMap::new()), this: this.clone() })
    }
    fn live_self(&self) -> Result<Rc<Self>, Failure> {
        self.this.upgrade().ok_or_else(|| failure("timer owner dropped"))
    }
    fn schedule(self: &Rc<Self>, action: Rc<dyn Fn() -> Result<(), Failure>>, delay: f64) -> EventValue {
        let id = self.next.get() + 1.0;
        self.next.set(id);
        let (runtime, timers) = (self.runtime.clone(), Rc::downgrade(self));
        let task = self.runtime.spawn_local(Box::pin(async move {
            if runtime.sleep(delay.max(0.0) as u64).await.is_err() {
                return;
            }
            if let Some(timers) = timers.upgrade() {
                timers.live.borrow_mut().remove(&(id as u64));
            }
            if let Err(error) = action() {
                runtime.report_exception(&error, &serde_json::json!({"operation":"binance timer"}));
            }
        }));
        self.live.borrow_mut().insert(id as u64, task);
        number(id)
    }
    fn cancel(&self, ticket: &EventValue) -> Result<(), Failure> {
        if ticket.nullish() {
            return Ok(());
        }
        let task = self.live.borrow_mut().remove(&(ticket.number()? as u64));
        if let Some(task) = task {
            task.cancel();
        }
        Ok(())
    }
}
impl PrivateTimers for NativeTimers {
    fn set(&self, action: CapturedAction, delay: f64) -> Result<EventValue, Failure> {
        let this = self.live_self()?;
        Ok(this.schedule(Rc::new(move || action.call()), delay))
    }
    fn clear(&self, ticket: EventValue) -> Result<(), Failure> {
        self.cancel(&ticket)
    }
}

/// Exchange-side listener operations for the public socket's listen bodies.
struct NativeListenerOperations {
    state: ListenerState,
    imports: Weak<NativeImports>,
    timers: Rc<NativeTimers>,
    runtime: Rc<dyn WsRuntime>,
}
impl ListenerHost for NativeListenerOperations {
    fn state(&self) -> &ListenerState {
        &self.state
    }
    fn send(&self, _payload: EventValue) -> Result<(), Failure> {
        Err(failure("listener sends belong to the public socket"))
    }
    fn set_timeout(&self, callback: public_listeners::Action, delay: f64) -> Result<EventValue, Failure> {
        Ok(self.timers.live_self()?.schedule(callback, delay))
    }
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure> {
        self.timers.cancel(&ticket)
    }
    fn cleanup(&self, _handler: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn nanoid(&self) -> Result<String, Failure> {
        Ok(nanoid())
    }
    fn get_snapshot(&self, venue: public_listeners::Venue, symbol: &str, limit: f64) -> Result<Snapshot, Failure> {
        let imports = self.imports.upgrade().ok_or_else(|| failure("native imports dropped"))?;
        let client = imports
            .public_client
            .borrow()
            .clone()
            .ok_or_else(|| failure("order book snapshot before the public client exists"))?;
        let path = if venue == public_listeners::Venue::Spot {
            crate::binancespot_types::ENDPOINTS.iter().find(|(k, _)| *k == "ORDERBOOK")
        } else {
            crate::binance_types::ENDPOINTS.iter().find(|(k, _)| *k == "ORDERBOOK")
        }
        .map(|(_, path)| *path)
        .ok_or_else(|| failure("generated ORDERBOOK endpoint"))?;
        let args = record([("params", record([("symbol", string(symbol)), ("limit", number(limit))]))]);
        Ok(Box::pin(run_request(imports, client, HttpMethod::Get, path, Some(args))))
    }
    fn spawn(&self, job: Job) {
        let runtime = self.runtime.clone();
        self.runtime.spawn_local(Box::pin(async move {
            if let Err(error) = job.await {
                runtime.report_exception(&error, &serde_json::json!({"operation":"binance listener job"}));
            }
        }));
    }
    fn reset_sync(&self, sync: SyncState) -> Result<(), Failure> {
        reset_binance_order_book_sync_state(&mut sync.borrow_mut());
        Ok(())
    }
    fn load(&self, book: Book, data: EventValue, sync: SyncState) -> Result<(), Failure> {
        let rows = |key: &str| -> Result<Vec<Vec<String>>, Failure> {
            let EventValue::Array(levels) = data.get(key)? else { return Ok(vec![]) };
            let levels = levels.borrow().clone();
            levels
                .iter()
                .map(|row| Ok(vec![row.get("0")?.text()?, row.get("1")?.text()?]))
                .collect()
        };
        load_binance_order_book_snapshot(
            &mut book.borrow_mut(),
            &rows("bids")?,
            &rows("asks")?,
            data.get("lastUpdateId")?.number()?,
            &mut sync.borrow_mut(),
        );
        Ok(())
    }
    fn buffered(&self, updates: Vec<EventValue>, last_id: EventValue) -> Result<Vec<EventValue>, Failure> {
        let last = last_id.number()?;
        let mut kept = vec![];
        for update in updates {
            if update.get("u")?.number()? > last {
                kept.push(update);
            }
        }
        Ok(kept)
    }
    fn apply(&self, book: Book, data: EventValue, sync: SyncState, previous: bool) -> Result<BinanceOrderBookApplyResult, Failure> {
        let Value::Object(map) = to_json(&data)? else {
            return Err(failure("depth update must be an object"));
        };
        apply_binance_depth_update(&mut book.borrow_mut(), &map, &mut sync.borrow_mut(), previous)
            .map_err(|e| failure(e.message))
    }
    fn total(&self, book: Book) -> Result<(), Failure> {
        safe_cex_store::orderbook::calc_order_book_total(&mut book.borrow_mut());
        Ok(())
    }
}

/// The imported private BaseWebSocket: a core socket (id 3) whose protocol
/// forwards open/message to the listeners the venue registers.
struct CorePrivateBase {
    socket: Rc<BaseWebSocket>,
    listeners: Rc<RefCell<Vec<(&'static str, PrivateCallback)>>>,
    hooks: Rc<RefCell<Option<(Action, Action)>>>,
}
struct ListenerProtocol {
    listeners: Rc<RefCell<Vec<(&'static str, PrivateCallback)>>>,
    hooks: Rc<RefCell<Option<(Action, Action)>>>,
}
fn into_exchange(error: Failure) -> ExchangeError {
    (*error).clone()
}
impl ListenerProtocol {
    fn fire(&self, event: &str, data: Option<EventValue>) -> Result<(), ExchangeError> {
        let targets: Vec<_> = self
            .listeners
            .borrow()
            .iter()
            .filter(|(name, _)| *name == event)
            .map(|(_, callback)| callback.clone())
            .collect();
        for callback in targets {
            match (&callback, &data) {
                (PrivateCallback::Message(f), Some(data)) => f(data.clone()).map_err(into_exchange)?,
                (PrivateCallback::Action(f), _) => f().map_err(into_exchange)?,
                (PrivateCallback::Message(_), None) => {}
            }
        }
        Ok(())
    }
}
impl WsProtocol for ListenerProtocol {
    fn ping(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let hook = self.hooks.borrow().as_ref().map(|(ping, _)| ping.clone());
        hook.map_or(Ok(()), |ping| ping().map_err(into_exchange))
    }
    fn connect_and_subscribe(&self, _socket: Rc<BaseWebSocket>, _is_reconnect: bool) -> Result<(), ExchangeError> {
        let hook = self.hooks.borrow().as_ref().map(|(_, connect)| connect.clone());
        hook.map_or(Ok(()), |connect| connect().map_err(into_exchange))
    }
    fn on_open(&self, _socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self.fire("open", None);
        Box::pin(async move { result })
    }
    fn on_message(&self, _socket: &Rc<BaseWebSocket>, frame: &SocketFrame) -> Result<(), ExchangeError> {
        let data = match frame {
            SocketFrame::Text(text) => string(text.clone()),
            SocketFrame::Binary(bytes) => string(String::from_utf8_lossy(bytes).into_owned()),
            SocketFrame::Close { .. } => return Ok(()),
        };
        self.fire("message", Some(data))
    }
}
impl CorePrivateBase {
    fn new(parent: Rc<dyn safe_cex_exchange::websocket::WsParent>, runtime: Rc<dyn WsRuntime>) -> Rc<Self> {
        let listeners = Rc::new(RefCell::new(vec![]));
        let hooks = Rc::new(RefCell::new(None));
        let protocol = Rc::new(ListenerProtocol { listeners: listeners.clone(), hooks: hooks.clone() });
        Rc::new(Self { socket: BaseWebSocket::new(3, parent, runtime, protocol), listeners, hooks })
    }
}
struct CoreSocketHandle {
    socket: Rc<BaseWebSocket>,
    listeners: Rc<RefCell<Vec<(&'static str, PrivateCallback)>>>,
}
impl NativePrivateSocket for CoreSocketHandle {
    fn remove_listener(&self, event: &'static str, callback: &PrivateCallback) -> Result<(), Failure> {
        self.listeners.borrow_mut().retain(|(name, c)| *name != event || !c.same(callback));
        Ok(())
    }
    fn ready_state(&self) -> Result<f64, Failure> {
        Ok(if self.socket.is_connected() { 1.0 } else if self.socket.has_websocket() { 0.0 } else { 3.0 })
    }
    fn close(&self) -> Result<(), Failure> {
        self.socket.force_close();
        Ok(())
    }
    fn add_listener(&self, event: &'static str, action: PrivateCallback) -> Result<(), Failure> {
        // Close handling stays with the core socket's own close path.
        if event != "close" {
            self.listeners.borrow_mut().push((event, action));
        }
        Ok(())
    }
}
fn ex(result: Result<(), ExchangeError>) -> Result<(), Failure> {
    result.map_err(Rc::new)
}
impl PrivateBase for CorePrivateBase {
    fn disposed(&self) -> bool {
        self.socket.is_disposed()
    }
    fn force_closed(&self) -> bool {
        self.socket.is_force_closed()
    }
    fn connected(&self) -> bool {
        self.socket.is_connected()
    }
    fn track(&self) -> Result<(), Failure> {
        ex(self.socket.track_last_received())
    }
    fn diagnostic(&self, data: EventValue) -> Result<(), Failure> {
        let value = match &data {
            EventValue::String(text) => Value::String(text.clone()),
            other => to_json(other)?,
        };
        ex(self.socket.emit_ws_diagnostic_receive(&value, &Default::default()))
    }
    fn stats(&self, topic: &str) -> Result<(), Failure> {
        ex(self.socket.record_message_handler_stats(topic))
    }
    fn set_ping_at(&self, _time: f64) -> Result<(), Failure> {
        self.socket.mark_ping_sent();
        Ok(())
    }
    fn diagnostic_send(&self, payload: &str) -> Result<(), Failure> {
        ex(self.socket.emit_ws_diagnostic_send(payload, &Default::default()))
    }
    fn pong(&self) -> Result<(), Failure> {
        ex(self.socket.handle_pong_event(5_000))
    }
    fn send(&self, payload: String) -> Result<(), Failure> {
        ex(self.socket.send(&payload))
    }
    fn create(&self, url: String) -> Result<Rc<dyn NativePrivateSocket>, Failure> {
        self.socket.start_websocket(url, vec![], None);
        Ok(Rc::new(CoreSocketHandle { socket: self.socket.clone(), listeners: self.listeners.clone() }))
    }
    fn on_close(&self) -> Result<(), Failure> {
        ex(self.socket.on_close(None, ""))
    }
    fn force_close(&self) -> Result<(), Failure> {
        self.socket.force_close();
        Ok(())
    }
    fn reconnect(&self) -> Result<(), Failure> {
        ex(self.socket.reconnect())
    }
    fn dispose(&self) -> Result<(), Failure> {
        self.socket.dispose();
        Ok(())
    }
    fn emit_error(&self, message: String) -> Result<(), Failure> {
        ex(self.socket.emit_and_log_error(&message, None, None))
    }
    fn install_hooks(&self, ping: Action, connect: Action) {
        *self.hooks.borrow_mut() = Some((ping, connect));
    }
    fn core_socket(&self) -> Option<Rc<BaseWebSocket>> {
        Some(self.socket.clone())
    }
}

/// Date.prototype.toISOString for a millisecond time value.
fn iso_string(ms: f64) -> Result<String, Failure> {
    if !ms.is_finite() || ms.abs() > 8.64e15 {
        let mut error = ExchangeError::new("Invalid time value");
        error.name = "RangeError".into();
        return Err(Rc::new(error));
    }
    let ms = ms.trunc() as i64;
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let year = if (0..=9999).contains(&year) { format!("{year:04}") } else { format!("{year:+07}") };
    Ok(format!(
        "{year}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3_600_000,
        rem / 60_000 % 60,
        rem / 1_000 % 60,
        rem % 1_000
    ))
}

/// externalPublicData on a host with no provider: every use fails visibly.
struct UnavailableExternalData(Rc<dyn WsRuntime>);
const NO_EXTERNAL: &str = "externalPublicData is not available on the native direct host";
impl safe_cex_exchange::external::ExternalPublicData for UnavailableExternalData {
    fn connect(&self) {
        self.0.report_exception(&ExchangeError::new(NO_EXTERNAL), &serde_json::json!({"operation":"external public data"}));
    }
    fn dispose(&self) {}
    fn is_initialized(&self) -> bool {
        false
    }
    fn is_healthy(&self) -> bool {
        false
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        None
    }
    fn listen_ohlcv(
        &self,
        _options: safe_cex_types::OHLCVOptions,
        _callback: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        Err(ExchangeError::new(NO_EXTERNAL))
    }
    fn listen_order_book(
        &self,
        _symbol: String,
        _callback: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        Err(ExchangeError::new(NO_EXTERNAL))
    }
}

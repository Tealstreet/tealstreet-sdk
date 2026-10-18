//! Production `ExchangeImports` for the native host. Requests run the ported
//! factory interceptors inside the shared `HttpClient` over real HTTP; the
//! private socket is a core `BaseWebSocket` (id 3). Requires a tokio `LocalSet`.
use crate::{
    api_factory::{
        self, ApiConfig, ApiFuture, ApiRejection, ClientMode, FactoryBindings, RemoteSignCall,
        RequestBindings, RequestInterceptor, RequestReturn, ResponseBindings, ResponseInterceptor,
        ResponseReturn, ResponseTransform, RetryRead,
    },
    class_jobs::NativeFuture,
    exchange_classes::{ExchangeDependencies, ExchangeImports, ExchangeState, HttpMethod, Method},
    lifecycle::TickTimer,
    orderbook::{
        apply_binance_depth_update, load_binance_order_book_snapshot,
        reset_binance_order_book_sync_state,
    },
    private_classes::{
        Action, CapturedAction, NativePrivateSocket, PrivateBase, PrivateCallback, PrivateParent,
        PrivateTimers,
    },
    private_event_value::{
        EventOps, EventValue, array_values, native_json, number, record, string,
    },
    private_rest::{SnapshotCompletion, SnapshotRequestFuture},
    private_session::{Factory, RateHook, Venue},
    protocol_types::BinanceOrderBookApplyResult,
    public_listeners::{self, Book, Job, ListenerHost, ListenerState, Snapshot, SyncState},
    request_weight::{Failure, Fulfilled, Rejected},
    rest_history::PageCall,
};
use safe_cex_exchange::{
    build_records::{BuildRecordsOptions, FoldableExchange, build_folded_position_records},
    http::{
        HttpClient, HttpClientOptions, HttpHost, install_transport_adapter, safe_cex_axios_create,
        safe_cex_axios_create_no_proxy,
    },
    income_paging::{IncomePageParams, IncomeRangeOptions, paginate_income_range},
    native::{NativeHttpEvents, NativeHttpHost, nanoid},
    runtime::{
        ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeSigners, ExchangeTask, RemoteSigner,
    },
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
};
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use safe_cex_transport::{
    RestAdapter, RestAdapterVariant, RestBody, RestFailure, RestRequest, RestResponse,
    TransportRegistry,
    signature_rejection::is_binance_style_signature_rejection,
    signing::{RemoteSignerRequest, RemoteSignerResponse},
    socket::SocketFrame,
};
use safe_cex_types::{Execution, ExecutionsOptions, IncomeRecord};
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
        .map_err(|e| exchange_failure(e.into()))?
    else {
        return Ok(Value::Null);
    };
    serde_json::from_str(&text).map_err(|e| failure(e.to_string()))
}

/// qs renders BigInt query leaves with String(), independently of JSON bodies.
fn query_json(value: &EventValue) -> Result<Value, Failure> {
    safe_cex_exchange::js_runtime::value::to_json(value)
        .map_err(|error| exchange_failure(error.to_exchange_error()))
}

/// Production Binance dependencies over a native runtime and terminal HTTP adapter.
/// `external` builds the externalPublicData provider for an exchange's base; the
/// registry supplies it, since the providers live above the venue layer.
/// `remote` is the account's hosted signer and its `accountName`, when it has one.
/// `events` is the account's exchange emitter slot for HTTP proxy failures.
pub fn native_dependencies(
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
    external: ExternalFactory,
    remote: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
    events: NativeHttpEvents,
) -> ExchangeDependencies {
    let token = remote.as_ref().map(|(_, account)| match account {
        Some(account) => record([("accountName", string(account.clone()))]),
        None => record([]),
    });
    ExchangeDependencies {
        runtime: runtime.clone(),
        signers: ExchangeSigners {
            remote_signer: remote.as_ref().map(|(signer, _)| signer.clone()),
            payload_signer: None,
        },
        imports: native_imports(runtime, adapter, external, remote, events),
        remote_signer_token: token,
        payload_signer_token: None,
    }
}
/// The hosted signer as the transport's signing capability, for the HTTP client.
struct TransportSigner(Rc<dyn RemoteSigner>);
// Transport adapters deliberately use Arc while native futures remain local.
#[allow(clippy::arc_with_non_send_sync)]
fn transport_signer(
    signer: Rc<dyn RemoteSigner>,
) -> Arc<dyn safe_cex_transport::signing::RemoteSigner> {
    Arc::new(TransportSigner(signer))
}
impl safe_cex_transport::signing::RemoteSigner for TransportSigner {
    fn sign(
        &self,
        request: RemoteSignerRequest,
    ) -> safe_cex_transport::TransportFuture<'_, RemoteSignerResponse> {
        self.0.sign(request)
    }
    fn sign_query(
        &self,
        request: safe_cex_transport::query_signing::HttpSigningRequest,
    ) -> safe_cex_transport::TransportFuture<
        '_,
        safe_cex_transport::query_signing::HttpSigningResponse,
    > {
        self.0.sign_query(request)
    }
}

struct NativeImports {
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
    host: Rc<dyn HttpHost>,
    registry: TransportRegistry,
    clients: RefCell<Vec<Rc<NativeClient>>>,
    timers: Rc<NativeTimers>,
    /// The exchange, bound weakly during construction.
    owner: Rc<RefCell<Weak<ExchangeState>>>,
    external: ExternalFactory,
    remote: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
    /// publicXhr (Futures/PM) or xhr (Spot): the order-book snapshot client.
    public_client: RefCell<Option<Rc<NativeClient>>>,
    this: Weak<NativeImports>,
}
type ExternalFactory = Rc<
    dyn Fn(
        &safe_cex_types::ExchangeOptionsExternalPublicData,
        Rc<safe_cex_exchange::exchange::BaseExchange>,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData>,
>;
fn native_imports(
    runtime: Rc<dyn WsRuntime>,
    adapter: Arc<dyn RestAdapter>,
    external: ExternalFactory,
    remote: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
    events: NativeHttpEvents,
) -> Rc<NativeImports> {
    Rc::new_cyclic(|this| NativeImports {
        external,
        remote,
        timers: native_timers(runtime.clone()),
        runtime,
        owner: Rc::new(RefCell::new(Weak::new())),
        public_client: RefCell::new(None),
        adapter,
        host: Rc::new(NativeHttpHost { events }),
        registry: TransportRegistry::default(),
        clients: RefCell::new(vec![]),
        this: this.clone(),
    })
}
fn register_client(
    this: &NativeImports,
    client: Rc<NativeClient>,
) -> (Rc<NativeClient>, EventValue) {
    let mut clients = this.clients.borrow_mut();
    clients.push(client.clone());
    (
        client,
        record([("nativeClient", number((clients.len() - 1) as f64))]),
    )
}
fn lookup_client(this: &NativeImports, handle: &EventValue) -> Result<Rc<NativeClient>, Failure> {
    let index = handle.read("nativeClient")?.number()?;
    this.clients
        .borrow()
        .get(index as usize)
        .cloned()
        .ok_or_else(|| failure("unknown native HTTP client handle"))
}

/// An axios instance: defaults, interceptors, retry and rate limit around one HttpClient.
struct NativeClient {
    http: Option<HttpClient>,
    /// createApiWithSecondaryKey's Proxy: `get` goes to the secondary client.
    route: Option<(Rc<NativeClient>, Rc<NativeClient>)>,
    request: RefCell<Vec<RequestInterceptor>>,
    response: RefCell<Vec<ResponseStage>>,
    /// (retries, read classification, signature-rejection arm).
    retry: RefCell<Option<(u32, RetryRead, bool)>>,
    transform: Cell<ResponseTransform>,
    limit: RefCell<Option<RateWindow>>,
    owner: Rc<RefCell<Weak<ExchangeState>>>,
}
enum ResponseStage {
    Api(ResponseInterceptor, Option<ResponseInterceptor>),
    Guard(Fulfilled<EventValue>, Rejected<EventValue, Failure>),
}
/// axios-rate-limit: at most `max` request starts in any `per_ms` window.
struct RateWindow {
    max: usize,
    per_ms: f64,
    starts: VecDeque<f64>,
}
fn client_http(this: &NativeClient) -> Result<&HttpClient, Failure> {
    this.http
        .as_ref()
        .ok_or_else(|| failure("routing client has no transport"))
}
async fn wait_for_slot(this: &NativeClient, runtime: &Rc<dyn WsRuntime>) -> Result<(), Failure> {
    loop {
        let delay = {
            let mut limit = this.limit.borrow_mut();
            let Some(window) = limit.as_mut() else {
                return Ok(());
            };
            let now = runtime.monotonic_now_ms();
            while window
                .starts
                .front()
                .is_some_and(|t| now - t >= window.per_ms)
            {
                window.starts.pop_front();
            }
            if window.starts.len() < window.max {
                window.starts.push_back(now);
                return Ok(());
            }
            window
                .starts
                .front()
                .map_or(0.0, |t| window.per_ms - (now - t))
        };
        runtime
            .sleep(delay.max(1.0) as u64)
            .await
            .map_err(exchange_failure)?;
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
        let target = if method == HttpMethod::Get {
            secondary
        } else {
            primary
        };
        return Box::pin(run_request(imports, target.clone(), method, path, args)).await;
    }
    let mut attempt = 0u32;
    // The config stamp: one signature resend per call, by retry or by interceptor.
    let mut signature_retried = false;
    loop {
        wait_for_slot(&client, &imports.runtime).await?;
        let outcome = dispatch(&client, method, path, args.clone()).await;
        let retry = client.retry.borrow().clone();
        if let (Err(Dispatch::Transport(error, _)), Some((retries, read, signature))) =
            (&outcome, retry)
            && attempt < retries
        {
            let network = error.is_retryable_network()
                && read.matches_read(Some(method_name(method)), Some(path));
            let rejected = signature
                && !signature_retried
                && match &error.response {
                    Some(response) => is_binance_style_signature_rejection(
                        &response_data(response, client.transform.get())
                            .into_value()
                            .map_err(|error| failure(error.to_string()))?,
                    )
                    .map_err(|error| exchange_failure(error.into()))?,
                    None => false,
                };
            if network || rejected {
                signature_retried |= rejected;
                attempt += 1;
                continue;
            }
        }
        match respond(&client, outcome, !signature_retried).await? {
            Some(data) => return Ok(data),
            None => signature_retried = true,
        }
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
    let request = RestRequest {
        method: method_name(method).into(),
        url: path.into(),
        ..Default::default()
    };
    let raised: Rc<RefCell<Option<Failure>>> = Rc::new(RefCell::new(None));
    let http = client_http(client).map_err(|e| {
        Dispatch::Transport(
            RestFailure::pre_transmit(e.message.clone(), RestRequest::default()),
            Some(e),
        )
    })?;
    let interceptors = client.request.borrow().clone();
    let slot = raised.clone();
    let result = http
        .execute_with_interceptor_receipt(request, move |request| async move {
            let original = request.clone();
            let outcome: Result<RestRequest, Failure> = async {
                let mut config = config_value(&request)?;
                // Request interceptors must observe the original arguments before
                // signing moves data into params or JSON transformation can fail.
                if let Some(value) = args.as_ref().filter(|value| !value.nullish()) {
                    config.write(
                        if matches!(method, HttpMethod::Get | HttpMethod::Delete) {
                            "params"
                        } else {
                            "data"
                        },
                        value.clone(),
                    );
                }
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
        config.write("baseURL", string(base));
    }
    if let Some(timeout) = request.timeout_ms {
        config.write("timeout", number(timeout as f64));
    }
    let headers = EventValue::record_object(Default::default());
    for (name, value) in &request.headers {
        headers.write(name, string(value));
    }
    config.write("headers", headers);
    if let Some(params) = &request.params {
        config.write("params", EventValue::from_json(params.clone()));
    }
    if let Some(body) = &request.body {
        let data = body.untransformed().map_err(|e| failure(e.to_string()))?;
        config.write(
            "data",
            safe_cex_exchange::js_runtime::value::from_retained_input(data.clone()),
        );
    }
    Ok(config)
}
fn rest_request(config: &EventValue, original: &RestRequest) -> Result<RestRequest, Failure> {
    let mut request = original.clone();
    request.url = config.read("url")?.text()?;
    let headers = config.read("headers")?;
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
    let params = config.read("params")?;
    request.params = if params.nullish() {
        None
    } else {
        Some(query_json(&params)?)
    };
    let data = config.read("data")?;
    request.body = if matches!(data, EventValue::Undefined) {
        None
    } else {
        Some(RestBody::Untransformed(
            safe_cex_exchange::js_runtime::value::to_retained_input(&data)
                .map_err(|error| exchange_failure(error.to_exchange_error()))?,
        ))
    };
    let timeout = config.read("timeout")?;
    if !timeout.nullish() {
        request.timeout_ms = Some(timeout.number()? as i64);
    }
    request.skip_remote_signing = config.read("safeCexSkipRemoteSigning")?.truthy();
    Ok(request)
}
fn response_data(
    response: &RestResponse,
    transform: ResponseTransform,
) -> safe_cex_types::retained_json::RetainedJsonInput {
    let text = match &response.body {
        safe_cex_transport::RestResponseBody::Encoded(bytes) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        safe_cex_transport::RestResponseBody::Untransformed(value)
        | safe_cex_transport::RestResponseBody::Transformed(value) => return value.clone(),
    };
    let trimmed = text.trim_start_matches('\u{feff}');
    // Axios keeps unparseable text as the string body.
    if serde_json::from_str::<serde::de::IgnoredAny>(trimmed).is_err() {
        return safe_cex_types::retained_json::RetainedJsonInput::Text(
            text.encode_utf16().collect(),
        );
    }
    match transform {
        ResponseTransform::BigInt => safe_cex_transport::how::big_int_as_string_json_parse(trimmed),
        ResponseTransform::Default => safe_cex_transport::how::json_parse(trimmed),
    }
}
fn response_value(
    response: &RestResponse,
    config: &RestRequest,
    transform: ResponseTransform,
) -> Result<EventValue, Failure> {
    let headers = EventValue::record_object(Default::default());
    for (name, value) in &response.headers {
        headers.write(name, string(value));
    }
    let headers = safe_cex_exchange::js_runtime::client::axios_headers_from(&headers)
        .map_err(|error| exchange_failure(error.into()))?;
    Ok(record([
        (
            "data",
            safe_cex_exchange::js_runtime::value::from_retained_input(response_data(
                response, transform,
            )),
        ),
        ("status", number(response.status as f64)),
        (
            "statusText",
            string(response.reason.clone().unwrap_or_default()),
        ),
        ("headers", headers),
        ("config", config_value(config)?),
    ]))
}
fn response_from_error(error: &Failure) -> Result<Option<EventValue>, Failure> {
    error
        .response
        .clone()
        .map(|response| {
            let response = EventValue::from_json(response);
            if !response.nullish() {
                let headers = response.read("headers")?;
                if !headers.nullish() {
                    let headers =
                        safe_cex_exchange::js_runtime::client::axios_headers_from(&headers)
                            .map_err(|error| exchange_failure(error.into()))?;
                    response.write("headers", headers);
                }
            }
            Ok(response)
        })
        .transpose()
}
fn error_value(error: &Failure) -> Result<EventValue, Failure> {
    let value = record([
        ("message", string(&error.message)),
        ("name", string(&error.name)),
    ]);
    value.write("code", error.code.clone().unwrap_or(EventValue::Undefined));
    value.write(
        "response",
        response_from_error(error)?.unwrap_or(EventValue::Undefined),
    );
    Ok(value)
}
/// None asks the caller to resend once: a response interceptor saw a signature rejection.
async fn respond(
    client: &Rc<NativeClient>,
    outcome: Result<(RestResponse, RestRequest), Dispatch>,
    resend_allowed: bool,
) -> Result<Option<EventValue>, Failure> {
    let transform = client.transform.get();
    let mut state: Result<EventValue, Failure> = match outcome {
        Ok((response, config)) => response_value(&response, &config, transform),
        Err(Dispatch::Transport(error, raised)) => Err(raised.unwrap_or_else(|| {
            let mut projected: ExchangeError = error.clone().into();
            if let (Some(response), Some(view)) = (&error.response, projected.response.as_mut()) {
                view["data"] = safe_cex_exchange::runtime::response_json_view(
                    &safe_cex_transport::RestResponseBody::Transformed(response_data(
                        response, transform,
                    )),
                );
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
    let mut bindings = NativeResponseBindings {
        owner: client.owner.clone(),
        resend_allowed,
        resend_requested: false,
    };
    for stage in stages {
        state = match (stage, state) {
            (ResponseStage::Api(success, _), Ok(response)) => {
                let outcome = success(response, &mut bindings)?;
                if bindings.resend_requested {
                    return Ok(None);
                }
                api_outcome(outcome, None)
            }
            (ResponseStage::Api(_, Some(failed)), Err(error)) => {
                api_outcome(failed(error_value(&error)?, &mut bindings)?, Some(error))
            }
            (ResponseStage::Api(_, None), Err(error)) => Err(error),
            (ResponseStage::Guard(success, _), Ok(response)) => success(response),
            (ResponseStage::Guard(_, rejected), Err(error)) => rejected(error)?.await,
        };
    }
    state.and_then(|response| response.read("data")).map(Some)
}
fn api_outcome(result: ResponseReturn, original: Option<Failure>) -> Result<EventValue, Failure> {
    match result {
        ResponseReturn::Value(response) => Ok(response),
        ResponseReturn::Rejected(ApiRejection::Original(value)) => {
            Err(original.unwrap_or_else(|| {
                failure(
                    value
                        .read("message")
                        .and_then(|m| m.text())
                        .unwrap_or_default(),
                )
            }))
        }
        ResponseReturn::Rejected(ApiRejection::New {
            name,
            message,
            code,
            response,
            emitted: _,
        }) => {
            let mut error = ExchangeError::new(message);
            error.name = name.into();
            error.code = (!code.is_undefined()).then_some(code);
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
        config.write("safeCexSkipRemoteSigning", EventValue::Bool(true));
        Ok(config)
    }
    fn sign(
        &mut self,
        config: EventValue,
        options: EventValue,
        name: EventValue,
        recv_window: EventValue,
    ) -> Result<ApiFuture, Failure> {
        let text = |key: &str| -> Result<String, Failure> {
            let value = options.get(key)?;
            if value.nullish() {
                Ok(String::new())
            } else {
                value.text()
            }
        };
        let (key, secret) = (text("key")?, text("secret")?);
        let remote = options.read("remoteSigner")?.truthy();
        let exchange: safe_cex_types::ExchangeName =
            serde_json::from_value(Value::String(name.text()?))
                .map_err(|e| failure(e.to_string()))?;
        let window = recv_window.number()?;
        if remote
            || key.is_empty()
            || !safe_cex_crypto::can_sign_payload(&safe_cex_crypto::SigningOptions {
                key: &key,
                secret: &secret,
                payload_signer: None,
            })
        {
            return Ok(Box::pin(async move { Ok(config) }));
        }
        // A signed body becomes query fields; BigInt uses qs decimal spelling.
        // Keep the original config untouched when signing is skipped.
        let Value::Object(map) = query_json(&config)? else {
            return Err(failure("request config must be an object"));
        };
        Ok(Box::pin(async move {
            let signing = safe_cex_crypto::SigningOptions {
                key: &key,
                secret: &secret,
                payload_signer: None,
            };
            let signed: Map<String, Value> = crate::signing::sign_binance_request(
                &map,
                &signing,
                exchange,
                remote,
                window,
                &VIRTUAL_CLOCK,
            )
            .await
            .map_err(failure)?;
            Ok(EventValue::from_json(Value::Object(signed)))
        }))
    }
}
struct NativeResponseBindings {
    owner: Rc<RefCell<Weak<ExchangeState>>>,
    resend_allowed: bool,
    resend_requested: bool,
}
impl ResponseBindings for NativeResponseBindings {
    fn emit(&mut self, message: EventValue) -> Result<(), Failure> {
        let owner = self.owner.borrow().upgrade();
        owner
            .ok_or_else(|| failure("response emitted with no live exchange bound"))?
            .emit(message)
    }
    // The truthy marker ends this pass; run_request re-dispatches the request once.
    fn resend(&mut self, response: EventValue) -> Result<EventValue, Failure> {
        if self.resend_allowed
            && is_binance_style_signature_rejection(&to_json(&response.read("data")?)?)
                .map_err(|error| exchange_failure(error.into()))?
        {
            self.resend_requested = true;
            return Ok(EventValue::Bool(true));
        }
        Ok(EventValue::Undefined)
    }
}

fn typed_options(options: &EventValue) -> Result<safe_cex_types::ExchangeOptions, Failure> {
    // The signer is host-held (`imports.remote`); typed options cannot name it.
    let view = match options {
        EventValue::Object(object) => {
            let view = EventValue::record_object(
                options
                    .spread()
                    .into_iter()
                    .filter(|(key, _)| key != "remoteSigner")
                    .collect(),
            );
            if let EventValue::Object(clone) = &view {
                *clone.proto.borrow_mut() = object.proto.borrow().clone();
            }
            view
        }
        other => other.clone(),
    };
    crate::class_decode::decode(view).map_err(|e| failure(e.to_string()))
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
        let typed = typed_options(&options)?;
        let name: safe_cex_types::ExchangeName =
            serde_json::from_value(Value::String(exchange.read("name")?.text()?))
                .map_err(|e| failure(e.to_string()))?;
        let mut request = RestRequest {
            base_url: Some(config.base_url.text()?),
            ..Default::default()
        };
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
            params_serializer: Some(Arc::new(move |params: &Value| {
                Ok(serializer.serialize(params))
            })),
            response_transform: None,
            exchange: name,
            options: typed,
            request,
            provider: Some(self.imports.runtime.transport()),
            signer: self
                .imports
                .remote
                .as_ref()
                .map(|(signer, _)| transport_signer(signer.clone())),
            account_name: self.imports.remote.as_ref().and_then(|(_, a)| a.clone()),
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
            owner: self.imports.owner.clone(),
        });
        self.handle = record([("pendingClient", EventValue::Bool(true))]);
        Ok(self.handle.clone())
    }
    fn retry(
        &mut self,
        _client: EventValue,
        retries: u32,
        read: RetryRead,
        signature: bool,
    ) -> Result<(), Failure> {
        let client = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("retry before create"))?;
        *client.retry.borrow_mut() = Some((retries, read, signature));
        Ok(())
    }
    fn request_use(
        &mut self,
        _client: EventValue,
        handler: RequestInterceptor,
    ) -> Result<(), Failure> {
        let client = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("interceptor before create"))?;
        client.request.borrow_mut().push(handler);
        Ok(())
    }
    // ts: installTransportAdapter(xhr, options, exchange.name, 'bigInt' | default)
    fn install(
        &mut self,
        _client: EventValue,
        options: EventValue,
        _name: EventValue,
        transform: ResponseTransform,
    ) -> Result<(), Failure> {
        let http = self
            .pending
            .as_ref()
            .and_then(|client| client.http.as_ref())
            .ok_or_else(|| failure("install before create"))?;
        let variant = match transform {
            ResponseTransform::BigInt => RestAdapterVariant::BigInt,
            ResponseTransform::Default => RestAdapterVariant::Default,
        };
        install_transport_adapter(
            http,
            &self.imports.registry,
            &typed_options(&options)?,
            Some(&self.imports.runtime.transport()),
            variant,
        );
        Ok(())
    }
    fn response_use(
        &mut self,
        _client: EventValue,
        success: ResponseInterceptor,
        failed: Option<ResponseInterceptor>,
    ) -> Result<(), Failure> {
        let client = self
            .pending
            .as_ref()
            .ok_or_else(|| failure("interceptor before create"))?;
        client
            .response
            .borrow_mut()
            .push(ResponseStage::Api(success, failed));
        Ok(())
    }
}

impl ExchangeImports for NativeImports {
    fn weight_error_response(&self, error: &Failure) -> Result<Option<EventValue>, Failure> {
        response_from_error(error)
    }
    fn weight_header_get_member(
        &self,
        headers: &EventValue,
    ) -> Result<crate::request_weight::HeaderGetMember, Failure> {
        use crate::request_weight::HeaderGetMember;
        let get = headers.read("get")?;
        if !get.is_callable() {
            return Ok(HeaderGetMember::NotCallable);
        }
        let receiver = headers.clone();
        Ok(HeaderGetMember::Callable(Rc::new(move |name| {
            let value = get
                .call(receiver.clone(), vec![string(name)])
                .map_err(|error| exchange_failure(error.into()))?;
            if value.nullish() {
                Ok(None)
            } else {
                to_json(&value).map(Some)
            }
        })))
    }
    fn factory(
        &self,
        venue: Venue,
        kind: Factory,
        opts: EventValue,
        owner: EventValue,
    ) -> Result<EventValue, Failure> {
        let mut b = Factorizer {
            imports: self,
            pending: None,
            handle: EventValue::Undefined,
        };
        match (venue, kind) {
            (Venue::Futures, Factory::PublicAPI) => {
                api_factory::create_futures_public_api(opts, owner, &mut b)?
            }
            (Venue::Futures, _) => api_factory::create_futures_api(opts, owner, &mut b)?,
            (Venue::PM, Factory::PublicAPI) => {
                api_factory::create_pm_public_api(opts, owner, &mut b)?
            }
            (Venue::PM, Factory::FAPI) => api_factory::create_fapi(opts, owner, &mut b)?,
            (Venue::PM, _) => api_factory::create_papi(opts, owner, &mut b)?,
            (Venue::Spot, _) => api_factory::create_spot_api(opts, owner, &mut b)?,
        };
        let client = b
            .pending
            .ok_or_else(|| failure("factory created no client"))?;
        let (client, handle) = register_client(self, Rc::new(client));
        if kind == Factory::PublicAPI {
            *self.public_client.borrow_mut() = Some(client);
        }
        Ok(handle)
    }
    fn bind_owner(&self, owner: Weak<ExchangeState>) {
        *self.owner.borrow_mut() = owner;
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
        let secondary_key = opts.read("secondary_key")?;
        let triple = |route: EventValue, primary: EventValue, secondary: EventValue| {
            if venue == Venue::Spot {
                route
            } else {
                EventValue::array(vec![route, primary, secondary])
            }
        };
        if !secondary_key.truthy() {
            if venue == Venue::Spot {
                *self.public_client.borrow_mut() = Some(lookup_client(self, &primary)?);
            }
            return Ok(triple(primary.clone(), primary.clone(), primary));
        }
        // ts: createApiWithSecondaryKey — secondary_* options replace their unprefixed keys.
        let secondary_options = EventValue::record_object(opts.spread());
        for (key, value) in opts.spread() {
            if let Some(stripped) = key.strip_prefix("secondary_") {
                secondary_options.write(stripped, value);
            }
        }
        let secondary = factory(secondary_options)?;
        let route = NativeClient {
            http: None,
            route: Some((
                lookup_client(self, &primary)?,
                lookup_client(self, &secondary)?,
            )),
            request: RefCell::new(vec![]),
            response: RefCell::new(vec![]),
            retry: RefCell::new(None),
            transform: Cell::new(ResponseTransform::Default),
            limit: RefCell::new(None),
            owner: self.owner.clone(),
        };
        let (route, handle) = register_client(self, Rc::new(route));
        if venue == Venue::Spot {
            *self.public_client.borrow_mut() = Some(route);
        }
        Ok(triple(handle, primary, secondary))
    }
    fn rate_limit(&self, client: EventValue, max_rps: f64) -> Result<EventValue, Failure> {
        *lookup_client(self, &client)?.limit.borrow_mut() = Some(RateWindow {
            max: max_rps.max(1.0) as usize,
            per_ms: 1000.0,
            starts: VecDeque::new(),
        });
        Ok(client)
    }
    fn request(
        &self,
        client: EventValue,
        method: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure> {
        let target = lookup_client(self, &client)?;
        let imports = self
            .this
            .upgrade()
            .ok_or_else(|| failure("native imports dropped"))?;
        let request = crate::class_jobs::start(
            self.runtime.clone(),
            Box::pin(run_request(imports, target, method, path, args)),
        );
        Ok(Box::pin(async move {
            SnapshotCompletion {
                reaction_order: 0,
                result: request.await,
            }
        }))
    }
    fn register_weight_guard(
        &self,
        client: EventValue,
        fulfilled: Fulfilled<EventValue>,
        rejected: Rejected<EventValue, Failure>,
    ) -> Result<(), Failure> {
        let mut target = lookup_client(self, &client)?;
        // Proxy reads fall through to the default (primary) instance's interceptors.
        if let Some((primary, _)) = &target.route {
            target = primary.clone();
        }
        target
            .response
            .borrow_mut()
            .push(ResponseStage::Guard(fulfilled, rejected));
        Ok(())
    }
    fn external_provider(
        &self,
        options: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        match self.owner.borrow().upgrade() {
            Some(owner) => (self.external)(options, owner.base.clone()),
            None => Rc::new(UnavailableExternalData(self.runtime.clone())),
        }
    }
    fn private_base(&self, parent: Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure> {
        let ws_parent = parent
            .socket_parent()
            .ok_or_else(|| failure("private parent supplies no socket parent"))?;
        Ok(core_private_base(ws_parent, self.runtime.clone()))
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
        let (signer, _) = self
            .remote
            .clone()
            .ok_or_else(|| failure("remoteSigner is set but the host supplied no signer"))?;
        Ok(Rc::new(move |request: EventValue| {
            let signer = signer.clone();
            let text =
                serde_json::to_string(&to_json(&request)?).map_err(|e| failure(e.to_string()))?;
            let request: RemoteSignerRequest =
                serde_json::from_str(&text).map_err(|e| failure(e.to_string()))?;
            Ok(Box::pin(async move {
                let response = signer
                    .sign(request)
                    .await
                    .map_err(|e| failure(e.to_string()))?;
                let value = serde_json::to_value(&response).map_err(|e| failure(e.to_string()))?;
                Ok(EventValue::from_json(value))
            }) as ApiFuture)
        }))
    }
    fn virtual_now(&self) -> f64 {
        VIRTUAL_CLOCK.get_current_time_ms()
    }
    fn uuid(&self) -> String {
        safe_cex_crypto::uuid::uuid().expect("operating system entropy is available")
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
    fn paginate(
        &self,
        fetch: PageCall,
        opts: EventValue,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        let required = |key: &str| -> Result<f64, Failure> { opts.get(key)?.number() };
        let optional = |key: &str| -> Result<Option<f64>, Failure> {
            let value = opts.get(key)?;
            if value.nullish() {
                Ok(None)
            } else {
                value.number().map(Some)
            }
        };
        let options = IncomeRangeOptions {
            start_time: required("startTime")?,
            end_time: required("endTime")?,
            chunk_ms: required("chunkMs")?,
            page_limit: required("pageLimit")?,
            chunk_delay_ms: optional("chunkDelayMs")?,
            max_pages_per_chunk: optional("maxPagesPerChunk")?,
        };
        let runtime = self.runtime.clone();
        Ok(Box::pin(async move {
            // ts: paginateIncomeRange — the shared pager over this venue's rethrowing page fetch.
            let mut fetch_page =
                move |page: IncomePageParams| -> ExchangeFuture<'static, Vec<IncomeRecord>> {
                    let call = fetch(record([
                        ("startTime", number(page.start_time)),
                        ("endTime", number(page.end_time)),
                        ("limit", number(page.limit)),
                    ]));
                    Box::pin(async move {
                        let rows = call.map_err(into_exchange)?.await.map_err(into_exchange)?;
                        crate::exchange_classes::income_records(&rows).map_err(into_exchange)
                    })
                };
            let mut delay = move |ms: f64| runtime.sleep(ms.max(0.0) as u64);
            let result = paginate_income_range(&mut fetch_page, options, &mut delay)
                .await
                .map_err(Rc::new)?;
            Ok(record([
                (
                    "records",
                    safe_cex_exchange::js_runtime::value::from_typed(&result.records)
                        .map_err(|error| exchange_failure(error.to_exchange_error()))?,
                ),
                ("truncated", EventValue::Bool(result.truncated)),
                ("exhaustedChunks", number(result.exhausted_chunks)),
                ("requests", number(result.requests)),
            ]))
        }))
    }
    fn iso(&self, time: EventValue) -> Result<String, Failure> {
        iso_string(time.number()?)
    }
    fn fold_closed_positions(
        &self,
        opts: EventValue,
        symbol_scoped: bool,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        // ts: BaseExchange#foldClosedPositionRecords over buildFoldedPositionRecords.
        let owner = self
            .owner
            .borrow()
            .upgrade()
            .ok_or_else(|| failure("fold with no live exchange bound"))?;
        let account = opts.read("account")?.text()?;
        let bound = |key: &str| -> Result<Option<f64>, Failure> {
            let value = opts.get(key)?;
            if value.nullish() {
                Ok(None)
            } else {
                value.number().map(Some)
            }
        };
        let (from, to) = (bound("from")?, bound("to")?);
        let now = self.runtime.now_ms();
        Ok(Box::pin(async move {
            let to = to.unwrap_or(now);
            let from = from.unwrap_or(to - 30.0 * 24.0 * 60.0 * 60.0 * 1000.0);
            let (fills_owner, income_owner) = (owner.clone(), owner.clone());
            let store = owner.base.store.clone();
            let mut settle = move |symbol: &str| {
                store
                    .markets_by_symbol()
                    .get(symbol)
                    .map(|market| market.quote.clone())
                    .filter(|quote| !quote.is_empty())
                    .unwrap_or_else(|| "USDT".into())
            };
            let mut exchange = FoldableExchange {
                fetch_historical_fills: &mut move |options: ExecutionsOptions| {
                    let owner = fills_owner.clone();
                    Box::pin(async move {
                        let args = vec![to_event(&options).map_err(into_exchange)?];
                        let rows = crate::class_jobs::reaction(
                            owner.invoke(Method::HistoricalFills, args),
                        )
                        .await
                        .map_err(into_exchange)?;
                        crate::class_decode::decode::<Vec<Execution>>(rows)
                            .map_err(|e| ExchangeError::new(e.to_string()))
                    })
                },
                fetch_income_history_range: Some(&mut move |from: f64, to: f64| {
                    let owner = income_owner.clone();
                    Box::pin(async move {
                        let args = vec![number(from), number(to), EventValue::Undefined];
                        let rows =
                            crate::class_jobs::reaction(owner.invoke(Method::IncomeRange, args))
                                .await
                                .map_err(into_exchange)?;
                        crate::exchange_classes::income_records(&rows).map_err(into_exchange)
                    })
                }),
            };
            let mut options = BuildRecordsOptions {
                account: &account,
                from,
                to,
                symbol_scoped,
                resolve_settle_currency: &mut settle,
            };
            let result = build_folded_position_records(&mut exchange, &mut options)
                .await
                .map_err(|e| Rc::new(e.error))?;
            serde_json::to_value(&result.records)
                .map(EventValue::from_json)
                .map_err(|e| failure(e.to_string()))
        }))
    }
    fn loop_timer(&self, callback: TickTimer, interval: EventValue) -> Result<(), Failure> {
        let runtime = self.runtime.clone();
        let interval = if interval.nullish() {
            None
        } else {
            Some(interval.number()?)
        };
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
                reporter.report_exception(
                    &error,
                    &serde_json::json!({"operation":"binance tick loop"}).into(),
                );
            }
        }));
        Ok(())
    }
}

/// setTimeout/clearTimeout with numeric tickets.
struct NativeTimers {
    runtime: Rc<dyn WsRuntime>,
    next: Cell<f64>,
    live: RefCell<HashMap<u64, Box<dyn ExchangeTask>>>,
    this: Weak<NativeTimers>,
}
fn native_timers(runtime: Rc<dyn WsRuntime>) -> Rc<NativeTimers> {
    Rc::new_cyclic(|this| NativeTimers {
        runtime,
        next: Cell::new(0.0),
        live: RefCell::new(HashMap::new()),
        this: this.clone(),
    })
}
fn timers_self(this: &NativeTimers) -> Result<Rc<NativeTimers>, Failure> {
    this.this
        .upgrade()
        .ok_or_else(|| failure("timer owner dropped"))
}
fn schedule_timer(
    this: &Rc<NativeTimers>,
    action: Rc<dyn Fn() -> Result<(), Failure>>,
    delay: f64,
) -> EventValue {
    let id = this.next.get() + 1.0;
    this.next.set(id);
    let (runtime, timers) = (this.runtime.clone(), Rc::downgrade(this));
    let task = this.runtime.spawn_local(Box::pin(async move {
        if runtime.sleep(delay.max(0.0) as u64).await.is_err() {
            return;
        }
        if let Some(timers) = timers.upgrade() {
            timers.live.borrow_mut().remove(&(id as u64));
        }
        if let Err(error) = action() {
            runtime.report_exception(
                &error,
                &serde_json::json!({"operation":"binance timer"}).into(),
            );
        }
    }));
    this.live.borrow_mut().insert(id as u64, task);
    number(id)
}
fn cancel_timer(this: &NativeTimers, ticket: &EventValue) -> Result<(), Failure> {
    if ticket.nullish() {
        return Ok(());
    }
    let task = this.live.borrow_mut().remove(&(ticket.number()? as u64));
    if let Some(task) = task {
        task.cancel();
    }
    Ok(())
}

impl PrivateTimers for NativeTimers {
    fn set(&self, action: CapturedAction, delay: f64) -> Result<EventValue, Failure> {
        let this = timers_self(&self)?;
        Ok(schedule_timer(&this, Rc::new(move || action.call()), delay))
    }
    fn clear(&self, ticket: EventValue) -> Result<(), Failure> {
        cancel_timer(&self, &ticket)
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
    fn set_timeout(
        &self,
        callback: public_listeners::Action,
        delay: f64,
    ) -> Result<EventValue, Failure> {
        Ok(schedule_timer(&timers_self(&self.timers)?, callback, delay))
    }
    fn clear_timeout(&self, ticket: EventValue) -> Result<(), Failure> {
        cancel_timer(&self.timers, &ticket)
    }
    fn cleanup(&self, _handler: &str) -> Result<(), Failure> {
        Ok(())
    }
    fn nanoid(&self) -> Result<String, Failure> {
        Ok(nanoid())
    }
    fn get_snapshot(
        &self,
        venue: public_listeners::Venue,
        symbol: &str,
        limit: f64,
    ) -> Result<Snapshot, Failure> {
        let imports = self
            .imports
            .upgrade()
            .ok_or_else(|| failure("native imports dropped"))?;
        let client = imports
            .public_client
            .borrow()
            .clone()
            .ok_or_else(|| failure("order book snapshot before the public client exists"))?;
        let path = if venue == public_listeners::Venue::Spot {
            crate::binancespot_types::ENDPOINTS
                .iter()
                .find(|(k, _)| *k == "ORDERBOOK")
        } else {
            crate::binance_types::ENDPOINTS
                .iter()
                .find(|(k, _)| *k == "ORDERBOOK")
        }
        .map(|(_, path)| *path)
        .ok_or_else(|| failure("generated ORDERBOOK endpoint"))?;
        let args = record([("symbol", string(symbol)), ("limit", number(limit))]);
        Ok(Box::pin(run_request(
            imports,
            client,
            HttpMethod::Get,
            path,
            Some(args),
        )))
    }
    fn spawn(&self, job: Job) {
        let runtime = self.runtime.clone();
        self.runtime.spawn_local(Box::pin(async move {
            if let Err(error) = job.await {
                runtime.report_exception(
                    &error,
                    &serde_json::json!({"operation":"binance listener job"}).into(),
                );
            }
        }));
    }
    fn reset_sync(&self, sync: SyncState) -> Result<(), Failure> {
        reset_binance_order_book_sync_state(&mut sync.borrow_mut());
        Ok(())
    }
    fn load(&self, book: Book, data: EventValue, sync: SyncState) -> Result<(), Failure> {
        let rows = |key: &str| -> Result<Vec<Vec<String>>, Failure> {
            let EventValue::Array(levels) = data.get(key)? else {
                return Ok(vec![]);
            };
            let levels = array_values(&levels);
            levels
                .iter()
                .map(|row| Ok(vec![row.read("0")?.text()?, row.read("1")?.text()?]))
                .collect()
        };
        load_binance_order_book_snapshot(
            &mut book.borrow_mut(),
            &rows("bids")?,
            &rows("asks")?,
            data.read("lastUpdateId")?.number()?,
            &mut sync.borrow_mut(),
        );
        Ok(())
    }
    fn buffered(
        &self,
        updates: Vec<EventValue>,
        last_id: EventValue,
    ) -> Result<Vec<EventValue>, Failure> {
        let last = last_id.number()?;
        let mut kept = vec![];
        for update in updates {
            if update.read("u")?.number()? > last {
                kept.push(update);
            }
        }
        Ok(kept)
    }
    fn apply(
        &self,
        book: Book,
        data: EventValue,
        sync: SyncState,
        previous: bool,
    ) -> Result<BinanceOrderBookApplyResult, Failure> {
        let Value::Object(map) = to_json(&data)? else {
            return Err(failure("depth update must be an object"));
        };
        apply_binance_depth_update(
            &mut book.borrow_mut(),
            &map,
            &mut sync.borrow_mut(),
            previous,
        )
        .map_err(|error| exchange_failure(error.into()))
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
    /// The base owns its current `ws`, as TS BaseWebSocket does; callers hold it weakly.
    current: RefCell<Option<Rc<CoreSocketHandle>>>,
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
fn fire_listeners(
    this: &ListenerProtocol,
    event: &str,
    data: Option<EventValue>,
) -> Result<(), ExchangeError> {
    let targets: Vec<_> = this
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

impl WsProtocol for ListenerProtocol {
    fn ping(&self, _socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let hook = self.hooks.borrow().as_ref().map(|(ping, _)| ping.clone());
        hook.map_or(Ok(()), |ping| ping().map_err(into_exchange))
    }
    fn connect_and_subscribe(
        &self,
        _socket: Rc<BaseWebSocket>,
        _is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        let hook = self
            .hooks
            .borrow()
            .as_ref()
            .map(|(_, connect)| connect.clone());
        hook.map_or(Ok(()), |connect| connect().map_err(into_exchange))
    }
    fn on_open(&self, _socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = fire_listeners(self, "open", None);
        Box::pin(async move { result })
    }
    fn on_message(
        &self,
        _socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        let data = match frame {
            SocketFrame::Text(text) => string(text.clone()),
            SocketFrame::Binary(bytes) => string(String::from_utf8_lossy(bytes).into_owned()),
            SocketFrame::Close { .. } => return Ok(()),
        };
        fire_listeners(self, "message", Some(data))
    }
}
fn core_private_base(
    parent: Rc<dyn safe_cex_exchange::websocket::WsParent>,
    runtime: Rc<dyn WsRuntime>,
) -> Rc<CorePrivateBase> {
    let listeners = Rc::new(RefCell::new(vec![]));
    let hooks = Rc::new(RefCell::new(None));
    let protocol = Rc::new(ListenerProtocol {
        listeners: listeners.clone(),
        hooks: hooks.clone(),
    });
    Rc::new(CorePrivateBase {
        socket: BaseWebSocket::new(3, parent, runtime, protocol),
        current: RefCell::new(None),
        listeners,
        hooks,
    })
}

struct CoreSocketHandle {
    socket: Rc<BaseWebSocket>,
    listeners: Rc<RefCell<Vec<(&'static str, PrivateCallback)>>>,
}
impl NativePrivateSocket for CoreSocketHandle {
    fn remove_listener(
        &self,
        event: &'static str,
        callback: &PrivateCallback,
    ) -> Result<(), Failure> {
        self.listeners
            .borrow_mut()
            .retain(|(name, c)| *name != event || !c.same(callback));
        Ok(())
    }
    fn ready_state(&self) -> Result<f64, Failure> {
        Ok(if self.socket.is_connected() {
            1.0
        } else if self.socket.has_websocket() {
            0.0
        } else {
            3.0
        })
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
            EventValue::String(_) => Value::String(data.text()?),
            other => to_json(other)?,
        };
        ex(self
            .socket
            .emit_ws_diagnostic_receive(&value, &Default::default()))
    }
    fn stats(&self, topic: &str) -> Result<(), Failure> {
        ex(self.socket.record_message_handler_stats(topic))
    }
    fn set_ping_at(&self, _time: f64) -> Result<(), Failure> {
        self.socket.mark_ping_sent();
        Ok(())
    }
    fn diagnostic_send(&self, payload: &str) -> Result<(), Failure> {
        ex(self
            .socket
            .emit_ws_diagnostic_send(payload, &Default::default()))
    }
    fn pong(&self) -> Result<(), Failure> {
        ex(self.socket.handle_pong_event(5_000))
    }
    fn send(&self, payload: String) -> Result<(), Failure> {
        ex(self.socket.send(&payload))
    }
    fn create(&self, url: String) -> Result<Rc<dyn NativePrivateSocket>, Failure> {
        self.socket.start_websocket(url, vec![], None);
        let handle = Rc::new(CoreSocketHandle {
            socket: self.socket.clone(),
            listeners: self.listeners.clone(),
        });
        *self.current.borrow_mut() = Some(handle.clone());
        Ok(handle)
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
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else {
        format!("{year:+07}")
    };
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
const NO_EXTERNAL: &str = "externalPublicData requested with no live exchange bound";
impl safe_cex_exchange::external::ExternalPublicData for UnavailableExternalData {
    fn connect(&self) {
        self.0.report_exception(
            &ExchangeError::new(NO_EXTERNAL),
            &serde_json::json!({"operation":"external public data"}).into(),
        );
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

fn to_event(value: &impl serde::Serialize) -> Result<EventValue, Failure> {
    serde_json::to_value(value)
        .map(EventValue::from_json)
        .map_err(|e| failure(e.to_string()))
}

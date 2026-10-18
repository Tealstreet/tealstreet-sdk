//! HTTP interceptor behavior from packages/safe-cex/src/safeCexAxiosCreate.ts.
use crate::runtime::{ExchangeError, ExchangeRuntime};
use safe_cex_store::Store;
use safe_cex_transport::query::{
    query_identity_eq, query_json_projection, query_truthy, render_query,
};
use safe_cex_transport::signing::{
    RemoteSigner, RemoteSigningAdapter, create_remote_signing_adapter,
    set_inner_adapter_for_remote_signing,
};
use safe_cex_transport::{
    RestAdapter, RestAdapterContext, RestAdapterVariant, RestAuthorizer, RestExecution,
    RestRequest, RestResponse, RestTransportContext, TransportError, TransportProvider,
    TransportRegistry,
};
use safe_cex_types::{
    ExchangeName, ExchangeOptions, RequestOutcome, retained_json::RetainedJsonInput as Input,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, LazyLock, Mutex},
};

pub const LAST_LOG_PERIODICY: u64 = 30000;
/// Transitional compatibility spelling; new adapter APIs use transport::RestFailure.
pub type HttpFailure = safe_cex_transport::RestFailure;
pub type HttpFuture<'a> = Pin<Box<dyn Future<Output = Result<RestExecution, HttpFailure>> + 'a>>;
pub trait HttpHost {
    /// Drive the supplied adapter exactly once; it owns signing and serialization.
    /// Sending RestRequest directly bypasses both, even if the HTTP request succeeds.
    /// Preserve the adapter failure and its request/response provenance.
    fn execute(&self, adapter: Arc<dyn RestAdapter>, request: RestRequest) -> HttpFuture<'_>;
    fn enhance_error(&self, error: HttpFailure, exchange: &ExchangeName) -> HttpFailure;
    fn emit(&self, event: &str, message: &str) -> Result<(), String>;
    fn note_response(&self, _response: &RestResponse, _request: &RestRequest) {}
    fn note_success(&self, exchange: &ExchangeName, response: &RestResponse);
    fn note_failure(&self, exchange: &ExchangeName, error: &HttpFailure);
    fn mark_start(&self, request: &mut RestRequest);
    fn signing_capture(&self, request: &RestRequest) -> bool;
    fn is_development(&self) -> bool;
    fn is_main_window(&self) -> bool;
}
#[derive(Default)]
pub struct HttpRequestStatsTracker {
    timestamps: Vec<f64>,
    last_log_time: f64,
    total_requests: u64,
    error_count: u64,
    requests_by_exchange: BTreeMap<String, u64>,
}
impl HttpRequestStatsTracker {
    pub fn record_request(&mut self, exchange: &str, now: f64) {
        self.timestamps.push(now);
        self.total_requests += 1;
        *self
            .requests_by_exchange
            .entry(exchange.into())
            .or_default() += 1;
        self.cleanup(now);
        self.maybe_log(now);
    }
    pub fn record_error(&mut self) {
        self.error_count += 1;
    }
    pub fn get_requests_per_minute(&mut self, now: f64) -> usize {
        self.cleanup(now);
        self.timestamps.len()
    }
    fn cleanup(&mut self, now: f64) {
        let count = self
            .timestamps
            .iter()
            .take_while(|&&time| time < now - 60000.)
            .count();
        self.timestamps.drain(..count);
    }
    fn maybe_log(&mut self, now: f64) {
        if now - self.last_log_time >= 10000. {
            self.last_log_time = now;
        }
    }
    pub fn get_stats(&mut self, now: f64) -> Value {
        json!({"perMinute":self.get_requests_per_minute(now),"total":self.total_requests,"errors":self.error_count})
    }
}
pub static HTTP_STATS: LazyLock<Arc<Mutex<HttpRequestStatsTracker>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HttpRequestStatsTracker::default())));
#[derive(Default)]
pub struct HttpLogState {
    request: BTreeMap<String, f64>,
    response: BTreeMap<String, f64>,
}
pub static HTTP_LOGS: LazyLock<Arc<Mutex<HttpLogState>>> =
    LazyLock::new(|| Arc::new(Mutex::new(HttpLogState::default())));
/// Axios factory codec, applied once after signing and before transmission.
/// Params-only signer results assert codec agreement; exact signed queries must
/// be baked into the returned URL with params absent. Errors refuse transmission.
pub type ParamsSerializer = Arc<dyn Fn(&Value) -> Result<String, TransportError>>;
enum AdapterInput {
    BeforeTransform,
    RemoteOutput,
}
struct SerializedParamsAdapter {
    input: AdapterInput,
    inner: Arc<dyn RestAdapter>,
    serialize: Option<ParamsSerializer>,
}
impl RestAdapter for SerializedParamsAdapter {
    fn query_in_url(&self) -> bool {
        self.inner.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, RestExecution> {
        Box::pin(async move {
            let mut retry_config = request.clone();
            // Custom HOW adapters own params in their frame. URL rendering belongs
            // exclusively to adapters whose default buildURL path consumes params.
            if self.inner.query_in_url() {
                if request.params.is_some() && request.query_params.is_some() {
                    return Err(HttpFailure::pre_transmit(
                        "Cannot combine legacy and native query parameters".into(),
                        retry_config,
                    ));
                }
                let query = if let Some(params) = request.query_params.as_ref() {
                    let profile = request.query_profile.ok_or_else(|| {
                        HttpFailure::pre_transmit(
                            "Native query parameters require an explicit serializer profile".into(),
                            retry_config.clone(),
                        )
                    })?;
                    if query_truthy(params) {
                        Some(render_query(params, profile).map_err(|error| {
                            HttpFailure::pre_transmit(error.to_string(), retry_config.clone())
                        })?)
                    } else {
                        None
                    }
                } else if let (Some(serialize), Some(params)) = (&self.serialize, &request.params) {
                    if safe_cex_num::js_truthy(Some(params)) {
                        Some(serialize(params).map_err(|error| {
                            HttpFailure::pre_transmit(error.to_string(), retry_config.clone())
                        })?)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if let Some(query) = query {
                    // Consume only the rendered input; retain present-but-falsy params.
                    if request.query_params.is_some() {
                        request.query_params = None;
                    } else {
                        request.params = None;
                    }
                    if !query.is_empty() {
                        if let Some(hash) = request.url.find('#') {
                            request.url.truncate(hash);
                        }
                        request
                            .url
                            .push(if request.url.contains('?') { '&' } else { '?' });
                        request.url.push_str(&query);
                    }
                }
            }
            match self.input {
                AdapterInput::BeforeTransform => {
                    let mut transformed_config = request.clone();
                    request = transformed_config
                        .transform_axios_request()
                        .map_err(|error| {
                            HttpFailure::pre_transmit(error.to_string(), request.clone())
                        })?;
                    retry_config.body = transformed_config.body;
                    retry_config.headers = transformed_config.headers;
                }
                // Server and client transforms have already run. Preserve the
                // returned adapter data shape and header deletions verbatim.
                AdapterInput::RemoteOutput => {}
            }
            let wire = request.clone();
            match self.inner.execute_with_receipt(request).await {
                Ok(mut execution) => {
                    restore_codec_config(&mut execution.config, &wire, retry_config);
                    Ok(execution)
                }
                Err(mut error) => {
                    restore_codec_config(&mut error.request, &wire, retry_config);
                    Err(error)
                }
            }
        })
    }
}
fn restore_codec_config(current: &mut RestRequest, wire: &RestRequest, retry: RestRequest) {
    // Undo only this codec's projections, retaining changes returned by the adapter.
    if current.url == wire.url {
        current.url = retry.url;
    }
    if current.params == wire.params {
        current.params = retry.params;
    }
    if match (&current.query_params, &wire.query_params) {
        (None, None) => true,
        (Some(left), Some(right)) => query_identity_eq(left, right),
        _ => false,
    } {
        current.query_params = retry.query_params;
    }
    if body_identity_eq(current.body.as_ref(), wire.body.as_ref()) {
        current.body = retry.body;
    }
}
fn body_identity_eq(
    left: Option<&safe_cex_transport::RestBody>,
    right: Option<&safe_cex_transport::RestBody>,
) -> bool {
    use safe_cex_transport::RestBody;
    fn same(left: &Input, right: &Input) -> bool {
        match (left, right) {
            (Input::Float(left), Input::Float(right)) => left.to_bits() == right.to_bits(),
            (Input::Array(left), Input::Array(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|(left, right)| same(left, right))
            }
            (Input::Object(left), Input::Object(right)) => {
                left.len() == right.len()
                    && left
                        .iter()
                        .zip(right)
                        .all(|((lk, lv), (rk, rv))| lk == rk && same(lv, rv))
            }
            _ => left == right,
        }
    }
    match (left, right) {
        (None, None) => true,
        (Some(RestBody::Encoded(left)), Some(RestBody::Encoded(right))) => left == right,
        (Some(RestBody::Untransformed(left)), Some(RestBody::Untransformed(right))) => {
            same(left, right)
        }
        (Some(RestBody::Transformed(left)), Some(RestBody::Transformed(right))) => {
            same(left.value(), right.value())
        }
        _ => false,
    }
}

#[allow(clippy::arc_with_non_send_sync)]
fn with_params_serializer(
    inner: Arc<dyn RestAdapter>,
    serializer: Option<&ParamsSerializer>,
    input: AdapterInput,
) -> Arc<dyn RestAdapter> {
    Arc::new(SerializedParamsAdapter {
        input,
        inner,
        serialize: serializer.cloned(),
    })
}
/// The shared wire codec for a hosted signer's already-transformed output.
pub(crate) fn remote_output_adapter(
    inner: Arc<dyn RestAdapter>,
    serializer: Option<&ParamsSerializer>,
) -> Arc<dyn RestAdapter> {
    with_params_serializer(inner, serializer, AdapterInput::RemoteOutput)
}
/// Factory-selected response transform, before shared response/error hooks.
pub type ResponseTransform =
    Rc<dyn Fn(RestResponse, &RestRequest) -> Result<RestResponse, HttpFailure>>;
pub struct HttpClient {
    params_serializer: Option<ParamsSerializer>,
    response_transform: Option<ResponseTransform>,
    exchange: ExchangeName,
    request: RestRequest,
    proxied: bool,
    authorize: Option<Arc<dyn RestAuthorizer>>,
    adapter: RefCell<Arc<dyn RestAdapter>>,
    remote: Option<Arc<RemoteSigningAdapter>>,
    host: Rc<dyn HttpHost>,
    runtime: Rc<dyn ExchangeRuntime>,
    store: Option<Rc<dyn Store>>,
    stats: Arc<Mutex<HttpRequestStatsTracker>>,
    logs: Arc<Mutex<HttpLogState>>,
    every: Vec<String>,
    periodic: Vec<String>,
    no_proxy: bool,
}
pub struct HttpClientOptions {
    pub params_serializer: Option<ParamsSerializer>,
    pub response_transform: Option<ResponseTransform>,
    pub exchange: ExchangeName,
    pub options: ExchangeOptions,
    pub request: RestRequest,
    pub provider: Option<Arc<dyn TransportProvider>>,
    pub signer: Option<Arc<dyn RemoteSigner>>,
    pub account_name: Option<String>,
    pub adapter: Arc<dyn RestAdapter>,
    pub host: Rc<dyn HttpHost>,
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub store: Option<Rc<dyn Store>>,
    pub stats: Option<Arc<Mutex<HttpRequestStatsTracker>>>,
    pub logs: Option<Arc<Mutex<HttpLogState>>>,
    pub every: Vec<String>,
    pub periodic: Vec<String>,
    pub next_id: Arc<dyn Fn() -> String>,
    pub signing_now: Arc<dyn Fn() -> u64>,
}
// Transport adapters deliberately use Arc while browser futures remain local.
#[allow(clippy::arc_with_non_send_sync)]
pub fn maybe_install_remote_signing_adapter(
    options: &HttpClientOptions,
) -> Option<Arc<RemoteSigningAdapter>> {
    options.signer.as_ref().map(|signer| {
        Arc::new(create_remote_signing_adapter(
            signer.clone(),
            options.exchange.clone(),
            options.account_name.clone(),
            options.next_id.clone(),
            options.signing_now.clone(),
            with_params_serializer(
                options.adapter.clone(),
                options.params_serializer.as_ref(),
                AdapterInput::RemoteOutput,
            ),
        ))
    })
}
pub fn safe_cex_axios_create_no_proxy(options: HttpClientOptions) -> HttpClient {
    create(options, None, true)
}
pub fn safe_cex_axios_create(
    options: HttpClientOptions,
    registry: &TransportRegistry,
) -> Result<HttpClient, TransportError> {
    let transport =
        registry
            .get_transport(options.provider.as_ref())
            .rest(RestTransportContext {
                exchange: &options.exchange,
                options: &options.options,
                config: &options.request,
            })?;
    Ok(create(options, Some(transport), false))
}
fn create(
    mut options: HttpClientOptions,
    transport: Option<safe_cex_transport::RestTransport>,
    no_proxy: bool,
) -> HttpClient {
    options.request.timeout_ms = Some(20000);
    options
        .request
        .merge_axios_headers(&indexmap::IndexMap::from([(
            "Accept".into(),
            "application/json, text/plain, */*".into(),
        )]));
    let remote = maybe_install_remote_signing_adapter(&options);
    let adapter: Arc<dyn RestAdapter> = remote
        .as_ref()
        .map(|r| r.clone() as Arc<dyn RestAdapter>)
        .unwrap_or_else(|| {
            with_params_serializer(
                options.adapter.clone(),
                options.params_serializer.as_ref(),
                AdapterInput::BeforeTransform,
            )
        });
    let (proxied, authorize) = if let Some(transport) = transport {
        options.request.base_url = transport.base_url;
        (transport.proxied, transport.authorize)
    } else {
        (false, None)
    };
    HttpClient {
        params_serializer: options.params_serializer,
        response_transform: options.response_transform,
        exchange: options.exchange,
        request: options.request,
        proxied,
        authorize,
        adapter: RefCell::new(adapter),
        remote,
        host: options.host,
        runtime: options.runtime,
        store: options.store,
        stats: options.stats.unwrap_or_else(|| HTTP_STATS.clone()),
        logs: options.logs.unwrap_or_else(|| HTTP_LOGS.clone()),
        every: options.every,
        periodic: options.periodic,
        no_proxy,
    }
}
pub fn install_transport_adapter(
    client: &HttpClient,
    registry: &TransportRegistry,
    options: &ExchangeOptions,
    provider: Option<&Arc<dyn TransportProvider>>,
    variant: RestAdapterVariant,
) {
    if let Some(adapter) = registry
        .get_transport(provider)
        .rest_adapter(RestAdapterContext {
            exchange: &client.exchange,
            options,
            variant,
        })
    {
        // Preserve sign -> serialize -> transmit when HOW replaces the transport.
        // The API-factory audit forbids direct adapter replacement because it bypassed
        // hosted signing. Wrap the NEW transport with the SAME codec to retain both.
        let input = if client.remote.is_some() {
            AdapterInput::RemoteOutput
        } else {
            AdapterInput::BeforeTransform
        };
        let adapter = with_params_serializer(adapter, client.params_serializer.as_ref(), input);
        if let Some(remote) = &client.remote {
            set_inner_adapter_for_remote_signing(remote, adapter);
        } else {
            *client.adapter.borrow_mut() = adapter;
        }
    }
}
pub fn stringify_response_body(body: &Value) -> String {
    if let Some(text) = body.as_str() {
        text.into()
    } else {
        body.to_string()
    }
}
pub fn is_tealstreet_proxy_auth_failure_body(body: &Value) -> bool {
    let message = body
        .as_str()
        .or_else(|| body.get("message").and_then(Value::as_str))
        .unwrap_or("");
    matches!(message, "Authorization header is missing" | "Invalid JWT")
}
impl RestAdapter for HttpClient {
    fn query_in_url(&self) -> bool {
        self.adapter.borrow().query_in_url()
    }
    fn execute(&self, request: RestRequest) -> safe_cex_transport::RestFuture<'_, RestResponse> {
        Box::pin(HttpClient::execute(self, request))
    }
    fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, RestExecution> {
        Box::pin(HttpClient::execute_with_receipt(self, request))
    }
}
impl HttpClient {
    /// Whether this request is dispatched only to capture its signed config.
    pub fn is_signing_capture(&self, request: &RestRequest) -> bool {
        self.host.signing_capture(request)
    }
    /// Signer/preflight refusal precedes common request statistics; serializers
    /// call the same failure constructor inside the adapter after those stats.
    pub fn refuse_before_transmit(
        &self,
        request: RestRequest,
        message: String,
    ) -> Result<RestResponse, HttpFailure> {
        self.failure(HttpFailure::pre_transmit(message, request))
    }
    pub async fn execute(&self, request: RestRequest) -> Result<RestResponse, HttpFailure> {
        self.execute_with_receipt(request)
            .await
            .map(|execution| execution.response)
    }
    pub async fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> Result<RestExecution, HttpFailure> {
        self.execute_with_interceptor_receipt(request, |request| std::future::ready(Ok(request)))
            .await
    }
    /// Apply factory defaults before the venue interceptor. Its deletions and
    /// replacements are authoritative; execution never merges defaults again.
    /// A rejected interceptor uses shared enhancement before request statistics.
    pub async fn execute_with_interceptor<F, I>(
        &self,
        request: RestRequest,
        interceptor: I,
    ) -> Result<RestResponse, HttpFailure>
    where
        I: FnOnce(RestRequest) -> F,
        F: Future<Output = Result<RestRequest, HttpFailure>>,
    {
        self.execute_with_interceptor_receipt(request, interceptor)
            .await
            .map(|execution| execution.response)
    }
    pub async fn execute_with_interceptor_receipt<F, I>(
        &self,
        mut request: RestRequest,
        interceptor: I,
    ) -> Result<RestExecution, HttpFailure>
    where
        I: FnOnce(RestRequest) -> F,
        F: Future<Output = Result<RestRequest, HttpFailure>>,
    {
        if request.base_url.is_none() {
            request.base_url = self.request.base_url.clone();
        }
        if request.timeout_ms.is_none() {
            request.timeout_ms = self.request.timeout_ms;
        }
        if request.query_profile.is_none() {
            request.query_profile = self.request.query_profile;
        }
        request.merge_axios_headers(&self.request.headers);
        let request = match interceptor(request).await {
            Ok(request) => request,
            Err(error) => return self.failure(error),
        };
        self.execute_after_interceptor(request).await
    }
    async fn execute_after_interceptor(
        &self,
        mut request: RestRequest,
    ) -> Result<RestExecution, HttpFailure> {
        if !self.no_proxy && self.should_log(&request.url, false) {
            let body = request.body.as_ref().map(|body| match body {
                safe_cex_transport::RestBody::Untransformed(value) => value.clone(),
                safe_cex_transport::RestBody::Transformed(data) => data.value().clone(),
                safe_cex_transport::RestBody::Encoded(bytes) => {
                    Input::Text(String::from_utf8_lossy(bytes).encode_utf16().collect())
                }
            });
            let mut summary = Input::from_value(json!({"method":request.method.to_uppercase(),"url":request.url,"params":request.query_params.as_ref().and_then(query_json_projection).or_else(|| request.params.clone())})).map_err(|e| HttpFailure::pre_transmit(e.to_string(),request.clone()))?;
            let Input::Object(fields) = &mut summary else {
                unreachable!("request summary")
            };
            if let Some(body) = body {
                fields.push(("body".encode_utf16().collect(), body));
            }
            let summary = native_json_text(&summary);
            let _ = self.host.emit(
                "grafana-http-request",
                &format!(
                    "Request {} {} {summary}",
                    request.method.to_uppercase(),
                    request.url
                ),
            );
        }
        if self.proxied
            && let Some(authorize) = &self.authorize
            && let Err(error) = authorize.authorize(&mut request).await
        {
            return self.failure(HttpFailure::from_transport(error, request));
        }
        let now = self.runtime.now_ms();
        self.stats
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_request(&exchange_label(&self.exchange), now);
        self.host.mark_start(&mut request);
        let adapter = self.adapter.borrow().clone();
        match self.host.execute(adapter, request.clone()).await {
            Ok(execution) => {
                let execution = if let Some(transform) = &self.response_transform {
                    match transform(execution.response, &execution.config) {
                        Ok(response) => RestExecution {
                            response,
                            config: execution.config,
                        },
                        Err(error) => return self.failure(error),
                    }
                } else {
                    execution
                };
                let response = &execution.response;
                let request = &execution.config;
                self.record_outcome(Some(response.status), true, request);
                if !self.host.signing_capture(request) {
                    self.host.note_response(response, request);
                    self.host.note_success(&self.exchange, response);
                }
                if !self.no_proxy && self.should_log(&request.url, true) {
                    let body = response.body.clone().into_data();
                    let summary = native_json_text(&Input::Object(vec![
                        (
                            "status".encode_utf16().collect(),
                            Input::Float(response.status as f64),
                        ),
                        ("body".encode_utf16().collect(), body),
                    ]));
                    let _ = self.host.emit(
                        "grafana-http-response",
                        &format!(
                            "Response {} {} {} {summary}",
                            request.method.to_uppercase(),
                            request.url,
                            response.status
                        ),
                    );
                }
                Ok(execution)
            }
            Err(mut error) => {
                if let Some(transform) = &self.response_transform
                    && let Some(response) = error.response.take()
                {
                    match transform(*response, &error.request) {
                        Ok(response) => error.response = Some(Box::new(response)),
                        Err(error) => return self.failure(error),
                    }
                }
                self.failure(error)
            }
        }
    }
    fn record_outcome(&self, status: Option<u16>, had_response: bool, request: &RestRequest) {
        if self.host.signing_capture(request) {
            return;
        }
        let outcome = if !had_response {
            RequestOutcome::Network
        } else {
            match status {
                Some(429 | 418) => RequestOutcome::RateLimit,
                Some(401 | 403) => RequestOutcome::Auth,
                Some(200..=299) => RequestOutcome::Ok,
                _ => RequestOutcome::Other,
            }
        };
        if let Some(store) = &self.store {
            // safeCexAxiosCreate.ts219-222 catches counter failures explicitly.
            let _ = store.record_request_outcome(outcome);
        }
    }
    fn failure<T>(&self, error: HttpFailure) -> Result<T, HttpFailure> {
        if self.no_proxy {
            self.stats
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .record_error();
        }
        let status = error.response.as_ref().map(|r| r.status);
        self.record_outcome(status, error.response.is_some(), &error.request);
        if !self.host.signing_capture(&error.request) {
            self.host.note_failure(&self.exchange, &error);
        }
        let transport_error = error.transport_error.clone();
        let mut error = self.host.enhance_error(error, &self.exchange);
        error.transport_error = transport_error;
        if !self.no_proxy {
            if self.proxied
                && let Some(status) = status
                && let Some(response) = &error.response
            {
                // Physical bytes have not reached the native venue transform yet.
                // Use the shared Axios view only for this diagnostic; native data
                // keeps its existing value and the failure retains its raw body.
                let body = match &response.body {
                    safe_cex_transport::RestResponseBody::Encoded(_) => response
                        .body
                        .clone()
                        .transform_axios_response()
                        .expect("default response transform parses or preserves text")
                        .into_data(),
                    _ => response.body.clone().into_data(),
                };
                let source = if matches!(status, 401 | 403) {
                    Some(if native_proxy_auth_failure_body(&body) {
                        "Tealstreet proxy auth failed"
                    } else {
                        "Upstream exchange auth failed via proxy"
                    })
                } else if status >= 500 {
                    Some("Proxy server error")
                } else {
                    None
                };
                if let Some(source) = source {
                    let message = format!(
                        "[{}] {source} ({status}): {}",
                        exchange_label(&self.exchange),
                        native_response_text(&body)
                    );
                    self.runtime.console("error", &message);
                    if let Err(error) = self.host.emit("error", &message) {
                        self.runtime.report_exception(
                            &ExchangeError::new(error),
                            &json!({"event":"error"}).into(),
                        );
                    }
                }
            }
            if self.host.is_main_window() {
                let prefix = if matches!(status, Some(429 | 418)) {
                    "[RATE LIMITED] "
                } else {
                    ""
                };
                let status = status
                    .map(|s| format!("HTTP {s}"))
                    .unwrap_or_else(|| "Error".into());
                let mut summary =
                    Input::from_value(json!({"message":error.message,"context":error.context}))
                        .expect("bounded error log metadata");
                let Input::Object(fields) = &mut summary else {
                    unreachable!("error log object")
                };
                if let Some(response) = &error.response {
                    fields.push((
                        "responseBody".encode_utf16().collect(),
                        response.body.clone().into_data(),
                    ));
                }
                let summary = native_json_text(&summary);
                let _ = self.host.emit(
                    "grafana-error",
                    &format!(
                        "{prefix}Response {} {} {status}: {summary}",
                        error.request.method.to_uppercase(),
                        error.request.url
                    ),
                );
            }
        }
        Err(error)
    }
    fn should_log(&self, url: &str, response: bool) -> bool {
        if self.host.is_development() {
            return true;
        }
        let mut should = self.every.iter().any(|endpoint| url.contains(endpoint));
        let now = self.runtime.now_ms();
        let mut logs = self
            .logs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let logs = if response {
            &mut logs.response
        } else {
            &mut logs.request
        };
        for endpoint in &self.periodic {
            if url.contains(endpoint) {
                if logs.get(endpoint).is_none_or(|&previous| {
                    previous == 0. || now - previous >= LAST_LOG_PERIODICY as f64
                }) {
                    should = true;
                    logs.insert(endpoint.clone(), now);
                }
                break;
            }
        }
        should
    }
}
fn native_json_text(value: &Input) -> String {
    safe_cex_num::js_retained_json_stringify(value)
        .expect("retained log tree")
        .unwrap_or_else(|| "undefined".into())
}
fn native_response_text(value: &Input) -> String {
    match value {
        Input::Text(units) => String::from_utf16_lossy(units),
        _ => native_json_text(value),
    }
}
fn native_proxy_auth_failure_body(value: &Input) -> bool {
    let value = match value {
        Input::Object(fields) => fields
            .iter()
            .rev()
            .find(|(key, _)| key.iter().copied().eq("message".encode_utf16()))
            .map(|(_, value)| value),
        value => Some(value),
    };
    matches!(value, Some(Input::Text(units)) if ["Authorization header is missing","Invalid JWT"].iter().any(|text| units.iter().copied().eq(text.encode_utf16())))
}

fn exchange_label(exchange: &ExchangeName) -> String {
    serde_json::to_value(exchange)
        .expect("exchange")
        .as_str()
        .expect("string")
        .into()
}

#[cfg(test)]
mod query_tests;

#[cfg(test)]
mod receipt_tests;

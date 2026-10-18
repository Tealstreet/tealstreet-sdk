//! Executable Backpack interceptors around the shared HTTP client.
use crate::native_value::NativeValue;
use crate::{api_factory as api, native_value::Value as Native};
use safe_cex_exchange::{exchange::BaseExchange, http::HttpClient, runtime::ExchangeError};
use safe_cex_num::JsJsonValue;
use safe_cex_transport::{RestBody, RestRequest, RestResponseBody};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
pub type BuildHttp = Box<
    dyn Fn(
        api::ClientConfig<ExchangeError>,
        Rc<RefCell<api::Options>>,
    ) -> Result<HttpClient, ExchangeError>,
>;
pub type InstallTransport =
    Rc<dyn Fn(&HttpClient, Rc<RefCell<api::Options>>) -> Result<(), ExchangeError>>;
pub struct Client {
    base: Rc<BaseExchange>,
    pub inner: Rc<HttpClient>,
    pub executor: RefCell<std::sync::Arc<dyn safe_cex_transport::RestAdapter>>,
    pub config: api::ClientConfig<ExchangeError>,
    pub retries: Cell<u32>,
    rate_limits: RefCell<Vec<Rc<safe_cex_transport::rate_limit::RateLimiter>>>,
    pub request: RefCell<Option<api::RequestHandler<ExchangeError>>>,
    pub success: RefCell<Option<api::ResponseHandler<ExchangeError>>>,
    pub failure: RefCell<Option<api::FailureHandler<ExchangeError>>>,
}
fn wire(v: &JsJsonValue) -> Result<Value, ExchangeError> {
    let encoded = safe_cex_num::js_json_stringify(v).map_err(coercion)?;
    encoded.map_or(Ok(Value::Null), |s| {
        serde_json::from_str(&s).map_err(|e| ExchangeError::new(e.to_string()))
    })
}
fn coercion(e: safe_cex_num::JsCoercionError) -> ExchangeError {
    let mut out = ExchangeError::new(e.message);
    out.name = e.name.into();
    out
}
impl Client {
    pub fn execute(
        self: &Rc<Self>,
        config: Rc<api::Config>,
    ) -> api::Task<Rc<api::Response>, ExchangeError> {
        let client = self.clone();
        let request = move |_| async move {
            let mut attempt = 0;
            let (response, config, req) = loop {
                let limits = client.rate_limits.borrow().clone();
                for limit in limits.into_iter().rev() {
                    limit
                        .acquire()
                        .await
                        .map_err(|error| ExchangeError::new(error.to_string()))?;
                }
                let request = client
                    .request
                    .borrow()
                    .as_ref()
                    .expect("Backpack request interceptor installed")
                    .clone();
                let config = request(config.clone()).await?;
                let mut req = RestRequest {
                    method: config.method.clone().unwrap_or_else(|| "GET".into()),
                    url: config.url.clone().unwrap_or_default(),
                    skip_remote_signing: config.skip_remote_signing,
                    ..Default::default()
                };
                if let Some(headers) = &config.headers {
                    for (k, v) in &*headers.borrow() {
                        let text = Native::from_prepared(v.clone()).utf8()?;
                        req.headers.insert(k.clone(), text);
                    }
                }
                if !matches!(&*config.params, JsJsonValue::Undefined) {
                    req.params = Some(wire(&config.params)?);
                }
                if !matches!(&*config.data, JsJsonValue::Undefined) {
                    req.body = Some(RestBody::Untransformed(
                        safe_cex_exchange::js_runtime::value::to_retained_input(
                            &Native::from_prepared((*config.data).clone()),
                        )
                        .map_err(|error| error.to_exchange_error())?,
                    ));
                }
                let executor = client.executor.borrow().clone();
                let response = executor.execute(req.clone()).await;
                if let Err(e) = &response {
                    if attempt < client.retries.get()
                        && safe_cex_transport::retry::is_read_only_method(&req)
                        && e.is_retryable_network()
                    {
                        attempt += 1;
                        continue;
                    }
                }
                break (response, config, req);
            };
            match response {
                Ok(response) => {
                    let data = match response.body {
                        RestResponseBody::Transformed(data) => data,
                        body => body
                            .transform_axios_response()
                            .map_err(|error| ExchangeError::new(error.to_string()))?
                            .into_data(),
                    };
                    let response = Rc::new(api::Response {
                        data,
                        config: Some(config),
                        status: Some(f64::from(response.status)),
                    });
                    let handler = client
                        .success
                        .borrow()
                        .as_ref()
                        .expect("Backpack response interceptor installed")
                        .clone();
                    handler(response).await
                }
                Err(error) => {
                    let mut e = ExchangeError::new(error.message);
                    e.metadata.is_axios_error = true;
                    e.metadata.config = Some(json!({"url":req.url,"method":req.method}));
                    e.metadata.code = error
                        .code
                        .map(|code| safe_cex_exchange::js_runtime::value::Js::str(&code));
                    if let Some(response) = error.response {
                        let data = safe_cex_exchange::runtime::response_json_view(&response.body);
                        let data = if let Value::String(s) = &data {
                            serde_json::from_str(s).unwrap_or(data.clone())
                        } else {
                            data
                        };
                        e.metadata.response = Some(json!({"data":data,"status":response.status}));
                    }
                    let handler = client
                        .failure
                        .borrow()
                        .as_ref()
                        .expect("Backpack error interceptor installed")
                        .clone();
                    handler(e).await
                }
            }
        };
        self.base.start_shared_operation(request).wait()
    }
    pub fn request(
        self: &Rc<Self>,
        method: &str,
        url: &str,
        params: JsJsonValue,
        data: JsJsonValue,
    ) -> api::Task<Native, ExchangeError> {
        let config = Rc::new(api::Config {
            url: Some(url.into()),
            method: Some(method.into()),
            params: Rc::new(params),
            data: Rc::new(data),
            headers: Some(Rc::new(RefCell::new(
                self.config
                    .headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), JsJsonValue::String(v.to_string())))
                    .collect(),
            ))),
            skip_remote_signing: false,
            extra: vec![],
        });
        let response = self.execute(config);
        Box::pin(async move {
            Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
                response.await?.data.clone(),
            ))
        })
    }
}
#[derive(Clone)]
pub struct RoutedClient {
    primary: Rc<Client>,
    reader: Rc<Client>,
}
impl RoutedClient {
    pub fn new(primary: Rc<Client>, reader: Rc<Client>) -> Self {
        Self { primary, reader }
    }
    pub fn request(
        &self,
        method: &str,
        url: &str,
        params: JsJsonValue,
        data: JsJsonValue,
    ) -> api::Task<Native, ExchangeError> {
        let client = if method == "GET" {
            &self.reader
        } else {
            &self.primary
        };
        client.request(method, url, params, data)
    }
}
impl std::ops::Deref for RoutedClient {
    type Target = Rc<Client>;
    fn deref(&self) -> &Self::Target {
        &self.primary
    }
}
pub struct Factory {
    pub base: Rc<BaseExchange>,
    pub build: BuildHttp,
    pub install: InstallTransport,
}
impl api::Host for Factory {
    type Error = ExchangeError;
    type Client = Rc<Client>;
    type RetryCondition = ();
    #[expect(clippy::arc_with_non_send_sync)]
    fn create(
        &mut self,
        config: api::ClientConfig<ExchangeError>,
        options: Rc<RefCell<api::Options>>,
    ) -> Result<Rc<Client>, ExchangeError> {
        let shadow = api::ClientConfig {
            base_url: config.base_url,
            timeout: config.timeout,
            headers: config.headers,
            serialize: config.serialize.clone(),
        };
        let inner = (self.build)(shadow, options)?;
        let inner = Rc::new(inner);
        Ok(Rc::new(Client {
            base: self.base.clone(),
            inner: inner.clone(),
            executor: RefCell::new(std::sync::Arc::new(HttpExecution { inner })),
            config,
            retries: Cell::new(0),
            rate_limits: RefCell::new(vec![]),
            request: RefCell::new(None),
            success: RefCell::new(None),
            failure: RefCell::new(None),
        }))
    }
    fn qs_repeat(&mut self, params: JsJsonValue) -> Result<String, ExchangeError> {
        fn query(v: JsJsonValue) -> safe_cex_types::QueryValue {
            use safe_cex_types::QueryValue as Q;
            match v {
                JsJsonValue::Undefined => Q::Undefined,
                JsJsonValue::Null => Q::Null,
                JsJsonValue::Bool(v) => Q::Bool(v),
                JsJsonValue::Number(v) => Q::Number(v),
                JsJsonValue::String(v) => Q::String(v),
                JsJsonValue::Array(v) => Q::Array(v.into_iter().map(query).collect()),
                JsJsonValue::Object(v) => {
                    Q::Object(v.into_iter().map(|(k, v)| (k, query(v))).collect())
                }
                JsJsonValue::BigInt(v) => Q::String(v.to_string()),
            }
        }
        safe_cex_transport::query::render_query(
            &query(params),
            safe_cex_transport::query::QueryProfile {
                key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                arrays: safe_cex_transport::query::QueryArrayFormat::Repeat,
                encode: true,
            },
        )
        .map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        })
    }

    fn read_retry_condition(&mut self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn retry(&mut self, client: &Rc<Client>, n: u32, _: ()) -> Result<(), ExchangeError> {
        client.retries.set(n);
        Ok(())
    }
    fn install_transport(
        &mut self,
        client: &Rc<Client>,
        options: Rc<RefCell<api::Options>>,
    ) -> Result<(), ExchangeError> {
        (self.install)(&client.inner, options)
    }
    fn register_request(
        &mut self,
        client: &Rc<Client>,
        handler: api::RequestHandler<ExchangeError>,
    ) -> Result<(), ExchangeError> {
        *client.request.borrow_mut() = Some(handler);
        Ok(())
    }
    fn register_response(
        &mut self,
        client: &Rc<Client>,
        success: api::ResponseHandler<ExchangeError>,
        failure: api::FailureHandler<ExchangeError>,
    ) -> Result<(), ExchangeError> {
        *client.success.borrow_mut() = Some(success);
        *client.failure.borrow_mut() = Some(failure);
        Ok(())
    }
    fn auth(
        &mut self,
        instruction: &str,
        params: JsJsonValue,
        key: String,
        secret: String,
        window: f64,
    ) -> Result<api::Task<Vec<(String, JsJsonValue)>, ExchangeError>, ExchangeError> {
        let now = safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms();
        let future = crate::signing::generate_auth_headers(
            instruction,
            &params,
            &key,
            &secret,
            Some(window),
            move || now,
        );
        Ok(Box::pin(async move {
            future
                .await
                .map(|v| {
                    v.into_iter()
                        .map(|(k, v)| (k, JsJsonValue::String(v)))
                        .collect()
                })
                .map_err(ExchangeError::new)
        }))
    }
    fn error_text(&mut self, e: &ExchangeError) -> Result<String, ExchangeError> {
        Ok(e.js_error_string())
    }
    fn error_message(&mut self, e: &ExchangeError) -> Result<Input, ExchangeError> {
        Ok(Input::Text(e.message.encode_utf16().collect()))
    }
    fn response(&mut self, e: &ExchangeError) -> Result<Option<Rc<api::Response>>, ExchangeError> {
        Ok(e.metadata.response.as_ref().map(|v| {
            Rc::new(api::Response {
                data: v.get("data").cloned().unwrap_or(Value::Null).into(),
                config: None,
                status: v.get("status").and_then(Value::as_f64),
            })
        }))
    }
    fn error_config(
        &mut self,
        e: &ExchangeError,
    ) -> Result<Option<Rc<api::Config>>, ExchangeError> {
        Ok(e.metadata.config.as_ref().map(|v| {
            Rc::new(api::Config {
                url: v.get("url").and_then(Value::as_str).map(str::to_owned),
                method: v.get("method").and_then(Value::as_str).map(str::to_owned),
                params: Rc::new(JsJsonValue::Undefined),
                data: Rc::new(JsJsonValue::Undefined),
                headers: None,
                skip_remote_signing: false,
                extra: vec![],
            })
        }))
    }
    fn emit_and_log(&mut self, message: String) -> Result<(), ExchangeError> {
        self.base.emit_and_log_error(
            safe_cex_exchange::error_reporting::ExchangeErrorInput::Message(message),
            None,
            None,
        )
    }
    fn emit_error(&mut self, message: String) -> Result<(), ExchangeError> {
        self.base.emit(
            "error",
            safe_cex_exchange::js_runtime::value::Js::object(vec![
                (
                    "name",
                    safe_cex_exchange::js_runtime::value::Js::str("Error"),
                ),
                (
                    "message",
                    safe_cex_exchange::js_runtime::value::Js::str(&message),
                ),
            ]),
        )
    }
    fn append_http_failure(
        &mut self,
        message: String,
        response: Rc<api::Response>,
    ) -> Result<String, ExchangeError> {
        let value = json!({"status":response.status});
        Ok(safe_cex_exchange::error_reporting::append_http_failure(
            Some(&Value::String(message)),
            &value,
        ))
    }
    fn make_error(&mut self, message: String) -> ExchangeError {
        ExchangeError::new(message)
    }
    fn coercion_error(&mut self, e: safe_cex_num::JsCoercionError) -> ExchangeError {
        coercion(e)
    }
}
struct HttpExecution {
    inner: Rc<HttpClient>,
}
impl safe_cex_transport::RestAdapter for HttpExecution {
    fn execute(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(self.inner.execute(request))
    }
    fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(self.inner.execute_with_receipt(request))
    }
}
struct RateClock {
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
}
impl safe_cex_transport::rate_limit::RateLimitClock for RateClock {
    fn now_ms(&self) -> u64 {
        self.runtime.monotonic_now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> safe_cex_transport::TransportFuture<'_, ()> {
        let f = self.runtime.sleep(ms);
        Box::pin(async move {
            f.await
                .map_err(|e| safe_cex_transport::TransportError::Host(e.message))
        })
    }
}
impl Client {
    #[expect(clippy::arc_with_non_send_sync)]
    pub fn limit_requests(
        &self,
        runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
        max_rps: usize,
    ) {
        self.rate_limits.borrow_mut().push(Rc::new(
            safe_cex_transport::rate_limit::RateLimiter::new(
                std::sync::Arc::new(RateClock { runtime }),
                max_rps,
                1000,
            ),
        ));
    }
}

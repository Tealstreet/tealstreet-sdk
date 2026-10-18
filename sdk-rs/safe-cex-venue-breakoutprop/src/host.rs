//! Concrete DXtrade host over the shared HTTP/value/promise runtime.
use crate::{
    api::{self, ApiDefaults, ApiFailure, ApiHost, ApiWire, Config},
    lifecycle::{BreakoutHost, BreakoutpropExchange, DxClient},
    native_reads::NativeRuntimeHost,
    polling::{HandlerTable, PollingAncestor},
};
use safe_cex_exchange::{
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    external::ExternalPublicData,
    js_runtime::{
        base_exchange::ExternalPublicDataFactory,
        client::{self, JsHttpHost},
        promise::await_js,
        value::{Js, Thrown, arg, from_json, to_json, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
    ws_parent::WsExchangeParent,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_store::Store;
use safe_cex_transport::{
    RestBody, RestRequest,
    retry::{ClassifiedRequest, ReadClassification},
};
use safe_cex_types::{
    ExchangeName, ExchangeOptions, ExchangeOptionsExternalPublicData, QueryValue,
};
use serde_json::json;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

/// Supply a fresh terminal HTTP host per account.
#[derive(Clone)]
pub struct BreakoutDependencies {
    pub runtime: Rc<dyn WsRuntime>,
    pub http: Rc<dyn JsHttpHost>,
    pub external_public_data: Option<ExternalPublicDataFactory>,
    /// Offset-less legacy timestamps use the embedding host's timezone policy.
    pub local_to_utc: Rc<dyn Fn(f64) -> f64>,
}
#[derive(Clone)]
pub struct ProductionHost {
    deps: BreakoutDependencies,
    parent: Rc<RefCell<Weak<BreakoutpropExchange>>>,
}
impl ProductionHost {
    pub fn new(deps: BreakoutDependencies) -> Rc<Self> {
        Rc::new(Self {
            deps,
            parent: Rc::default(),
        })
    }
    fn parent(&self) -> Result<Rc<BreakoutpropExchange>, ExchangeError> {
        self.parent
            .borrow()
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Breakout Prop host is not attached to an account"))
    }
    fn owned(&self, future: ExchangeFuture<'static, ()>) {
        let parent = self
            .parent()
            .expect("live source caller retains its exchange");
        let runtime = self.deps.runtime.clone();
        parent.base.start_owned_operation(async move {
            if let Err(error) = future.await {
                runtime.report_exception(
                    &error,
                    &json!({"venue":"breakoutprop","source":"owned continuation"}).into(),
                );
            }
        });
    }
}
impl BreakoutHost for ProductionHost {
    fn runtime_host(&self) -> Rc<dyn NativeRuntimeHost> {
        Rc::new(self.clone())
    }
    fn construct_base(
        &self,
        name: ExchangeName,
        options: ExchangeOptions,
        store: Rc<dyn Store>,
    ) -> Result<BaseExchange, ExchangeError> {
        if name != ExchangeName::Breakoutprop {
            return Err(ExchangeError::new(
                "Breakout Prop host received another venue name",
            ));
        }
        if options.external_public_data.is_some() && self.deps.external_public_data.is_none() {
            return Err(ExchangeError::new(
                "externalPublicData requires a provider factory",
            ));
        }
        self.deps
            .http
            .configure_exchange(name.clone())
            .map_err(ExchangeError::from)?;
        let mut base = BaseExchange::new(
            name,
            options,
            store,
            self.deps.runtime.clone(),
            ExchangeSettings::default(),
        );
        base.signers.remote_signer = self.deps.http.hosted_signer().map(|(signer, _)| signer);
        Ok(base)
    }
    fn create_api(
        &self,
        exchange: Rc<BreakoutpropExchange>,
    ) -> Result<Rc<dyn DxClient>, ExchangeError> {
        if self
            .parent
            .borrow()
            .upgrade()
            .is_some_and(|old| !Rc::ptr_eq(&old, &exchange))
        {
            return Err(ExchangeError::new(
                "Breakout Prop host already belongs to another account",
            ));
        }
        *self.parent.borrow_mut() = Rc::downgrade(&exchange);
        let host = ClientFactory {
            http: self.deps.http.clone(),
            xhr: RefCell::new(None),
            provider: self.deps.runtime.transport(),
        };
        let options = exchange.base.options();
        api::create_api(&host, options, &exchange).map(|api| api as Rc<dyn DxClient>)
    }
    fn rate_limit(
        &self,
        client: Rc<dyn DxClient>,
        max_rps: f64,
    ) -> Result<Rc<dyn DxClient>, ExchangeError> {
        client.rate_limit(max_rps)?;
        Ok(client)
    }
    fn construct_public_ancestor(
        &self,
        exchange: Rc<BreakoutpropExchange>,
        handlers: HandlerTable,
    ) -> Result<Rc<dyn PollingAncestor>, ExchangeError> {
        let venue: Rc<dyn Exchange> = exchange;
        let core = BaseWebSocket::new(
            0,
            WsExchangeParent::new(&venue),
            self.deps.runtime.clone(),
            Rc::new(PollingProtocol),
        );
        Ok(Rc::new(PollingBase {
            core,
            runtime: self.deps.runtime.clone(),
            handlers,
        }))
    }
    fn spawn_login(&self, future: ExchangeFuture<'static, ()>) {
        self.owned(future)
    }
    fn report_abandoned_login(&self, error: &ExchangeError) {
        self.deps.runtime.report_exception(
            error,
            &json!({"venue":"breakoutprop","source":"abandoned login"}).into(),
        );
    }
}
impl NativeRuntimeHost for ProductionHost {
    fn create_external(
        &self,
        config: &ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn ExternalPublicData> {
        let parent = self.parent().expect("live caller owns exchange");
        (self
            .deps
            .external_public_data
            .as_ref()
            .expect("construction validated the provider factory"))(
            config, parent.base.clone()
        )
    }
    fn local_to_utc(&self, value: f64) -> f64 {
        (self.deps.local_to_utc)(value)
    }
    fn uuid(&self) -> String {
        self.deps.runtime.unique_id().replace(['-', '_'], "")
    }
    fn spawn_order(&self, future: ExchangeFuture<'static, ()>) {
        self.owned(future)
    }
    fn schedule_loop(
        &self,
        next: Rc<dyn Fn() -> ExchangeFuture<'static, ()>>,
        delay: Option<f64>,
    ) -> Result<(), ExchangeError> {
        if delay == Some(f64::INFINITY) {
            return Ok(());
        }
        let parent = self.parent()?;
        let runtime = self.deps.runtime.clone();
        let sleep_runtime = runtime.clone();
        let warn_runtime = runtime.clone();
        let owner = self.clone();
        let enqueue = Rc::new(move |callback: Rc<dyn Fn()>| -> Result<(), String> {
            let parent = owner.parent().map_err(|error| error.message)?;
            let runtime = owner.deps.runtime.clone();
            parent.base.start_owned_operation(async move {
                if let Err(error) = runtime.sleep(0).await {
                    runtime.report_exception(
                        &error,
                        &json!({"venue":"breakoutprop","source":"poll callback"}).into(),
                    );
                    return;
                }
                callback();
            });
            Ok(())
        });
        let host = safe_cex_exchange::loop_util::LoopHost {
            runtime: runtime.clone(),
            sleep: Rc::new(move |ms| {
                let pending = sleep_runtime.sleep(ms.max(0.0) as u64);
                Box::pin(async move { pending.await.map_err(|error| error.message) })
            }),
            // Native and worker hosts have no visible browser document.
            document_visible: Rc::new(|| false),
            request_animation_frame: enqueue.clone(),
            set_timeout_zero: enqueue,
            warn: Rc::new(move |message| warn_runtime.console("warn", message)),
        };
        let owner = self.clone();
        let callback = Rc::new(move || owner.owned(next()));
        let pending = safe_cex_exchange::loop_util::loop_once(
            callback,
            &"() => this.tick()".encode_utf16().collect::<Vec<_>>(),
            delay,
            &host,
        );
        parent.base.start_owned_operation(async move {
            if let Err(error) = pending.await {
                runtime.report_exception(
                    &error,
                    &json!({"venue":"breakoutprop","source":"poll loop"}).into(),
                );
            }
        });
        Ok(())
    }
}
struct PollingProtocol;
impl WsProtocol for PollingProtocol {
    fn recovers_after_offline(&self) -> bool {
        false
    }
}
struct PollingBase {
    core: Rc<BaseWebSocket>,
    runtime: Rc<dyn WsRuntime>,
    handlers: HandlerTable,
}
impl PollingAncestor for PollingBase {
    fn now_ms(&self) -> f64 {
        safe_cex_num::js_time_clip(self.runtime.now_ms())
    }
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        // Source callback lists remain owned by the polling container. The core
        // performs the inherited stats cleanup; no network socket is started.
        let source = self.handlers.borrow().clone();
        let mut rows = Vec::new();
        for (topic, handlers) in source {
            let callbacks: Vec<safe_cex_exchange::websocket::MessageHandler> = handlers
                .borrow()
                .iter()
                .cloned()
                .map(|handler| {
                    Rc::new(move |value: &serde_json::Value| {
                        let ticker = safe_cex_num::numeric_ingress::js_from_value_ref(value)
                            .map_err(|error| ExchangeError::new(error.to_string()))?;
                        handler(&ticker)
                    }) as safe_cex_exchange::websocket::MessageHandler
                })
                .collect();
            rows.push((topic, Rc::new(RefCell::new(callbacks))));
        }
        *self.core.message_handlers.borrow_mut() = rows.into_iter().collect();
        self.core.cleanup_message_handler_stats(topic)
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.core.dispose();
        Ok(())
    }
}
struct ClientFactory {
    http: Rc<dyn JsHttpHost>,
    xhr: RefCell<Option<Js>>,
    provider: std::sync::Arc<dyn safe_cex_transport::TransportProvider>,
}
struct Wire {
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
    xhr: Js,
}
impl ApiHost for ClientFactory {
    fn create(
        &self,
        defaults: ApiDefaults,
        options: &ExchangeOptions,
        exchange: &Rc<BreakoutpropExchange>,
    ) -> Result<Rc<dyn ApiWire>, ExchangeError> {
        let serialize = defaults.serialize_params;
        let xhr = client::factory::create(
            self.http.clone(),
            &*self.provider,
            Js::object(vec![
                ("baseURL", Js::str(defaults.base_url)),
                ("timeout", Js::Number(defaults.timeout)),
                (
                    "headers",
                    Js::object(vec![("Content-Type", Js::str(defaults.content_type))]),
                ),
                (
                    "paramsSerializer",
                    Js::function("serialize", move |_, args| {
                        Ok(Js::str(&serialize(&query(&arg(&args, 0))?)))
                    }),
                ),
            ]),
            &safe_cex_exchange::js_runtime::value::from_typed(options)?,
            &exchange.base,
        )?;
        *self.xhr.borrow_mut() = Some(xhr.clone());
        Ok(Rc::new(Wire {
            runtime: self.http.runtime(),
            xhr,
        }))
    }
    fn install_network_retry(
        &self,
        _wire: &Rc<dyn ApiWire>,
        retries: usize,
        reads: ReadClassification,
    ) -> Result<(), ExchangeError> {
        let condition = Js::function("networkErrorOnRead", move |_, args| {
            let error = arg(&args, 0);
            let config = error.get("config")?;
            if config.is_nullish() || !error.get("response")?.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let request = RestRequest {
                method: config.get("method")?.to_display(),
                url: config.get("url")?.to_display(),
                ..Default::default()
            };
            if !matches!(reads.classify(request.clone()), ClassifiedRequest::Read(_)) {
                return Ok(Js::Bool(false));
            }
            let code = error.get("code")?;
            let failure = safe_cex_transport::RestFailure::from_transport(
                safe_cex_transport::TransportError::Network(
                    safe_cex_transport::NetworkFailure::from_code(
                        error.get("message")?.to_display(),
                        if code.truthy() {
                            Some(code.to_display())
                        } else {
                            None
                        },
                    ),
                ),
                request,
            );
            Ok(Js::Bool(failure.is_retryable_network()))
        });
        self.xhr
            .borrow()
            .as_ref()
            .expect("create precedes retry")
            .call_method(
                "__configureRetry",
                vec![Js::object(vec![
                    ("retries", Js::Number(retries as f64)),
                    ("retryCondition", condition),
                ])],
            )?;
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        _wire: &Rc<dyn ApiWire>,
        options: &ExchangeOptions,
        exchange: &Rc<BreakoutpropExchange>,
    ) -> Result<(), ExchangeError> {
        client::factory::install_transport_adapter(
            self.http.clone(),
            &*self.provider,
            self.xhr
                .borrow()
                .as_ref()
                .expect("create precedes transport"),
            &safe_cex_exchange::js_runtime::value::from_typed(options)?,
            exchange.base.name.clone(),
            safe_cex_transport::RestAdapterVariant::Default,
        )?;
        Ok(())
    }
}
impl ApiWire for Wire {
    fn install_request_interceptor(&self, api: Weak<api::DxApi>) -> Result<(), ExchangeError> {
        let interceptor = Js::function("dxSessionAuth", move |_, args| {
            let config = arg(&args, 0);
            let api = api
                .upgrade()
                .ok_or_else(|| Thrown::from(ExchangeError::new("DXtrade API dropped")))?;
            let mut projected = api::DxConfig {
                request: RestRequest {
                    url: config.get("url")?.to_display(),
                    params: if config.get("params")?.is_undefined() {
                        None
                    } else {
                        Some(
                            safe_cex_exchange::js_runtime::value::to_retained_input(
                                &config.get("params")?,
                            )?
                            .into_value()
                            .map_err(|error| Thrown::type_error(&error.to_string()))?,
                        )
                    },
                    ..Default::default()
                },
                session_retried: false,
            };
            api.intercept_request(&mut projected)?;
            let next = Js::object(vec![]);
            for (key, value) in config.own_entries()? {
                next.define_key(&key, value)?;
            }
            next.define("url", Js::str(&projected.request.url))?;
            next.define(
                "params",
                from_json(projected.request.params.as_ref().unwrap()),
            )?;
            let headers = config.get("headers")?;
            for (key, value) in projected.request.headers {
                headers.call_method("set", vec![Js::str(&key), Js::str(&value), Js::Bool(true)])?;
            }
            next.define("headers", headers)?;
            Ok(next)
        });
        self.xhr
            .get("interceptors")?
            .get("request")?
            .call_method("use", vec![interceptor])?;
        Ok(())
    }
    fn rate_limit(&self, max_rps: f64) -> Result<(), ExchangeError> {
        self.xhr.call_method(
            "__configureRateLimit",
            vec![Js::object(vec![("maxRPS", Js::Number(max_rps))])],
        )?;
        Ok(())
    }
    fn request(
        &self,
        config: Config,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<JsNumberInput, ApiFailure>>>>
    {
        let xhr = self.xhr.clone();
        let runtime = self.runtime.clone();
        let request = config.borrow().request.clone();
        Box::pin(async move {
            let result = async {
                let body = match request.body {
                    None => Js::Undefined,
                    Some(RestBody::Untransformed(value)) => {
                        safe_cex_exchange::js_runtime::value::from_retained_input(value)
                    }
                    Some(RestBody::Encoded(bytes)) => Js::str(
                        &String::from_utf8(bytes)
                            .map_err(|e| Thrown::type_error(&e.to_string()))?,
                    ),
                    Some(RestBody::Transformed(value)) => {
                        safe_cex_exchange::js_runtime::value::from_retained_input(
                            value.value().clone(),
                        )
                    }
                };
                let mut fields = vec![
                    ("method", Js::str(&request.method)),
                    ("url", Js::str(&request.url)),
                    ("data", body),
                    (
                        "headers",
                        from_json(
                            &serde_json::to_value(request.headers)
                                .map_err(|e| Thrown::type_error(&e.to_string()))?,
                        ),
                    ),
                ];
                if let Some(params) = request.params {
                    fields.push(("params", from_json(&params)));
                }
                if let Some(timeout) = request.timeout_ms {
                    fields.push(("timeout", Js::Number(timeout as f64)));
                }
                let response = await_js(
                    &runtime,
                    xhr.call_method("request", vec![Js::object(fields)])?,
                )
                .await?;
                to_typed::<JsNumberInput>(&response.get("data")?)
            }
            .await;
            result.map_err(|error: Thrown| {
                let response = error.value.get("response").unwrap_or(Js::Undefined);
                let status = response.get("status").ok().and_then(|v| v.as_f64());
                let has_config = error.value.get("config").is_ok_and(|v| !v.is_nullish());
                let mut native = error.to_exchange_error();
                if !response.is_undefined() {
                    match safe_cex_exchange::js_runtime::value::to_retained_input(&response) {
                        Ok(response) => native = native.with_retained_response(response),
                        Err(cause) => {
                            return ApiFailure {
                                error: cause.into(),
                                status,
                                config: has_config.then_some(config),
                            };
                        }
                    }
                }
                if let Ok(code) = error.value.get("code")
                    && (error.value.has_own("code") || !code.is_undefined())
                {
                    native.code = Some(code);
                }
                if has_config
                    && let Ok(config) = error.value.get("config").and_then(|v| to_json(&v))
                {
                    native.config = Some(config);
                }
                native.is_axios_error |= error.value.get("isAxiosError").is_ok_and(|v| v.truthy());
                ApiFailure {
                    error: native,
                    status,
                    config: has_config.then_some(config),
                }
            })
        })
    }
}
fn query(value: &Js) -> Result<QueryValue, Thrown> {
    Ok(match value {
        Js::Undefined => QueryValue::Undefined,
        Js::Null => QueryValue::Null,
        Js::Bool(v) => QueryValue::Bool(*v),
        Js::Number(v) => QueryValue::Number(*v),
        Js::String(v) => QueryValue::String(
            String::from_utf16(v)
                .map_err(|_| Thrown::type_error("query exceeds native Unicode boundary"))?,
        ),
        value if value.is_array() => QueryValue::Array(
            value
                .array_values()?
                .unwrap_or_default()
                .iter()
                .map(query)
                .collect::<Result<_, _>>()?,
        ),
        value => QueryValue::Object(
            value
                .own_entries()?
                .into_iter()
                .map(|(key, v)| {
                    Ok((
                        String::from_utf16(&key).map_err(|_| {
                            Thrown::type_error("query key exceeds native Unicode boundary")
                        })?,
                        query(&v)?,
                    ))
                })
                .collect::<Result<_, Thrown>>()?,
        ),
    })
}

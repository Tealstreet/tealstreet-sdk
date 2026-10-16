use super::hosts::{Shared, error, js};
use crate::{
    api_runtime::{ApexApi, ApexApiConfiguration, ApexApiHost},
    exchange::ApexRequest,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::{
        client::{self, JsHttpHost},
        promise::{JsFuture, Promise, await_js},
        value::{Js, Thrown, arg, to_js_json},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_num::JsJsonValue as J;
use safe_cex_transport::{
    RestAdapterContext, RestAdapterVariant, RestRequest, RestTransportContext,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};

struct Route {
    scope: Rc<Shared>,
    sender: RefCell<Rc<dyn JsHttpHost>>,
    authorize: Option<std::sync::Arc<dyn safe_cex_transport::RestAuthorizer>>,
}
impl JsHttpHost for Route {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.scope.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let sender = self.sender.borrow().clone();
        let auth = self.authorize.clone();
        Box::pin(async move {
            if let Some(auth) = auth {
                let mut request = RestRequest {
                    method: config.get("method")?.to_display(),
                    url: config.get("url")?.to_display(),
                    ..Default::default()
                };
                auth.authorize(&mut request)
                    .await
                    .map_err(|e| Thrown::error("Error", &e.to_string()))?;
                for (name, value) in request.headers {
                    config.get("headers")?.define(&name, Js::str(&value))?;
                }
            }
            sender.send(config).await
        })
    }
}
struct ApiHost {
    scope: Rc<Shared>,
    client: OnceCell<Js>,
    route: OnceCell<Rc<Route>>,
}
pub(super) struct ApiBridge {
    // Own the API while its installed interceptors retain only Weak<ApexApi>.
    _api: Rc<ApexApi>,
    host: Rc<ApiHost>,
}
impl ApiBridge {
    pub fn new(
        scope: Rc<Shared>,
        options: ExchangeOptions,
        base: Rc<BaseExchange>,
        max_rps: Option<u8>,
    ) -> Result<Rc<Self>, ExchangeError> {
        let host = Rc::new(ApiHost {
            scope,
            client: OnceCell::new(),
            route: OnceCell::new(),
        });
        let api = crate::api_runtime::create_api(options, base, host.clone())?;
        if let Some(max) = max_rps {
            host.client()
                .call_method(
                    "__configureRateLimit",
                    vec![Js::object(vec![("maxRPS", Js::Number(f64::from(max)))])],
                )
                .map_err(error)?;
        }
        Ok(Rc::new(Self { _api: api, host }))
    }
    pub async fn request(&self, request: ApexRequest) -> Result<J, ExchangeError> {
        let config = Js::object(vec![
            ("method", Js::str(request.method)),
            ("url", Js::str(request.path)),
            ("params", js(&request.params)),
            ("data", js(&request.data)),
        ]);
        if let Some(value) = request.content_type {
            config
                .define(
                    "headers",
                    Js::object(vec![("Content-Type", Js::str(value))]),
                )
                .map_err(error)?;
        }
        if let Some(value) = request.timeout_ms {
            config.define("timeout", Js::Number(value)).map_err(error)?;
        }
        let response = await_js(
            self.host
                .client()
                .call_method("request", vec![config])
                .map_err(error)?,
        )
        .await
        .map_err(error)?;
        // Source callers destructure Axios response.data before reading data/code.
        response
            .get("data")
            .and_then(|data| to_js_json(&data))
            .map_err(error)
    }
}
impl ApiHost {
    fn client(&self) -> &Js {
        self.client.get().expect("API factory creates client first")
    }
}
impl ApexApiHost for ApiHost {
    fn create_client(
        &self,
        config: ApexApiConfiguration,
        options: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        let provider = self.scope.runtime.transport();
        let request = RestRequest {
            base_url: Some(config.base_url.into()),
            ..Default::default()
        };
        let routing = provider
            .rest(RestTransportContext {
                exchange: &ExchangeName::Apexomni,
                options,
                config: &request,
            })
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let route = Rc::new(Route {
            scope: self.scope.clone(),
            sender: RefCell::new(self.scope.http.clone()),
            authorize: routing.authorize,
        });
        let serializer = config.params_serializer;
        let client = client::create(
            route.clone(),
            Js::object(vec![
                (
                    "baseURL",
                    Js::str(routing.base_url.as_deref().unwrap_or(config.base_url)),
                ),
                ("timeout", Js::Number(config.timeout_ms)),
                ("headers", js(&config.headers)),
                (
                    "paramsSerializer",
                    Js::function("serialize", move |_, args| {
                        serializer(&to_js_json(&arg(&args, 0))?)
                            .map(|v| Js::str(&v))
                            .map_err(|e| Thrown::error("Error", &e))
                    }),
                ),
            ]),
        );
        self.route
            .set(route)
            .map_err(|_| ExchangeError::new("duplicate Apex route"))?;
        self.client
            .set(client)
            .map_err(|_| ExchangeError::new("duplicate Apex client"))?;
        Ok(())
    }
    fn configure_retry(&self, retries: u8) -> Result<(), ExchangeError> {
        self.client()
            .call_method(
                "__configureRetry",
                vec![Js::object(vec![
                    ("retries", Js::Number(f64::from(retries))),
                    (
                        "retryCondition",
                        Js::function("networkErrorOnRead", |_, args| {
                            let failure = arg(&args, 0);
                            let config = failure.get("config")?;
                            if config.is_nullish() {
                                return Ok(Js::Bool(false));
                            }
                            let request = RestRequest {
                                method: config.get("method")?.to_display(),
                                url: config.get("url")?.to_display(),
                                ..Default::default()
                            };
                            if !matches!(
                                request.method.to_ascii_lowercase().as_str(),
                                "get" | "head"
                            ) {
                                return Ok(Js::Bool(false));
                            }
                            let mut native = safe_cex_transport::RestFailure::pre_transmit(
                                failure.get("message")?.to_display(),
                                request,
                            );
                            let code = failure.get("code")?;
                            if code.is_string() {
                                native.code = Some(code.to_display());
                            }
                            if !failure.get("response")?.is_nullish() {
                                return Ok(Js::Bool(false));
                            }
                            Ok(Js::Bool(safe_cex_transport::retry::is_network_error(
                                &native,
                            )))
                        }),
                    ),
                ])],
            )
            .map_err(error)?;
        Ok(())
    }
    fn register_request_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError> {
        self.client()
            .get("interceptors")
            .and_then(|i| i.get("request"))
            .and_then(|i| {
                i.call_method(
                    "use",
                    vec![Js::function("apexSign", move |_, args| {
                        let owner = api
                            .upgrade()
                            .ok_or_else(|| Thrown::error("Error", "Apex API released"))?;
                        let result = owner
                            .request_handler(&to_js_json(&arg(&args, 0))?)
                            .map_err(Thrown::from)?;
                        Ok(js(&result))
                    })],
                )
            })
            .map_err(error)?;
        Ok(())
    }
    fn register_response_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError> {
        let runtime: Rc<dyn ExchangeRuntime> = self.scope.runtime.clone();
        self.client()
            .get("interceptors")
            .and_then(|i| i.get("response"))
            .and_then(|i| {
                i.call_method(
                    "use",
                    vec![Js::function("apexResponse", move |_, args| {
                        let owner = api
                            .upgrade()
                            .ok_or_else(|| Thrown::error("Error", "Apex API released"))?;
                        let response = to_js_json(&arg(&args, 0))?;
                        Ok(Js::Promise(Promise::spawn(
                            &runtime,
                            Box::pin(async move {
                                owner
                                    .response_handler(&response)
                                    .await
                                    .map(|v| js(&v))
                                    .map_err(Thrown::from)
                            }),
                        )))
                    })],
                )
            })
            .map_err(error)?;
        Ok(())
    }
    fn install_transport_adapter(
        &self,
        options: &ExchangeOptions,
        _: &str,
    ) -> Result<(), ExchangeError> {
        if let Some(adapter) = self
            .scope
            .runtime
            .transport()
            .rest_adapter(RestAdapterContext {
                exchange: &ExchangeName::Apexomni,
                options,
                variant: RestAdapterVariant::Default,
            })
        {
            *self.route.get().expect("route created").sender.borrow_mut() =
                client::rest_http_host(self.scope.runtime.clone(), adapter);
        }
        Ok(())
    }
    fn now_ms(&self) -> f64 {
        self.scope.runtime.now_ms()
    }
    fn legacy_timestamp_ms(&self, now: f64) -> Result<f64, ExchangeError> {
        Ok((safe_cex_num::js_time_clip(now) / 1000.).floor() * 1000.)
    }
    fn axios_headers(&self, fields: J) -> Result<J, ExchangeError> {
        Ok(fields)
    }
    fn set_content_type(&self, headers: &mut J, value: &str) -> Result<(), ExchangeError> {
        let J::Object(fields) = headers else {
            return Err(ExchangeError::new("Apex headers require an object"));
        };
        if let Some((_, old)) = fields
            .iter_mut()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        {
            *old = J::String(value.into());
        } else {
            fields.push(("Content-Type".into(), J::String(value.into())));
        }
        Ok(())
    }
    fn request_retry(&self, config: J) -> ExchangeFuture<'_, J> {
        Box::pin(async move {
            let response = await_js(
                self.client()
                    .call_method("request", vec![js(&config)])
                    .map_err(error)?,
            )
            .await
            .map_err(error)?;
            to_js_json(&response).map_err(error)
        })
    }
}

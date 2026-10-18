use super::hosts::{Shared, error, js};
use crate::{
    api_runtime::{ApexApi, ApexApiConfiguration, ApexApiHost},
    exchange::ApexRequest,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::{
        client,
        promise::{Promise, await_js},
        value::{Js, Thrown, arg, to_js_json, to_retained_input},
    },
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime},
};
use safe_cex_num::JsJsonValue as J;
use safe_cex_transport::{RestAdapterVariant, RestRequest};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{
    cell::OnceCell,
    rc::{Rc, Weak},
};

struct ApiHost {
    scope: Rc<Shared>,
    client: OnceCell<Js>,
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
    pub async fn request(
        &self,
        request: ApexRequest,
    ) -> Result<safe_cex_types::retained_json::RetainedJsonInput, ExchangeError> {
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
            .and_then(|data| to_retained_input(&data))
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
        base: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        let provider = self.scope.runtime.transport();
        let serializer = config.params_serializer;
        let client = client::factory::create(
            self.scope.http.clone(),
            provider.as_ref(),
            Js::object(vec![
                ("baseURL", Js::str(config.base_url)),
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
            &safe_cex_exchange::js_runtime::value::from_typed(options).map_err(error)?,
            base,
        )
        .map_err(error)?;
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
                        let original = arg(&args, 0);
                        let before = to_js_json(&original)?;
                        let result = owner.request_handler(&before).map_err(Thrown::from)?;
                        carry_config(&original, &before, &result)
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
                        let response = arg(&args, 0);
                        Ok(Js::Promise(Promise::spawn(
                            &runtime,
                            Box::pin(async move {
                                owner
                                    .response_handler(&response)
                                    .await
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
        client::factory::install_transport_adapter(
            self.scope.http.clone(),
            self.scope.runtime.transport().as_ref(),
            self.client(),
            &safe_cex_exchange::js_runtime::value::from_typed(options).map_err(error)?,
            ExchangeName::Apexomni,
            RestAdapterVariant::Default,
        )
        .map_err(error)
    }

    fn now_ms(&self) -> f64 {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
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
    fn request_retry(&self, config: Js) -> ExchangeFuture<'_, Js> {
        Box::pin(async move {
            // Native response/config retains the original adapter and serializer.
            let response = await_js(
                self.client()
                    .call_method("request", vec![config])
                    .map_err(error)?,
            )
            .await
            .map_err(error)?;
            Ok(response)
        })
    }
}

// Preserve executable capabilities from the shared config, replacing only the
// fields the typed source interceptor changed. A data-only roundtrip would erase
// paramsSerializer and the transport adapter before the terminal request.
fn carry_config(original: &Js, before: &J, after: &J) -> Result<Js, Thrown> {
    let next = original.spread_object()?;
    let J::Object(fields) = after else {
        return Err(Thrown::type_error("Apex config requires an object"));
    };
    for (name, value) in fields {
        if crate::data::get(before, name) == value {
            continue;
        }
        let mut projected = js(value);
        if name == "headers" {
            let old = original.get("headers")?;
            if let (Js::Object(old), Js::Object(_)) = (&old, &projected) {
                let headers = Js::object_with_class("AxiosHeaders", vec![]);
                if let Js::Object(new) = &headers {
                    *new.proto.borrow_mut() = old.proto.borrow().clone();
                }
                for (key, value) in projected.own_entries()? {
                    headers.define_key(&key, value)?;
                }
                projected = headers;
            }
        }
        next.define(name, projected)?;
    }
    Ok(next)
}

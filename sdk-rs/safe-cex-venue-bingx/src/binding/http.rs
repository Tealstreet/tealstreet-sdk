use super::hosts::{Host, error};
use crate::{
    api::{ApiDefaults, ApiHost, BingxApi},
    data_reads::ReadTransport,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::{
        client,
        promise::await_js,
        value::{Js, Thrown, arg, from_json, from_typed, to_json},
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_transport::{
    RestAdapterVariant, RestBody, RestRequest,
    retry::{ClassifiedRequest, ReadClassification},
    signature_rejection::{IsSignatureRejection, SignatureRetry, signature_rejected_response},
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub(super) struct ClientEntry {
    transport: Weak<dyn ReadTransport>,
    client: Js,
    signer: Rc<RefCell<Weak<BingxApi>>>,
    signature: Rc<RefCell<Option<IsSignatureRejection>>>,
}
struct Transport {
    runtime: Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>,
    client: Js,
    signer: Rc<RefCell<Weak<BingxApi>>>,
    signature: Rc<RefCell<Option<IsSignatureRejection>>>,
}
impl ReadTransport for Transport {
    fn send(&self, request: RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        self.send_with_signature_retry(request, &SignatureRetry::default())
    }
    fn send_with_signature_retry(
        &self,
        request: RestRequest,
        retry: &SignatureRetry,
    ) -> ExchangeFuture<'_, JsNumberInput> {
        let retry = retry.clone();
        Box::pin(async move {
            let original = request.clone();
            let config = Js::object(vec![
                ("url", Js::str(&request.url)),
                ("method", Js::str(&request.method.to_lowercase())),
                (
                    "headers",
                    Js::object(
                        request
                            .headers
                            .iter()
                            .map(|(k, v)| (k.as_str(), Js::str(v)))
                            .collect(),
                    ),
                ),
            ]);
            if let Some(params) = request.params {
                config.define("params", from_json(&params)).map_err(error)?;
            }
            if let Some(body) = request.body {
                let data = match body {
                    RestBody::Untransformed(v) => {
                        safe_cex_exchange::js_runtime::value::from_retained_input(v)
                    }
                    other => Js::str(&String::from_utf8_lossy(
                        &other
                            .wire_bytes()
                            .map_err(|e| ExchangeError::new(e.to_string()))?,
                    )),
                };
                config.define("data", data).map_err(error)?;
            }
            // Each native API call already prepared its first request. Axios network
            // retries re-enter this function, signing the exact serialized payload.
            let first = Cell::new(true);
            let signer = self.signer.borrow().clone();
            config
                .define(
                    "__bingxSignAttempt",
                    Js::function("signAttempt", move |config, _| {
                        if first.replace(false) {
                            return Ok(config);
                        }
                        let api = signer
                            .upgrade()
                            .ok_or_else(|| Thrown::error("Error", "BingX API disposed"))?;
                        let mut next = RestRequest {
                            method: config.get("method")?.to_display(),
                            url: config.get("url")?.to_display(),
                            ..Default::default()
                        };
                        let params = config.get("params")?;
                        if !params.is_nullish() {
                            next.params = Some(to_json(&params)?);
                        }
                        let body = config.get("data")?;
                        if !body.is_nullish() {
                            next.body = Some(RestBody::Untransformed(
                                safe_cex_exchange::js_runtime::value::to_retained_input(&body)?,
                            ));
                        }
                        let next = api.prepare(next).map_err(Thrown::from)?;
                        config.define(
                            "params",
                            next.params.as_ref().map(from_json).unwrap_or(Js::Undefined),
                        )?;
                        config.define(
                            "data",
                            match next.body {
                                Some(RestBody::Untransformed(v)) => {
                                    safe_cex_exchange::js_runtime::value::from_retained_input(v)
                                }
                                _ => Js::Undefined,
                            },
                        )?;
                        for (name, value) in next.headers {
                            config.get("headers")?.define(&name, Js::str(&value))?;
                        }
                        Ok(config)
                    }),
                )
                .map_err(error)?;
            let response = loop {
                match await_js(
                    &self.runtime,
                    self.client
                        .call_method("request", vec![config.clone()])
                        .map_err(error)?,
                )
                .await
                {
                    Ok(response) => break response,
                    Err(thrown) => {
                        let response = thrown
                            .value
                            .get("response")
                            .ok()
                            .filter(|r| !r.is_nullish());
                        let mut failure = safe_cex_transport::RestFailure::pre_transmit(
                            thrown.value.get("message").map_err(error)?.to_display(),
                            original.clone(),
                        );
                        if let Some(response) = response {
                            let raw = response.get("data").map_err(error)?;
                            let data =
                                safe_cex_exchange::js_runtime::value::to_retained_input(&raw)
                                    .map_err(error)?;
                            failure.response = Some(Box::new(safe_cex_transport::RestResponse {
                                status: response
                                    .get("status")
                                    .map_err(error)?
                                    .to_number()
                                    .map_err(error)? as u16,
                                reason: None,
                                headers: Default::default(),
                                body: safe_cex_transport::RestResponseBody::Untransformed(data),
                            }));
                        }
                        let predicate = self
                            .signature
                            .borrow()
                            .clone()
                            .expect("factory configures signature retry");
                        if signature_rejected_response(&failure, &retry, &predicate)
                            .map_err(ExchangeError::from)?
                        {
                            continue;
                        }
                        return Err(error(thrown));
                    }
                }
            };
            let data = response.get("data").map_err(error)?;
            let value = to_json(&data).map_err(error)?;
            serde_json::from_value(value).map_err(|e| ExchangeError::new(e.to_string()))
        })
    }
}
pub(super) struct Secondary {
    pub primary: Rc<dyn ReadTransport>,
    pub reader: Rc<dyn ReadTransport>,
}
impl ReadTransport for Secondary {
    fn send(&self, request: RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        if request.method.eq_ignore_ascii_case("get") {
            self.reader.send(request)
        } else {
            self.primary.send(request)
        }
    }
}
impl Host {
    pub fn configure_signing(&self, api: &Rc<BingxApi>) -> Result<(), ExchangeError> {
        let entries = self.clients.borrow();
        let entry = entries
            .iter()
            .find(|entry| {
                entry
                    .transport
                    .upgrade()
                    .is_some_and(|t| Rc::ptr_eq(&t, &api.client))
            })
            .ok_or_else(|| ExchangeError::new("BingX client missing"))?;
        *entry.signer.borrow_mut() = Rc::downgrade(api);
        Ok(())
    }
    fn client(&self, transport: &Rc<dyn ReadTransport>) -> Result<Js, ExchangeError> {
        self.clients
            .borrow()
            .iter()
            .find_map(|entry| {
                entry
                    .transport
                    .upgrade()
                    .filter(|t| Rc::ptr_eq(t, transport))
                    .map(|_| entry.client.clone())
            })
            .ok_or_else(|| ExchangeError::new("BingX HTTP client was not created by this host"))
    }
    pub fn configure_limit(&self, api: &Rc<BingxApi>, rps: f64) -> Result<(), ExchangeError> {
        self.configure_signing(api)?;
        let client = self.client(&api.client)?;
        client
            .call_method(
                "__configureRateLimit",
                vec![from_json(&json!({"maxRPS":rps}))],
            )
            .map_err(error)?;
        Ok(())
    }
}
impl ApiHost for Host {
    fn has_remote_signer(&self) -> bool {
        self.scope.http.hosted_signer().is_some()
    }
    fn create(
        &self,
        defaults: ApiDefaults,
        options: &ExchangeOptions,
        base: &Rc<BaseExchange>,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError> {
        if self.scope.base.get().is_none() {
            let _ = self.scope.base.set(base.clone());
        }
        let provider = self.scope.runtime.transport();
        let serializer = defaults.params_serializer;
        let client = client::factory::create(
            self.scope.http.clone(),
            provider.as_ref(),
            Js::object(vec![
                ("baseURL", Js::str(&defaults.base_url)),
                ("timeout", Js::Number(defaults.timeout)),
                (
                    "transformResponse",
                    Js::function("bigIntAsStringJsonParse", |_, args| {
                        let value = arg(&args, 0);
                        if !value.is_string() {
                            return Ok(value);
                        }
                        Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
                            safe_cex_transport::how::big_int_as_string_json_parse(
                                &value.to_display(),
                            ),
                        ))
                    }),
                ),
                (
                    "headers",
                    Js::object(
                        defaults
                            .headers
                            .iter()
                            .map(|(k, v)| (k.as_str(), Js::str(v)))
                            .collect(),
                    ),
                ),
                (
                    "paramsSerializer",
                    Js::function("serialize", move |_, args| {
                        let value = to_json(&arg(&args, 0))?;
                        serializer(&value)
                            .map(|v| Js::str(&v))
                            .map_err(Thrown::from)
                    }),
                ),
            ]),
            &from_typed(options).map_err(error)?,
            base,
        )
        .map_err(error)?;
        client
            .get("interceptors")
            .and_then(|i| i.get("request"))
            .and_then(|i| {
                i.call_method(
                    "use",
                    vec![Js::function("retrySigning", |_, args| {
                        let config = arg(&args, 0);
                        let prepare = config.get("__bingxSignAttempt")?;
                        if prepare.is_callable() {
                            prepare.call(config, vec![])
                        } else {
                            Ok(config)
                        }
                    })],
                )
            })
            .map_err(error)?;
        let signer = Rc::new(RefCell::new(Weak::new()));
        let signature = Rc::new(RefCell::new(None));
        let transport: Rc<dyn ReadTransport> = Rc::new(Transport {
            runtime: self.scope.http.runtime(),
            client: client.clone(),
            signer: signer.clone(),
            signature: signature.clone(),
        });
        self.clients.borrow_mut().push(ClientEntry {
            transport: Rc::downgrade(&transport),
            client,
            signer,
            signature,
        });
        Ok(transport)
    }
    fn retry(
        &self,
        transport: &Rc<dyn ReadTransport>,
        retries: usize,
        reads: ReadClassification,
        signature: IsSignatureRejection,
    ) -> Result<(), ExchangeError> {
        for entry in self.clients.borrow().iter() {
            if entry
                .transport
                .upgrade()
                .is_some_and(|t| Rc::ptr_eq(&t, transport))
            {
                *entry.signature.borrow_mut() = Some(signature.clone());
            }
        }
        let client = self.client(transport)?;
        let condition = Js::function("retryRead", move |_, args| {
            let failure = arg(&args, 0);
            let config = failure.get("config")?;
            let request = RestRequest {
                method: config.get("method")?.to_display(),
                url: config.get("url")?.to_display(),
                ..Default::default()
            };
            if !matches!(reads.classify(request.clone()), ClassifiedRequest::Read(_)) {
                return Ok(Js::Bool(false));
            }
            let status = failure
                .get("response")?
                .get("status")
                .ok()
                .and_then(|s| s.as_f64());
            let mut native = safe_cex_transport::RestFailure::pre_transmit(
                failure.get("message")?.to_display(),
                request,
            );
            let code = failure.get("code")?;
            if code.is_string() {
                native.code = Some(code.to_display());
            }
            native.response = status.map(|s| {
                Box::new(safe_cex_transport::RestResponse {
                    status: s as u16,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::Null,
                    ),
                })
            });
            Ok(Js::Bool(
                safe_cex_transport::retry::is_network_or_idempotent_request_error(&native),
            ))
        });
        client
            .call_method(
                "__configureRetry",
                vec![Js::object(vec![
                    ("retries", Js::Number(retries as f64)),
                    ("retryCondition", condition),
                ])],
            )
            .map_err(error)?;
        Ok(())
    }
    fn install_adapter(
        &self,
        client: &Rc<dyn ReadTransport>,
        options: &ExchangeOptions,
        _: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError> {
        client::factory::install_transport_adapter(
            self.scope.http.clone(),
            self.scope.runtime.transport().as_ref(),
            &self.client(client)?,
            &from_typed(options).map_err(error)?,
            ExchangeName::Bingx,
            RestAdapterVariant::Default,
        )
        .map_err(error)
    }
    fn serialize(
        &self,
        params: &safe_cex_types::retained_json::RetainedJsonInput,
    ) -> Result<String, ExchangeError> {
        self.scope
            .globals
            .get("qs")
            .and_then(|q| {
                let collator = safe_cex_num::JsRootCollator::try_new()
                    .map_err(|e| Thrown::error(e.name, e.message))?;
                let sort = Js::function("localeCompare", move |_, args| {
                    let left =
                        String::from_utf16(&arg(&args, 0).to_js_string()?).map_err(|_| {
                            Thrown::error(
                                "RepresentationError",
                                "query key is outside the shared string boundary",
                            )
                        })?;
                    let right =
                        String::from_utf16(&arg(&args, 1).to_js_string()?).map_err(|_| {
                            Thrown::error(
                                "RepresentationError",
                                "query key is outside the shared string boundary",
                            )
                        })?;
                    Ok(Js::Number(match collator.compare(&left, &right) {
                        std::cmp::Ordering::Less => -1.0,
                        std::cmp::Ordering::Equal => 0.0,
                        std::cmp::Ordering::Greater => 1.0,
                    }))
                });
                q.call_method(
                    "stringify",
                    vec![
                        safe_cex_exchange::js_runtime::value::from_retained_input(params.clone()),
                        Js::object(vec![("arrayFormat", Js::str("repeat")), ("sort", sort)]),
                    ],
                )
            })
            .map(|s| s.to_display())
            .map_err(error)
    }
    fn hmac_sha256_hex(&self, secret: &str, message: &str) -> Result<String, ExchangeError> {
        self.global("createHmac", vec![Js::str("sha256"), Js::str(secret)])?
            .call_method("update", vec![Js::str(message)])
            .and_then(|h| h.call_method("digest", vec![Js::str("hex")]))
            .map(|s| s.to_display())
            .map_err(error)
    }
    fn now_ms(&self) -> f64 {
        safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
    }
}

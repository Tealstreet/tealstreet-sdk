//! WOO HTTP factory over shared signing, transform and classified-read retry operations.
use crate::{
    api::{sign_v1, sign_v3},
    constants::{BROKER_ID, TESTNET_BROKER_ID},
};
use safe_cex_exchange::js_runtime::value::{
    Js, from_json, from_retained_input, to_json, to_retained_input,
};
use safe_cex_transport::TransportProvider;
use safe_cex_types::ExchangeOptions;

pub(crate) async fn prepare_attempt(
    input: &Js,
    options: &ExchangeOptions,
    remote_signer: bool,
    provider: &dyn TransportProvider,
    timestamp: f64,
) -> Result<Js, String> {
    let next = input.spread_object().map_err(|error| error.message())?;
    let url = next.get("url").map_err(|error| error.message())?;
    let url = url
        .as_str()
        .map(|units| String::from_utf16(units))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    if url.contains("/public/") {
        return Ok(next);
    }
    let method = next.get("method").map_err(|error| error.message())?;
    let method = method
        .as_str()
        .map(|units| String::from_utf16(units))
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| "get".into());
    let broker_key = if method.eq_ignore_ascii_case("post") {
        match url.as_str() {
            "/v1/order" => Some("broker_id"),
            "/v3/algo/order" => Some("brokerId"),
            _ => None,
        }
    } else {
        None
    };
    let broker = if options.testnet == Some(true) {
        TESTNET_BROKER_ID
    } else {
        BROKER_ID
    };
    let mut array_property = None;
    if let Some(key) = broker_key {
        let data = next.get("data").map_err(|error| error.message())?;
        match &data {
            Js::Object(_) => data
                .set(key, Js::str(broker))
                .map_err(|error| error.message())?,
            Js::Array(_) => {
                array_property = Some(key);
            }
            // Source catches failed property assignment and returns UNSIGNED.
            _ => return Ok(next),
        }
    }
    if remote_signer || options.key.is_empty() || options.secret.is_empty() {
        return Ok(next);
    }
    if url.contains("v1") {
        if let Some(key) = array_property {
            // qs observes the named array property; JSON array output omits it.
            let data = next
                .get("data")
                .map_err(|error| error.message())?
                .spread_object()
                .map_err(|error| error.message())?;
            data.set(key, Js::str(broker))
                .map_err(|error| error.message())?;
            next.set("data", data).map_err(|error| error.message())?;
        }
        Ok(sign_v1(&next, options, provider, timestamp).await?.config)
    } else {
        Ok(sign_v3(&next, options, provider, timestamp).await?.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    fn ready<F: Future>(future: F) -> F::Output {
        match std::pin::pin!(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("fixture HMAC unexpectedly yielded"),
        }
    }
    #[test]
    fn factory_interceptor_matches_preimplementation_captures() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/fixtures/signing.json")).unwrap();
        let mut count = 0;
        for case in cases.iter().filter(|case| case["version"] == "factory") {
            let extra = &case["options"];
            let options: ExchangeOptions = serde_json::from_value(serde_json::json!({
                "key": extra.get("key").unwrap_or(&Value::String("fixture-key".into())),
                "secret": extra.get("secret").unwrap_or(&Value::String("fixture-secret".into())),
                "testnet": extra.get("testnet").unwrap_or(&Value::Bool(false))
            }))
            .unwrap();
            let result = ready(prepare_attempt(
                &from_json(&case["input"]),
                &options,
                extra.get("remoteSigner").is_some(),
                &safe_cex_transport::DirectTransport,
                1700000000123.0,
            ))
            .unwrap();
            let result = to_json(&result).unwrap();
            let expected = &case["adapterInput"];
            assert_eq!(result["url"], expected["url"], "{} URL", case["name"]);
            let data = result.get("data").map(|data| match data {
                Value::Null | Value::String(_) => data.clone(),
                _ => Value::String(
                    safe_cex_num::js_json_stringify(&data.clone().into())
                        .unwrap()
                        .unwrap(),
                ),
            });
            assert_eq!(
                data.as_ref(),
                expected.get("body"),
                "{} body view",
                case["name"]
            );
            assert_eq!(
                result["headers"].get("x-api-signature"),
                expected["headers"].get("x-api-signature"),
                "{} signing bypass",
                case["name"]
            );
            count += 1;
        }
        assert_eq!(count, 14);
    }
}

use safe_cex_exchange::http::{
    HttpClient, HttpClientOptions, install_transport_adapter, safe_cex_axios_create,
};
use safe_cex_transport::{
    RestAdapter, RestAdapterVariant, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    TransportError, TransportRegistry,
    rate_limit::RateLimitClock,
    retry::{ClassifiedRequest, ReadClassification, network_error_on_read},
};
use std::sync::Arc;

struct Attempt {
    client: HttpClient,
    options: ExchangeOptions,
    provider: Arc<dyn TransportProvider>,
    remote: bool,
    now: Arc<dyn Fn() -> u64>,
}
impl RestAdapter for Attempt {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            let config = from_json(
                &serde_json::json!({"url":request.url,"method":request.method,"headers":request.headers}),
            );
            if let Some(params) = &request.params {
                config
                    .set("params", from_json(params))
                    .map_err(|error| RestFailure::pre_transmit(error.message(), request.clone()))?;
            }
            if let Some(body) = &request.body {
                config
                    .set(
                        "data",
                        from_retained_input(
                            body.untransformed()
                                .map_err(|error| {
                                    RestFailure::from_transport(error, request.clone())
                                })?
                                .clone(),
                        ),
                    )
                    .map_err(|error| RestFailure::pre_transmit(error.message(), request.clone()))?;
            }
            let next = prepare_attempt(
                &config,
                &self.options,
                self.remote,
                self.provider.as_ref(),
                (self.now)() as f64,
            )
            .await
            .map_err(|error| RestFailure::pre_transmit(error, request.clone()))?;
            let read = |key| {
                next.get(key)
                    .map_err(|error| RestFailure::pre_transmit(error.message(), request.clone()))
            };
            let next_url = read("url")?;
            let next_method = read("method")?;
            let params = read("params")?;
            let data = read("data")?;
            let headers = read("headers")?;
            if let Some(units) = next_url.as_str() {
                request.url = String::from_utf16(units).map_err(|error| {
                    RestFailure::pre_transmit(error.to_string(), request.clone())
                })?;
            }
            if let Some(units) = next_method.as_str() {
                request.method = String::from_utf16(units).map_err(|error| {
                    RestFailure::pre_transmit(error.to_string(), request.clone())
                })?;
            }
            request.params =
                if params.is_undefined() {
                    None
                } else {
                    Some(to_json(&params).map_err(|error| {
                        RestFailure::pre_transmit(error.message(), request.clone())
                    })?)
                };
            request.body = if next.has_own("data") {
                let data = to_retained_input(&data)
                    .map_err(|error| RestFailure::pre_transmit(error.message(), request.clone()))?;
                Some(
                    match &request.body {
                        Some(body) => body.with_signed_data(data),
                        None => Ok(RestBody::Untransformed(data)),
                    }
                    .map_err(|error| RestFailure::from_transport(error, request.clone()))?,
                )
            } else {
                None
            };
            if matches!(headers, Js::Object(_)) {
                request.headers = headers
                    .own_entries()
                    .map_err(|error| RestFailure::pre_transmit(error.message(), request.clone()))?
                    .into_iter()
                    .filter_map(|(key, value)| {
                        value.as_str().map(|units| {
                            String::from_utf16(&key)
                                .and_then(|key| String::from_utf16(units).map(|value| (key, value)))
                        })
                    })
                    .collect::<Result<_, _>>()
                    .map_err(|error| {
                        RestFailure::pre_transmit(error.to_string(), request.clone())
                    })?;
            }
            self.client.execute_with_receipt(request).await
        })
    }
}
struct Api {
    attempt: Attempt,
    clock: Arc<dyn RateLimitClock>,
    timeout: i64,
}
impl RestAdapter for Api {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        mut request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            if request.method.is_empty() {
                request.method = "get".into();
            }
            if request.timeout_ms.is_none() {
                request.timeout_ms = Some(self.timeout);
            }
            match ReadClassification::Methods.classify(request) {
                ClassifiedRequest::Read(read) => {
                    network_error_on_read(read, 3)
                        .execute_with_receipt(&self.attempt, self.clock.as_ref())
                        .await
                }
                ClassifiedRequest::Write(write) => write.execute_with_receipt(&self.attempt).await,
            }
        })
    }
}

/// Builds the WOO client on the shared HTTP and classified-read retry machinery.
/// Host options carry the store/runtime/logging bridge; no venue-local task owner.
pub fn create_api(
    mut host: HttpClientOptions,
    registry: &TransportRegistry,
    clock: Arc<dyn RateLimitClock>,
) -> Result<impl RestAdapter + use<>, TransportError> {
    let options = host.options.clone();
    let timeout = options
        .extra
        .as_ref()
        .and_then(|extra| extra.recv_window)
        .unwrap_or(20000.0);
    if !timeout.is_finite()
        || timeout.fract() != 0.0
        || timeout < i64::MIN as f64
        || timeout >= i64::MAX as f64
    {
        return Err(TransportError::Host(
            "WOO timeout is outside the shared integer-millisecond request boundary".into(),
        ));
    }
    let provider = registry.get_transport(host.provider.as_ref());
    let remote = host.signer.is_some();
    let now = host.signing_now.clone();
    host.exchange = safe_cex_types::ExchangeName::Woo;
    host.request.base_url = Some(
        if options.testnet == Some(true) {
            "https://api.staging.woox.io"
        } else {
            "https://api.woox.io"
        }
        .into(),
    );
    host.request
        .headers
        .insert("cache-control".into(), "no-cache".into());
    host.request
        .headers
        .insert("x-api-key".into(), options.key.clone());
    host.params_serializer = Some(Arc::new(|params| {
        crate::api::sorted_query(&from_json(params), false).map_err(TransportError::Host)
    }));
    let client = safe_cex_axios_create(host, registry)?;
    if options.tealstreet_tunnel == Some(true) {
        install_transport_adapter(
            &client,
            registry,
            &options,
            Some(&provider),
            RestAdapterVariant::Default,
        );
    }
    Ok(Api {
        attempt: Attempt {
            client,
            options,
            provider,
            remote,
            now,
        },
        clock,
        timeout: timeout as i64,
    })
}

#[cfg(test)]
#[path = "../tests/support/factory_wire.rs"]
mod wire_tests;

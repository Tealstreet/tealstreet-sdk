//! KuCoin API factories composed from the shared HTTP, signing and retry layers.
use crate::{api_helpers, broker, spot_types, types};
use safe_cex_crypto::{Encoding, SigningOptions, sign_payload};
use safe_cex_exchange::http::{
    HttpClient, HttpClientOptions, install_transport_adapter, safe_cex_axios_create,
};
use safe_cex_num::{JsJsonValue, js_number_string, js_truthy};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestAdapterVariant, RestFailure, RestFuture, RestRequest, RestResponse,
    TransportError, TransportRegistry,
    rate_limit::RateLimitClock,
    retry::reads_and_posted,
    signature_rejection::{
        IsSignatureRejection, SignatureRetry, resend_on_signature_rejection,
        retry_network_reads_and_signature_rejection,
    },
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::{rc::Rc, sync::Arc};

// ts: createAPI
pub async fn create_api(
    http: HttpClientOptions,
    registry: &TransportRegistry,
    clock: VirtualClock,
    retry_clock: Rc<dyn RateLimitClock>,
) -> Result<impl RestAdapter + use<>, TransportError> {
    create(http, registry, clock, retry_clock, false).await
}
// ts: createKucoinSpotAPI
pub async fn create_kucoin_spot_api(
    http: HttpClientOptions,
    registry: &TransportRegistry,
    clock: VirtualClock,
    retry_clock: Rc<dyn RateLimitClock>,
) -> Result<impl RestAdapter + use<>, TransportError> {
    create(http, registry, clock, retry_clock, true).await
}
struct Api {
    attempt: Attempt,
    retry_clock: Rc<dyn RateLimitClock>,
}
struct Attempt {
    client: HttpClient,
    options: ExchangeOptions,
    clock: VirtualClock,
    passphrase: String,
    spot: bool,
    remote: bool,
}
async fn hmac(options: &ExchangeOptions, payload: &str) -> Result<String, TransportError> {
    sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &options.secret,
            payload_signer: None,
        },
        ExchangeName::Kucoin,
        payload,
        Encoding::Base64,
    )
    .await
    .map(|result| result.signature)
    .map_err(TransportError::Host)
}
async fn create(
    mut http: HttpClientOptions,
    registry: &TransportRegistry,
    clock: VirtualClock,
    retry_clock: Rc<dyn RateLimitClock>,
    spot: bool,
) -> Result<Api, TransportError> {
    let options = http.options.clone();
    let passphrase = hmac(&options, options.passphrase.as_deref().unwrap_or_default()).await?;
    // In-process capabilities are supplied by HttpClientOptions, not the
    // uninhabited serde marker in ExchangeOptions.
    let remote = http.signer.is_some();
    http.exchange = if spot {
        ExchangeName::Kucoinspot
    } else {
        ExchangeName::Kucoin
    };
    http.request.base_url = Some(
        if spot {
            spot_types::KUCOIN_SPOT_BASE_URL
        } else {
            types::BASE_URL
        }
        .into(),
    );
    // safeCexAxiosCreate overwrites recvWindow with 20_000 before Axios sees it.
    // Do not reject a fractional recvWindow that the source never uses.
    http.request.timeout_ms = Some(20000);
    let mut headers = indexmap::IndexMap::from([
        ("Accept".into(), "application/json, text/plain, */*".into()),
        (
            "X-CHANNEL-API-CODE".into(),
            if spot {
                spot_types::KUCOIN_SPOT_BROKER_ID
            } else {
                types::BROKER_ID
            }
            .into(),
        ),
        ("Content-Type".into(), "application/json".into()),
    ]);
    if !spot && options.public_data_only != Some(true) {
        headers.insert("KC-API-KEY".into(), options.key.clone());
        headers.insert("KC-API-PASSPHRASE".into(), passphrase.clone());
        headers.insert("KC-API-KEY-VERSION".into(), "2".into());
    }
    http.request.headers = headers;
    // Additive native-query callers inherit the same indexed/encoded qs profile;
    // legacy Value callers continue using their existing serializer unchanged.
    http.request.query_profile = Some(safe_cex_transport::query::QueryProfile {
        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
        arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
        encode: true,
    });
    http.params_serializer = Some(Arc::new(|params| {
        Ok(api_helpers::stringify_query(
            &JsJsonValue::from(params.clone()),
            true,
        ))
    }));
    let provider = http.provider.clone();
    let client = safe_cex_axios_create(http, registry)?;
    if options.public_data_only != Some(true) {
        install_transport_adapter(
            &client,
            registry,
            &options,
            provider.as_ref(),
            RestAdapterVariant::Default,
        );
    }
    Ok(Api {
        attempt: Attempt {
            client,
            options,
            clock,
            passphrase,
            spot,
            remote,
        },
        retry_clock,
    })
}
// ts: isKucoinSignatureRejection, shared with KuCoin spot.
pub fn is_kucoin_signature_rejection(
    data: &safe_cex_types::retained_json::RetainedJsonInput,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let field = |key| data.get(key);
    if safe_cex_types::js_coercion::js_retained_string_utf16(
        field("code").unwrap_or(&safe_cex_types::retained_json::RetainedJsonInput::Undefined),
    )? == "400005".encode_utf16().collect::<Vec<_>>()
    {
        return Ok(true);
    }
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    let message =
        match field("msg").filter(|value| !matches!(value, Input::Null | Input::Undefined)) {
            Some(value) => safe_cex_types::js_coercion::js_retained_string_utf16(value)?,
            None => Vec::new(),
        };
    let pattern = "invalid kc-api-sign".encode_utf16().collect::<Vec<_>>();
    Ok(message.windows(pattern.len()).any(|units| {
        units.iter().zip(&pattern).all(|(unit, expected)| {
            let folded = if (65..=90).contains(unit) {
                *unit + 32
            } else {
                *unit
            };
            folded == *expected
        })
    }))
}
impl Api {
    // axios-retry (networkErrorOnRead || signatureRejectedResponse) over one attempt.
    async fn retrying(
        &self,
        request: RestRequest,
        retry: &SignatureRetry,
        predicate: &IsSignatureRejection,
    ) -> Result<safe_cex_transport::RestExecution, RestFailure> {
        let classified = reads_and_posted(vec![
            "/api/v1/bullet-public".into(),
            "/api/v1/bullet-private".into(),
        ])
        .classify(request);
        retry_network_reads_and_signature_rejection(
            classified,
            3,
            &self.attempt,
            self.retry_clock.as_ref(),
            retry,
            predicate,
        )
        .await
    }
}
// The future owns no venue latch or task: drop releases the shared attempt.
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
        // Axios normalizes method/defaults before running interceptors. The shared
        // factory overwrites its configured recvWindow with its 20-second default.
        request.method = if request.method.is_empty() {
            "get".into()
        } else {
            request.method.to_lowercase()
        };
        if request.timeout_ms.is_none() {
            request.timeout_ms = Some(20000);
        }
        Box::pin(async move {
            // One stamp per call stands in for config.safeCexSignatureRetry.
            let retry = SignatureRetry::default();
            let predicate: IsSignatureRejection = Rc::new(is_kucoin_signature_rejection);
            let execution = self.retrying(request, &retry, &predicate).await?;
            // Last interceptor; over the tunnel any status resolves.
            if resend_on_signature_rejection(&execution, &retry, &predicate)? {
                return self.retrying(execution.config, &retry, &predicate).await;
            }
            Ok(execution)
        })
    }
}
fn set_header(headers: &mut indexmap::IndexMap<String, String>, name: &str, value: String) {
    if let Some((_, existing)) = headers
        .iter_mut()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
    {
        *existing = value;
    } else {
        headers.insert(name.into(), value);
    }
}
impl Attempt {
    async fn sign(&self, request: &mut RestRequest) -> Result<(), String> {
        if self.options.public_data_only == Some(true)
            || (self.spot && api_helpers::is_public_endpoint(Some(&request.url)))
        {
            return Ok(());
        }
        // Only the device's signing pass keeps the server's timestamp, which the
        // broker signature was made over; a replay must not reuse the previous one.
        let incoming = if self.client.is_signing_capture(request) {
            broker::get_kucoin_request_timestamp(Some(&request.headers))
        } else {
            None
        };
        let timestamp =
            incoming.unwrap_or_else(|| js_number_string(self.clock.get_current_time_ms()));
        broker::add_kucoin_broker_headers(&mut request.headers, &self.options, &timestamp).await?;
        // Set even without a broker config, so a hosted replay cannot hand the device the old one.
        set_header(&mut request.headers, "KC-API-TIMESTAMP", timestamp.clone());
        if self.remote {
            return Ok(());
        }
        let mut url = request.url.clone();
        if let Some(params) = request
            .query_params
            .as_ref()
            .filter(|v| safe_cex_transport::query::query_truthy(v))
        {
            url.push('?');
            url.push_str(
                &safe_cex_transport::query::render_query(
                    params,
                    safe_cex_transport::query::QueryProfile {
                        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                        arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
                        encode: self.spot,
                    },
                )
                .map_err(|error| error.to_string())?,
            );
        } else if let Some(params) = request.params.as_ref().filter(|v| js_truthy(Some(v))) {
            url.push('?');
            url.push_str(&api_helpers::stringify_query(
                &JsJsonValue::from(params.clone()),
                self.spot,
            ));
        }
        let body = request
            .body
            .as_ref()
            .map(|body| body.untransformed())
            .transpose()
            .map_err(|e| e.to_string())?;
        // A replayed config carries the body Axios already serialized: sign those bytes.
        let body = if let Some(safe_cex_types::retained_json::RetainedJsonInput::Text(text)) = body
        {
            String::from_utf16(text).map_err(|error| error.to_string())?
        } else if body.is_some_and(safe_cex_num::js_retained_truthy) {
            safe_cex_num::js_retained_json_stringify(body.unwrap())
                .map_err(|e| e.to_string())?
                .unwrap_or_default()
        } else {
            String::new()
        };
        let signature = hmac(
            &self.options,
            &format!("{timestamp}{}{url}{body}", request.method.to_uppercase()),
        )
        .await
        .map_err(|e| e.to_string())?;
        broker::add_kucoin_broker_headers(&mut request.headers, &self.options, &timestamp).await?;
        if self.spot {
            set_header(&mut request.headers, "KC-API-KEY", self.options.key.clone());
            set_header(
                &mut request.headers,
                "KC-API-PASSPHRASE",
                self.passphrase.clone(),
            );
        }
        set_header(&mut request.headers, "KC-API-SIGN", signature);
        set_header(&mut request.headers, "KC-API-TIMESTAMP", timestamp);
        set_header(&mut request.headers, "KC-API-KEY-VERSION", "2".into());
        Ok(())
    }
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
        request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        Box::pin(async move {
            self.client
                .execute_with_interceptor_receipt(request, |mut request| async move {
                    if let Err(message) = self.sign(&mut request).await {
                        return Err(RestFailure::pre_transmit(message, request));
                    }
                    Ok(request)
                })
                .await
        })
    }
}

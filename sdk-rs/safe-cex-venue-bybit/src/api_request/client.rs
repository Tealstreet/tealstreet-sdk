//! Request-side composition. Response/probe/retry orchestration is separate.
use super::{RequestSigning, prepare_request, query_bytes};
use safe_cex_crypto::{PayloadSigner, SigningOptions};
use safe_cex_exchange::http::{
    HttpClient, HttpClientOptions, install_transport_adapter, safe_cex_axios_create,
};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, TransportError,
    TransportRegistry,
};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use serde_json::{Map, Value};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

pub(crate) struct RequestClient {
    client: HttpClient,
    exchange: ExchangeName,
    options: ExchangeOptions,
    site_headers: Map<String, Value>,
    recv_window: f64,
    payload_signer: Option<Rc<dyn PayloadSigner>>,
    clock: safe_cex_store::virtual_clock::VirtualClock,
    next_id: Arc<dyn Fn() -> String>,
    remote: bool,
}
impl RequestClient {
    /// The outer factory selects the base URL and tunnel adapter. This operation
    /// preserves interceptor -> common HTTP hooks -> signing codec -> transport.
    pub(crate) fn new(
        mut options: HttpClientOptions,
        registry: &TransportRegistry,
        site_headers: Map<String, Value>,
        payload_signer: Option<Rc<dyn PayloadSigner>>,
        clock: safe_cex_store::virtual_clock::VirtualClock,
    ) -> Result<Self, TransportError> {
        let recv_window = options
            .options
            .extra
            .as_ref()
            .and_then(|extra| extra.recv_window)
            .unwrap_or(crate::types::RECV_WINDOW as f64);
        let mut headers = BTreeMap::from([
            ("X-Referer".into(), crate::types::BROKER_ID.into()),
            (
                "X-BAPI-RECV-WINDOW".into(),
                safe_cex_num::js_number_string(recv_window),
            ),
            (
                "Content-Type".into(),
                "application/json, charset=utf-8".into(),
            ),
        ]);
        for (key, value) in &site_headers {
            let value = value
                .as_str()
                .ok_or_else(|| TransportError::Host("Bybit site header must be a string".into()))?;
            headers.insert(key.clone(), value.into());
        }
        headers.extend(std::mem::take(&mut options.request.headers));
        // Core merges factory defaults before the callback, exactly once.
        options.request.headers = headers.into_iter().collect();
        options.request.query_profile = Some(safe_cex_transport::query::QueryProfile {
            key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
            arrays: safe_cex_transport::query::QueryArrayFormat::Indexed,
            encode: true,
        });
        options.params_serializer = Some(Arc::new(|value| match value {
            Value::Object(map) => Ok(query_bytes(map)),
            _ => Err(TransportError::Host(
                "Bybit request params must be an object".into(),
            )),
        }));
        options.response_transform = Some(Rc::new(|mut response, config| {
            let original = response.clone();
            response.body = response.body.transform_axios_response().map_err(|cause| {
                let mut error = RestFailure::from_transport(cause, config.clone());
                error.response = Some(Box::new(original));
                error
            })?;
            Ok(response)
        }));
        let exchange = options.exchange.clone();
        let account = options.options.clone();
        let remote = options.signer.is_some();
        let next_id = options.next_id.clone();
        let provider = options.provider.clone();
        let client = safe_cex_axios_create(options, registry)?;
        if account.tealstreet_tunnel == Some(true) {
            let region = match account.proxy_region.as_deref() {
                None => None,
                Some("JA") => Some(safe_cex_transport::RestAdapterRegion::Ja),
                Some("SG") => Some(safe_cex_transport::RestAdapterRegion::Sg),
                Some(other) => Some(safe_cex_transport::RestAdapterRegion::Other(other.into())),
            };
            install_transport_adapter(
                &client,
                registry,
                &account,
                provider.as_ref(),
                safe_cex_transport::RestAdapterVariant::Region { region },
            );
        }
        Ok(Self {
            client,
            exchange,
            options: account,
            site_headers,
            recv_window,
            payload_signer,
            clock,
            next_id,
            remote,
        })
    }
}
impl RestAdapter for RequestClient {
    fn query_in_url(&self) -> bool {
        self.client.query_in_url()
    }
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
        Box::pin(self.client.execute_with_interceptor_receipt(
            request,
            move |mut request| async move {
                // Axios normalizes the method before invoking its interceptors.
                request.method = if request.method.is_empty() {
                    "get".into()
                } else {
                    request.method.to_lowercase()
                };
                let original = request.clone();
                let id = (self.next_id)();
                prepare_request(
                    request,
                    RequestSigning {
                        exchange: self.exchange.clone(),
                        credentials: SigningOptions {
                            key: &self.options.key,
                            secret: &self.options.secret,
                            payload_signer: self.payload_signer.as_deref(),
                        },
                        remote_signer: self.remote,
                        recv_window: self.recv_window,
                        timestamp: &|| self.clock.get_current_time_ms(),
                        request_id: &id,
                        site_headers: &self.site_headers,
                    },
                )
                .await
                .map_err(|message| RestFailure::pre_transmit(message, original))
            },
        ))
    }
}

//! Query-key ordering from bingx.api.ts.
use safe_cex_num::JsRootCollator;
use std::cmp::Ordering;

/// Compare keys with the shared ICU root profile (English default-locale capture).
/// Caller retains the collator across the complete query-key sort.
pub fn sort(collator: &JsRootCollator, a: &str, b: &str) -> Ordering {
    collator.compare(a, b)
}

use crate::{constants, data_reads::ReadTransport};
use safe_cex_exchange::js_runtime::value::{Js, from_retained_input, to_retained_input};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::js_number;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_transport::{
    RestBody, RestRequest,
    retry::{ReadClassification, reads_and_posted},
    signature_rejection::{IsSignatureRejection, SignatureRetry, resend_on_rejected_data},
    signing::try_parse_json,
};
use safe_cex_types::{ExchangeOptions, retained_json::RetainedJsonInput};
use serde_json::Value;
use std::rc::Rc;

pub type ParamsSerializer = Rc<dyn Fn(&Value) -> Result<String, ExchangeError>>;
pub struct ApiDefaults {
    pub base_url: String,
    pub timeout: f64,
    pub headers: Vec<(String, String)>,
    pub bigint_response: bool,
    pub params_serializer: ParamsSerializer,
}
/// Imported Axios factory, qs, HMAC, retry and adapter capabilities.
pub trait ApiHost {
    /// Runtime capability corresponding to source options.remoteSigner.
    fn has_remote_signer(&self) -> bool {
        false
    }
    fn create(
        &self,
        defaults: ApiDefaults,
        options: &ExchangeOptions,
        exchange: &Rc<BaseExchange>,
    ) -> Result<Rc<dyn ReadTransport>, ExchangeError>;
    /// axios-retry with networkErrorOnRead(reads) || signatureRejectedResponse(signature).
    fn retry(
        &self,
        client: &Rc<dyn ReadTransport>,
        retries: usize,
        reads: ReadClassification,
        signature: IsSignatureRejection,
    ) -> Result<(), ExchangeError>;
    fn install_adapter(
        &self,
        client: &Rc<dyn ReadTransport>,
        options: &ExchangeOptions,
        exchange: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError>;
    fn serialize(&self, params: &RetainedJsonInput) -> Result<String, ExchangeError>;
    fn hmac_sha256_hex(&self, secret: &str, message: &str) -> Result<String, ExchangeError>;
    fn now_ms(&self) -> f64;
}
// ts: isBingxSignatureRejection
pub fn is_bingx_signature_rejection(data: &Value) -> Result<bool, safe_cex_num::JsCoercionError> {
    let field = |key| data.as_object().and_then(|object| object.get(key));
    if js_number(field("code"))? == 100001.0 {
        return Ok(true);
    }
    let message = match field("msg").filter(|value| !value.is_null()) {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    // A non-unicode /i regex never folds a non-ASCII unit onto an ASCII one.
    Ok(message
        .to_ascii_lowercase()
        .contains("signature verification failed"))
}
// ts: unsignedPayload. A replayed POST arrives with the body Axios serialized and a
// replayed GET with the signed params it sent; drop the previous pass's signature.
pub fn unsigned_payload(payload: Option<RetainedJsonInput>) -> RetainedJsonInput {
    let mut fields = match payload.map(try_parse_json) {
        Some(RetainedJsonInput::Object(fields)) => fields,
        Some(RetainedJsonInput::Array(items)) => items
            .into_iter()
            .enumerate()
            .map(|(index, item)| (index.to_string().encode_utf16().collect(), item))
            .collect(),
        _ => Vec::new(),
    };
    fields.retain(|(key, _)| !key.iter().copied().eq("signature".encode_utf16()));
    RetainedJsonInput::Object(fields)
}

pub struct BingxApi {
    pub options: ExchangeOptions,
    pub host: Rc<dyn ApiHost>,
    pub client: Rc<dyn ReadTransport>,
}
pub fn create_api(
    options: ExchangeOptions,
    exchange: &Rc<BaseExchange>,
    host: Rc<dyn ApiHost>,
) -> Result<Rc<BingxApi>, ExchangeError> {
    let serializer = host.clone();
    let defaults = ApiDefaults {
        params_serializer: Rc::new(move |params| {
            serializer.serialize(
                &RetainedJsonInput::from_value(params.clone())
                    .map_err(|error| ExchangeError::new(error.to_string()))?,
            )
        }),
        base_url: constants::BASE_URL[usize::from(options.testnet == Some(true))]
            .1
            .into(),
        timeout: options
            .extra
            .as_ref()
            .and_then(|e| e.recv_window)
            .unwrap_or(constants::RECV_WINDOW),
        headers: vec![
            ("X-BX-APIKEY".into(), options.key.clone()),
            ("X-SOURCE-KEY".into(), "Tealstreet".into()),
            (
                "Content-Type".into(),
                "application/json, charset=utf-8".into(),
            ),
        ],
        bigint_response: true,
    };
    let client = host.create(defaults, &options, exchange)?;
    host.retry(
        &client,
        3,
        reads_and_posted(vec![constants::ENDPOINTS[16].1.into()]),
        Rc::new(is_bingx_signature_rejection),
    )?;
    if options.public_data_only != Some(true) && options.tealstreet_tunnel != Some(false) {
        host.install_adapter(&client, &options, exchange)?;
    }
    Ok(Rc::new(BingxApi {
        options,
        host,
        client,
    }))
}
impl BingxApi {
    pub fn prepare(&self, mut config: RestRequest) -> Result<RestRequest, ExchangeError> {
        if config.url == constants::ENDPOINTS[16].1 && self.options.testnet != Some(true) {
            return Ok(config);
        }
        if self.options.remote_signer.is_some()
            || self.host.has_remote_signer()
            || self.options.key.is_empty()
            || self.options.secret.is_empty()
        {
            return Ok(config);
        }
        let body = config
            .body
            .as_ref()
            .map(RestBody::untransformed)
            .transpose()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        let params = config
            .params
            .clone()
            .map(RetainedJsonInput::from_value)
            .transpose()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        let data = from_retained_input(unsigned_payload(
            body.filter(|value| safe_cex_num::js_retained_truthy(value))
                .cloned()
                .or_else(|| params.filter(safe_cex_num::js_retained_truthy)),
        ));
        if data.has_own("__proto__") {
            return Err(ExchangeError::new(
                "BingX native signing refuses prototype-mutating key",
            ));
        }
        data.set("timestamp", Js::Number(self.host.now_ms()))
            .map_err(|error| error.to_exchange_error())?;
        let collator =
            JsRootCollator::try_new().map_err(|error| ExchangeError::new(error.to_string()))?;
        let mut keys = data
            .own_keys()
            .into_iter()
            .map(|key| String::from_utf16(&key))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ExchangeError::new(error.to_string()))?;
        keys.sort_by(|left, right| sort(&collator, left, right));
        let sorted = Js::object(Vec::new());
        for key in keys {
            sorted
                .set(
                    &key,
                    data.get(&key).map_err(|error| error.to_exchange_error())?,
                )
                .map_err(|error| error.to_exchange_error())?;
        }
        let query = self
            .host
            .serialize(&to_retained_input(&sorted).map_err(|error| error.to_exchange_error())?)?;
        let signature = self.host.hmac_sha256_hex(&self.options.secret, &query)?;
        sorted
            .set("signature", Js::str(&signature))
            .map_err(|error| error.to_exchange_error())?;
        if config.method != "post" {
            // Existing legacy query ABI remains a separately declared projection.
            config.params = Some(
                to_retained_input(&sorted)
                    .map_err(|error| error.to_exchange_error())?
                    .into_value()
                    .map_err(|error| ExchangeError::new(error.to_string()))?,
            );
            config.body = None;
        } else {
            for field in ["quantity", "price"] {
                let value = sorted
                    .get(field)
                    .map_err(|error| error.to_exchange_error())?;
                if value.truthy() {
                    sorted
                        .set(
                            field,
                            Js::Number(
                                value
                                    .to_number()
                                    .map_err(|error| error.to_exchange_error())?,
                            ),
                        )
                        .map_err(|error| error.to_exchange_error())?;
                }
            }
            config
                .headers
                .insert("Content-Type".into(), "application/json".into());
            let serialized = safe_cex_num::js_retained_json_stringify(
                &to_retained_input(&sorted).map_err(|error| error.to_exchange_error())?,
            )
            .map_err(|error| ExchangeError::new(error.to_string()))?
            .expect("object JSON");
            config.body = Some(RestBody::Untransformed(RetainedJsonInput::Text(
                serialized.encode_utf16().collect(),
            )));
        }
        Ok(config)
    }
}
impl ReadTransport for BingxApi {
    fn send(&self, request: RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        Box::pin(async move {
            let mut request = request;
            request.method = request.method.to_lowercase();
            let retry = SignatureRetry::default();
            let prepared = self.prepare(request)?;
            let data = self
                .client
                .send_with_signature_retry(prepared.clone(), &retry)
                .await?;
            // BingX answers on HTTP 200, and over the tunnel any status resolves.
            let view =
                serde_json::to_value(&data).map_err(|e| ExchangeError::new(e.to_string()))?;
            let predicate: IsSignatureRejection = Rc::new(is_bingx_signature_rejection);
            if resend_on_rejected_data(&view, &retry, &predicate).map_err(ExchangeError::from)? {
                // The resend runs the whole chain again, re-signing what was sent.
                return self
                    .client
                    .send_with_signature_retry(self.prepare(prepared)?, &retry)
                    .await;
            }
            Ok(data)
        })
    }
}

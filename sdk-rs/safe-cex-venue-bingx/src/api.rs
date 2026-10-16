//! Query-key ordering from bingx.api.ts.
use safe_cex_num::JsRootCollator;
use std::cmp::Ordering;

/// Compare keys with the shared ICU root profile (English default-locale capture).
/// Caller retains the collator across the complete query-key sort.
pub fn sort(collator: &JsRootCollator, a: &str, b: &str) -> Ordering {
    collator.compare(a, b)
}

use crate::{constants, data_reads::ReadTransport};
use safe_cex_exchange::{
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_num::{js_number, js_truthy};
use safe_cex_transport::{
    RestBody, RestRequest,
    retry::{ReadClassification, reads_and_posted},
    signature_rejection::{IsSignatureRejection, SignatureRetry, resend_on_rejected_data},
    signing::try_parse_json,
};
use safe_cex_types::{ExchangeOptions, retained_json::RetainedJsonInput};
use serde_json::{Value, json};
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
    fn serialize(&self, params: &Value) -> Result<String, ExchangeError>;
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
pub fn unsigned_payload(payload: Option<Value>) -> serde_json::Map<String, Value> {
    let mut rest = match payload.map(try_parse_json) {
        Some(Value::Object(object)) => object,
        // Object rest over an array copies its index keys.
        Some(Value::Array(items)) => items
            .into_iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item))
            .collect(),
        _ => serde_json::Map::new(),
    };
    rest.shift_remove("signature");
    rest
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
        params_serializer: Rc::new(move |params| serializer.serialize(params)),
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
        let mut data = unsigned_payload(
            body.filter(|v| js_truthy(Some(v)))
                .or_else(|| config.params.as_ref().filter(|v| js_truthy(Some(v))))
                .cloned(),
        );
        if data.contains_key("__proto__") {
            return Err(ExchangeError::new(
                "BingX native signing refuses prototype-mutating key",
            ));
        }
        data.insert(
            "timestamp".into(),
            RetainedJsonInput::Float(self.host.now_ms())
                .into_value()
                .map_err(|_| ExchangeError::new("BingX native signing refuses NaN timestamp"))?,
        );
        let collator = JsRootCollator::try_new().map_err(|e| ExchangeError::new(e.to_string()))?;
        let mut keys = data.keys().cloned().collect::<Vec<_>>();
        keys.sort_by(|a, b| sort(&collator, a, b));
        let mut sorted = serde_json::Map::new();
        for key in keys {
            sorted.insert(key.clone(), data.remove(&key).expect("sorted key"));
        }
        let query = self.host.serialize(&Value::Object(sorted.clone()))?;
        let signature = self.host.hmac_sha256_hex(&self.options.secret, &query)?;
        sorted.insert("signature".into(), json!(signature));
        if config.method != "post" {
            config.params = Some(Value::Object(sorted));
            config.body = None;
        } else {
            for field in ["quantity", "price"] {
                if let Some(v) = sorted.get(field)
                    && js_truthy(Some(v))
                {
                    let n = js_number(Some(v)).map_err(|e| ExchangeError::new(e.message))?;
                    sorted.insert(
                        field.into(),
                        RetainedJsonInput::Float(n).into_value().map_err(|_| {
                            ExchangeError::new("BingX native signing refuses NaN field")
                        })?,
                    );
                }
            }
            config
                .headers
                .insert("Content-Type".into(), "application/json".into());
            // Serialized here: axios-retry resends with an identity transformRequest.
            let serialized = safe_cex_num::js_json_stringify(&Value::Object(sorted).into())
                .map_err(|e| ExchangeError::new(e.message))?
                .expect("an object serializes");
            config.body = Some(RestBody::Untransformed(Value::String(serialized)));
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
            if resend_on_rejected_data(&view, &retry, &predicate)
                .map_err(|e| ExchangeError::new(e.message))?
            {
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

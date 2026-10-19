use crate::{RestAdapter, RestRequest, RestResponse, TransportError, TransportFuture};
use indexmap::IndexMap;
use safe_cex_types::ExchangeName;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, RwLock};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SigningConfig {
    pub method: String,
    pub url: String,
    #[serde(rename = "baseURL")]
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(
        skip_serializing_if = "crate::native_data::omitted",
        serialize_with = "crate::native_data::serialize_optional"
    )]
    pub data: Option<Input>,
    pub headers: IndexMap<String, String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpSigningRequest {
    pub id: String,
    pub exchange_name: ExchangeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_name: Option<String>,
    pub config: SigningConfig,
}
#[derive(Default, Serialize)]
pub struct SignedConfig {
    pub url: Option<String>,
    pub params: Option<Value>,
    #[serde(
        skip_serializing_if = "crate::native_data::omitted",
        serialize_with = "crate::native_data::serialize_optional"
    )]
    pub data: Option<Input>,
    pub headers: IndexMap<String, String>,
}
#[derive(Default, Serialize)]
pub struct HttpSigningResponse {
    pub id: String,
    pub config: Option<SignedConfig>,
    pub error: Option<String>,
}
/// One remote operation, with canonical bottom-layer WS payloads.
pub enum RemoteSignerRequest {
    Http(HttpSigningRequest),
    WsAuth {
        exchange_name: ExchangeName,
        account_name: Option<String>,
        request: safe_cex_types::ws_auth::WsAuthRequest,
    },
}
/// Responses have no wire discriminator in TS. Decode with the originating
/// request, so an error-only response is never guessed to be HTTP or WS.
#[derive(Serialize)]
#[serde(untagged)]
pub enum RemoteSignerResponse {
    Http(HttpSigningResponse),
    WsAuth(safe_cex_types::ws_auth::WsAuthResponse),
}
impl RemoteSignerRequest {
    pub fn decode_response(&self, json: &str) -> Result<RemoteSignerResponse, TransportError> {
        let result = match self {
            Self::Http(_) => serde_json::from_str(json).map(RemoteSignerResponse::Http),
            Self::WsAuth { .. } => serde_json::from_str(json).map(RemoteSignerResponse::WsAuth),
        };
        result.map_err(|error| TransportError::Host(error.to_string()))
    }
}
impl Serialize for RemoteSignerRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        match self {
            Self::Http(request) => {
                map.serialize_entry("type", "http")?;
                map.serialize_entry("id", &request.id)?;
                map.serialize_entry("exchangeName", &request.exchange_name)?;
                if let Some(account) = &request.account_name {
                    map.serialize_entry("accountName", account)?;
                }
                map.serialize_entry("config", &request.config)?;
            }
            Self::WsAuth {
                exchange_name,
                account_name,
                request,
            } => {
                // The union arm owns the operation discriminator, not a caller string.
                map.serialize_entry("type", "ws-auth")?;
                if let Some(id) = &request.id {
                    map.serialize_entry("id", id)?;
                }
                map.serialize_entry("exchangeName", exchange_name)?;
                if let Some(account) = account_name {
                    map.serialize_entry("accountName", account)?;
                }
                map.serialize_entry("wsAuth", &request.ws_auth)?;
            }
        }
        map.end()
    }
}
struct SigningHeader {
    kind: String,
    exchange_name: ExchangeName,
    account_name: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "SigningHeader", rename_all = "camelCase")]
struct SigningHeaderWire {
    #[serde(rename = "type")]
    kind: String,
    exchange_name: ExchangeName,
    account_name: Option<String>,
}

impl<'de> Deserialize<'de> for SigningHeader {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        SigningHeaderWire::deserialize(safe_cex_types::strict_decode::object(d))
    }
}

impl<'de> Deserialize<'de> for RemoteSignerRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Do not buffer through Value: canonical WS extraData retains deep/overflow
        // metadata as RawValue. This header decode skips the untouched payload.
        let raw = Box::<serde_json::value::RawValue>::deserialize(deserializer)?;
        let header: SigningHeader =
            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
        match header.kind.as_str() {
            "http" => serde_json::from_str(raw.get())
                .map(Self::Http)
                .map_err(serde::de::Error::custom),
            "ws-auth" => Ok(Self::WsAuth {
                exchange_name: header.exchange_name,
                account_name: header.account_name,
                request: serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?,
            }),
            _ => Err(serde::de::Error::custom(
                "unsupported remote signing request type",
            )),
        }
    }
}
pub trait RemoteSigner {
    fn sign(&self, request: RemoteSignerRequest) -> TransportFuture<'_, RemoteSignerResponse>;
    /// Opt-in typed native protocol. Existing hosts keep the legacy Value API;
    /// an unsupported native host refuses rather than projecting through Value.
    fn sign_query(
        &self,
        _request: crate::query_signing::HttpSigningRequest,
    ) -> TransportFuture<'_, crate::query_signing::HttpSigningResponse> {
        Box::pin(async {
            Err(TransportError::Host(
                "Native query parameters require a native-profile signing host".into(),
            ))
        })
    }
}
pub struct SigningRequest {
    pub config: SigningConfig,
    body_template: Option<crate::RestBody>,
    skip_remote_signing: bool,
    timeout_ms: Option<i64>,
    pub query_profile: Option<crate::query::QueryProfile>,
}
impl SigningRequest {
    pub fn new(config: SigningConfig) -> Self {
        Self {
            body_template: config.data.clone().map(crate::RestBody::Untransformed),
            config,
            skip_remote_signing: false,
            timeout_ms: None,
            query_profile: None,
        }
    }
}
pub fn mark_skip_remote_signing(request: &mut SigningRequest) -> &mut SigningRequest {
    request.skip_remote_signing = true;
    request
}
pub fn try_parse_json(value: Input) -> Input {
    if let Input::Text(text) = &value
        && let Ok(parsed @ (Input::Object(_) | Input::Array(_))) = Input::parse_json(text)
    {
        return parsed;
    }
    value
}
/// Preserve plain-object entries, as TypeScript serializeHeaders does. The string
/// type excludes null/non-string values but does not normalize header names.
/// AxiosHeaders normalization belongs to request assembly, not this projection.
pub fn serialize_headers(headers: &IndexMap<String, String>) -> IndexMap<String, String> {
    headers.clone()
}

pub struct RemoteSigningAdapter {
    signer: Arc<dyn RemoteSigner>,
    exchange: ExchangeName,
    account_name: Option<String>,
    next_id: Arc<dyn Fn() -> String>,
    now_ms: Arc<dyn Fn() -> u64>,
    inner: RwLock<Arc<dyn RestAdapter>>,
}
pub fn create_remote_signing_adapter(
    signer: Arc<dyn RemoteSigner>,
    exchange: ExchangeName,
    account_name: Option<String>,
    next_id: Arc<dyn Fn() -> String>,
    now_ms: Arc<dyn Fn() -> u64>,
    inner: Arc<dyn RestAdapter>,
) -> RemoteSigningAdapter {
    RemoteSigningAdapter {
        signer,
        exchange,
        account_name,
        next_id,
        now_ms,
        inner: RwLock::new(inner),
    }
}
pub fn set_inner_adapter_for_remote_signing(
    adapter: &RemoteSigningAdapter,
    inner: Arc<dyn RestAdapter>,
) {
    *adapter.inner.write().expect("inner adapter lock") = inner;
}
fn usable(value: &str) -> bool {
    value.starts_with("Bearer ") && value.len() > "Bearer ".len()
}
fn header(headers: &IndexMap<String, String>, name: &str) -> Option<String> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}
fn stamp_signing_diagnostics(request: &mut RestRequest, elapsed: f64, now: u64) {
    request
        .config_extensions
        .insert("safeCexRestDiagSignMs".into(), Input::Float(elapsed));
    request
        .config_extensions
        .insert("safeCexRestDiagSignedAt".into(), Input::Float(now as f64));
    let timestamp = header(&request.headers, "x-bapi-timestamp")
        .map_or(f64::NAN, |value| safe_cex_num::number_from_string(&value));
    if timestamp.is_finite() && timestamp > 0.0 {
        request.config_extensions.insert(
            "safeCexRestDiagSignatureAgeMs".into(),
            Input::Float(now as f64 - timestamp),
        );
    }
}
impl RemoteSigningAdapter {
    fn execute_query(
        &self,
        mut request: RestRequest,
    ) -> crate::RestFuture<'_, crate::RestExecution> {
        Box::pin(async move {
            let original = request.clone();
            if request.params.is_some() || request.query_profile.is_none() {
                return Err(crate::RestFailure::pre_transmit(
                    "Native signing requires an exclusive query profile".into(),
                    original,
                ));
            }
            request
                .transform_axios_request()
                .map_err(|e| crate::RestFailure::from_transport(e, original.clone()))?;
            let started = (self.now_ms)();
            let response = self
                .signer
                .sign_query(crate::query_signing::HttpSigningRequest {
                    id: (self.next_id)(),
                    exchange_name: self.exchange.clone(),
                    account_name: self.account_name.clone(),
                    config: crate::query_signing::SigningConfig {
                        method: if request.method.is_empty() {
                            "GET".into()
                        } else {
                            request.method.to_uppercase()
                        },
                        url: request.url.clone(),
                        base_url: request.base_url.clone().unwrap_or_default(),
                        params: request
                            .query_params
                            .clone()
                            .map(crate::query::try_parse_query_json),
                        data: request
                            .body
                            .as_ref()
                            .map(crate::RestBody::untransformed)
                            .transpose()
                            .map_err(|e| crate::RestFailure::from_transport(e, original.clone()))?
                            .cloned()
                            .map(try_parse_json),
                        headers: serialize_headers(&request.headers),
                    },
                })
                .await
                .map_err(|e| crate::RestFailure::from_transport(e, original.clone()))?;
            let completed = (self.now_ms)();
            if let Some(error) = response.error.filter(|s| !s.is_empty()) {
                return Err(crate::RestFailure::pre_transmit(
                    format!("Remote signing failed: {error}"),
                    original,
                ));
            }
            let signed = response.config.ok_or_else(|| {
                crate::RestFailure::pre_transmit(
                    "Remote signer returned no config".into(),
                    original.clone(),
                )
            })?;
            let mut headers = signed.headers;
            for name in ["Authorization", "Tealstreet-Authorization"] {
                if header(&headers, name).is_some_and(|value| usable(&value)) {
                    continue;
                }
                headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
                if let Some(value) = header(&request.headers, name).filter(|value| usable(value)) {
                    headers.insert(name.into(), value);
                }
            }
            request.headers = headers;
            request.merge_axios_headers(&IndexMap::new());
            if let Some(url) = signed.url.filter(|url| !url.is_empty()) {
                request.url = url;
            }
            request.query_params = signed.params;
            request.body = signed.data.map(crate::RestBody::from_remote_adapter_data);
            request.sign_ms = Some(completed.saturating_sub(started));
            stamp_signing_diagnostics(
                &mut request,
                completed as f64 - started as f64,
                (self.now_ms)(),
            );
            let inner = self.inner.read().expect("inner adapter lock").clone();
            inner.execute_with_receipt(request).await
        })
    }
    pub fn execute_signed(&self, request: SigningRequest) -> crate::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_signed_with_receipt(request, IndexMap::new())
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_signed_with_receipt(
        &self,
        request: SigningRequest,
        config_extensions: IndexMap<String, safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> crate::RestFuture<'_, crate::RestExecution> {
        let mut original = RestRequest {
            method: request.config.method.clone(),
            url: request.config.url.clone(),
            base_url: Some(request.config.base_url.clone()),
            headers: request.config.headers.clone(),
            params: request.config.params.clone(),
            query_params: None,
            query_profile: request.query_profile,
            body: request.body_template.clone(),
            timeout_ms: request.timeout_ms,
            skip_remote_signing: request.skip_remote_signing,
            sign_ms: None,
            config_extensions: config_extensions.clone(),
        };
        original.merge_axios_headers(&IndexMap::new());
        Box::pin(async move {
            // This server-side adapter is entered AFTER Axios transformRequest.
            // Preserve the resulting JS data, not an origin inferred from bytes.
            original.transform_axios_request().map_err(|error| {
                crate::RestFailure::pre_transmit(error.to_string(), original.clone())
            })?;
            let failure = |error| crate::RestFailure::from_transport(error, original.clone());
            let mut config = request.config;
            config.headers = original.headers.clone();
            config.data = original
                .body
                .as_ref()
                .map(crate::RestBody::untransformed)
                .transpose()
                .map_err(failure)?
                .cloned();
            let mut sign_ms = None;
            let mut diagnostics = None;
            if !request.skip_remote_signing {
                let started = (self.now_ms)();
                let response = self
                    .signer
                    .sign(RemoteSignerRequest::Http(HttpSigningRequest {
                        id: (self.next_id)(),
                        exchange_name: self.exchange.clone(),
                        account_name: self.account_name.clone(),
                        config: SigningConfig {
                            method: if config.method.is_empty() {
                                "GET".into()
                            } else {
                                config.method.to_uppercase()
                            },
                            params: config.params.clone().map(try_parse_params_json),
                            data: config.data.clone().map(try_parse_json),
                            headers: serialize_headers(&config.headers),
                            ..config.clone()
                        },
                    }))
                    .await
                    .map_err(failure)?;
                let response = match response {
                    RemoteSignerResponse::Http(response) => response,
                    RemoteSignerResponse::WsAuth(_) => {
                        return Err(failure(TransportError::Host(
                            "Remote signer returned WS auth for an HTTP request".into(),
                        )));
                    }
                };
                let signed_at = (self.now_ms)();
                sign_ms = Some(signed_at.saturating_sub(started));
                diagnostics = Some(signed_at as f64 - started as f64);
                if let Some(error) = response.error.filter(|s| !s.is_empty()) {
                    return Err(failure(TransportError::Host(format!(
                        "Remote signing failed: {error}"
                    ))));
                }
                let signed = response.config.ok_or_else(|| {
                    failure(TransportError::Host(
                        "Remote signer returned no config".into(),
                    ))
                })?;
                let mut headers = signed.headers;
                for name in ["Authorization", "Tealstreet-Authorization"] {
                    if header(&headers, name).is_some_and(|value| usable(&value)) {
                        continue;
                    }
                    headers.retain(|key, _| !key.eq_ignore_ascii_case(name));
                    if let Some(value) = header(&config.headers, name).filter(|value| usable(value))
                    {
                        headers.insert(name.into(), value);
                    }
                }
                config.headers = headers;
                if let Some(url) = signed.url.filter(|s| !s.is_empty()) {
                    config.url = url;
                }
                // Signed omissions are authoritative: restoring a body breaks Binance signatures.
                config.params = signed.params;
                config.data = signed.data;
            }
            if !request.skip_remote_signing
                && config.url.contains('?')
                && config
                    .params
                    .as_ref()
                    .is_some_and(|params| params.as_object().is_some_and(|obj| !obj.is_empty()))
            {
                warn_double_serialization(&self.exchange, &config);
            }
            let inner = self.inner.read().expect("inner adapter lock").clone();
            let body = config.data.map(crate::RestBody::from_remote_adapter_data);
            let mut retry_config = RestRequest {
                method: config.method,
                url: config.url,
                base_url: Some(config.base_url),
                headers: config.headers,
                body,
                params: config.params,
                query_params: None,
                query_profile: request.query_profile,
                skip_remote_signing: request.skip_remote_signing,
                sign_ms,
                timeout_ms: request.timeout_ms,
                config_extensions,
            };
            // The source calls the selected inner adapter directly. Running
            // Axios transformation here would stringify an echoed object again.
            retry_config.merge_axios_headers(&IndexMap::new());
            if let Some(elapsed) = diagnostics {
                stamp_signing_diagnostics(&mut retry_config, elapsed, (self.now_ms)());
            }
            inner.execute_with_receipt(retry_config).await
        })
    }
}
impl RestAdapter for RemoteSigningAdapter {
    fn query_in_url(&self) -> bool {
        self.inner
            .read()
            .expect("inner adapter lock")
            .query_in_url()
    }
    fn execute(&self, request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(
        &self,
        request: RestRequest,
    ) -> crate::RestFuture<'_, crate::RestExecution> {
        // Already-signed wire requests may bypass signing, but never be reparsed
        // to infer whether their caller supplied an object or a string.
        if request.skip_remote_signing {
            let inner = self.inner.read().expect("inner adapter lock").clone();
            return Box::pin(async move {
                let mut retry_config = request;
                retry_config.transform_axios_request().map_err(|error| {
                    crate::RestFailure::pre_transmit(error.to_string(), retry_config.clone())
                })?;
                inner.execute_with_receipt(retry_config).await
            });
        }
        if request.query_params.is_some() {
            return self.execute_query(request);
        }
        let data = match request
            .body
            .as_ref()
            .map(crate::RestBody::untransformed)
            .transpose()
        {
            Ok(body) => body.cloned(),
            Err(error) => {
                return Box::pin(
                    async move { Err(crate::RestFailure::from_transport(error, request)) },
                );
            }
        };
        let skip_remote_signing = request.skip_remote_signing;
        let config_extensions = request.config_extensions;
        self.execute_signed_with_receipt(
            SigningRequest {
                body_template: request.body.clone(),
                skip_remote_signing,
                timeout_ms: request.timeout_ms,
                query_profile: request.query_profile,
                config: SigningConfig {
                    method: request.method,
                    url: request.url,
                    base_url: request.base_url.unwrap_or_default(),
                    headers: request.headers,
                    params: request.params,
                    data,
                },
            },
            config_extensions,
        )
    }
}

fn warn_double_serialization(exchange: &ExchangeName, config: &SigningConfig) {
    let message = format!(
        "[remoteSigningAdapter] {exchange:?}: URL has query params AND params object set — possible double-serialization: {} {:?}",
        config.url, config.params
    );
    #[cfg(not(target_arch = "wasm32"))]
    eprintln!("{message}");
    #[cfg(target_arch = "wasm32")]
    {
        #[wasm_bindgen::prelude::wasm_bindgen]
        extern "C" {
            #[wasm_bindgen(js_namespace = console, js_name = warn)]
            fn warn(message: &str);
        }
        warn(&message);
    }
}

#[derive(Deserialize)]
#[serde(remote = "SigningConfig")]
#[serde(rename_all = "camelCase")]
struct WireSigningConfig {
    method: String,
    url: String,
    #[serde(rename = "baseURL")]
    base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "present_json")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default, deserialize_with = "crate::native_data::deserialize_present")]
    data: Option<Input>,
    headers: IndexMap<String, String>,
}
impl<'de> Deserialize<'de> for SigningConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireSigningConfig::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "HttpSigningRequest")]
#[serde(rename_all = "camelCase")]
struct WireHttpSigningRequest {
    id: String,
    exchange_name: ExchangeName,
    #[serde(skip_serializing_if = "Option::is_none")]
    account_name: Option<String>,
    config: SigningConfig,
}
impl<'de> Deserialize<'de> for HttpSigningRequest {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireHttpSigningRequest::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "SignedConfig")]
struct WireSignedConfig {
    url: Option<String>,
    #[serde(default, deserialize_with = "present_json")]
    params: Option<Value>,
    #[serde(default, deserialize_with = "crate::native_data::deserialize_present")]
    data: Option<Input>,
    headers: IndexMap<String, String>,
}
impl<'de> Deserialize<'de> for SignedConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireSignedConfig::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

#[derive(Deserialize)]
#[serde(remote = "HttpSigningResponse")]
struct WireHttpSigningResponse {
    id: String,
    config: Option<SignedConfig>,
    error: Option<String>,
}
impl<'de> Deserialize<'de> for HttpSigningResponse {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        WireHttpSigningResponse::deserialize(safe_cex_types::strict_decode::object(deserializer))
    }
}

// Omission is authoritative, but an explicitly present JSON null is still data.
fn present_json<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

// Legacy params retain their bounded Value ABI; request data does not use this projection.
pub fn try_parse_params_json(value: Value) -> Value {
    let Value::String(text) = &value else {
        return value;
    };
    let Ok(parsed) = crate::native_data::parse(text) else {
        return value;
    };
    if !matches!(parsed, Input::Object(_) | Input::Array(_)) {
        return value;
    }
    let Ok(mut projected) = parsed.into_value() else {
        return value;
    };
    crate::how::normalize_json_numbers(&mut projected, false, false);
    projected
}

#[cfg(test)]
mod header_shape_tests {
    use super::{ExchangeName, SigningHeader};
    use serde_json::json;

    #[test]
    fn header_requires_object_and_preserves_native_fields() {
        crate::accepted_frame_shapes::record::<SigningHeader>(
            json!({"type":"http","exchangeName":"binance","accountName":"native-header-account"}),
            json!(["http", "binance", "native-header-account"]),
            |row| {
                assert_eq!(row.kind, "http");
                assert_eq!(row.exchange_name, ExchangeName::Binance);
                assert_eq!(row.account_name.as_deref(), Some("native-header-account"));
            },
        );
    }
}

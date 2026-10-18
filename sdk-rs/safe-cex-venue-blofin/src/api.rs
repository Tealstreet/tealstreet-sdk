//! Blofin factory and its request/response interceptors over configured transports.
use crate::{order_services::body, types::BASE_URL};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_num::{js_number_string, js_own_keys, js_string, js_truthy};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
    rate_limit::RateLimitClock,
    retry::{ClassifiedRequest, ReadClassification, network_error_on_read},
};
use safe_cex_types::ExchangeOptions;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

/// Common safeCexAxiosCreate/remote-signing/proxy composition remains in the host.
/// Clock/nonce are actual virtual-clock/UUID boundaries, not counters in the port.
pub trait ApiRuntime: RateLimitClock {
    fn timestamp(&self) -> f64;
    fn nonce(&self) -> String;
    /// Opaque remoteSigner capability is supplied by the common HTTP host.
    fn remote_signer_present(&self) -> bool;
    fn record_auth_failure(&self);
    fn record_auth_success(&self);
}
pub struct BlofinApi {
    options: ExchangeOptions,
    xhr: Rc<dyn RestAdapter>,
    runtime: Rc<dyn ApiRuntime>,
}
/// The supplied factory must apply the shared common HTTP stack, including remote
/// signing when options.remoteSigner is present. Venue signing runs before it.
pub fn create_api(
    options: &ExchangeOptions,
    runtime: Rc<dyn ApiRuntime>,
    safe_http: impl FnOnce(&ExchangeOptions, &str, i64) -> Result<Rc<dyn RestAdapter>, String>,
) -> Result<BlofinApi, String> {
    if options.passphrase.as_deref().is_none_or(str::is_empty)
        && options.public_data_only != Some(true)
    {
        return Err("Blofin requires a passphrase".into());
    }
    let base = BASE_URL[if options.testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    }]
    .as_str()
    .expect("static URL");
    let xhr = safe_http(options, base, 20_000)?;
    Ok(BlofinApi {
        options: options.clone(),
        xhr,
        runtime,
    })
}

// qs.stringify default profile followed by decodeURIComponent. UTF-8 JSON-only
// input excludes JS getters, dates, cycles, undefined own properties and lone surrogates.
fn query(value: &Value) -> Result<String, String> {
    fn append(key: &str, value: &Value, out: &mut Vec<String>) -> Result<(), String> {
        match value {
            Value::Array(a) => {
                for (i, v) in a.iter().enumerate() {
                    append(&format!("{key}[{i}]"), v, out)?;
                }
            }
            Value::Object(m) => {
                for k in js_own_keys(m.keys().map(String::as_str)) {
                    append(&format!("{key}[{k}]"), &m[k], out)?;
                }
            }
            Value::Null => out.push(format!("{key}=")),
            v => out.push(format!(
                "{key}={}",
                js_string(Some(v)).map_err(|e| e.message.to_owned())?
            )),
        }
        Ok(())
    }
    let mut out = Vec::new();
    match value {
        Value::Object(m) => {
            for k in js_own_keys(m.keys().map(String::as_str)) {
                append(k, &m[k], &mut out)?;
            }
        }
        Value::Array(a) => {
            for (i, v) in a.iter().enumerate() {
                append(&i.to_string(), v, &mut out)?;
            }
        }
        _ => (),
    }
    Ok(out.join("&"))
}
impl BlofinApi {
    pub fn intercept_request(&self, mut config: RestRequest) -> Result<RestRequest, String> {
        if config.url.contains("public")
            || config.url.contains("market")
            || self.options.public_data_only == Some(true)
            || self.options.remote_signer.is_some()
            || self.runtime.remote_signer_present()
            || self.options.key.is_empty()
            || self.options.secret.is_empty()
        {
            return Ok(config);
        }
        if config.query_params.is_some() {
            return Err("Blofin signing requires the JSON params profile".into());
        }
        let method = if config.method.is_empty() {
            "GET".into()
        } else {
            config.method.to_uppercase()
        };
        let params = config
            .params
            .as_ref()
            .filter(|p| js_truthy(Some(p)))
            .map(query)
            .transpose()?;
        let url = match params.filter(|p| !p.is_empty()) {
            Some(p) => format!("{}?{p}", config.url),
            None => config.url.clone(),
        };
        let data = config
            .body
            .as_ref()
            .map(|b| b.untransformed())
            .transpose()
            .map_err(|e| e.to_string())?
            .map(|v| match v {
                // A replayed config carries the body axios already serialized: sign those bytes.
                safe_cex_types::retained_json::RetainedJsonInput::Text(text) => {
                    String::from_utf16(text)
                        .map(Some)
                        .map_err(|error| error.to_string())
                }
                v if safe_cex_num::js_retained_truthy(v) => {
                    safe_cex_num::js_retained_json_stringify(v).map_err(|e| e.message.to_owned())
                }
                _ => Ok(None),
            })
            .transpose()?
            .flatten();
        let timestamp = self.runtime.timestamp();
        let nonce = self.runtime.nonce();
        let mut payload = format!("{url}{method}{}{nonce}", js_number_string(timestamp));
        if let Some(data) = data.filter(|s| !s.is_empty()) {
            payload.push_str(&data);
        }
        // Blofin signs BASE64(ASCII_HEX(HMAC)), not BASE64(raw HMAC).
        let signature = STANDARD.encode(hmac_sha256(
            self.options.secret.as_bytes(),
            payload.as_bytes(),
            Encoding::Hex,
        ));
        let mut headers = config.headers.clone();
        for (key, value) in [
            ("ACCESS-KEY", self.options.key.clone()),
            ("ACCESS-SIGN", signature),
            ("ACCESS-NONCE", nonce),
            ("ACCESS-TIMESTAMP", js_number_string(timestamp)),
            (
                "ACCESS-PASSPHRASE",
                self.options.passphrase.clone().unwrap_or_default(),
            ),
        ] {
            headers.insert(key.into(), value);
        }
        config.headers.clear();
        config.merge_axios_headers(&headers);
        Ok(config)
    }
    pub fn intercept_response(
        &self,
        response: &RestResponse,
        config: &RestRequest,
    ) -> Result<(), String> {
        // Source deliberately accesses the exact property, not case-insensitive get().
        if config
            .headers
            .get("ACCESS-SIGN")
            .is_none_or(String::is_empty)
        {
            return Ok(());
        }
        let data = body::<String>(&response.body)
            .map_err(|e| format!("Blofin response ingress: {e:?}"))?;
        let code = data.json().get("code").filter(|v| !v.is_null());
        let code = js_string(code.or(Some(&Value::String(String::new()))))
            .map_err(|e| e.message.to_owned())?;
        if code == "152401" || code == "152406" {
            self.runtime.record_auth_failure();
        } else if code == "0" {
            self.runtime.record_auth_success();
        }
        Ok(())
    }
}
struct Attempt<'a> {
    api: &'a BlofinApi,
    calls: Cell<usize>,
    last: RefCell<Option<RestRequest>>,
}
impl RestAdapter for Attempt<'_> {
    fn query_in_url(&self) -> bool {
        self.api.xhr.query_in_url()
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
        Box::pin(async move {
            self.calls.set(self.calls.get() + 1);
            let request = self
                .api
                .intercept_request(request.clone())
                .map_err(|e| RestFailure::pre_transmit(e, request))?;
            *self.last.borrow_mut() = Some(request.clone());
            let execution = self.api.xhr.execute_with_receipt(request.clone()).await?;
            let response = &execution.response;
            *self.last.borrow_mut() = Some(execution.config.clone());
            self.api
                .intercept_response(response, &execution.config)
                .map_err(|e| RestFailure::pre_transmit(e, execution.config.clone()))?;
            Ok(execution)
        })
    }
}
impl RestAdapter for BlofinApi {
    fn query_in_url(&self) -> bool {
        self.xhr.query_in_url()
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
        mut request: RestRequest,
    ) -> RestFuture<'_, safe_cex_transport::RestExecution> {
        // Axios merges factory defaults before axios-retry captures its budget.
        request.timeout_ms = Some(request.timeout_ms.unwrap_or(20_000));
        request.method = if request.method.is_empty() {
            "get".into()
        } else {
            request.method.to_lowercase()
        };
        Box::pin(async move {
            let attempt = Attempt {
                api: self,
                calls: Cell::new(0),
                last: RefCell::new(None),
            };
            let result = match ReadClassification::Methods.classify(request) {
                ClassifiedRequest::Read(r) => {
                    network_error_on_read(r, 3)
                        .execute_with_receipt(&attempt, self.runtime.as_ref())
                        .await
                }
                ClassifiedRequest::Write(r) => r.execute_with_receipt(&attempt).await,
            };
            // axios-retry nests xhr.request promises. The venue response handler
            // runs again on each successful outer unwind, including auth failures.
            if let Ok(execution) = &result {
                for _ in 1..attempt.calls.get() {
                    let last = attempt.last.borrow();
                    let config = last.as_ref().expect("completed attempt");
                    self.intercept_response(&execution.response, config)
                        .map_err(|e| RestFailure::pre_transmit(e, config.clone()))?;
                }
            }
            result
        })
    }
}

//! Factory and installed request/response bodies over imported transport capabilities.
use crate::{
    api::{serialize_apex_payload, serialize_apex_payload_legacy},
    data::{self, get, property, truthy},
    exchange::failure,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_js_json, to_json};
use safe_cex_exchange::{
    error_reporting::{ExchangeErrorInput, append_http_failure},
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_num::{JsJsonValue as J, js_number_string};
use safe_cex_types::ExchangeOptions;
use std::rc::{Rc, Weak};
pub struct ApexApiConfiguration {
    pub base_url: &'static str,
    pub timeout_ms: f64,
    pub headers: J,
    pub params_serializer: fn(&J) -> Result<String, String>,
}
pub trait ApexApiHost {
    fn create_client(
        &self,
        config: ApexApiConfiguration,
        options: &ExchangeOptions,
        exchange: &Rc<BaseExchange>,
    ) -> Result<(), ExchangeError>;
    /// axios-retry with `networkErrorOnRead()`: signature failures arrive in a 200
    /// envelope and take the legacy-signing resend instead.
    fn configure_retry(&self, retries: u8) -> Result<(), ExchangeError>;
    fn register_request_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError>;
    fn register_response_handler(&self, api: Weak<ApexApi>) -> Result<(), ExchangeError>;
    fn install_transport_adapter(
        &self,
        options: &ExchangeOptions,
        name: &str,
    ) -> Result<(), ExchangeError>;
    fn now_ms(&self) -> f64;
    fn legacy_timestamp_ms(&self, now: f64) -> Result<f64, ExchangeError>;
    fn axios_headers(&self, fields: J) -> Result<J, ExchangeError>;
    fn set_content_type(&self, headers: &mut J, value: &str) -> Result<(), ExchangeError>;
    fn request_retry(&self, config: Js) -> ExchangeFuture<'_, Js>;
}
pub struct ApexApi {
    pub options: ExchangeOptions,
    pub exchange: Rc<BaseExchange>,
    pub host: Rc<dyn ApexApiHost>,
}
fn object_fields(value: &J) -> Result<Vec<(String, J)>, ExchangeError> {
    match value {
        J::Undefined | J::Null => Ok(Vec::new()),
        J::Object(fields) => Ok(fields.clone()),
        J::Array(fields) => Ok(fields
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect()),
        J::String(value) => value
            .encode_utf16()
            .enumerate()
            .map(|(i, c)| {
                Ok((
                    i.to_string(),
                    J::String(String::from_utf16(&[c]).map_err(|_| {
                        failure("prepared string property spread refuses a lone surrogate".into())
                    })?),
                ))
            })
            .collect(),
        _ => Ok(Vec::new()),
    }
}
fn set(fields: &mut Vec<(String, J)>, key: &str, value: J) {
    if let Some((_, old)) = fields.iter_mut().rev().find(|(k, _)| k == key) {
        *old = value;
    } else {
        fields.push((key.into(), value));
    }
}
fn text(value: &J) -> Result<String, ExchangeError> {
    data::string(value).map_err(failure)
}
fn join_text(value: &J) -> Result<String, ExchangeError> {
    if matches!(value, J::Undefined | J::Null) {
        Ok(String::new())
    } else {
        text(value)
    }
}
// ts: createAPI
pub fn create_api(
    options: ExchangeOptions,
    exchange: Rc<BaseExchange>,
    host: Rc<dyn ApexApiHost>,
) -> Result<Rc<ApexApi>, ExchangeError> {
    let timeout = options
        .extra
        .as_ref()
        .and_then(|e| e.recv_window)
        .unwrap_or(crate::protocol::RECV_WINDOW as f64);
    let base_url = if options.testnet == Some(true) {
        crate::protocol::BASE_URL[1].1
    } else {
        crate::protocol::BASE_URL[0].1
    };
    host.create_client(
        ApexApiConfiguration {
            base_url,
            timeout_ms: timeout,
            headers: data::object([(
                "Content-Type",
                J::String("application/json, charset=utf-8".into()),
            )]),
            params_serializer: serialize_apex_payload,
        },
        &options,
        &exchange,
    )?;
    host.configure_retry(3)?;
    let api = Rc::new(ApexApi {
        options,
        exchange,
        host,
    });
    api.host.register_request_handler(Rc::downgrade(&api))?;
    if api.options.tealstreet_tunnel == Some(true) {
        api.host
            .install_transport_adapter(&api.options, "apexomni")?;
    }
    api.host.register_response_handler(Rc::downgrade(&api))?;
    Ok(api)
}
impl ApexApi {
    pub fn request_handler(&self, config: &J) -> Result<J, ExchangeError> {
        let mut next = object_fields(config)?;
        set(
            &mut next,
            "params",
            J::Object(object_fields(property(config, "params")?)?),
        );
        if get(config, "method") == &J::String("get".into())
            && get(config, "url") == &J::String("/v3/klines".into())
        {
            set(
                &mut next,
                "headers",
                self.host.axios_headers(J::Object(Vec::new()))?,
            );
            return Ok(J::Object(next));
        }
        let method = if truthy(get(config, "method")) {
            text(get(config, "method"))?
        } else {
            "get".into()
        }
        .to_uppercase();
        let query = method == "GET" || method == "DELETE";
        let next_view = J::Object(next.clone());
        let payload = if query {
            get(&next_view, "params")
        } else {
            get(&next_view, "data")
        };
        let legacy = truthy(get(config, "__apexUseLegacySigning"));
        let message = if legacy {
            let params = serialize_apex_payload_legacy(payload).map_err(failure)?;
            let timestamp = self.host.legacy_timestamp_ms(self.host.now_ms())?;
            set(&mut next, "__apexTimestamp", J::Number(timestamp));
            let path = if truthy(get(config, "url")) {
                text(get(config, "url"))?
            } else {
                String::new()
            };
            let extra = if path.contains("/v3/order") {
                join_text(get(config, "data"))?
            } else {
                String::new()
            };
            format!(
                "{}{}{}{}{}{}{}",
                js_number_string(timestamp),
                method,
                "/api",
                path,
                if query && !params.is_empty() { "?" } else { "" },
                params,
                extra
            )
        } else {
            let payload = serialize_apex_payload(payload).map_err(failure)?;
            let timestamp = js_number_string(self.host.now_ms());
            set(&mut next, "__apexTimestamp", J::String(timestamp.clone()));
            let path = format!(
                "/api{}{}",
                text(get(config, "url"))?,
                if query && !payload.is_empty() {
                    format!("?{payload}")
                } else {
                    String::new()
                }
            );
            format!(
                "{timestamp}{method}{path}{}",
                if query { String::new() } else { payload }
            )
        };
        let key = STANDARD.encode(self.options.secret.as_bytes());
        let signature = crate::utils::hmac_sha256_base64(
            &message.encode_utf16().collect::<Vec<_>>(),
            &key.encode_utf16().collect::<Vec<_>>(),
        );
        let mut header_fields = object_fields(get(config, "headers"))?;
        set(&mut header_fields, "APEX-SIGNATURE", J::String(signature));
        set(
            &mut header_fields,
            "APEX-API-KEY",
            J::String(self.options.key.clone()),
        );
        set(
            &mut header_fields,
            "APEX-PASSPHRASE",
            J::String(
                self.options
                    .passphrase
                    .clone()
                    .unwrap_or_else(|| "undefined".into()),
            ),
        );
        set(
            &mut header_fields,
            "APEX-TIMESTAMP",
            J::String(text(
                &next
                    .iter()
                    .rev()
                    .find(|(k, _)| k == "__apexTimestamp")
                    .unwrap()
                    .1,
            )?),
        );
        let mut headers = self.host.axios_headers(J::Object(header_fields))?;
        if let J::String(url) = get(config, "url")
            && url.contains("/v3/set-initial-margin-rate")
        {
            self.host
                .set_content_type(&mut headers, "multipart/form-data")?;
        }
        set(&mut next, "headers", headers);
        Ok(J::Object(next))
    }
    pub async fn response_handler(&self, response: &Js) -> Result<Js, ExchangeError> {
        let read = || -> Result<(Js, Js), Thrown> {
            let body = response.get("data")?;
            let code = body.get("code")?;
            Ok((body, code))
        };
        let (body, code) = read().map_err(|e| e.to_exchange_error())?;
        if code.truthy() && !matches!(&code, Js::Number(v) if *v == 0.) {
            let config = response.get("config").map_err(|e| e.to_exchange_error())?;
            let numeric = code.to_number().map_err(|e| e.to_exchange_error())?;
            let msg = body.get("msg").map_err(|e| e.to_exchange_error())?;
            let message = if msg.is_nullish() {
                String::new()
            } else {
                msg.to_js_string()
                    .and_then(|s| {
                        String::from_utf16(&s).map_err(|_| {
                            Thrown::type_error("Cannot represent JavaScript UTF-16 string as UTF-8")
                        })
                    })
                    .map_err(|e| e.to_exchange_error())?
            };
            let signature_failure = numeric == 20016.
                || numeric == 401.
                || message.to_ascii_lowercase().contains("sign");
            if signature_failure
                && !config
                    .get("__apexLegacyRetried")
                    .map_err(|e| e.to_exchange_error())?
                    .truthy()
            {
                let next = config.spread_object().map_err(|e| e.to_exchange_error())?;
                let headers = config.get("headers").map_err(|e| e.to_exchange_error())?;
                let headers = if headers.truthy() {
                    headers
                } else {
                    Js::object(vec![])
                };
                // Request headers remain the strict prepared signing domain.
                let headers = self
                    .host
                    .axios_headers(to_js_json(&headers).map_err(|e| e.to_exchange_error())?)?;
                next.define("headers", data::js(&headers))
                    .map_err(|e| e.to_exchange_error())?;
                next.define("__apexLegacyRetried", Js::Bool(true))
                    .map_err(|e| e.to_exchange_error())?;
                next.define("__apexUseLegacySigning", Js::Bool(true))
                    .map_err(|e| e.to_exchange_error())?;
                return self.host.request_retry(next).await;
            }
            let render = |v: &Js| -> Result<String, ExchangeError> {
                String::from_utf16(&v.to_js_string().map_err(|e| e.to_exchange_error())?).map_err(
                    |_| failure("Cannot represent JavaScript UTF-16 string as UTF-8".into()),
                )
            };
            let message = format!("[{}] {}", render(&code)?, render(&msg)?);
            let status = response.get("status").map_err(|e| e.to_exchange_error())?;
            let status_view =
                serde_json::json!({"status":to_json(&status).map_err(|e|e.to_exchange_error())?});
            let message =
                append_http_failure(Some(&serde_json::Value::String(message)), &status_view);
            self.exchange.emit_and_log_error(
                ExchangeErrorInput::Message(message.clone()),
                None,
                None,
            )?;
            return Err(ExchangeError::new(message));
        }
        Ok(response.clone())
    }
}

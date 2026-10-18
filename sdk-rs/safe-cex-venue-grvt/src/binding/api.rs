//! The three source GRVT Axios factories, including their session interceptors.
use super::runtime::{Host, value};
use crate::{api_runtime::ApiHost, builder_workflow::JsResult};
use safe_cex_exchange::{
    js_runtime::{
        client,
        value::{Js, arg},
    },
    runtime::LogSeverity,
};

impl ApiHost for Host {
    fn base_urls(&self) -> Js {
        Js::object(
            crate::protocol::BASE_URLS
                .iter()
                .map(|(key, url)| (*key, Js::str(url)))
                .collect(),
        )
    }
    fn create_axios(&self, config: Js, options: Js, _exchange: Js) -> JsResult<Js> {
        value(client::factory::create(
            self.0.shared.http.clone(),
            self.0.shared.runtime.transport().as_ref(),
            config,
            &options,
            self.0.base(),
        ))
    }
    fn read_retry_condition(&self) -> JsResult<Js> {
        Ok(Js::function("networkErrorOnGrvtRead", move |_, args| {
            let error = arg(&args, 0);
            let config = error.get("config")?;
            if config.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let url = config.get("url")?;
            let url = if url.is_nullish() {
                String::new()
            } else {
                String::from_utf16_lossy(&url.to_js_string()?)
            };
            let request = safe_cex_transport::RestRequest {
                url,
                ..Default::default()
            };
            let read = safe_cex_transport::retry::is_grvt_read(&request);
            let code = error.get("code")?;
            let code = if code.is_undefined() {
                None
            } else {
                Some(String::from_utf16_lossy(&code.to_js_string()?))
            };
            let network = error.get("response")?.is_nullish()
                && safe_cex_transport::retry::is_network_error(
                    &safe_cex_transport::RestFailure::from_transport(
                        safe_cex_transport::TransportError::Network(
                            safe_cex_transport::NetworkFailure::from_code(String::new(), code),
                        ),
                        safe_cex_transport::RestRequest::default(),
                    ),
                );
            Ok(Js::Bool(read && network))
        }))
    }
    fn install_retry(&self, client: Js, options: Js) -> JsResult<Js, ()> {
        value(client.call_method("__configureRetry", vec![options]))?;
        Ok(())
    }
    fn severity(&self, severity: LogSeverity) -> Js {
        Js::str(severity.as_str())
    }
}

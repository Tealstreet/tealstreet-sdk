//! Axios-shaped `xhr` over the typed signed clients: the JS client runs no interceptors
//! of its own, so signing, re-signing, retries and rate limits stay in the Rust createAPI.
use super::value::{Js, Thrown, from_json, to_json};
use safe_cex_exchange::{
    js_runtime::{client::JsHttpHost, promise::JsFuture},
    runtime::ExchangeRuntime,
};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestRequest, RestResponse, secondary::SecondaryKeyClients,
};
use std::{rc::Rc, sync::Arc};

pub(crate) enum Wire {
    /// `this.xhr`: GETs go through the secondary-key client, everything else primary.
    Split(Rc<SecondaryKeyClients>),
    /// `this.unlimitedXHR`: one client, no rate limiter.
    Single(Arc<dyn RestAdapter>),
}

pub(crate) struct TypedHttpHost {
    pub wire: Wire,
    pub runtime: Rc<dyn ExchangeRuntime>,
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn request(config: &Js) -> Result<RestRequest, Thrown> {
    let params = config.get("params")?;
    let data = config.get("data")?;
    let retry = config.get("axios-retry")?;
    let mut config_extensions = Default::default();
    if !retry.is_undefined() {
        config_extensions = [("axios-retry".into(), to_json(&retry)?)].into();
    }
    Ok(RestRequest {
        config_extensions,
        method: text(&config.get("method")?)?.to_ascii_lowercase(),
        url: text(&config.get("url")?)?,
        params: (!params.is_nullish())
            .then(|| to_json(&params))
            .transpose()?,
        body: (!data.is_undefined())
            .then(|| to_json(&data).map(RestBody::Untransformed))
            .transpose()?,
        ..Default::default()
    })
}

fn response(response: RestResponse, config: Js) -> Result<Js, Thrown> {
    let data = response
        .body
        .json_value()
        .map_err(|e| Thrown::error("SyntaxError", &e.to_string()))?;
    let headers = Js::object(
        response
            .headers
            .iter()
            .map(|(k, v)| (k.as_str(), Js::str(v)))
            .collect(),
    );
    Ok(Js::object(vec![
        ("data", from_json(&data)),
        ("status", Js::Number(f64::from(response.status))),
        ("headers", headers),
        ("config", config),
    ]))
}

/// A failure the venue answered keeps its response, so the client rejects it as an
/// AxiosError; one with no response (signing, network) rejects with its own message.
fn failure(failure: RestFailure, config: Js) -> Result<Js, Thrown> {
    match failure.response {
        Some(answer) => response(*answer, config),
        None => {
            let error = super::value::error_object("Error", &failure.message);
            error.define("config", config)?;
            if let Some(code) = failure.code {
                error.define("code", Js::str(&code))?;
            }
            Err(Thrown::new(error))
        }
    }
}

impl JsHttpHost for TypedHttpHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let built = request(&config);
        let wire = match &self.wire {
            Wire::Split(clients) => Wire::Split(clients.clone()),
            Wire::Single(adapter) => Wire::Single(adapter.clone()),
        };
        Box::pin(async move {
            let request = built?;
            let result = match &wire {
                Wire::Split(clients) if request.method == "get" => clients.get(request).await,
                Wire::Split(clients) => clients.request(request).await,
                Wire::Single(adapter) => adapter.execute(request).await,
            };
            match result {
                Ok(answer) => response(answer, config),
                Err(error) => failure(error, config),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_keeps_per_request_retry_namespace() {
        let config = Js::object(vec![
            ("method", Js::str("post")),
            ("url", Js::str("/v5/order/create")),
            ("data", Js::object(vec![("symbol", Js::str("BTCUSDT"))])),
            (
                "axios-retry",
                Js::object(vec![("retries", Js::Number(0.0))]),
            ),
        ]);
        let built = request(&config).unwrap();
        assert_eq!(
            built.config_extensions.get("axios-retry"),
            Some(&serde_json::json!({"retries":0.0})),
            "binding_retry_namespace_reaches_typed_factory"
        );
        config.set("axios-retry", Js::Undefined).unwrap();
        assert!(
            !request(&config)
                .unwrap()
                .config_extensions
                .contains_key("axios-retry")
        );
    }
}

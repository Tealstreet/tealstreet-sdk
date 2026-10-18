//! The wire under the Axios-shaped clients createAPI/createKucoinSpotAPI return:
//! each request runs through the ported KuCoin API factory (signing, broker
//! headers, read retry) and its shared HTTP client.
use crate::api::{create_api, create_kucoin_spot_api};
use safe_cex_exchange::{
    http::HttpClientOptions,
    js_runtime::{
        base_exchange::HostedSigner,
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown, error_object, from_json, to_json},
    },
    runtime::ExchangeRuntime,
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestRequest, RestResponse, TransportRegistry,
    rate_limit::RateLimitClock,
};
use safe_cex_types::ExchangeOptions;
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub(crate) struct ApiHost {
    pub me: Weak<ApiHost>,
    pub runtime: Rc<dyn ExchangeRuntime>,
    pub options: ExchangeOptions,
    pub hosted_signer: Option<HostedSigner>,
    pub spot: bool,
    pub http: Rc<dyn Fn(&ExchangeOptions) -> HttpClientOptions>,
    pub registry: Rc<TransportRegistry>,
    pub clock: VirtualClock,
    pub retry_clock: Rc<dyn RateLimitClock>,
    pub api: RefCell<Option<Rc<dyn RestAdapter>>>,
}

fn transport(e: impl std::fmt::Display) -> Thrown {
    Thrown::error("Error", &e.to_string())
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn headers(map: &std::collections::BTreeMap<String, String>) -> Js {
    Js::object(map.iter().map(|(k, v)| (k.as_str(), Js::str(v))).collect())
}

fn response_object(response: &RestResponse, config: &Js) -> Result<Js, Thrown> {
    let data = response.body.json_value().map_err(transport)?;
    Ok(Js::object(vec![
        ("data", from_json(&data)),
        ("status", Js::Number(f64::from(response.status))),
        ("headers", headers(&response.headers)),
        ("config", config.clone()),
    ]))
}

/// The Axios error the source catch blocks read (`response.data`, `message`).
fn failure_error(failure: &RestFailure, config: &Js) -> Result<Thrown, Thrown> {
    let error = error_object("AxiosError", &failure.message);
    error.define("isAxiosError", Js::Bool(true))?;
    error.define("config", config.clone())?;
    if let Some(code) = &failure.code {
        error.define("code", Js::str(code))?;
    }
    if let Some(response) = &failure.response {
        error.define("response", response_object(response, config)?)?;
        error.define("status", Js::Number(f64::from(response.status)))?;
    }
    Ok(Thrown::new(error))
}

fn request(config: &Js) -> Result<RestRequest, Thrown> {
    let method = config.get("method")?;
    let mut request = RestRequest {
        method: if method.is_nullish() {
            "get".into()
        } else {
            text(&method)?.to_lowercase()
        },
        url: text(&config.get("url")?)?,
        ..Default::default()
    };
    let header_values = config.get("headers")?;
    if header_values.is_object() {
        for (k, v) in header_values.own_entries()? {
            if !v.is_nullish() {
                request
                    .headers
                    .insert(String::from_utf16_lossy(&k), text(&v)?);
            }
        }
    }
    let params = config.get("params")?;
    if !params.is_undefined() {
        request.params = Some(to_json(&params)?);
    }
    let data = config.get("data")?;
    if !data.is_undefined() {
        request.body = Some(RestBody::Untransformed(
            safe_cex_exchange::js_runtime::value::to_retained_input(&data)?,
        ));
    }
    Ok(request)
}

impl ApiHost {
    /// A fresh client wire for `createAPI(options)`: each call owns its signer and adapter.
    pub(crate) fn with_options(&self, options: ExchangeOptions) -> Rc<ApiHost> {
        Rc::new_cyclic(|me| ApiHost {
            me: me.clone(),
            runtime: self.runtime.clone(),
            options,
            hosted_signer: self.hosted_signer.clone(),
            spot: self.spot,
            http: self.http.clone(),
            registry: self.registry.clone(),
            clock: self.clock.clone(),
            retry_clock: self.retry_clock.clone(),
            api: RefCell::new(None),
        })
    }

    async fn api(self: Rc<Self>) -> Result<Rc<dyn RestAdapter>, Thrown> {
        if let Some(api) = self.api.borrow().clone() {
            return Ok(api);
        }
        let http = (self.http)(&self.options);
        let api: Rc<dyn RestAdapter> = if self.spot {
            Rc::new(
                create_kucoin_spot_api(
                    http,
                    &self.registry,
                    self.clock.clone(),
                    self.retry_clock.clone(),
                )
                .await
                .map_err(transport)?,
            )
        } else {
            Rc::new(
                create_api(
                    http,
                    &self.registry,
                    self.clock.clone(),
                    self.retry_clock.clone(),
                )
                .await
                .map_err(transport)?,
            )
        };
        let existing = self.api.borrow().clone();
        Ok(existing.unwrap_or_else(|| {
            *self.api.borrow_mut() = Some(api.clone());
            api
        }))
    }
}

impl JsHttpHost for ApiHost {
    fn hosted_signer(&self) -> Option<HostedSigner> {
        self.hosted_signer.clone()
    }
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let me = self.me.upgrade();
        Box::pin(async move {
            let me = me.ok_or_else(|| Thrown::error("Error", "KuCoin API host dropped"))?;
            let request = request(&config)?;
            let api = me.api().await?;
            match api.execute(request).await {
                Ok(response) => response_object(&response, &config),
                Err(failure) => Err(failure_error(&failure, &config)?),
            }
        })
    }
}

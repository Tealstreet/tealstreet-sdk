use super::{
    crypto::CryptoHost,
    error::{exchange_error, failure, from_thrown},
    rt::Host,
    value::{self, from_js, text, to_js},
};
use crate::{
    api_factory::{ApiCreateConfig, ApiInterceptor, ApiRequest, CoinbaseApiHost},
    cdp_signing::{self, CdpSigningReceiver},
    portfolios_balances::PortfolioFailure,
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use indexmap::IndexMap;
use safe_cex_exchange::{
    js_runtime::{
        client,
        promise::Promise as JsPromise,
        value::{Js, Thrown, arg, to_json, to_typed},
    },
    runtime::ExchangeError,
};
use safe_cex_transport::{
    NetworkFailure, RestFailure, RestRequest, RestTransportContext, TransportError,
    query::{QueryArrayFormat, QueryKeyOrder, QueryProfile, render_query},
};
use std::{cell::RefCell, rc::Rc};

fn fields(config: &Js) -> Result<ApiRequest, PortfolioFailure> {
    let mut fields = IndexMap::new();
    for (key, value) in config.own_entries().map_err(from_thrown)? {
        if key.as_ref() != "paramsSerializer".encode_utf16().collect::<Vec<_>>()
            && key.as_ref() != "adapter".encode_utf16().collect::<Vec<_>>()
        {
            fields.insert(key.to_vec(), from_js(&value)?);
        }
    }
    Ok(Rc::new(RefCell::new(fields)))
}

fn frame_config(config: &ApiRequest) -> Js {
    let value = Js::object(vec![]);
    for (key, field) in config.borrow().iter() {
        value
            .define_key(key, field.as_ref().map(to_js).unwrap_or(Js::Undefined))
            .expect("fresh config");
    }
    value
}

impl CoinbaseApiHost for Host {
    type Client = Frame;
    fn safe_create(
        &self,
        config: ApiCreateConfig,
        _options: Frame,
        _exchange: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        let headers = Js::object(vec![(
            "Accept",
            Js::str("application/json, text/plain, */*"),
        )]);
        for (name, value) in config.headers {
            headers
                .define(name, value.as_ref().map(to_js).unwrap_or(Js::Undefined))
                .map_err(from_thrown)?;
        }
        let serialize = config.serialize;
        let client = client::factory::create(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            Js::object(vec![
                ("baseURL", Js::str(config.base_url)),
                ("timeout", to_js(&config.timeout)),
                ("headers", headers),
                (
                    "paramsSerializer",
                    Js::object(vec![(
                        "serialize",
                        Js::function("serialize", move |_, args| {
                            let value = from_js(&arg(&args, 0))
                                .map_err(|error| Thrown::from(exchange_error(&error)))?
                                .unwrap_or_else(|| Frame::Object(Rc::default()));
                            serialize(value)
                                .map(|text| Js::str(&text))
                                .map_err(|error| Thrown::from(exchange_error(&error)))
                        }),
                    )]),
                ),
            ]),
            &to_js(&_options),
            self.base(),
        )
        .map_err(from_thrown)?;
        Ok(self.register_client(client))
    }
    fn serialize_repeat(&self, params: Frame) -> Result<String, PortfolioFailure> {
        render_query(
            &value::query(&params)?,
            QueryProfile {
                arrays: QueryArrayFormat::Repeat,
                encode: true,
                key_order: QueryKeyOrder::Preserve,
            },
        )
        .map_err(|error| from_thrown(error.into()))
    }
    fn network_error_on_read(&self) -> Result<Frame, PortfolioFailure> {
        Ok(Frame::Object(Rc::default())) // Opaque imported predicate handle, consumed by retry below.
    }
    fn retry(
        &self,
        identity: Frame,
        retries: f64,
        _condition: Frame,
    ) -> Result<(), PortfolioFailure> {
        let condition = Js::function("networkErrorOnRead", |_, args| {
            let error = arg(&args, 0);
            let config = error.get("config")?;
            if config.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let method = config.get("method")?;
            let method = if method.is_nullish() {
                String::new()
            } else {
                method.to_display().to_lowercase()
            };
            if !matches!(method.as_str(), "get" | "head") || error.get("response")?.truthy() {
                return Ok(Js::Bool(false));
            }
            let code = error.get("code")?;
            let code = if code.is_undefined() {
                None
            } else {
                Some(code.to_display())
            };
            let network = RestFailure::from_transport(
                TransportError::Network(NetworkFailure::from_code(String::new(), code)),
                RestRequest::default(),
            );
            Ok(Js::Bool(safe_cex_transport::retry::is_network_error(
                &network,
            )))
        });
        self.client(&identity)?
            .call_method(
                "__configureRetry",
                vec![Js::object(vec![
                    ("retries", Js::Number(retries)),
                    ("retryCondition", condition),
                ])],
            )
            .map(|_| ())
            .map_err(from_thrown)
    }
    fn install_interceptor(
        &self,
        identity: Frame,
        interceptor: ApiInterceptor,
    ) -> Result<(), PortfolioFailure> {
        let runtime = self.runtime();
        let native = Js::function("coinbaseRequest", move |_, args| {
            let original = arg(&args, 0);
            let serializer = original.get("paramsSerializer")?;
            let adapter = original.get("adapter")?;
            let config = fields(&original).map_err(|error| Thrown::from(exchange_error(&error)))?;
            let result = interceptor(config);
            let serializer = serializer.clone();
            Ok(Js::Promise(JsPromise::spawn(
                &runtime,
                Box::pin(async move {
                    let config = result
                        .await
                        .map_err(|error| Thrown::from(exchange_error(&error)))?;
                    let output = frame_config(&config);
                    if !serializer.is_undefined() {
                        output.define("paramsSerializer", serializer)?;
                    }
                    if !adapter.is_undefined() {
                        output.define("adapter", adapter)?;
                    }
                    Ok(output)
                }),
            )))
        });
        self.client(&identity)?
            .get("interceptors")
            .map_err(from_thrown)?
            .get("request")
            .map_err(from_thrown)?
            .call_method("use", vec![native])
            .map(|_| ())
            .map_err(from_thrown)
    }
    fn install_transport_adapter(
        &self,
        _client: Frame,
        _options: Frame,
        _exchange_name: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        client::factory::install_transport_adapter(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            &self.client(&_client)?,
            &to_js(&_options),
            to_typed(&_exchange_name.as_ref().map(to_js).unwrap_or(Js::Undefined))
                .map_err(from_thrown)?,
            safe_cex_transport::RestAdapterVariant::Default,
        )
        .map_err(from_thrown)
    }
    fn replace_url_params(
        &self,
        url: Option<Frame>,
        params: Option<Frame>,
    ) -> Result<ApiRequest, PortfolioFailure> {
        let original_url = url.clone();
        let params = params.unwrap_or_else(|| Frame::Object(Rc::default()));
        let url = url.as_ref().map(to_js).unwrap_or(Js::Undefined);
        if !url.truthy() {
            return Ok(Rc::new(RefCell::new(IndexMap::from([
                ("url".encode_utf16().collect(), original_url),
                ("params".encode_utf16().collect(), Some(params)),
            ]))));
        }
        let url = url
            .as_str()
            .ok_or_else(|| Self::json_error("TypeError", "url.match is not a function"))?;
        let url = String::from_utf16(url)
            .map_err(|_| Self::json_error("TypeError", "Lone UTF-16 URL at a native boundary"))?;
        let params_js = to_js(&params).spread_object().map_err(from_thrown)?;
        let params_json = to_json(&params_js).map_err(from_thrown)?;
        let (url, params) = safe_cex_types::model_utils::replace_url_params(
            Some(&url),
            params_json.as_object().expect("spread object"),
        )
        .map_err(|error| from_thrown(error.into()))?;
        Ok(Rc::new(RefCell::new(IndexMap::from([
            ("url".encode_utf16().collect(), url.as_deref().map(text)),
            (
                "params".encode_utf16().collect(),
                Some(Frame::from_json(&serde_json::Value::Object(params))),
            ),
        ]))))
    }
    fn mark_skip_remote_signing(&self, config: ApiRequest) -> Result<(), PortfolioFailure> {
        config.borrow_mut().insert(
            "safeCexSkipRemoteSigning".encode_utf16().collect(),
            Some(Frame::Bool(true)),
        );
        Ok(())
    }
    fn proxy_prefix(
        &self,
        _exchange: &'static str,
        options: Frame,
        config: ApiRequest,
    ) -> Result<Vec<u16>, PortfolioFailure> {
        let options =
            safe_cex_exchange::js_runtime::base_exchange::exchange_options(&to_js(&options))
                .map_err(from_thrown)?;
        let config = frame_config(&config);
        let mut request = RestRequest {
            method: config.get("method").map_err(from_thrown)?.to_display(),
            url: config.get("url").map_err(from_thrown)?.to_display(),
            ..Default::default()
        };
        let base = config.get("baseURL").map_err(from_thrown)?;
        if !base.is_undefined() {
            request.base_url = Some(base.to_display());
        }
        let data = config.get("data").map_err(from_thrown)?;
        if !data.is_undefined() {
            request.body = Some(safe_cex_transport::RestBody::Untransformed(
                safe_cex_exchange::js_runtime::value::to_retained_input(&data)
                    .map_err(from_thrown)?,
            ));
        }
        let timeout = config.get("timeout").map_err(from_thrown)?;
        request.timeout_ms = timeout.as_f64().map(|value| value as i64);
        for (key, value) in config
            .get("headers")
            .map_err(from_thrown)?
            .own_entries()
            .map_err(from_thrown)?
        {
            if !value.is_undefined() {
                request
                    .headers
                    .insert(String::from_utf16_lossy(&key), value.to_display());
            }
        }
        request.skip_remote_signing = config
            .get("safeCexSkipRemoteSigning")
            .map_err(from_thrown)?
            .truthy();
        request.params =
            Some(to_json(&config.get("params").map_err(from_thrown)?).map_err(from_thrown)?);
        let prefix = self
            .shared
            .runtime
            .transport()
            .proxy_prefix(RestTransportContext {
                exchange: &safe_cex_types::ExchangeName::Coinbase,
                options: &options,
                config: &request,
            });
        Ok(prefix.encode_utf16().collect())
    }
    fn headers(&self, fields: Frame, _from: bool) -> Result<Frame, PortfolioFailure> {
        Ok(fields)
    }
    fn sign(
        &self,
        key: Option<Frame>,
        secret: Option<Frame>,
        uri: Vec<u16>,
    ) -> Result<Promise<Result<Frame, PortfolioFailure>>, PortfolioFailure> {
        let key = crate::public_data::string(key)
            .map_err(|error| Self::json_error(error.name, &error.message))?;
        let secret = crate::public_data::string(secret)
            .map_err(|error| Self::json_error(error.name, &error.message))?;
        let utf8 = |value: Vec<u16>| {
            String::from_utf16(&value).map_err(|_| {
                failure(ExchangeError::new(
                    "Lone UTF-16 signing input at native boundary",
                ))
            })
        };
        let receiver = Rc::new(CdpSigningReceiver {
            host: CryptoHost {
                runtime: self.runtime(),
            },
            cache: self.shared.extra.signing_cache.clone(),
            spawn: self.shared.extra.spawn.clone(),
        });
        let result = cdp_signing::sign(receiver, utf8(key)?, utf8(secret)?, Some(utf8(uri)?));
        Ok(self.promise(async move { result.await.map(|token| text(&token)) }))
    }
}

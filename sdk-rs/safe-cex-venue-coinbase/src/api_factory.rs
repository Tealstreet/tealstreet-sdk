//! HTTP factory and actual async request interceptor. Native own-property records
//! retain undefined slots and shallow nested Frame identities before header leaves.
use crate::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use indexmap::IndexMap;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
pub type ApiRequest = Rc<RefCell<IndexMap<Vec<u16>, Option<Frame>>>>;
pub type ApiInterceptor = Rc<dyn Fn(ApiRequest) -> Promise<Result<ApiRequest, PortfolioFailure>>>;
pub type ApiSerializer = Rc<dyn Fn(Frame) -> Result<String, PortfolioFailure>>;
#[derive(Clone)]
pub struct ApiCreateConfig {
    pub base_url: &'static str,
    pub timeout: Frame,
    pub headers: IndexMap<&'static str, Option<Frame>>,
    pub serialize: ApiSerializer,
}
pub trait CoinbaseApiHost {
    type Client: Clone;
    fn safe_create(
        &self,
        config: ApiCreateConfig,
        options: Frame,
        exchange: Frame,
    ) -> Result<Self::Client, PortfolioFailure>;
    fn serialize_repeat(&self, params: Frame) -> Result<String, PortfolioFailure>;
    fn network_error_on_read(&self) -> Result<Frame, PortfolioFailure>;
    fn retry(
        &self,
        client: Self::Client,
        retries: f64,
        condition: Frame,
    ) -> Result<(), PortfolioFailure>;
    fn install_interceptor(
        &self,
        client: Self::Client,
        interceptor: ApiInterceptor,
    ) -> Result<(), PortfolioFailure>;
    fn install_transport_adapter(
        &self,
        client: Self::Client,
        options: Frame,
        exchange_name: Option<Frame>,
    ) -> Result<(), PortfolioFailure>;
    fn replace_url_params(
        &self,
        url: Option<Frame>,
        params: Option<Frame>,
    ) -> Result<ApiRequest, PortfolioFailure>;
    fn mark_skip_remote_signing(&self, config: ApiRequest) -> Result<(), PortfolioFailure>;
    fn proxy_prefix(
        &self,
        exchange: &'static str,
        options: Frame,
        config: ApiRequest,
    ) -> Result<Vec<u16>, PortfolioFailure>;
    fn headers(&self, fields: Frame, from: bool) -> Result<Frame, PortfolioFailure>;
    fn sign(
        &self,
        key: Option<Frame>,
        secret: Option<Frame>,
        uri: Vec<u16>,
    ) -> Result<Promise<Result<Frame, PortfolioFailure>>, PortfolioFailure>;
}
pub struct CoinbaseApiReceiver<H> {
    pub host: H,
    pub options: Frame,
    pub exchange: Frame,
    pub spawn: CoinbaseStarbasePromiseSpawner,
}
fn get(v: &ApiRequest, key: &str) -> Option<Frame> {
    v.borrow()
        .get(&key.encode_utf16().collect::<Vec<_>>())
        .cloned()
        .flatten()
}
fn put(v: &ApiRequest, key: &str, value: Option<Frame>) {
    v.borrow_mut().insert(key.encode_utf16().collect(), value);
}
fn string(v: &str) -> Frame {
    Frame::String(v.encode_utf16().collect())
}
fn truthy(v: Option<&Frame>) -> bool {
    match v {
        None | Some(Frame::Null) => false,
        Some(Frame::Bool(v)) => *v,
        Some(Frame::Number(v)) => *v != 0.0 && !v.is_nan(),
        Some(Frame::String(v)) => !v.is_empty(),
        _ => true,
    }
}
fn text(v: Option<Frame>) -> Result<Vec<u16>, PortfolioFailure> {
    crate::public_data::string(v).map_err(|e| {
        Rc::new(PortfolioThrown::from_value(Some(
            json!({"name":e.name,"message":e.message}),
        )))
    })
}
fn copy(v: &ApiRequest) -> ApiRequest {
    Rc::new(RefCell::new(v.borrow().clone()))
}
fn content() -> Frame {
    Frame::Object(Rc::new(RefCell::new(IndexMap::from([(
        "Content-Type".encode_utf16().collect(),
        string("application/json, charset=utf-8"),
    )]))))
}
pub fn create_api<H: CoinbaseApiHost + 'static>(
    r: Rc<CoinbaseApiReceiver<H>>,
) -> Result<H::Client, PortfolioFailure> {
    let network = if truthy(r.options.get("testnet").as_ref()) {
        "testnet"
    } else {
        "livenet"
    };
    let base = crate::tables::BASE_URL
        .iter()
        .find(|(k, _)| *k == network)
        .expect("two native source networks")
        .1;
    let timeout = r
        .options
        .get("extra")
        .and_then(|v| v.get("recvWindow"))
        .filter(|v| !matches!(v, Frame::Null))
        .unwrap_or(Frame::Number(crate::tables::RECV_WINDOW as f64));
    let serializer_host = r.clone();
    let serialize: ApiSerializer =
        Rc::new(move |params| serializer_host.host.serialize_repeat(params));
    let config = ApiCreateConfig {
        base_url: base,
        timeout,
        headers: IndexMap::from([
            ("X-MBX-APIKEY", r.options.get("key")),
            (
                "Content-Type",
                Some(string("application/json, charset=utf-8")),
            ),
        ]),
        serialize,
    };
    let client = r
        .host
        .safe_create(config, r.options.clone(), r.exchange.clone())?;
    let condition = r.host.network_error_on_read()?;
    r.host.retry(client.clone(), 3.0, condition)?;
    let interceptor_receiver = r.clone();
    let interceptor: ApiInterceptor =
        Rc::new(move |config| intercept(interceptor_receiver.clone(), config));
    r.host.install_interceptor(client.clone(), interceptor)?;
    if truthy(r.options.get("tealstreetTunnel").as_ref()) {
        r.host.install_transport_adapter(
            client.clone(),
            r.options.clone(),
            r.exchange.get("name"),
        )?
    }
    Ok(client)
}
pub fn intercept<H: CoinbaseApiHost + 'static>(
    r: Rc<CoinbaseApiReceiver<H>>,
    original: ApiRequest,
) -> Promise<Result<ApiRequest, PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            // Object spread of the original happens before evaluating replaceUrlParams.
            let config = copy(&original);
            let replaced = r
                .host
                .replace_url_params(get(&original, "url"), get(&original, "params"))?;
            for (k, v) in replaced.borrow().iter() {
                config.borrow_mut().insert(k.clone(), v.clone());
            }
            let url = text(get(&config, "url"))?;
            let is_public = crate::tables::PUBLIC_ENDPOINTS
                .iter()
                .any(|e| e.encode_utf16().eq(url.iter().copied()));
            if is_public {
                r.host.mark_skip_remote_signing(config.clone())?
            }
            let public = is_public || truthy(r.options.get("publicDataOnly").as_ref());
            if let Some(url) = get(&config, "url") {
                if !matches!(url, Frame::Null) {
                    let Frame::String(url) = url else {
                        return Err(Rc::new(PortfolioThrown::from_value(Some(
                            json!({"name":"TypeError","message":"config?.url?.startsWith is not a function"}),
                        ))));
                    };
                    if url.starts_with(&"http".encode_utf16().collect::<Vec<_>>()) {
                        let prefix =
                            r.host
                                .proxy_prefix("coinbase", r.options.clone(), config.clone())?;
                        if !prefix.is_empty() {
                            let current = text(get(&config, "url"))?;
                            let rewritten = prefix
                                .into_iter()
                                .chain(std::iter::once(47))
                                .chain(current)
                                .collect();
                            put(&config, "url", Some(Frame::String(rewritten)));
                        }
                    }
                }
            }
            if public {
                let returned = copy(&config);
                let headers = r.host.headers(content(), true)?;
                put(&returned, "headers", Some(headers));
                put(&returned, "timeout", Some(Frame::Number(20000.0)));
                return Ok(returned);
            }
            if truthy(r.options.get("remoteSigner").as_ref()) {
                let returned = copy(&config);
                put(&returned, "timeout", Some(Frame::Number(20000.0)));
                return Ok(returned);
            }
            let method = match get(&config, "method") {
                None | Some(Frame::Null) => vec![],
                Some(Frame::String(v)) => {
                    let mut uppercase = Vec::new();
                    for scalar in char::decode_utf16(v) {
                        match scalar {
                            Ok(c) => {
                                for upper in c.to_uppercase() {
                                    uppercase
                                        .extend(upper.encode_utf16(&mut [0; 2]).iter().copied());
                                }
                            }
                            Err(unpaired) => uppercase.push(unpaired.unpaired_surrogate()),
                        }
                    }
                    uppercase
                }
                _ => {
                    return Err(Rc::new(PortfolioThrown::from_value(Some(
                        json!({"name":"TypeError","message":"config.method?.toUpperCase is not a function"}),
                    ))));
                }
            };
            let mut uri = if method.is_empty() {
                "GET".encode_utf16().collect::<Vec<_>>()
            } else {
                method
            };
            uri.extend(" api.coinbase.com".encode_utf16());
            uri.extend(text(get(&config, "url"))?);
            let key = r.options.get("key");
            let secret = r.options.get("secret");
            let token = r.host.sign(key, secret, uri)?.await?;
            let mut header_fields = IndexMap::new();
            if let Some(Frame::Object(headers)) = get(&config, "headers") {
                for key in Frame::Object(headers.clone()).own_keys() {
                    header_fields.insert(key.clone(), headers.borrow()[&key].clone());
                }
            }
            let name = if truthy(r.options.get("tealstreetTunnel").as_ref()) {
                "Authorization"
            } else {
                "Coinbase-Authorization"
            };
            let mut bearer = "Bearer ".encode_utf16().collect::<Vec<_>>();
            bearer.extend(text(Some(token))?);
            header_fields.insert(name.encode_utf16().collect(), Frame::String(bearer));
            let headers = r
                .host
                .headers(Frame::Object(Rc::new(RefCell::new(header_fields))), false)?;
            let returned = copy(&config);
            put(&returned, "headers", Some(headers));
            put(&returned, "timeout", Some(Frame::Number(20000.0)));
            Ok(returned)
        }),
        &spawn,
    )
}

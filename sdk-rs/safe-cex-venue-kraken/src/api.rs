//! API factory and interceptor bodies; Axios, retry, qs and crypto are host bindings.
use crate::{
    rest_account::{Handle, ReadResult},
    trading::{ErrorHost, eq, field, obj, opt, optional_or, text, units},
};
use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY;
use std::{cell::Cell, rc::Rc};

#[derive(Default)]
pub struct FuturesModule {
    counter: Cell<f64>,
}
impl FuturesModule {
    pub fn with_counter(counter: f64) -> Self {
        Self {
            counter: Cell::new(counter),
        }
    }
    pub fn counter(&self) -> f64 {
        self.counter.get()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Form {
    Sorted1738,
    Repeat,
}
pub trait ApiHost: ErrorHost {
    fn create_client(
        &self,
        config: Handle<Self>,
        options: Handle<Self>,
        exchange: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>>;
    fn retry_condition(&self, posted: Option<Vec<&'static str>>) -> ReadResult<Self, Handle<Self>>;
    fn retry(&self, client: Handle<Self>, config: Handle<Self>) -> ReadResult<Self, ()>;
    fn install_transport(
        &self,
        client: Handle<Self>,
        options: Handle<Self>,
        name: Handle<Self>,
    ) -> ReadResult<Self, ()>;
    fn request_use(&self, client: Handle<Self>, api: Rc<Api<Self>>) -> ReadResult<Self, ()>
    where
        Self: Sized;
    fn response_use(&self, client: Handle<Self>, api: Rc<Api<Self>>) -> ReadResult<Self, ()>
    where
        Self: Sized;
    fn strip_slash(&self, value: &Self::Value) -> ReadResult<Self, Vec<u16>>;
    fn qs(&self, value: Handle<Self>, form: Form) -> ReadResult<Self, Handle<Self>>;
    fn json_component(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn sha256(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn decode_secret(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn path_hash(&self, path: Handle<Self>, hash: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    fn hmac512(
        &self,
        secret: Handle<Self>,
        payload: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>>;
    fn join(&self, value: &Self::Value, separator: &str) -> ReadResult<Self, Handle<Self>>;
    fn mark_skip(&self, config: Handle<Self>) -> ReadResult<Self, ()>;
    /// ts: the Spot `retryCondition` arrow: `networkErrorOnRead(readsAndPosted(...posted))(error)
    /// || signatureRejectedResponse(isKrakenSpotSignatureRejection)(error)`, evaluated per error.
    fn spot_retry_condition(&self, posted: Vec<&'static str>) -> ReadResult<Self, Handle<Self>>;
    /// ts: `qs.parse(data)`.
    fn qs_parse(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>>;
    /// ts: `xhr.request(config)` — the full pipeline again, interceptors included.
    fn resend(
        &self,
        client: Handle<Self>,
        config: Handle<Self>,
    ) -> ReadResult<Self, impl std::future::Future<Output = ReadResult<Self, Handle<Self>>>>;
}
/// ts: utils/signatureRejection hasRetriedSignature — `Boolean(config?.[KEY])`.
fn has_retried_signature<H: ApiHost>(h: &H, config: &H::Value) -> ReadResult<H, bool> {
    Ok(h.truthy(&*opt(h, config, SIGNATURE_RETRY_CONFIG_KEY)?))
}

/// ts: krakenspot.api isKrakenSpotSignatureRejection — `Array.isArray(data?.error) &&
/// data.error.some((error) => String(error).includes('EAPI:Invalid signature'))`.
pub fn is_kraken_spot_signature_rejection<H: ErrorHost>(
    h: &H,
    data: &H::Value,
) -> ReadResult<H, bool> {
    let errors = opt(h, data, "error")?;
    if !h.is_array(&errors) {
        return Ok(false);
    }
    for error in h.items(&errors)? {
        if h.string(&error)?
            .windows(units("EAPI:Invalid signature").len())
            .any(|w| w == units("EAPI:Invalid signature"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// ts: krakenspot.api unsignedFormParams — a resend's string body is re-parsed
/// without its previous nonce; an object is signed as is; anything else signs `{}`.
pub fn unsigned_form_params<H: ApiHost>(h: &H, data: Handle<H>) -> ReadResult<H, Handle<H>> {
    if h.is_string(&data) {
        let parsed = h.qs_parse(data)?;
        let rest = h
            .entries(&parsed)?
            .into_iter()
            .filter(|(k, _)| *k != units("nonce"))
            .collect();
        return h.object(rest);
    }
    if h.truthy(&data) && h.is_object(&data) {
        return Ok(data);
    }
    obj(h, vec![])
}

pub struct Api<H: ApiHost> {
    pub client: Handle<H>,
    host: Rc<H>,
    options: Handle<H>,
    spot: bool,
    local_counter: Cell<f64>,
    module: Rc<FuturesModule>,
}
fn bases(spot: bool, history: bool, testnet: bool) -> &'static str {
    let table = if spot {
        crate::spot_types::BASE_URL
    } else if history {
        crate::types::HISTORY_BASE_URL
    } else {
        crate::types::BASE_URL
    };
    table
        .iter()
        .find(|(k, _)| *k == if testnet { "testnet" } else { "livenet" })
        .expect("source base URL")
        .1
}
fn create<H: ApiHost>(
    h: Rc<H>,
    options: Handle<H>,
    exchange: Handle<H>,
    spot: bool,
    history: bool,
    module: Rc<FuturesModule>,
) -> ReadResult<H, Rc<Api<H>>> {
    let testnet = h.truthy(&*field(&*h, &options, "testnet")?);
    let config = obj(
        &*h,
        vec![
            ("baseURL", text(&*h, bases(spot, history, testnet))),
            ("timeout", h.number(20000.0)),
            ("headers", obj(&*h, vec![])?),
        ],
    )?;
    let client = h.create_client(config, options.clone(), exchange.clone())?;
    let condition = if spot {
        let posted = [
            "BALANCE",
            "EXTENDED_BALANCE",
            "TRADE_BALANCE",
            "OPEN_ORDERS",
            "CLOSED_ORDERS",
            "TRADES_HISTORY",
            "OPEN_POSITIONS",
            "WS_TOKEN",
        ]
        .iter()
        .map(|key| {
            crate::spot_types::ENDPOINTS
                .iter()
                .find(|(k, _)| k == key)
                .expect("posted read endpoint")
                .1
        })
        .collect();
        h.spot_retry_condition(posted)?
    } else {
        h.retry_condition(None)?
    };
    h.retry(
        client.clone(),
        obj(
            &*h,
            vec![("retries", h.number(3.0)), ("retryCondition", condition)],
        )?,
    )?;
    if !spot && !h.truthy(&*field(&*h, &options, "publicDataOnly")?) {
        h.install_transport(
            client.clone(),
            options.clone(),
            field(&*h, &exchange, "name")?,
        )?;
    }
    let api = Rc::new(Api {
        client: client.clone(),
        host: h.clone(),
        options,
        spot,
        local_counter: Cell::new(0.0),
        module,
    });
    h.request_use(client.clone(), api.clone())?;
    h.response_use(client, api.clone())?;
    Ok(api)
}
pub fn futures_create_api<H: ApiHost>(
    h: Rc<H>,
    options: Handle<H>,
    exchange: Handle<H>,
    base_kind: &H::Value,
    module: Rc<FuturesModule>,
) -> ReadResult<H, Rc<Api<H>>> {
    let history = eq(&*h, base_kind, "history");
    create(h, options, exchange, false, history, module)
}
pub fn spot_create_api<H: ApiHost>(
    h: Rc<H>,
    options: Handle<H>,
    exchange: Handle<H>,
) -> ReadResult<H, Rc<Api<H>>> {
    create(
        h,
        options,
        exchange,
        true,
        false,
        Rc::new(FuturesModule::default()),
    )
}
impl<H: ApiHost> Api<H> {
    pub fn counter(&self) -> f64 {
        if self.spot {
            self.local_counter.get()
        } else {
            self.module.counter.get()
        }
    }
    pub fn request(&self, config: Handle<H>) -> ReadResult<H, Handle<H>> {
        let h = &*self.host;
        let options = &*self.options;
        let path = if self.spot {
            None
        } else {
            Some(h.strip_slash(&*optional_or(h, &config, "url", text(h, ""))?)?)
        };
        let public = if self.spot {
            crate::spot_types::PUBLIC_ENDPOINTS
        } else {
            crate::types::PUBLIC_ENDPOINTS
        };
        let mut is_public = false;
        for endpoint in public {
            let url = optional_or(h, &config, "url", text(h, ""))?;
            if h.includes(&url, endpoint)? {
                is_public = true;
                break;
            }
        }
        if !self.spot && is_public {
            h.mark_skip(config.clone())?;
        }
        if is_public
            || h.truthy(&*field(h, options, "publicDataOnly")?)
            || !h.truthy(&*field(h, options, "key")?)
        {
            return Ok(config);
        }
        if h.truthy(&*field(h, options, "remoteSigner")?) {
            return Ok(config);
        }
        let method = h.lower_method(
            &*optional_or(h, &config, "method", text(h, "get"))?,
            "(config.method || \"get\")",
        )?;
        let mut post = text(h, "");
        if !self.spot {
            if eq(h, &method, "get") || eq(h, &method, "delete") {
                let params = field(h, &config, "params")?;
                if h.truthy(&params) && !h.entries(&*field(h, &config, "params")?)?.is_empty() {
                    post = h.qs(field(h, &config, "params")?, Form::Sorted1738)?;
                }
            } else if eq(h, &method, "post") || eq(h, &method, "put") || eq(h, &method, "patch") {
                let data = field(h, &config, "data")?;
                let object = if h.truthy(&data) && h.is_object(&*field(h, &config, "data")?) {
                    Some(field(h, &config, "data")?)
                } else {
                    None
                };
                if let Some(data) = object {
                    post = if path
                        .as_ref()
                        .expect("futures path")
                        .ends_with(&units("batchorder"))
                    {
                        let encoded = h.json_component(data)?;
                        h.text(
                            &units("json=")
                                .into_iter()
                                .chain(h.string(&encoded)?)
                                .collect::<Vec<_>>(),
                        )
                    } else {
                        h.qs(data, Form::Sorted1738)?
                    };
                    h.assign(&config, &units("data"), post.clone())?;
                }
            }
        }
        let timestamp = h.now()?;
        let counter = if self.spot {
            &self.local_counter
        } else {
            &self.module.counter
        };
        let previous = counter.get();
        counter.set(previous + 1.0);
        let nonce = h.text(&h.string(&*h.number(timestamp * 1000.0 + previous % 1000.0))?);
        if self.spot {
            let data = unsigned_form_params(h, field(h, &config, "data")?)?;
            let body = h.spread(&data, vec![(units("nonce"), nonce.clone())])?;
            post = h.qs(body, Form::Repeat)?;
        } else if (eq(h, &method, "get") || eq(h, &method, "delete")) && h.truthy(&post) {
            let url = optional_or(h, &config, "url", text(h, ""))?;
            let url = h.string(&url)?;
            let path = url.split(|u| *u == 63).next().expect("split has first");
            let url = h.text(
                &path
                    .iter()
                    .copied()
                    .chain([63])
                    .chain(h.string(&post)?)
                    .collect::<Vec<_>>(),
            );
            h.assign(&config, &units("url"), url)?;
            h.assign(
                &config,
                &units("params"),
                opt(h, &*obj(h, vec![])?, "missing")?,
            )?;
        }
        let hash_input = if self.spot {
            h.text(
                &h.string(&nonce)?
                    .into_iter()
                    .chain(h.string(&post)?)
                    .collect::<Vec<_>>(),
            )
        } else {
            h.text(
                &h.string(&post)?
                    .into_iter()
                    .chain(h.string(&nonce)?)
                    .chain(units("/api/"))
                    .chain(path.expect("futures path"))
                    .collect::<Vec<_>>(),
            )
        };
        let url = if self.spot {
            Some(optional_or(h, &config, "url", text(h, ""))?)
        } else {
            None
        };
        let hash = h.sha256(hash_input)?;
        let secret = h.decode_secret(field(h, options, "secret")?)?;
        let payload = match url {
            Some(url) => h.path_hash(url, hash)?,
            None => hash,
        };
        let signature = h.hmac512(secret, payload)?;
        let headers = optional_or(h, &config, "headers", obj(h, vec![])?)?;
        let mut fields = vec![
            (
                units("Content-Type"),
                text(h, "application/x-www-form-urlencoded"),
            ),
            (units("Accept"), text(h, "application/json")),
            (
                units(if self.spot { "API-Key" } else { "APIKey" }),
                field(h, options, "key")?,
            ),
            (
                units(if self.spot { "API-Sign" } else { "Authent" }),
                signature,
            ),
        ];
        if !self.spot {
            fields.push((units("Nonce"), nonce));
        }
        let headers = h.spread(&headers, fields)?;
        if self.spot && eq(h, &method, "post") {
            h.assign(&config, &units("data"), post)?;
        }
        h.spread(&config, vec![(units("headers"), headers)])
    }
    pub async fn success(&self, response: Handle<H>) -> ReadResult<H, Handle<H>> {
        let h = &*self.host;
        if self.spot {
            let config = field(h, &response, "config")?;
            let rejected = !has_retried_signature(h, &config)?
                && is_kraken_spot_signature_rejection(h, &*field(h, &response, "data")?)?;
            if rejected {
                let stamped = h.spread(
                    &*field(h, &response, "config")?,
                    vec![(units(SIGNATURE_RETRY_CONFIG_KEY), h.boolean(true))],
                )?;
                return h.resend(self.client.clone(), stamped)?.await;
            }
        }
        if h.truthy(&*field(h, &response, "data")?) {
            let data = field(h, &response, "data")?;
            if self.spot {
                let errors = field(h, &data, "error")?;
                if h.truthy(&errors) && h.is_array(&errors) && !h.items(&errors)?.is_empty() {
                    let joined = h.join(&errors, "; ")?;
                    let mut message = h.text(
                        &units("Kraken Spot API Error: ")
                            .into_iter()
                            .chain(h.string(&joined)?)
                            .collect::<Vec<_>>(),
                    );
                    for (needle, replacement) in [
                        (
                            "EAPI:Invalid key",
                            "Authentication failed. Please check your API key.",
                        ),
                        (
                            "EAPI:Invalid nonce",
                            "Invalid nonce. Please check your system time is synchronized.",
                        ),
                        (
                            "EAPI:Invalid signature",
                            "Invalid signature. Please verify your API secret is correct.",
                        ),
                        (
                            "EGeneral:Permission denied",
                            "API key lacks required permissions.",
                        ),
                        (
                            "EOrder:Rate limit exceeded",
                            "Order rate limit exceeded. Please try again.",
                        ),
                        (
                            "EOrder:Insufficient funds",
                            "Insufficient funds for this order.",
                        ),
                    ] {
                        if h.includes(&joined, needle)? {
                            message = text(h, replacement);
                            break;
                        }
                    }
                    return Err(h.new_error(message, None, None));
                }
            } else {
                let result = h.keyed(&data, "result")?;
                let error = h.keyed(&data, "error")?;
                if eq(h, &result, "error") && h.truthy(&error) {
                    let mut message = h.text(
                        &units("Kraken API Error: ")
                            .into_iter()
                            .chain(h.string(&error)?)
                            .collect::<Vec<_>>(),
                    );
                    let lower = h.lower_method(&error, "error")?;
                    if h.includes(&lower, "authentication")? || h.includes(&lower, "apikey")? {
                        message = text(
                            h,
                            "Authentication failed. Please check your API key and secret.",
                        );
                    } else if h.includes(&lower, "nonce")? {
                        message = text(
                            h,
                            "Invalid nonce. Please check your system time is synchronized.",
                        );
                    } else if h.includes(&lower, "permission")? {
                        message = text(
                            h,
                            "API key lacks required permissions. Please ensure General API is set to \"Full access\" on your Kraken Futures API key.",
                        );
                    } else if h.includes(&lower, "signature")? {
                        message = text(
                            h,
                            "Invalid signature. Please verify your API secret is correct.",
                        );
                    }
                    return Err(h.new_error(message, Some(response.clone()), None));
                }
                if h.truthy(&error) && !h.truthy(&result) {
                    let message = h.text(
                        &units("Kraken API Error: ")
                            .into_iter()
                            .chain(h.string(&error)?)
                            .collect::<Vec<_>>(),
                    );
                    return Err(h.new_error(message, Some(response.clone()), None));
                }
            }
        }
        Ok(response)
    }
    pub async fn rejected(&self, error: H::Error) -> ReadResult<H, Handle<H>> {
        let h = &*self.host;
        let response = h.error_response(&error)?;
        if h.truthy(&response) {
            let response = h.error_response(&error)?;
            let status = field(h, &response, "status")?;
            let data = field(h, &*h.error_response(&error)?, "data")?;
            let numeric = |n| h.strict_eq(&status, &*h.number(n));
            let mut message = h.text(
                &units(if self.spot {
                    "Kraken Spot API Error ("
                } else {
                    "Kraken API Error ("
                })
                .into_iter()
                .chain(h.string(&status)?)
                .chain([41])
                .collect::<Vec<_>>(),
            );
            if self.spot {
                if numeric(401.0) || numeric(403.0) {
                    message = h.text(
                        &units("Authentication failed (")
                            .into_iter()
                            .chain(h.string(&status)?)
                            .chain(units("). Please check your API key and secret."))
                            .collect::<Vec<_>>(),
                    );
                } else if numeric(429.0) {
                    message = text(h, "Rate limit exceeded (429). Please try again later.");
                } else {
                    let errors = opt(h, &data, "error")?;
                    if h.truthy(&*opt(h, &errors, "length")?) {
                        let joined = h.join(&*field(h, &data, "error")?, "; ")?;
                        message = h.text(
                            &units("Kraken Spot API Error: ")
                                .into_iter()
                                .chain(h.string(&joined)?)
                                .collect::<Vec<_>>(),
                        );
                    }
                }
                Err(h.new_error(message, None, None))
            } else {
                if numeric(401.0) {
                    message = text(
                        h,
                        "Authentication failed (401). Please check your API key and secret.",
                    );
                } else if numeric(403.0) {
                    message = text(
                        h,
                        "Access forbidden (403). Your API key may lack required permissions or IP is not whitelisted.",
                    );
                } else if numeric(429.0) {
                    message = text(h, "Rate limit exceeded (429). Please try again later.");
                } else if h.truthy(&*opt(h, &data, "error")?) {
                    message = h.text(
                        &units("Kraken API Error: ")
                            .into_iter()
                            .chain(h.string(&*field(h, &data, "error")?)?)
                            .collect::<Vec<_>>(),
                    );
                }
                Err(h.new_error(message, Some(h.error_response(&error)?), Some(&error)))
            }
        } else {
            Err(error)
        }
    }
}

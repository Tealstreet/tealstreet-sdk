//! Factory and Spot auth bodies at configured imported-helper/client boundaries.
//! Current decoded options/config graphs retain original referents. Concrete
//! Axios/client/Header/retry/transport/signing wiring and background async job
//! ownership are supplied by the caller, not claimed by these extracted bodies.
//! Executable property getters/prototypes/symbol keys and mutable Error metadata
//! are outside this decoded-data boundary. Bound helper throws retain their Rc.
use crate::{
    binance_types as f, binancepm_types as p, binancespot_types as s,
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use std::{future::Future, pin::Pin, rc::Rc};
pub type ApiFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientMode {
    Proxy,
    NoProxy,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Default denotes omitted transformResponse / omitted fourth install argument.
pub enum ResponseTransform {
    Default,
    BigInt,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RetryRead {
    Reads,
    ReadsAndPath(&'static str),
}
impl RetryRead {
    /// Configured read-policy adapter for decoded string method/URL inputs.
    /// Network-error classification is supplied by the imported retry helper.
    pub fn matches_read(&self, method: Option<&str>, url: Option<&str>) -> bool {
        if method.is_some_and(|m| matches!(m.to_lowercase().as_str(), "get" | "head")) {
            return true;
        }
        match self {
            Self::Reads => false,
            Self::ReadsAndPath(path) => url.unwrap_or("").split('?').next() == Some(*path),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuerySerializer {
    Repeat,
}
impl QuerySerializer {
    pub fn serialize(self, params: &serde_json::Value) -> String {
        crate::signing::serialize_query(params)
    }
}
#[derive(Clone, Debug)]
pub struct ApiConfig {
    pub base_url: EventValue,
    pub timeout: EventValue,
    pub headers: EventValue,
    pub serializer: QuerySerializer,
    pub transform: ResponseTransform,
}
pub enum RequestReturn {
    Value(EventValue),
    Future(ApiFuture),
}
pub type RequestInterceptor =
    Rc<dyn Fn(EventValue, &mut dyn RequestBindings) -> Result<RequestReturn, Failure>>;
/// Imports are current configured helper operations. The returned sign future is
/// a handle to an already launched, caller-retained job; cancellation semantics
/// and concrete signBinanceRequest composition remain the caller's responsibility.
pub trait RequestBindings {
    fn headers_from(&mut self, headers: EventValue) -> Result<EventValue, Failure>;
    fn mark_skip(&mut self, config: EventValue) -> Result<EventValue, Failure>;
    fn sign(
        &mut self,
        config: EventValue,
        options: EventValue,
        name: EventValue,
        recv_window: EventValue,
    ) -> Result<ApiFuture, Failure>;
}
pub enum ResponseReturn {
    Value(EventValue),
    Rejected(ApiRejection),
}
#[derive(Debug)]
pub enum ApiRejection {
    Original(EventValue),
    New {
        name: &'static str,
        message: String,
        code: EventValue,
        response: EventValue,
        /// ts: `__binanceEmitted`, so the failure handler does not emit it again.
        emitted: bool,
    },
}
pub trait ResponseBindings {
    fn emit(&mut self, message: EventValue) -> Result<(), Failure>;
    /// Binds `resendOnSignatureRejection(xhr, response, isBinanceStyleSignatureRejection)`:
    /// the resend promise, or undefined. Hosts decide through
    /// `safe_cex_transport::signature_rejection`.
    fn resend(&mut self, response: EventValue) -> Result<EventValue, Failure>;
}
pub type ResponseInterceptor =
    Rc<dyn Fn(EventValue, &mut dyn ResponseBindings) -> Result<ResponseReturn, Failure>>;
pub trait FactoryBindings {
    fn create(
        &mut self,
        mode: ClientMode,
        config: ApiConfig,
        options: EventValue,
        exchange: EventValue,
    ) -> Result<EventValue, Failure>;
    /// axios-retry with networkErrorOnRead(read), OR'd with
    /// signatureRejectedResponse(isBinanceStyleSignatureRejection) when `signature`.
    fn retry(
        &mut self,
        client: EventValue,
        retries: u32,
        read: RetryRead,
        signature: bool,
    ) -> Result<(), Failure>;
    fn request_use(
        &mut self,
        client: EventValue,
        handler: RequestInterceptor,
    ) -> Result<(), Failure>;
    fn install(
        &mut self,
        client: EventValue,
        options: EventValue,
        name: EventValue,
        transform: ResponseTransform,
    ) -> Result<(), Failure>;
    fn response_use(
        &mut self,
        client: EventValue,
        success: ResponseInterceptor,
        failure: Option<ResponseInterceptor>,
    ) -> Result<(), Failure>;
}
#[derive(Clone, Copy)]
enum Family {
    Futures,
    Pm,
    Spot,
}
fn recv(options: &EventValue, default: f64) -> Result<EventValue, Failure> {
    let extra = if options.nullish() {
        EventValue::Undefined
    } else {
        options.read("extra")?
    };
    let value = if extra.nullish() {
        EventValue::Undefined
    } else {
        extra.read("recvWindow")?
    };
    Ok(nullish(value, number(default)))
}
fn endpoint(table: &[(&str, &'static str)], key: &str) -> &'static str {
    table
        .iter()
        .find(|(k, _)| *k == key)
        .expect("generated endpoint")
        .1
}
fn public_config(config: EventValue, b: &mut dyn RequestBindings) -> Result<EventValue, Failure> {
    let next = EventValue::record_object(config.spread());
    next.write(
        "headers",
        b.headers_from(record([(
            "Content-Type",
            string("application/json, charset=utf-8"),
        )]))?,
    );
    next.write("timeout", number(20000.0));
    b.mark_skip(next)
}
fn request(
    options: EventValue,
    exchange: EventValue,
    family: Family,
    public: bool,
) -> RequestInterceptor {
    Rc::new(move |config, b| {
        if public {
            return Ok(RequestReturn::Value(public_config(config, b)?));
        }
        let (listen, publics, window) = match family {
            Family::Futures => (
                Some(endpoint(f::ENDPOINTS, "LISTEN_KEY")),
                f::PUBLIC_ENDPOINTS,
                f::RECV_WINDOW,
            ),
            Family::Pm => (
                Some(endpoint(p::ENDPOINTS, "LISTEN_KEY")),
                p::PUBLIC_ENDPOINTS,
                p::RECV_WINDOW,
            ),
            Family::Spot => (None, s::PUBLIC_ENDPOINTS, s::RECV_WINDOW),
        };
        if let Some(listen) = listen {
            if config.read("url")?.same(&string(listen)) && !options.read("testnet")?.truthy() {
                return Ok(RequestReturn::Value(b.mark_skip(config)?));
            }
        }
        if publics.contains(&config.read("url")?.text()?.as_str()) {
            return Ok(RequestReturn::Value(public_config(config, b)?));
        }
        let name = exchange.read("name")?;
        let window = recv(&options, window)?;
        Ok(RequestReturn::Future(b.sign(
            config,
            options.clone(),
            name,
            window,
        )?))
    })
}
fn silent(code: &EventValue) -> bool {
    [-2011.0, -2013.0, -4059.0, -5022.0]
        .iter()
        .any(|n| code.same(&number(*n)))
}
fn has_code(data: &EventValue) -> bool {
    match data {
        EventValue::Object(o) => {
            data.has_own("code") || o.proto.borrow().as_ref().is_some_and(has_code)
        }
        _ => false,
    }
}
// Last, so a resend is not put through any later interceptor twice.
fn resend_response() -> ResponseInterceptor {
    Rc::new(|response, b| {
        let resent = b.resend(response.clone())?;
        Ok(ResponseReturn::Value(if resent.nullish() {
            response
        } else {
            resent
        }))
    })
}
fn success_response() -> ResponseInterceptor {
    Rc::new(|response, b| {
        let resent = b.resend(response.clone())?;
        if resent.truthy() {
            return Ok(ResponseReturn::Value(resent));
        }
        let data = response.read("data")?;
        if data.truthy()
            && matches!(data, EventValue::Object(_) | EventValue::Array(_))
            && has_code(&data)
            && data.read("code")?.number()? < 0.0
        {
            let code = data.read("code")?;
            let value = data.read("msg")?;
            let msg = if value.truthy() {
                value
            } else {
                string(format!("Binance Error: {}", code.text()?))
            };
            if !silent(&code) {
                b.emit(msg.clone())?;
            }
            // Error(message) coercion happens after the emitter returns; its mutations
            // of a referenced message are observed here, matching the source order.
            Ok(ResponseReturn::Rejected(ApiRejection::New {
                name: "Error",
                message: msg.text()?,
                code,
                response,
                emitted: true,
            }))
        } else {
            Ok(ResponseReturn::Value(response))
        }
    })
}
fn optional(value: EventValue, key: &str) -> Result<EventValue, Failure> {
    if value.nullish() {
        Ok(EventValue::Undefined)
    } else {
        value.read(key)
    }
}
fn failure_response() -> ResponseInterceptor {
    Rc::new(|error, b| {
        let response = error.read("response")?;
        let code = optional(optional(response, "data")?, "code")?;
        let msg = optional(optional(error.read("response")?, "data")?, "msg")?;
        // A retried request's rejection comes back through this handler twice (its
        // own chain, then the original's): emit it once.
        if msg.truthy() && !silent(&code) && !error.read("__binanceEmitted")?.truthy() {
            error.write("__binanceEmitted", EventValue::Bool(true));
            b.emit(error.read("response")?.read("data")?.read("msg")?)?;
        }
        Ok(ResponseReturn::Rejected(ApiRejection::Original(error)))
    })
}
fn create(
    options: EventValue,
    exchange: EventValue,
    family: Family,
    public: bool,
    api_type: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    let test = options.read("testnet")?.truthy();
    let urls = match family {
        Family::Futures => f::BASE_URL,
        Family::Spot => s::BASE_URL,
        Family::Pm => {
            if public || !api_type.same(&string("papi")) {
                p::BASE_FAPI_URL
            } else {
                p::BASE_PAPI_URL
            }
        }
    };
    let env = if test { "testnet" } else { "livenet" };
    let url = urls
        .iter()
        .find(|(k, _)| *k == env)
        .expect("generated base URL")
        .1;
    let window = match family {
        Family::Futures => f::RECV_WINDOW,
        Family::Pm => p::RECV_WINDOW,
        Family::Spot => s::RECV_WINDOW,
    };
    let timeout = recv(&options, window)?;
    let headers = record([]);
    if !public {
        headers.write("X-MBX-APIKEY", options.read("key")?);
    }
    headers.write("Content-Type", string("application/json, charset=utf-8"));
    let transform = if matches!(family, Family::Futures) {
        ResponseTransform::BigInt
    } else {
        ResponseTransform::Default
    };
    let config = ApiConfig {
        base_url: string(url),
        timeout,
        headers,
        serializer: QuerySerializer::Repeat,
        transform,
    };
    let client = b.create(
        if public {
            ClientMode::NoProxy
        } else {
            ClientMode::Proxy
        },
        config,
        options.clone(),
        exchange.clone(),
    )?;
    let read = match family {
        Family::Futures => RetryRead::ReadsAndPath(endpoint(f::ENDPOINTS, "LISTEN_KEY")),
        Family::Pm => RetryRead::ReadsAndPath(endpoint(p::ENDPOINTS, "LISTEN_KEY")),
        Family::Spot => RetryRead::Reads,
    };
    b.retry(client.clone(), 3, read, !public)?;
    b.request_use(
        client.clone(),
        request(options.clone(), exchange.clone(), family, public),
    )?;
    if !matches!(family, Family::Spot) && options.read("tealstreetTunnel")?.truthy() {
        b.install(client.clone(), options, exchange.read("name")?, transform)?;
    }
    if matches!(family, Family::Futures) && !public {
        b.response_use(client.clone(), success_response(), Some(failure_response()))?;
    } else if !public {
        b.response_use(client.clone(), resend_response(), None)?;
    }
    Ok(client)
}
// ts: createPublicAPI (binance.api.ts)
pub fn create_futures_public_api(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create(
        options,
        exchange,
        Family::Futures,
        true,
        EventValue::Undefined,
        b,
    )
}
// ts: createAPI (binance.api.ts)
pub fn create_futures_api(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create(
        options,
        exchange,
        Family::Futures,
        false,
        EventValue::Undefined,
        b,
    )
}
// ts: createBinanceAPI
pub fn create_pm_api(
    options: EventValue,
    exchange: EventValue,
    api_type: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    let api_type = if matches!(api_type, EventValue::Undefined) {
        string("papi")
    } else {
        api_type
    };
    create(options, exchange, Family::Pm, false, api_type, b)
}
// ts: createPAPI
pub fn create_papi(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create_pm_api(options, exchange, string("papi"), b)
}
// ts: createFAPI
pub fn create_fapi(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create_pm_api(options, exchange, string("fapi"), b)
}
// ts: createPublicAPI (binancepm.api.ts)
pub fn create_pm_public_api(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create(
        options,
        exchange,
        Family::Pm,
        true,
        EventValue::Undefined,
        b,
    )
}
// ts: createAPI (binancespot.api.ts)
pub fn create_spot_api(
    options: EventValue,
    exchange: EventValue,
    b: &mut impl FactoryBindings,
) -> Result<EventValue, Failure> {
    create(
        options,
        exchange,
        Family::Spot,
        false,
        EventValue::Undefined,
        b,
    )
}

pub type RemoteSignCall = Rc<dyn Fn(EventValue) -> Result<ApiFuture, Failure>>;
pub trait SpotAuthBindings {
    fn options(&mut self) -> Result<EventValue, Failure>;
    fn now(&mut self) -> Result<f64, Failure>;
    /// Capture current signer.sign bound to this receiver before argument reads.
    fn capture_remote_sign(&mut self, signer: EventValue) -> Result<RemoteSignCall, Failure>;
    fn hmac_sha256_hex(
        &mut self,
        secret: EventValue,
        payload: String,
    ) -> Result<EventValue, Failure>;
}
// ts: BinancePrivateWebsocket#buildUserDataSubscriptionParams (binancespot.ws-private.ts)
pub async fn build_spot_user_data_params(
    b: &mut impl SpotAuthBindings,
) -> Result<EventValue, Failure> {
    let api_key = b.options()?.read("key")?;
    let timestamp = b.now()?;
    let payload = crate::spot_protocol::build_binance_spot_user_data_signature_payload(
        &api_key.text()?,
        timestamp,
        None,
    );
    let remote = b.options()?.read("remoteSigner")?;
    let signature = if remote.truthy() {
        let remote = b.options()?.read("remoteSigner")?;
        let sign = b.capture_remote_sign(remote)?;
        let extra = record([
            ("apiKey", api_key.clone()),
            ("recvWindow", number(s::USER_DATA_RECV_WINDOW)),
            ("timestamp", number(timestamp)),
        ]);
        let auth = record([
            ("toSign", string(payload)),
            ("outputFormat", string("hex")),
            ("extraData", extra),
        ]);
        let request = record([
            (
                "id",
                string(crate::spot_protocol::BINANCE_SPOT_USER_DATA_REQUEST_ID),
            ),
            ("type", string("ws-auth")),
            ("exchangeName", string("binancespot")),
            (
                "accountName",
                b.options()?.read("remoteSigner")?.read("accountName")?,
            ),
            ("wsAuth", auth),
        ]);
        let result = sign(request)?.await?;
        if result.read("error")?.truthy() || !result.read("wsAuthSignature")?.truthy() {
            return Err(Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
                fallback(
                    result.read("error")?,
                    string("Remote signer returned no Binance Spot WebSocket signature"),
                )
                .text()?,
            )));
        }
        result.read("wsAuthSignature")?
    } else {
        let secret = b.options()?.read("secret")?;
        b.hmac_sha256_hex(secret, payload)?
    };
    Ok(record([
        ("apiKey", api_key),
        ("recvWindow", number(s::USER_DATA_RECV_WINDOW)),
        ("timestamp", number(timestamp)),
        ("signature", signature),
    ]))
}
/// Concrete UTF8-secret adapter for the imported createHmac('sha256')/hex path.
/// Buffer/key-object inputs and their Node-specific coercion are unclaimed.
pub fn utf8_hmac_sha256_hex(secret: EventValue, payload: String) -> Result<EventValue, Failure> {
    let EventValue::String(secret) = secret else {
        return Err(type_error(
            "HMAC secret must be a UTF8 string at this adapter boundary",
        ));
    };
    Ok(string(safe_cex_crypto::hmac_sha256(
        EventValue::String(secret).text()?.as_bytes(),
        payload.as_bytes(),
        safe_cex_crypto::Encoding::Hex,
    )))
}

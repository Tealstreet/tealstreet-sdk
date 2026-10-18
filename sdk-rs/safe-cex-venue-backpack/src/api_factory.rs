//! HTTP client composition and executable request/response interceptors.
use crate::api;
use safe_cex_num::JsJsonValue;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
pub type Task<T, E> = Pin<Box<dyn Future<Output = Result<T, E>>>>;
pub type Headers = Rc<RefCell<Vec<(String, JsJsonValue)>>>;
#[derive(Clone)]
pub struct Config {
    pub url: Option<String>,
    pub method: Option<String>,
    pub params: Rc<JsJsonValue>,
    pub data: Rc<JsJsonValue>,
    pub headers: Option<Headers>,
    pub skip_remote_signing: bool,
    pub extra: Vec<(String, JsJsonValue)>,
}
pub type Options = safe_cex_types::ExchangeOptions;
#[derive(Clone)]
pub struct Response {
    pub data: Input,
    pub config: Option<Rc<Config>>,
    pub status: Option<f64>,
}
pub struct ClientConfig<E> {
    pub base_url: &'static str,
    pub timeout: f64,
    pub headers: [(&'static str, &'static str); 2],
    pub serialize: Rc<dyn Fn(JsJsonValue) -> Result<String, E>>,
}
pub type RequestHandler<E> = Rc<dyn Fn(Rc<Config>) -> Task<Rc<Config>, E>>;
pub type ResponseHandler<E> = Rc<dyn Fn(Rc<Response>) -> Task<Rc<Response>, E>>;
pub type FailureHandler<E> = Rc<dyn Fn(E) -> Task<Rc<Response>, E>>;
pub trait Host: 'static {
    type Error: 'static;
    type Client: Clone;
    type RetryCondition;
    fn create(
        &mut self,
        config: ClientConfig<Self::Error>,
        options: Rc<RefCell<Options>>,
    ) -> Result<Self::Client, Self::Error>;
    fn qs_repeat(&mut self, params: JsJsonValue) -> Result<String, Self::Error>;
    fn read_retry_condition(&mut self) -> Result<Self::RetryCondition, Self::Error>;
    fn retry(
        &mut self,
        client: &Self::Client,
        retries: u32,
        condition: Self::RetryCondition,
    ) -> Result<(), Self::Error>;
    fn install_transport(
        &mut self,
        client: &Self::Client,
        options: Rc<RefCell<Options>>,
    ) -> Result<(), Self::Error>;
    fn register_request(
        &mut self,
        client: &Self::Client,
        handler: RequestHandler<Self::Error>,
    ) -> Result<(), Self::Error>;
    fn register_response(
        &mut self,
        client: &Self::Client,
        success: ResponseHandler<Self::Error>,
        failure: FailureHandler<Self::Error>,
    ) -> Result<(), Self::Error>;
    fn auth(
        &mut self,
        instruction: &str,
        params: JsJsonValue,
        key: String,
        secret: String,
        window: f64,
    ) -> Result<Task<Vec<(String, JsJsonValue)>, Self::Error>, Self::Error>;
    fn error_text(&mut self, error: &Self::Error) -> Result<String, Self::Error>;
    fn error_message(&mut self, error: &Self::Error) -> Result<Input, Self::Error>;
    fn response(&mut self, error: &Self::Error) -> Result<Option<Rc<Response>>, Self::Error>;
    fn error_config(&mut self, error: &Self::Error) -> Result<Option<Rc<Config>>, Self::Error>;
    fn emit_and_log(&mut self, message: String) -> Result<(), Self::Error>;
    fn emit_error(&mut self, message: String) -> Result<(), Self::Error>;
    fn append_http_failure(
        &mut self,
        message: String,
        response: Rc<Response>,
    ) -> Result<String, Self::Error>;
    fn make_error(&mut self, message: String) -> Self::Error;
    fn coercion_error(&mut self, error: safe_cex_num::JsCoercionError) -> Self::Error;
}
fn field<'a>(v: &'a Input, key: &str) -> &'a Input {
    v.get(key).unwrap_or(&Input::Undefined)
}
fn truth(v: &Input) -> bool {
    safe_cex_num::js_retained_truthy(v)
}
fn text<H: Host>(h: &mut H, v: &Input) -> Result<String, H::Error> {
    safe_cex_num::js_retained_string(Some(v)).map_err(|e| h.coercion_error(e))
}
fn json<H: Host>(h: &mut H, v: &Input) -> Result<String, H::Error> {
    Ok(safe_cex_num::js_retained_json_stringify(v)
        .map_err(|e| h.coercion_error(e))?
        .unwrap_or_else(|| "undefined".into()))
}
// ts: createAPI
pub fn create_api<H: Host>(
    host: Rc<RefCell<H>>,
    options: Rc<RefCell<Options>>,
) -> Result<H::Client, H::Error> {
    let o = options.borrow();
    let base_url = crate::constants::BASE_URLS
        .iter()
        .find(|(k, _)| {
            *k == if o.testnet == Some(true) {
                "TESTNET"
            } else {
                "PRODUCTION"
            }
        })
        .expect("source base URL")
        .1;
    let recv_window = o.extra.as_ref().and_then(|extra| extra.recv_window);
    let auth_window = api::resolve_backpack_auth_window(recv_window);
    let timeout = recv_window
        .filter(|n| *n != 0.0 && !n.is_nan())
        .unwrap_or(30000.0);
    drop(o);
    let serialize_host = host.clone();
    let config = ClientConfig {
        base_url,
        timeout,
        headers: [
            ("Content-Type", "application/json"),
            ("X-Broker-Id", "1200"),
        ],
        serialize: Rc::new(move |p| serialize_host.borrow_mut().qs_repeat(p)),
    };
    let client = host.borrow_mut().create(config, options.clone())?;
    let condition = host.borrow_mut().read_retry_condition()?;
    host.borrow_mut().retry(&client, 3, condition)?;
    if options.borrow().tealstreet_tunnel == Some(true) {
        host.borrow_mut()
            .install_transport(&client, options.clone())?;
    }
    let request_host = host.clone();
    let opts = options.clone();
    let request: RequestHandler<H::Error> = Rc::new(move |config| {
        let mut next = (*config).clone();
        let public = api::is_public_endpoint(config.url.as_deref().unwrap_or(""));
        if public {
            next.skip_remote_signing = true;
        }
        let prepared =
            if !public && !opts.borrow().key.is_empty() && !opts.borrow().secret.is_empty() {
                let result = (|| {
                    let instruction = api::get_instruction(
                        &config
                            .method
                            .as_ref()
                            .map(|s| s.to_uppercase())
                            .filter(|s| !s.is_empty())
                            .unwrap_or_else(|| "GET".into()),
                        config.url.as_deref().unwrap_or(""),
                    );
                    let params = api::collect_auth_params(&config.params, &config.data)
                        .map_err(|s| request_host.borrow_mut().make_error(s.into()))?;
                    let o = opts.borrow();
                    let key = o.key.clone();
                    let secret = o.secret.clone();
                    drop(o);
                    request_host
                        .borrow_mut()
                        .auth(instruction, params, key, secret, auth_window)
                })();
                Some(result)
            } else {
                None
            };
        let h = request_host.clone();
        let catch = move |error: H::Error| {
            let mut h = h.borrow_mut();
            let text = h.error_text(&error)?;
            h.emit_and_log(format!("Failed to generate auth headers: {text}"))?;
            Err(error)
        };
        match prepared {
            None => Box::pin(std::future::ready(Ok(Rc::new(next)))),
            Some(Err(e)) => Box::pin(std::future::ready(catch(e))),
            Some(Ok(response)) => Box::pin(async move {
                let headers = match response.await {
                    Ok(v) => v,
                    Err(e) => return catch(e),
                };
                if let Some(target) = &next.headers {
                    let mut target = target.borrow_mut();
                    for (k, v) in headers {
                        if let Some((_, old)) = target.iter_mut().find(|(key, _)| key == &k) {
                            *old = v;
                        } else {
                            target.push((k, v));
                        }
                    }
                }
                Ok(Rc::new(next))
            }),
        }
    });
    host.borrow_mut().register_request(&client, request)?;
    let success_host = host.clone();
    let success: ResponseHandler<H::Error> = Rc::new(move |response| {
        let result = (|| {
            let mut h = success_host.borrow_mut();
            let error = field(&response.data, "error");
            let message = if truth(error) {
                let msg = field(error, "msg");
                Some(format!(
                    "Backpack API Error: {}",
                    text(&mut *h, if truth(msg) { msg } else { error })?
                ))
            } else {
                let code = field(&response.data, "code");
                let msg = field(&response.data, "msg");
                if truth(code) && truth(msg) {
                    Some(format!(
                        "Backpack API Error {}: {}",
                        text(&mut *h, code)?,
                        text(&mut *h, msg)?
                    ))
                } else {
                    None
                }
            };
            if let Some(message) = message {
                let message = h.append_http_failure(message, response.clone())?;
                h.emit_and_log(message.clone())?;
                return Err(h.make_error(message));
            }
            Ok(response)
        })();
        Box::pin(std::future::ready(result))
    });
    let register_host = host.clone();
    let failure: FailureHandler<H::Error> = Rc::new(move |error| {
        let result = (|| {
            let mut h = host.borrow_mut();
            let response = h.response(&error)?;
            if response.is_some_and(|r| truth(&r.data)) {
                let response = h.response(&error)?.ok_or_else(|| {
                    h.coercion_error(safe_cex_num::JsCoercionError {
                        name: "TypeError",
                        message: "Cannot read properties of undefined (reading 'data')",
                    })
                })?;
                let data = &response.data;
                let nested = field(field(data, "error"), "msg");
                let message = field(data, "message");
                let msg = field(data, "msg");
                let mut full = "Backpack API Error".to_owned();
                let mut code = String::new();
                if truth(nested) {
                    full += &format!(": {}", text(&mut *h, nested)?);
                } else if truth(message) {
                    full += &format!(": {}", text(&mut *h, message)?);
                    let raw = field(data, "code");
                    if truth(raw) {
                        code = format!(" [{}]", text(&mut *h, raw)?);
                    }
                } else if truth(msg) {
                    full += &format!(": {}", text(&mut *h, msg)?);
                } else if let Input::Text(_) = data {
                    full += &format!(": {}", text(&mut *h, data)?);
                } else {
                    full += &format!(": {}", json(&mut *h, data)?);
                }
                full += &code;
                h.emit_error(full.clone())?;
                let current_response = h.response(&error)?;
                let status = current_response
                    .and_then(|r| r.status)
                    .map_or_else(|| "undefined".into(), safe_cex_num::js_number_string);
                let config = h.error_config(&error)?;
                let url = config
                    .as_ref()
                    .and_then(|c| c.url.clone())
                    .unwrap_or_else(|| "undefined".into());
                h.emit_and_log(format!("{full} (Status: {status}, URL: {url})"))?;
                Err(h.make_error(full))
            } else {
                let message = h.error_message(&error)?;
                let text = text(&mut *h, &message)?;
                h.emit_and_log(format!("Backpack API request failed: {text}"))?;
                Err(error)
            }
        })();
        Box::pin(std::future::ready(result))
    });
    register_responses(client, success, failure, register_host)
}
fn register_responses<H: Host>(
    client: H::Client,
    success: ResponseHandler<H::Error>,
    failure: FailureHandler<H::Error>,
    host: Rc<RefCell<H>>,
) -> Result<H::Client, H::Error> {
    host.borrow_mut()
        .register_response(&client, success, failure)?;
    Ok(client)
}

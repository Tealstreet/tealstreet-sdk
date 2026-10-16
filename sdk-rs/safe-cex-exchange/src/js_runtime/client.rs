//! Axios-shaped client: request interceptors (LIFO), host send, response
//! interceptors (FIFO), axios-retry and axios-rate-limit spacing.
use super::{
    promise::{JsFuture, Promise, await_js},
    value::{Js, Thrown, arg},
};
use crate::runtime::ExchangeRuntime;
use std::{cell::RefCell, rc::Rc};

/// What a client needs from its host: the runtime owning launched requests and
/// the wire send, which receives the config after the request interceptors ran
/// and resolves the Axios response object `{ data, status, headers, config }`.
pub trait JsHttpHost {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime>;
    fn send(&self, config: Js) -> JsFuture<'static>;
}

type Host = Rc<dyn JsHttpHost>;

fn spawn(host: &Host, body: JsFuture<'static>) -> Js {
    Js::Promise(Promise::spawn(&host.runtime(), body))
}

#[derive(Default)]
struct ClientState {
    requests: Vec<Js>,
    responses: Vec<Js>,
    retries: u32,
    retry_condition: Option<Js>,
    max_rps: Option<f64>,
    sent_at: Vec<f64>,
}

/// safeCexAxiosCreate(config): a client object whose verbs run the full pipeline.
pub fn create(host: Host, defaults: Js) -> Js {
    let state = Rc::new(RefCell::new(ClientState::default()));
    let client = Js::object_with_class("Axios", vec![("defaults", defaults.clone())]);
    let use_fn = |list: fn(&mut ClientState) -> &mut Vec<Js>| {
        let state = state.clone();
        Js::function("use", move |_, a| {
            list(&mut state.borrow_mut()).push(arg(&a, 0));
            Ok(Js::Number(0.0))
        })
    };
    let _ = client.define(
        "interceptors",
        Js::object(vec![
            (
                "request",
                Js::object(vec![("use", use_fn(|s| &mut s.requests))]),
            ),
            (
                "response",
                Js::object(vec![("use", use_fn(|s| &mut s.responses))]),
            ),
        ]),
    );
    let verb = |method: &'static str, has_data: bool| {
        let host = host.clone();
        let state = state.clone();
        let defaults = defaults.clone();
        Js::function(method, move |_, a| {
            let url = arg(&a, 0);
            let (data, config) = if has_data {
                (arg(&a, 1), arg(&a, 2))
            } else {
                (Js::Undefined, arg(&a, 1))
            };
            let request = config.spread_object()?;
            request.define("url", url)?;
            request.define("method", Js::str(method))?;
            if has_data {
                request.define("data", data)?;
            }
            Ok(spawn(
                &host,
                dispatch(host.clone(), state.clone(), defaults.clone(), request),
            ))
        })
    };
    let _ = client.define("get", verb("get", false));
    let _ = client.define("delete", verb("delete", false));
    let _ = client.define("post", verb("post", true));
    let _ = client.define("put", verb("put", true));
    let request_host = host.clone();
    let request_state = state.clone();
    let request_defaults = defaults.clone();
    let _ = client.define(
        "request",
        Js::function("request", move |_, a| {
            let config = arg(&a, 0).spread_object()?;
            Ok(spawn(
                &request_host,
                dispatch(
                    request_host.clone(),
                    request_state.clone(),
                    request_defaults.clone(),
                    config,
                ),
            ))
        }),
    );
    let retry_state = state.clone();
    let _ = client.define(
        "__configureRetry",
        Js::function("__configureRetry", move |_, a| {
            let opts = arg(&a, 0);
            let mut s = retry_state.borrow_mut();
            s.retries = opts.get("retries")?.to_number()? as u32;
            let condition = opts.get("retryCondition")?;
            s.retry_condition = condition.is_callable().then_some(condition);
            Ok(Js::Undefined)
        }),
    );
    let limit_state = state;
    let _ = client.define(
        "__configureRateLimit",
        Js::function("__configureRateLimit", move |_, a| {
            limit_state.borrow_mut().max_rps = Some(arg(&a, 0).get("maxRPS")?.to_number()?);
            Ok(Js::Undefined)
        }),
    );
    client
}

fn merged_config(defaults: &Js, request: &Js) -> Result<Js, Thrown> {
    let config = defaults.spread_object()?;
    let headers = defaults.get("headers")?.spread_object()?;
    for (k, v) in request.get("headers")?.spread_object()?.own_entries()? {
        headers.define_key(&k, v)?;
    }
    for (k, v) in request.own_entries()? {
        config.define_key(&k, v)?;
    }
    config.define("headers", headers)?;
    Ok(config)
}

fn axios_error(response: &Js, config: &Js) -> Result<Thrown, Thrown> {
    let status = response.get("status")?;
    let error = super::value::error_object(
        "AxiosError",
        &format!("Request failed with status code {}", status.to_display()),
    );
    error.define("response", response.clone())?;
    error.define("config", config.clone())?;
    error.define("isAxiosError", Js::Bool(true))?;
    error.define("status", status)?;
    Ok(Thrown::new(error))
}

fn dispatch(
    host: Host,
    state: Rc<RefCell<ClientState>>,
    defaults: Js,
    request: Js,
) -> JsFuture<'static> {
    Box::pin(async move {
        let mut attempt = 0;
        loop {
            match send_once(&host, &state, &defaults, request.clone()).await {
                Ok(response) => return Ok(response),
                Err(error) => {
                    let (retries, condition) = {
                        let s = state.borrow();
                        (s.retries, s.retry_condition.clone())
                    };
                    let retry = attempt < retries
                        && match &condition {
                            Some(condition) => condition
                                .call(Js::Undefined, vec![error.value.clone()])?
                                .truthy(),
                            None => false,
                        };
                    if !retry {
                        return Err(error);
                    }
                    attempt += 1;
                }
            }
        }
    })
}

async fn send_once(
    host: &Host,
    state: &Rc<RefCell<ClientState>>,
    defaults: &Js,
    request: Js,
) -> Result<Js, Thrown> {
    let mut config = merged_config(defaults, &request)?;
    let interceptors: Vec<Js> = state.borrow().requests.iter().rev().cloned().collect();
    for interceptor in interceptors {
        config = await_js(interceptor.call(Js::Undefined, vec![config])?).await?;
    }
    throttle(host, state).await;
    let response = host.send(config.clone()).await?;
    if !response.get("config")?.is_object() {
        response.define("config", config.clone())?;
    }
    let status = response.get("status")?.to_number().unwrap_or(200.0);
    if !(200.0..300.0).contains(&status) {
        return Err(axios_error(&response, &config)?);
    }
    let mut response = response;
    let interceptors: Vec<Js> = state.borrow().responses.clone();
    for interceptor in interceptors {
        response = await_js(interceptor.call(Js::Undefined, vec![response])?).await?;
    }
    Ok(response)
}

async fn throttle(host: &Host, state: &Rc<RefCell<ClientState>>) {
    let Some(max) = state.borrow().max_rps.filter(|m| *m > 0.0) else {
        return;
    };
    loop {
        let runtime = host.runtime();
        let now = runtime.monotonic_now_ms();
        let wait = {
            let mut s = state.borrow_mut();
            s.sent_at.retain(|t| now - t < 1000.0);
            if (s.sent_at.len() as f64) < max {
                s.sent_at.push(now);
                None
            } else {
                Some(1000.0 - (now - s.sent_at[0]))
            }
        };
        match wait {
            None => return,
            Some(ms) => {
                let _ = runtime.sleep(ms.max(1.0) as u64).await;
            }
        }
    }
}

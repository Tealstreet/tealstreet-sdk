//! Module-scope values the Crypto.com bodies import: the BaseExchange and
//! BaseWebSocket bases, cryptocom.types constants, the ported cryptocom helpers,
//! and the axios / lodash / signatureRejection utilities.
use super::rt::{Rt, Shared};
use crate::{
    api, api_envelope_body, api_error, api_guards, signing, signing_value::SigningValue, types,
};
use safe_cex_exchange::js_runtime::{
    base_exchange::{self, BaseExchangeParts},
    client,
    globals::{install_builtins, install_enums},
    library::install_library,
    ops,
    promise::await_js,
    scope::scope_of,
    sockets,
    value::{Js, Thrown, arg, from_json, to_json, to_typed},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

fn scope(weak: &Weak<Shared>) -> Result<Rt, Thrown> {
    scope_of(weak)
}

fn units(v: &Js) -> Result<Vec<u16>, Thrown> {
    Ok(v.to_js_string()?.to_vec())
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&units(v)?))
}

fn coercion(e: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}

/// `typeof value === 'object' && value !== null`.
fn non_null_object(v: &Js) -> Option<&Js> {
    (v.type_of() == "object" && !matches!(v, Js::Null)).then_some(v)
}

fn is_record(v: &Js) -> bool {
    api_guards::is_record(non_null_object(v), |v| Ok::<_, Thrown>(v.is_array())).unwrap_or(false)
}

/// `new BaseExchange(name, options, store)`; the Rust core is kept for `Exchange::base()`.
pub(crate) fn base_exchange(rt: &Rt, name: Js, options: Js, store: Js) -> Result<Js, Thrown> {
    let extra = &rt.shared.extra;
    let parts = BaseExchangeParts {
        name: extra.name.clone(),
        options: extra.options.clone(),
        store: rt.shared.store.clone(),
        settings: super::exchange::settings(),
        external_public_data: extra.external_public_data.clone(),
    };
    let (this, core) =
        base_exchange::create(&rt.shared.sockets, parts, name, options, store, |_| Ok(()))?;
    let _ = rt.shared.base.set(core);
    Ok(this)
}

fn frozen_table(entries: &[(&str, &str)]) -> Js {
    Js::object(entries.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn strings(values: &[&str]) -> Js {
    Js::array(values.iter().map(|v| Js::str(v)).collect())
}

/// The SigningValue the native signer reads; the request keeps the caller's params.
fn signing_params(params: &Js) -> Result<SigningValue, Thrown> {
    Ok(SigningValue::from_value(to_json(params)?))
}

struct ErrorHost<'a> {
    rt: &'a Rt,
}

impl api_error::ApiErrorHost for ErrorHost<'_> {
    type Value = Js;
    type Error = Thrown;
    fn undefined_value(&self) -> Js {
        Js::Undefined
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn undefined(&self, value: &Js) -> bool {
        value.is_undefined()
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn zero_number(&self, value: &Js) -> bool {
        matches!(value, Js::Number(n) if *n == 0.0)
    }
    fn primitive_string(&self, value: &Js) -> bool {
        value.is_string()
    }
    fn get(&self, value: &Js, key: &'static str) -> Result<Js, Thrown> {
        value.get(key)
    }
    fn is_record(&self, value: &Js) -> Result<bool, Thrown> {
        Ok(is_record(value))
    }
    fn stringify_response_body(&self, value: Js) -> Result<Js, Thrown> {
        let body = match value {
            Js::String(s) => api_envelope_body::ResponseBody::String(s.to_vec()),
            other => api_envelope_body::ResponseBody::Other(other),
        };
        let json = self.rt.global("JSON")?;
        let out = api_envelope_body::stringify_response_body(
            body,
            |v| {
                let s = json.call_method("stringify", vec![v.clone()])?;
                Ok::<_, Thrown>(if s.is_undefined() {
                    None
                } else {
                    Some(units(&s)?)
                })
            },
            units,
        )?;
        Ok(out.map_or(Js::Undefined, |u| Js::utf16(&u)))
    }
    fn template(&self, value: Js) -> Result<Vec<u16>, Thrown> {
        units(&value)
    }
    fn at_least_500(&self, value: &Js) -> Result<bool, Thrown> {
        ops::greater_equal(value, &Js::Number(500.0))
    }
    fn string_value(&self, units: Vec<u16>) -> Js {
        Js::utf16(&units)
    }
}

fn envelope_value(v: &Js) -> api_envelope_body::EnvelopeValue<Js> {
    match v {
        Js::String(s) => api_envelope_body::EnvelopeValue::String(s.to_vec()),
        Js::Number(n) => api_envelope_body::EnvelopeValue::Number(*n),
        other => match non_null_object(other) {
            Some(o) => api_envelope_body::EnvelopeValue::Object(o.clone()),
            None => api_envelope_body::EnvelopeValue::Other,
        },
    }
}

const SIGNATURE_RETRY_CONFIG_KEY: &str = "safeCexSignatureRetry";

fn has_retried_signature(config: &Js) -> Result<bool, Thrown> {
    Ok(!config.is_nullish() && config.get(SIGNATURE_RETRY_CONFIG_KEY)?.truthy())
}

/// lodash `throttle(fn, wait, { leading: true, trailing: true })`: the first call
/// runs now and opens a window; calls inside it collapse into one trailing call,
/// which opens the next window.
fn throttle(rt: &Rt, callback: Js, wait: f64) -> Js {
    #[derive(Default)]
    struct State {
        open: bool,
        pending: Option<(Js, Vec<Js>)>,
        last: Option<Js>,
        timer: Option<Js>,
    }
    fn open_window(rt: Rt, state: Rc<RefCell<State>>, callback: Js, wait: f64) {
        state.borrow_mut().open = true;
        let (timer_rt, timer_state) = (rt.clone(), state.clone());
        let timer = rt.set_timeout(
            Rc::new(move || {
                let pending = timer_state.borrow_mut().pending.take();
                match pending {
                    Some((this, args)) => {
                        let result = callback.call(this, args)?;
                        timer_state.borrow_mut().last = Some(result);
                        open_window(
                            timer_rt.clone(),
                            timer_state.clone(),
                            callback.clone(),
                            wait,
                        );
                    }
                    None => {
                        let mut s = timer_state.borrow_mut();
                        s.open = false;
                        s.timer = None;
                    }
                }
                Ok(())
            }),
            wait,
        );
        state.borrow_mut().timer = Some(timer);
    }
    let state: Rc<RefCell<State>> = Rc::default();
    let (call_rt, call_state, call_callback) = (rt.clone(), state.clone(), callback);
    let throttled = Js::function("throttled", move |this, a| {
        if call_state.borrow().open {
            call_state.borrow_mut().pending = Some((this, a));
            return Ok(call_state.borrow().last.clone().unwrap_or(Js::Undefined));
        }
        let result = call_callback.call(this, a)?;
        call_state.borrow_mut().last = Some(result.clone());
        open_window(
            call_rt.clone(),
            call_state.clone(),
            call_callback.clone(),
            wait,
        );
        Ok(result)
    });
    let (cancel_rt, cancel_state) = (rt.clone(), state);
    let _ = throttled.define(
        "cancel",
        Js::function("cancel", move |_, _| {
            let mut s = cancel_state.borrow_mut();
            if let Some(timer) = s.timer.take() {
                cancel_rt.clear_timeout(&timer);
            }
            *s = State::default();
            Ok(Js::Undefined)
        }),
    );
    throttled
}

/// axios-retry precedes the venue's rejection formatter in the source, while
/// the shared client's retry loop surrounds the entire response chain. Keep
/// rejection formatting outside that loop so it retains response/config when
/// checking retries. Tag fulfilled-handler errors to skip their own rejected
/// partner, as Axios does, then replay the remaining rejection chain.
fn with_rejected_handlers(rt: &Rt, xhr: Js) -> Result<Js, Thrown> {
    type Pairs = Rc<RefCell<Vec<(Js, Js)>>>;
    type Raised = Rc<RefCell<Vec<(Js, usize)>>>;
    let pairs: Pairs = Rc::default();
    let raised: Raised = Rc::default();
    let response = xhr.get("interceptors")?.get("response")?;
    let register = response.get("use")?;
    let use_runtime = rt.exchange_runtime();
    let (use_pairs, use_raised, use_owner) = (pairs.clone(), raised.clone(), response.clone());
    response.define(
        "use",
        Js::function("use", move |_, a| {
            let (fulfilled, rejected) = (arg(&a, 0), arg(&a, 1));
            let index = use_pairs.borrow().len();
            use_pairs.borrow_mut().push((fulfilled.clone(), rejected));
            let tags = use_raised.clone();
            let tag_runtime = use_runtime.clone();
            let tagged = Js::function("fulfilled", move |this, v| {
                let result = fulfilled.call(this, v).inspect_err(|e| {
                    tags.borrow_mut().push((e.value.clone(), index));
                })?;
                let Js::Promise(promise) = &result else {
                    return Ok(result);
                };
                let (promise, tags) = (promise.clone(), tags.clone());
                let runtime = tag_runtime.clone();
                Ok(Js::Promise(
                    safe_cex_exchange::js_runtime::promise::Promise::spawn(
                        &promise.runtime(),
                        Box::pin(async move {
                            await_js(&runtime, Js::Promise(promise))
                                .await
                                .inspect_err(|e| {
                                    tags.borrow_mut().push((e.value.clone(), index));
                                })
                        }),
                    ),
                ))
            });
            register.call(use_owner.clone(), vec![tagged])
        }),
    )?;
    for verb in ["get", "delete", "post", "put", "request"] {
        let send = xhr.get(verb)?;
        let (owner, pairs, raised, verb_rt) =
            (xhr.clone(), pairs.clone(), raised.clone(), rt.clone());
        xhr.define(
            verb,
            Js::function(verb, move |_, a| {
                let pending = send.call(owner.clone(), a)?;
                let (pairs, raised) = (pairs.clone(), raised.clone());
                let runtime = verb_rt.exchange_runtime();
                Ok(verb_rt.spawn(Box::pin(async move {
                    let error = match await_js(&runtime, pending).await {
                        Ok(value) => return Ok(value),
                        Err(error) => error,
                    };
                    let start = {
                        let mut tags = raised.borrow_mut();
                        let found = tags
                            .iter()
                            .position(|(v, _)| v.same_reference(&error.value));
                        found.map_or(0, |i| tags.remove(i).1 + 1)
                    };
                    let handlers: Vec<(Js, Js)> =
                        pairs.borrow()[start.min(pairs.borrow().len())..].to_vec();
                    let mut outcome: Result<Js, Thrown> = Err(error);
                    for (fulfilled, rejected) in handlers {
                        outcome = match outcome {
                            Ok(value) => {
                                await_js(&runtime, fulfilled.call(Js::Undefined, vec![value])?)
                                    .await
                            }
                            Err(error) if rejected.is_callable() => {
                                await_js(&runtime, rejected.call(Js::Undefined, vec![error.value])?)
                                    .await
                            }
                            Err(error) => Err(error),
                        };
                    }
                    outcome
                })))
            }),
        )?;
    }
    Ok(xhr)
}

fn sort_levels(levels: &Js, ascending: bool) -> Result<(), Thrown> {
    let Some(values) = levels.array_values() else {
        return Err(Thrown::type_error("orderBook levels are not an array"));
    };
    let mut keyed = values
        .into_iter()
        .map(|level| Ok((level.get("price")?.to_number()?, level)))
        .collect::<Result<Vec<_>, Thrown>>()?;
    keyed.sort_by(|(a, _), (b, _)| {
        if ascending {
            a.total_cmp(b)
        } else {
            b.total_cmp(a)
        }
    });
    for (i, (_, level)) in keyed.into_iter().enumerate() {
        levels.set(&i.to_string(), level)?;
    }
    Ok(())
}

/// `/^.{1,36}$/`: 1–36 UTF-16 units, none a line terminator.
pub(crate) fn client_order_id_valid(id: &str) -> bool {
    let units: Vec<u16> = id.encode_utf16().collect();
    (1..=36).contains(&units.len())
        && !units
            .iter()
            .any(|u| matches!(u, 0x0a | 0x0d | 0x2028 | 0x2029))
}

pub(crate) fn client_order_id_rule() -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        "Crypto.com".into(),
        "/^.{1,36}$/".into(),
        Rc::new(client_order_id_valid),
        Default::default(),
    )
}

pub(crate) fn build(weak: Weak<Shared>) -> Js {
    let g = Js::object(vec![]);
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };

    // cryptocom.types.ts
    def("ENDPOINTS", frozen_table(types::ENDPOINTS));
    def("BASE_URLS", frozen_table(types::BASE_URLS));
    def(
        "WSS_URLS",
        Js::object(
            types::WSS_URLS
                .iter()
                .map(|(k, v)| (*k, frozen_table(v)))
                .collect(),
        ),
    );
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        strings(types::GRAFANA_ENDPOINTS_EVERY),
    );
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        strings(types::GRAFANA_ENDPOINTS_PERIODIC),
    );
    def(
        "CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS",
        Js::Number(types::CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS),
    );
    def(
        "CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS",
        Js::Number(types::CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS),
    );
    def(
        "mapCandlestickInterval",
        Js::function("mapCandlestickInterval", |_, a| {
            use safe_cex_num::prototype_realm::PrototypeValue;
            match types::map_candlestick_interval_units(&units(&arg(&a, 0))?) {
                Ok(PrototypeValue::String(s)) => Ok(Js::str(&s)),
                Ok(_) => Err(Thrown::error(
                    "Error",
                    "Crypto.com binding: inherited candlestick interval keys are not modelled",
                )),
                Err(message) => Err(Thrown::new(
                    safe_cex_exchange::js_runtime::value::error_object(
                        "Error",
                        &String::from_utf16_lossy(&message),
                    ),
                )),
            }
        }),
    );

    // The bases the class graph extends.
    let w = weak.clone();
    def(
        "BaseExchange",
        Js::constructor("BaseExchange", None, move |_, a| {
            base_exchange(&scope(&w)?, arg(&a, 0), arg(&a, 1), arg(&a, 2))
        }),
    );
    let w = weak.clone();
    def(
        "BaseWebSocket",
        Js::constructor("BaseWebSocket", None, move |_, a| {
            Ok(sockets::base_websocket(
                &scope(&w)?.shared.sockets,
                arg(&a, 0),
            ))
        }),
    );

    // cryptocom.api.ts
    def(
        "isRecord",
        Js::function("isRecord", |_, a| Ok(Js::Bool(is_record(&arg(&a, 0))))),
    );
    let w = weak.clone();
    def(
        "parseJsonObject",
        Js::function("parseJsonObject", move |_, a| {
            let json = scope(&w)?.global("JSON")?;
            let parsed = api_guards::parse_json_object(
                &units(&arg(&a, 0))?,
                |u| json.call_method("parse", vec![Js::utf16(u)]),
                |v| Ok::<_, Thrown>(is_record(v)),
            );
            Ok(parsed.unwrap_or(Js::Undefined))
        }),
    );
    def(
        "isAuthenticatedEnvelope",
        Js::function("isAuthenticatedEnvelope", |_, a| {
            Ok(Js::Bool(api_envelope_body::is_authenticated_envelope(
                &envelope_value(&arg(&a, 0)),
                &units(&arg(&a, 1))?,
                &units(&arg(&a, 2))?,
                |v| Ok::<_, Thrown>(v.is_array()),
                |v, k| Ok(envelope_value(&v.get(k)?)),
            )?))
        }),
    );
    def(
        "isPublicEndpoint",
        Js::function("isPublicEndpoint", |_, a| {
            Ok(Js::Bool(api::is_public_endpoint_units(&units(&arg(
                &a, 0,
            ))?)))
        }),
    );
    def(
        "isNonRetryablePrivateWrite",
        Js::function("isNonRetryablePrivateWrite", |_, a| {
            let url = arg(&a, 0);
            let url = if url.is_undefined() {
                None
            } else {
                Some(units(&url)?)
            };
            Ok(Js::Bool(api::is_non_retryable_private_write_units(
                url.as_deref(),
            )))
        }),
    );
    let w = weak.clone();
    def(
        "formatCryptocomApiError",
        Js::function("formatCryptocomApiError", move |_, a| {
            let rt = scope(&w)?;
            api_error::format_cryptocom_api_error(&ErrorHost { rt: &rt }, arg(&a, 0))
        }),
    );
    def(
        "isCryptocomAuthenticationErrorCode",
        Js::function("isCryptocomAuthenticationErrorCode", |_, a| {
            Ok(Js::Bool(
                api_guards::is_cryptocom_authentication_error_code(arg(&a, 0), |v| units(&v))?,
            ))
        }),
    );
    def(
        "isCryptocomAuthenticationFailureMessage",
        Js::function("isCryptocomAuthenticationFailureMessage", |_, a| {
            let message = arg(&a, 0);
            let message = message.as_str().map(|s| s.to_vec());
            Ok(Js::Bool(
                api_guards::is_cryptocom_authentication_failure_message(message.as_deref()),
            ))
        }),
    );
    def(
        "shouldSuppressValidationAuthErrorEmission",
        Js::function("shouldSuppressValidationAuthErrorEmission", |_, a| {
            let exchange = arg(&a, 0);
            Ok(Js::Bool(
                api_guards::should_suppress_validation_auth_error_emission(
                    arg(&a, 1),
                    |v| units(&v),
                    || {
                        Ok(
                            match exchange.get("suppressCryptocomValidationErrorEmission")? {
                                Js::Bool(b) => Some(b),
                                _ => None,
                            },
                        )
                    },
                )?,
            ))
        }),
    );
    def(
        "isCryptocomSignatureRejection",
        Js::function("isCryptocomSignatureRejection", |_, a| {
            Ok(Js::Bool(
                api::is_cryptocom_signature_rejection(&to_json(&arg(&a, 0))?).map_err(coercion)?,
            ))
        }),
    );

    // cryptocom.signing.ts
    def(
        "generateAuthenticatedRequest",
        Js::function("generateAuthenticatedRequest", |_, a| {
            let params = arg(&a, 1);
            let nonce = arg(&a, 4);
            let nonce = if nonce.is_nullish() {
                None
            } else {
                Some(nonce.to_number()?)
            };
            let request = signing::generate_authenticated_request(
                &text(&arg(&a, 0))?,
                signing_params(&params)?,
                &text(&arg(&a, 2))?,
                &text(&arg(&a, 3))?,
                nonce,
                &safe_cex_store::virtual_clock::VIRTUAL_CLOCK,
            )
            .map_err(|e| Thrown::error("Error", &e.message))?;
            Ok(Js::object(vec![
                ("id", Js::Number(request.id)),
                ("method", Js::str(&request.method)),
                ("api_key", Js::str(&request.api_key)),
                ("params", params),
                ("sig", Js::str(&request.sig)),
                ("nonce", Js::Number(request.nonce)),
            ]))
        }),
    );
    def(
        "generateWsAuthMessage",
        Js::function("generateWsAuthMessage", |_, a| {
            let message = signing::generate_ws_auth_message(
                &text(&arg(&a, 0))?,
                &text(&arg(&a, 1))?,
                &safe_cex_store::virtual_clock::VIRTUAL_CLOCK,
            )
            .map_err(|e| Thrown::error("Error", &e.message))?;
            Ok(Js::object(vec![
                ("id", Js::Number(message.id)),
                ("method", Js::str(message.method)),
                ("api_key", Js::str(&message.api_key)),
                ("sig", Js::str(&message.sig)),
                ("nonce", Js::Number(message.nonce)),
            ]))
        }),
    );

    // safeCexAxiosCreate, axios-retry, axios-rate-limit
    let w = weak.clone();
    def(
        "safeCexAxiosCreate",
        Js::function("safeCexAxiosCreate", move |_, a| {
            let rt = scope(&w)?;
            let xhr = client::factory::create(
                rt.shared.http.clone(),
                rt.shared.runtime.transport().as_ref(),
                arg(&a, 0),
                &arg(&a, 1),
                rt.base(),
            )?;
            with_rejected_handlers(&rt, xhr)
        }),
    );
    def(
        "retry",
        Js::function("retry", |_, a| {
            arg(&a, 0).call_method("__configureRetry", vec![arg(&a, 1)])
        }),
    );
    def(
        "rateLimit",
        Js::function("rateLimit", |_, a| {
            let xhr = arg(&a, 0);
            xhr.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
            Ok(xhr)
        }),
    );
    def(
        "isNetworkError",
        Js::function("isNetworkError", |_, a| {
            let error = arg(&a, 0);
            if error.is_nullish() || !error.get("response")?.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let code = error.get("code")?;
            let failure = safe_cex_transport::RestFailure {
                code: (!code.is_nullish()).then(|| text(&code)).transpose()?,
                transport_error: None,
                message: text(&error.get("message")?).unwrap_or_default(),
                response: None,
                request: Box::default(),
                context: None,
            };
            Ok(Js::Bool(safe_cex_transport::retry::is_network_error(
                &failure,
            )))
        }),
    );
    let w = weak.clone();
    def(
        "installTransportAdapter",
        Js::function("installTransportAdapter", move |_, a| {
            let rt = scope(&w)?;
            client::factory::install_transport_adapter(
                rt.shared.http.clone(),
                rt.shared.runtime.transport().as_ref(),
                &arg(&a, 0),
                &arg(&a, 1),
                to_typed(&arg(&a, 2))?,
                safe_cex_transport::RestAdapterVariant::Default,
            )?;
            Ok(Js::Undefined)
        }),
    );

    // utils/signatureRejection.ts
    def(
        "signatureRejectedResponse",
        Js::function("signatureRejectedResponse", |_, a| {
            let is_rejection = arg(&a, 0);
            Ok(Js::function("", move |_, e| {
                let error = arg(&e, 0);
                let config = error.get("config")?;
                let response = error.get("response")?;
                if !config.truthy() || !response.truthy() || has_retried_signature(&config)? {
                    return Ok(Js::Bool(false));
                }
                if !is_rejection
                    .call(Js::Undefined, vec![response.get("data")?])?
                    .truthy()
                {
                    return Ok(Js::Bool(false));
                }
                config.set(SIGNATURE_RETRY_CONFIG_KEY, Js::Bool(true))?;
                Ok(Js::Bool(true))
            }))
        }),
    );
    def(
        "resendOnSignatureRejection",
        Js::function("resendOnSignatureRejection", |_, a| {
            let (xhr, response, is_rejection) = (arg(&a, 0), arg(&a, 1), arg(&a, 2));
            let config = response.get("config")?;
            if has_retried_signature(&config)?
                || !is_rejection
                    .call(Js::Undefined, vec![response.get("data")?])?
                    .truthy()
            {
                return Ok(Js::Undefined);
            }
            let resend = config.spread_object()?;
            resend.define(SIGNATURE_RETRY_CONFIG_KEY, Js::Bool(true))?;
            xhr.call_method("request", vec![resend])
        }),
    );

    // patternClientOrderIdRule, jsonParse, sortOrderBook, lodash throttle, WeakSet
    def(
        "patternClientOrderIdRule",
        Js::function("patternClientOrderIdRule", |_, a| {
            let rule = client_order_id_rule();
            Ok(Js::object(vec![
                ("label", arg(&a, 0)),
                ("requirement", Js::str(&rule.requirement)),
                (
                    "isValid",
                    Js::function("isValid", |_, v| {
                        Ok(Js::Bool(client_order_id_valid(&text(&arg(&v, 0))?)))
                    }),
                ),
            ]))
        }),
    );
    let w = weak.clone();
    def(
        "jsonParse",
        Js::function("jsonParse", move |_, a| {
            let json = scope(&w)?.global("JSON")?;
            Ok(json
                .call_method("parse", vec![arg(&a, 0)])
                .unwrap_or(Js::Null))
        }),
    );
    def(
        "sortOrderBook",
        Js::function("sortOrderBook", |_, a| {
            let book = arg(&a, 0);
            sort_levels(&book.get("asks")?, true)?;
            sort_levels(&book.get("bids")?, false)?;
            Ok(Js::Undefined)
        }),
    );
    let w = weak.clone();
    def(
        "throttle",
        Js::function("throttle", move |_, a| {
            Ok(throttle(&scope(&w)?, arg(&a, 0), arg(&a, 1).to_number()?))
        }),
    );
    def(
        "WeakSet",
        Js::constructor("WeakSet", None, |_, _| Ok(Js::new_set())),
    );

    install_enums(&g);
    install_builtins(&g, weak.clone());
    install_library(&g, weak);
    let _ = from_json;
    g
}

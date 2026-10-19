//! Source Axios factory routing, using the same provider as the venue's sockets.
use super::{Host, effective_config, rest_request, spawn};
use crate::{
    exchange::BaseExchange,
    http::{HTTP_STATS, is_tealstreet_proxy_auth_failure_body, stringify_response_body},
    js_runtime::{
        base_exchange::exchange_options,
        value::{Js, Thrown, arg, to_json},
    },
};
use safe_cex_transport::{
    RestAdapterContext, RestAdapterVariant, RestTransportContext, TransportProvider,
};
use safe_cex_types::{ExchangeName, RequestOutcome};
use serde_json::Value;
use std::rc::{Rc, Weak};

/// safeCexAxiosCreate: provider-selected baseURL, the source's fixed timeout,
/// proxy authorization before terminal signing, and the factory's response
/// interceptor ahead of the venue's own interceptors on the returned client.
pub fn create(
    host: Host,
    provider: &dyn TransportProvider,
    config: Js,
    options: &Js,
    owner: &Rc<BaseExchange>,
) -> Result<Js, Thrown> {
    let exchange = owner.name().clone();
    let options = exchange_options(options)?;
    let request = rest_request(&config, false)?;
    let transport = provider
        .rest(RestTransportContext {
            exchange: &exchange,
            options: &options,
            config: &request,
        })
        .map_err(|error| Thrown::error("Error", &error.to_string()))?;
    host.configure_exchange(exchange.clone())?;
    let defaults = config.spread_object()?;
    defaults.define("timeout", Js::Number(20_000.0))?;
    let headers = super::axios_headers()?;
    super::set_header(
        &headers,
        Js::str("Accept"),
        Js::str("application/json, text/plain, */*"),
        Js::Undefined,
    )?;
    for (name, value) in config.get("headers")?.spread_object()?.own_entries()? {
        super::set_header(&headers, Js::utf16(&name), value, Js::Undefined)?;
    }
    defaults.define("headers", headers)?;
    defaults.define(
        "baseURL",
        transport.base_url.as_deref().map_or(Js::Undefined, Js::str),
    )?;
    let client = super::create(host.clone(), defaults);
    let stats_runtime = host.runtime();
    let stats_exchange = serde_json::to_value(&exchange)
        .expect("exchange name")
        .as_str()
        .expect("exchange name string")
        .to_owned();
    install_outcome_interceptor(
        &client,
        Rc::downgrade(owner),
        stats_exchange.clone(),
        transport.proxied,
        true,
    )?;
    let record_request = Js::function("recordRequestStats", move |_, args| {
        HTTP_STATS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_request(&stats_exchange, stats_runtime.now_ms());
        let config = arg(&args, 0);
        // Source telemetry catches a sealed/read-only config write without failing HTTP.
        drop(config.set(
            "safeCexRestDiagStartedAt",
            Js::Number(stats_runtime.now_ms().floor()),
        ));
        Ok(config)
    });
    client
        .get("interceptors")?
        .get("request")?
        .call_method("use", vec![record_request])?;
    if transport.proxied
        && let Some(authorize) = transport.authorize
    {
        let interceptor = Js::function("authorize", move |_, args| {
            let config = arg(&args, 0);
            let authorize = authorize.clone();
            Ok(spawn(
                &host,
                Box::pin(async move {
                    let mut request = rest_request(&config, false)?;
                    let before = effective_config(&Js::object(vec![]), request.clone())?;
                    authorize
                        .authorize(&mut request)
                        .await
                        .map_err(|error| Thrown::error("Error", &error.to_string()))?;
                    let after = effective_config(&Js::object(vec![]), request)?;
                    apply_changes(&config, &before, &after)?;
                    Ok(config)
                }),
            ))
        });
        client
            .get("interceptors")?
            .get("request")?
            .call_method("use", vec![interceptor])?;
    }
    let weak_owner = Rc::downgrade(owner);
    let runtime = owner.runtime.clone();
    let log_request = Js::function("logHttpRequest", move |_, args| {
        let config = arg(&args, 0);
        let result = (|| {
            if let Some(owner) = weak_owner.upgrade() {
                emit_telemetry(&owner, &config, false)?;
            }
            Ok(config)
        })();
        Ok(Js::Promise(crate::js_runtime::promise::Promise::settled(
            &runtime, result,
        )))
    });
    client
        .get("interceptors")?
        .get("request")?
        .call_method("use", vec![log_request])?;
    Ok(client)
}

/// Source safeCexAxiosCreateNoProxy: bypass provider REST routing while keeping
/// the same terminal host, fixed timeout and owning exchange outcome observer.
pub fn create_no_proxy(host: Host, config: Js, owner: &Rc<BaseExchange>) -> Result<Js, Thrown> {
    let exchange = owner.name().clone();
    host.configure_exchange(exchange.clone())?;
    let defaults = config.spread_object()?;
    defaults.define("timeout", Js::Number(20_000.0))?;
    let headers = super::axios_headers_from(&Js::object(vec![(
        "Accept",
        Js::str("application/json, text/plain, */*"),
    )]))?;
    for (name, value) in config.get("headers")?.spread_object()?.own_entries()? {
        super::set_header(&headers, Js::utf16(&name), value, Js::Undefined)?;
    }
    defaults.define("headers", headers)?;
    let stats_runtime = host.runtime();
    let stats_exchange = serde_json::to_value(&exchange)
        .expect("exchange name")
        .as_str()
        .expect("exchange name string")
        .to_owned();
    let client = super::create(host, defaults);
    install_outcome_interceptor(
        &client,
        Rc::downgrade(owner),
        stats_exchange.clone(),
        false,
        false,
    )?;
    let record_request = Js::function("recordRequestStats", move |_, args| {
        HTTP_STATS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_request(&stats_exchange, stats_runtime.now_ms());
        let config = arg(&args, 0);
        // Source telemetry catches a sealed/read-only config write without failing HTTP.
        drop(config.set(
            "safeCexRestDiagStartedAt",
            Js::Number(stats_runtime.now_ms().floor()),
        ));
        Ok(config)
    });
    client
        .get("interceptors")?
        .get("request")?
        .call_method("use", vec![record_request])?;
    Ok(client)
}

// ts: the safeCexAxiosCreate response interceptor's recordRequestOutcome and
// proxy-failure `error` emission. Registered first, so venue interceptors see
// the request afterwards, as in the source.
fn install_outcome_interceptor(
    client: &Js,
    owner: Weak<BaseExchange>,
    exchange: String,
    proxied: bool,
    enhance_before_outcome: bool,
) -> Result<(), Thrown> {
    let fulfilled_owner = owner.clone();
    let fulfilled_exchange = exchange.clone();
    let on_fulfilled = Js::function("recordResponseOutcome", move |_, args| {
        let response = arg(&args, 0);
        if let Some(owner) = fulfilled_owner.upgrade() {
            record_outcome(
                &owner,
                &fulfilled_exchange,
                &response,
                Some(&response),
                true,
            );
            let observe = || -> Result<(), Thrown> {
                let config = optional(&response, "config")?;
                if optional(&config, "__tealstreetSigningCapture")?.truthy() {
                    return Ok(());
                }
                let url = optional(&config, "url")?;
                let url = match url {
                    Js::String(text) => Some(String::from_utf16(&text).map_err(|_| {
                        Thrown::type_error("lone UTF-16 surrogate at a typed boundary")
                    })?),
                    _ => None,
                };
                if response.type_of() != "object" || response.is_nullish() || response.is_array() {
                    owner.note_http_failure(None);
                    return Ok(());
                }
                let Js::Number(status) = response.get("status")? else {
                    owner.note_http_failure(None);
                    return Ok(());
                };
                if status < 400.0 {
                    owner.note_http_failure(None);
                    return Ok(());
                }
                let candidates = [response.get("statusText")?, response.get("status_text")?];
                let status_text = candidates
                    .into_iter()
                    .find_map(|candidate| match candidate {
                        Js::String(text) if !text.is_empty() => Some(text),
                        _ => None,
                    });
                let status_text = status_text.map_or(Ok(String::new()), |text| {
                    String::from_utf16(&text).map_err(|_| {
                        Thrown::type_error("lone UTF-16 surrogate at a typed boundary")
                    })
                })?;
                owner.note_http_failure(Some(crate::error_reporting::HttpFailureRecord {
                    status,
                    status_text,
                    url,
                    at: owner.runtime.now_ms(),
                }));
                Ok(())
            };
            // TS suppresses telemetry failures so they cannot reject the request.
            drop(observe());
            if enhance_before_outcome {
                let result = emit_telemetry(&owner, &response, true).map(|()| response.clone());
                return Ok(Js::Promise(crate::js_runtime::promise::Promise::settled(
                    &owner.runtime,
                    result,
                )));
            }
        }
        Ok(response)
    });
    let on_rejected = Js::function("recordErrorOutcome", move |_, args| {
        let error = arg(&args, 0);
        let enhanced = if let Some(owner) = owner.upgrade() {
            let enhance = || {
                super::error_enhancer::enhance_axios_error(
                    &error,
                    &exchange,
                    owner.runtime.now_ms(),
                )
            };
            let enhanced = if enhance_before_outcome {
                Some(enhance()?)
            } else {
                None
            };
            let response = optional(&error, "response")?;
            let response = response.truthy().then_some(response);
            record_outcome(&owner, &exchange, &error, response.as_ref(), false);
            let enhanced = match enhanced {
                Some(value) => value,
                None => enhance()?,
            };
            if proxied && let Some(response) = &response {
                report_proxy_failure(&owner, response)?;
            }
            enhanced
        } else {
            error
        };
        Err(Thrown::new(enhanced))
    });
    client
        .get("interceptors")?
        .get("response")?
        .call_method("use", vec![on_fulfilled, on_rejected])?;
    Ok(())
}

fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_object() {
        value.get(key)
    } else {
        Ok(Js::Undefined)
    }
}

fn status_of(response: &Js) -> Result<Option<f64>, Thrown> {
    let status = response.get("status")?;
    Ok(match status {
        Js::Number(status) if status != 0.0 && !status.is_nan() => Some(status),
        _ => None,
    })
}

fn record_outcome(
    owner: &BaseExchange,
    exchange: &str,
    context: &Js,
    response: Option<&Js>,
    success: bool,
) {
    // TS deliberately ignores counter failures so telemetry cannot fail a request.
    let classify = || -> Result<Option<RequestOutcome>, Thrown> {
        let config = optional(context, "config")?;
        if optional(&config, "__tealstreetSigningCapture")?.truthy() {
            return Ok(None);
        }
        Ok(Some(match response {
            None => RequestOutcome::Network,
            Some(response) => match status_of(response)? {
                None => RequestOutcome::Other,
                Some(429.0 | 418.0) => RequestOutcome::RateLimit,
                Some(401.0 | 403.0) => RequestOutcome::Auth,
                Some(status) if (200.0..300.0).contains(&status) => RequestOutcome::Ok,
                Some(_) => RequestOutcome::Other,
            },
        }))
    };
    if let Ok(Some(outcome)) = classify() {
        // The source try/catch includes the Store write, not only classification.
        let _ = owner.store.record_request_outcome(outcome);
    }
    let diagnostics = || -> Result<(), Thrown> {
        use safe_cex_store::venue_rest_diagnostics as diagnostic;
        let config = optional(context, "config")?;
        if optional(&config, "__tealstreetSigningCapture")?.truthy() {
            return Ok(());
        }
        let text = |value: Js| -> Result<Option<String>, Thrown> {
            match value {
                Js::String(units) => String::from_utf16(&units).map(Some).map_err(|_| {
                    Thrown::type_error("lone UTF-16 surrogate at a typed diagnostic boundary")
                }),
                _ => Ok(None),
            }
        };
        let headers = |source: Js| -> Result<diagnostic::DiagnosticHeaders, Thrown> {
            if source.type_of() != "object" || source.is_nullish() {
                return Ok(diagnostic::DiagnosticHeaders::default());
            }
            let mut lower = Vec::new();
            for (key, value) in source.own_entries()? {
                let key = Js::utf16(&key)
                    .call_method("toLowerCase", vec![])?
                    .to_js_string()?;
                lower.push((key, value));
            }
            Ok(diagnostic::DiagnosticHeaders {
                values: Default::default(),
                get: Some(Rc::new(move |name| {
                    let read = || -> Result<Option<Value>, Thrown> {
                        let getter = source.get("get")?;
                        let candidate = if getter.is_callable() {
                            source.call_method("get", vec![Js::str(name)])?
                        } else {
                            Js::Undefined
                        };
                        let candidate = if candidate.is_nullish() {
                            lower
                                .iter()
                                .rev()
                                .find(|(key, _)| key.iter().copied().eq(name.encode_utf16()))
                                .map_or(Js::Undefined, |(_, value)| value.clone())
                        } else {
                            candidate
                        };
                        if candidate.is_nullish() {
                            return Ok(None);
                        }
                        let units = candidate.to_js_string()?;
                        let value = String::from_utf16(&units).map_err(|_| {
                            Thrown::type_error(
                                "lone UTF-16 surrogate at a typed diagnostic boundary",
                            )
                        })?;
                        Ok(Some(Value::String(value)))
                    };
                    read().map_err(|error| error.message())
                })),
            })
        };
        let config = if config.is_object() {
            let proxy_header = text(optional(&config, "safeCexRestDiagProxyHeader")?)?;
            let mut numbers = indexmap::IndexMap::new();
            for key in [
                "safeCexRestDiagStartedAt",
                "safeCexRestDiagSignMs",
                "safeCexRestDiagSignatureAgeMs",
            ] {
                if let Js::Number(value) = config.get(key)? {
                    numbers.insert(key.into(), value);
                }
            }
            Some(diagnostic::RestDiagnosticConfig {
                url: text(config.get("url")?)?,
                base_url: text(config.get("baseURL")?)?,
                headers: if matches!(proxy_header.as_deref(), Some("present" | "empty")) {
                    headers(config.get("headers")?)?
                } else {
                    Default::default()
                },
                numbers,
                proxy_header,
                ..Default::default()
            })
        } else {
            None
        };
        let response = response
            .map(|source| -> Result<_, Thrown> {
                let data = source.get("data")?;
                let data = if data.is_nullish() {
                    None
                } else if data.type_of() == "object" {
                    let mut keys = serde_json::Map::new();
                    for key in ["retCode", "retMsg", "code"] {
                        if super::super::library::in_operator(&data, key)? {
                            keys.insert(key.into(), Value::Null);
                        }
                    }
                    Some(Value::Object(keys))
                } else {
                    Some(Value::String(
                        String::from_utf16(&data.to_js_string()?).map_err(|_| {
                            Thrown::type_error(
                                "lone UTF-16 surrogate at a typed diagnostic boundary",
                            )
                        })?,
                    ))
                };
                Ok(diagnostic::RestDiagnosticResponse {
                    config: config.clone(),
                    status: match source.get("status")? {
                        Js::Number(value) => Some(value),
                        _ => None,
                    },
                    headers: headers(source.get("headers")?)?,
                    data,
                })
            })
            .transpose()?;
        if success {
            diagnostic::note_rest_success(
                exchange,
                response.as_ref(),
                owner.runtime.now_ms().floor(),
            );
        } else {
            diagnostic::note_rest_failure(
                exchange,
                Some(&diagnostic::RestDiagnosticError {
                    config,
                    response,
                    code: text(optional(context, "code")?)?,
                    message: text(optional(context, "message")?)?,
                    name: text(optional(context, "name")?)?,
                }),
                owner.runtime.now_ms().floor(),
            );
        }
        Ok(())
    };
    // The maintained diagnostics catch telemetry failures without changing HTTP.
    drop(diagnostics());
}

fn report_proxy_failure(owner: &BaseExchange, response: &Js) -> Result<(), Thrown> {
    let Some(status) = status_of(response)? else {
        return Ok(());
    };
    let data = response.get("data")?;
    let body: Value = if data.is_undefined() {
        Value::Null
    } else {
        to_json(&data)?
    };
    let source = if status == 401.0 || status == 403.0 {
        if is_tealstreet_proxy_auth_failure_body(&body) {
            "Tealstreet proxy auth failed"
        } else {
            "Upstream exchange auth failed via proxy"
        }
    } else if status >= 500.0 {
        "Proxy server error"
    } else {
        return Ok(());
    };
    let text = if data.is_undefined() {
        "undefined".to_owned()
    } else {
        stringify_response_body(&body)
    };
    let name = serde_json::to_value(&owner.name).expect("exchange name");
    let name = name.as_str().expect("exchange name string");
    let message = format!(
        "[{name}] {source} ({}): {text}",
        Js::Number(status).to_display()
    );
    owner.runtime.console("error", &message);
    owner.emit("error", Js::str(&message)).map_err(Thrown::from)
}

fn same(left: &Js, right: &Js) -> Result<bool, Thrown> {
    Ok(left.is_undefined() == right.is_undefined() && to_json(left)? == to_json(right)?)
}

// Apply only provider changes. Unchanged params/data retain their JS identity,
// native number bits and callbacks; AxiosHeaders keeps its receiver/prototype.
fn apply_changes(config: &Js, before: &Js, after: &Js) -> Result<(), Thrown> {
    for (key, value) in after.own_entries()? {
        let previous = before.get_key(&key)?;
        if same(&previous, &value)? {
            continue;
        }
        if key.as_ref() == super::super::value::utf16("headers").as_ref() {
            let headers = config.get("headers")?;
            for old in previous.own_keys() {
                if !value.own_keys().iter().any(|key| key == &old) {
                    headers.delete(&old)?;
                }
            }
            for (name, value) in value.own_entries()? {
                if !same(&previous.get_key(&name)?, &value)? {
                    super::set_header(&headers, Js::utf16(&name), value, Js::Undefined)?;
                }
            }
        } else if key.as_ref() == super::super::value::utf16("safeCexRestDiagProxyHeader").as_ref()
        {
            // Proxy telemetry is best-effort even when the live config is read-only.
            drop(config.set_key(&key, value));
        } else {
            config.set_key(&key, value)?;
        }
    }
    Ok(())
}

/// Source installTransportAdapter, called explicitly by the venue. Provider
/// selection never strips the native host's remote signer or query codec.
pub fn install_transport_adapter(
    host: Host,
    provider: &dyn TransportProvider,
    client: &Js,
    options: &Js,
    exchange: ExchangeName,
    variant: RestAdapterVariant,
) -> Result<(), Thrown> {
    let options = exchange_options(options)?;
    if let Some(adapter) = provider.rest_adapter(RestAdapterContext {
        exchange: &exchange,
        options: &options,
        variant,
    }) {
        let adapter_host =
            super::rest_http_host_signed(host.runtime(), adapter, host.hosted_signer());
        adapter_host.configure_exchange(exchange)?;
        let adapter = Js::function("transportAdapter", move |_, args| {
            Ok(spawn(&adapter_host, adapter_host.send(arg(&args, 0))))
        });
        client.get("defaults")?.set("adapter", adapter)?;
    }
    Ok(())
}

fn emit_telemetry(owner: &BaseExchange, value: &Js, response: bool) -> Result<(), Thrown> {
    if !crate::http_telemetry::should_log(
        || {
            let config = if response {
                value.get("config")?
            } else {
                value.clone()
            };
            config.get("url")
        },
        |name| owner.http_endpoints(name),
        crate::http_telemetry::is_development(),
        || owner.runtime.now_ms(),
        &crate::http::HTTP_LOGS,
        response,
    )? {
        return Ok(());
    }
    let event = if response {
        "grafana-http-response"
    } else {
        "grafana-http-request"
    };
    let message = if response {
        crate::http_telemetry::response_message(value)
    } else {
        crate::http_telemetry::request_message(value)
    };
    if let Err(error) = message.and_then(|message| owner.emit(event, message).map_err(Thrown::from))
    {
        owner.runtime.console_values(
            "error",
            &[Js::str(&format!("Error emitting {event}")), error.value],
        );
    }
    Ok(())
}

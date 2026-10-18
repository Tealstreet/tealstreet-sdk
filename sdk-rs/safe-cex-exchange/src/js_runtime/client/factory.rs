//! Source Axios factory routing, using the same provider as the venue's sockets.
use super::{Host, effective_config, rest_request, spawn};
use crate::{
    exchange::BaseExchange,
    http::{is_tealstreet_proxy_auth_failure_body, stringify_response_body},
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
    host.configure_exchange(exchange)?;
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
    install_outcome_interceptor(&client, Rc::downgrade(owner), transport.proxied)?;
    let runtime = host.runtime();
    let mark_start = Js::function("markRestRequestStart", move |_, args| {
        let config = arg(&args, 0);
        // Source diagnostics cannot turn an otherwise valid request into a rejection.
        if let Err(error) = config.set("safeCexRestDiagStartedAt", Js::Number(runtime.now_ms())) {
            drop(error);
        }
        Ok(config)
    });
    client
        .get("interceptors")?
        .get("request")?
        .call_method("use", vec![mark_start])?;
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
    Ok(client)
}

// ts: the safeCexAxiosCreate response interceptor's recordRequestOutcome and
// proxy-failure `error` emission. Registered first, so venue interceptors see
// the request afterwards, as in the source.
fn install_outcome_interceptor(
    client: &Js,
    owner: Weak<BaseExchange>,
    proxied: bool,
) -> Result<(), Thrown> {
    let fulfilled_owner = owner.clone();
    let on_fulfilled = Js::function("recordResponseOutcome", move |_, args| {
        let response = arg(&args, 0);
        if let Some(owner) = fulfilled_owner.upgrade() {
            record_outcome(&owner, &response, Some(&response));
            record_rest_diagnostics(&owner, &response, true);
        }
        Ok(response)
    });
    let on_rejected = Js::function("recordErrorOutcome", move |_, args| {
        let error = arg(&args, 0);
        if let Some(owner) = owner.upgrade() {
            let response = optional(&error, "response")?;
            let response = response.truthy().then_some(response);
            record_outcome(&owner, &error, response.as_ref());
            record_rest_diagnostics(&owner, &error, false);
            if proxied && let Some(response) = &response {
                report_proxy_failure(&owner, response)?;
            }
        }
        Err(Thrown::new(error))
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

fn record_outcome(owner: &BaseExchange, context: &Js, response: Option<&Js>) {
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
        owner.store.record_request_outcome(outcome);
    }
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
                    headers.delete(&old);
                }
            }
            for (name, value) in value.own_entries()? {
                if !same(&previous.get_key(&name)?, &value)? {
                    super::set_header(&headers, Js::utf16(&name), value, Js::Undefined)?;
                }
            }
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

fn diagnostic_headers(headers: Js) -> safe_cex_store::venue_rest_diagnostics::DiagnosticHeaders {
    use safe_cex_store::venue_rest_diagnostics::DiagnosticHeaders;
    let lower = std::cell::RefCell::new(None::<indexmap::IndexMap<String, Js>>);
    DiagnosticHeaders {
        values: Default::default(),
        get: Some(Rc::new(move |name| {
            let read = || -> Result<Option<Value>, Thrown> {
                if headers.type_of() != "object" || headers.is_nullish() {
                    return Ok(None);
                }
                if lower.borrow().is_none() {
                    let entries = headers
                        .own_entries()?
                        .into_iter()
                        .map(|(key, value)| (String::from_utf16_lossy(&key).to_lowercase(), value))
                        .collect();
                    *lower.borrow_mut() = Some(entries);
                }
                let getter = headers.get("get")?;
                let value = if getter.is_callable() {
                    getter.call(headers.clone(), vec![Js::str(name)])?
                } else {
                    Js::Undefined
                };
                let value = if value.is_nullish() {
                    lower
                        .borrow()
                        .as_ref()
                        .and_then(|values| values.get(name))
                        .cloned()
                        .unwrap_or(Js::Undefined)
                } else {
                    value
                };
                if value.is_nullish() {
                    return Ok(None);
                }
                let text = if matches!(value, Js::SymbolIterator) {
                    "Symbol(Symbol.iterator)".into()
                } else {
                    String::from_utf16_lossy(&value.to_js_string()?)
                };
                Ok(Some(Value::String(text)))
            };
            read().map_err(|error| error.message())
        })),
    }
}
fn diagnostic_config(
    config: &Js,
) -> Result<safe_cex_store::venue_rest_diagnostics::RestDiagnosticConfig, Thrown> {
    use safe_cex_store::venue_rest_diagnostics::RestDiagnosticConfig;
    let text = |key| -> Result<Option<String>, Thrown> {
        Ok(match optional(config, key)? {
            Js::String(value) => Some(String::from_utf16_lossy(&value)),
            _ => None,
        })
    };
    let mut result = RestDiagnosticConfig {
        url: text("url")?,
        base_url: text("baseURL")?,
        proxy_header: text("safeCexRestDiagProxyHeader")?,
        headers: diagnostic_headers(optional(config, "headers")?),
        ..Default::default()
    };
    for name in [
        "safeCexRestDiagStartedAt",
        "safeCexRestDiagSignMs",
        "safeCexRestDiagSignedAt",
        "safeCexRestDiagSignatureAgeMs",
    ] {
        if let Js::Number(value) = optional(config, name)? {
            result.numbers.insert(name.into(), value);
        }
    }
    Ok(result)
}
fn diagnostic_response(
    response: &Js,
    include_body: bool,
) -> Result<safe_cex_store::venue_rest_diagnostics::RestDiagnosticResponse, Thrown> {
    use safe_cex_store::venue_rest_diagnostics::RestDiagnosticResponse;
    let data = if include_body {
        optional(response, "data")?
    } else {
        Js::Undefined
    };
    Ok(RestDiagnosticResponse {
        config: Some(diagnostic_config(&optional(response, "config")?)?),
        status: match optional(response, "status")? {
            Js::Number(value) => Some(value),
            _ => None,
        },
        headers: diagnostic_headers(optional(response, "headers")?),
        data: crate::rest_diagnostics::body(&data)?,
    })
}
fn record_rest_diagnostics(owner: &BaseExchange, context: &Js, success: bool) {
    use safe_cex_store::venue_rest_diagnostics::{self as diagnostics, RestDiagnosticError};
    let record = || -> Result<(), Thrown> {
        let config = optional(context, "config")?;
        if optional(&config, "__tealstreetSigningCapture")?.truthy() {
            return Ok(());
        }
        let name = serde_json::to_value(&owner.name).expect("exchange name");
        let name = name.as_str().expect("exchange string");
        if success {
            diagnostics::note_rest_success(
                name,
                Some(&diagnostic_response(context, false)?),
                owner.runtime.now_ms(),
            );
        } else {
            let response = optional(context, "response")?;
            let text = |key| -> Result<Option<String>, Thrown> {
                Ok(match optional(context, key)? {
                    Js::String(value) => Some(String::from_utf16_lossy(&value)),
                    _ => None,
                })
            };
            let error = RestDiagnosticError {
                config: Some(diagnostic_config(&config)?),
                response: if response.truthy() {
                    Some(diagnostic_response(&response, true)?)
                } else {
                    None
                },
                code: text("code")?,
                message: text("message")?,
                name: text("name")?,
            };
            diagnostics::note_rest_failure(name, Some(&error), owner.runtime.now_ms());
        }
        Ok(())
    };
    if let Err(error) = record() {
        drop(error);
    }
}

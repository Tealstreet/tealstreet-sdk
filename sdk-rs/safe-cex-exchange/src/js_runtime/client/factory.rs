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
    install_outcome_interceptor(&client, Rc::downgrade(owner), transport.proxied)?;
    let stats_runtime = host.runtime();
    let stats_exchange = serde_json::to_value(&exchange)
        .expect("exchange name")
        .as_str()
        .expect("exchange name string")
        .to_owned();
    let record_request = Js::function("recordRequestStats", move |_, args| {
        HTTP_STATS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .record_request(&stats_exchange, stats_runtime.now_ms());
        Ok(arg(&args, 0))
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
        }
        Ok(response)
    });
    let on_rejected = Js::function("recordErrorOutcome", move |_, args| {
        let error = arg(&args, 0);
        if let Some(owner) = owner.upgrade() {
            let response = optional(&error, "response")?;
            let response = response.truthy().then_some(response);
            record_outcome(&owner, &error, response.as_ref());
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

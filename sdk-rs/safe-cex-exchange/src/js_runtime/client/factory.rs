//! Source Axios factory routing, using the same provider as the venue's sockets.
use super::{Host, effective_config, rest_request, spawn};
use crate::js_runtime::{
    base_exchange::exchange_options,
    value::{Js, Thrown, arg, to_json},
};
use safe_cex_transport::{
    RestAdapterContext, RestAdapterVariant, RestTransportContext, TransportProvider,
};
use safe_cex_types::ExchangeName;

/// The routing portion of safeCexAxiosCreate: provider-selected baseURL, the
/// source's fixed timeout, and proxy authorization before terminal signing.
/// Venue request/response interceptors remain on the returned ordinary client.
pub fn create(
    host: Host,
    provider: &dyn TransportProvider,
    config: Js,
    options: &Js,
    exchange: ExchangeName,
) -> Result<Js, Thrown> {
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

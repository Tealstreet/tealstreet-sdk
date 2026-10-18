//! WOO signs the live native configuration; JSON projection occurs only in V3's preimage.
use safe_cex_crypto::{Encoding, SigningOptions, sign_payload};
use safe_cex_exchange::js_runtime::value::{Js, to_json, to_retained_input};
use safe_cex_transport::{RestRequest, RestTransportContext, TransportProvider};
use safe_cex_types::{ExchangeName, ExchangeOptions};

pub struct WooSignedRequest {
    pub config: Js,
    pub preimage: String,
}
fn get(config: &Js, key: &str) -> Result<Js, String> {
    config.get(key).map_err(|error| error.message())
}
fn set(config: &Js, key: &str, value: Js) -> Result<(), String> {
    config.set(key, value).map_err(|error| error.message())
}
fn string(value: &Js) -> Result<String, String> {
    let units = value.to_js_string().map_err(|error| error.message())?;
    String::from_utf16(&units).map_err(|error| error.to_string())
}
fn method(config: &Js, expression: &str) -> Result<String, String> {
    let value = get(config, "method")?;
    match value {
        Js::String(units) if !units.is_empty() => {
            string(&Js::String(units)).map(|value| value.to_uppercase())
        }
        Js::Object(_) if !get(&value, "toUpperCase")?.is_nullish() => {
            Err(format!("{expression} is not a function"))
        }
        _ => Ok("GET".into()),
    }
}
fn query(data: &Js, repeat: bool, sorted: bool) -> Result<String, String> {
    let options = Js::object(vec![(
        "arrayFormat",
        Js::str(if repeat { "repeat" } else { "indices" }),
    )]);
    if sorted {
        let collator =
            safe_cex_num::JsRootCollator::try_new().map_err(|error| error.message.to_owned())?;
        let sort = Js::function("localeCompare", move |_, args| {
            let left = safe_cex_exchange::js_runtime::value::arg(&args, 0);
            let right = safe_cex_exchange::js_runtime::value::arg(&args, 1);
            let left = String::from_utf16(&left.to_js_string()?).map_err(|_| {
                safe_cex_exchange::js_runtime::value::Thrown::error(
                    "RepresentationError",
                    "query key is outside the shared string boundary",
                )
            })?;
            let right = String::from_utf16(&right.to_js_string()?).map_err(|_| {
                safe_cex_exchange::js_runtime::value::Thrown::error(
                    "RepresentationError",
                    "query key is outside the shared string boundary",
                )
            })?;
            Ok(Js::Number(match collator.compare(&left, &right) {
                std::cmp::Ordering::Less => -1.0,
                std::cmp::Ordering::Equal => 0.0,
                std::cmp::Ordering::Greater => 1.0,
            }))
        });
        options.set("sort", sort).map_err(|error| error.message())?;
    }
    safe_cex_exchange::js_runtime::library::qs_stringify(data, &options)
        .map_err(|error| error.message())
}
pub(crate) fn sorted_query(data: &Js, repeat: bool) -> Result<String, String> {
    query(data, repeat, true)
}
fn proxy_request(config: &Js, url: &str) -> Result<RestRequest, String> {
    let timeout = get(config, "timeout")?
        .to_number()
        .ok()
        .filter(|value| {
            value.is_finite()
                && value.fract() == 0.0
                && *value >= i64::MIN as f64
                && *value < i64::MAX as f64
        })
        .map(|value| value as i64);
    let headers = get(config, "headers")?;
    let headers = if headers.is_nullish() {
        Default::default()
    } else {
        headers
            .own_entries()
            .map_err(|error| error.message())?
            .into_iter()
            .filter_map(|(key, value)| match value {
                Js::String(units) => Some(
                    String::from_utf16(&key)
                        .and_then(|key| String::from_utf16(&units).map(|value| (key, value))),
                ),
                _ => None,
            })
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?
    };
    let params = get(config, "params")?;
    let data = get(config, "data")?;
    Ok(RestRequest {
        url: url.into(),
        method: get(config, "method")?
            .as_str()
            .map(|units| String::from_utf16(units))
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or_else(|| "GET".into()),
        // This legacy params view remains the separate declared query boundary.
        params: if params.is_undefined() {
            None
        } else {
            Some(to_json(&params).map_err(|error| error.message())?)
        },
        body: if data.is_undefined() {
            None
        } else {
            Some(safe_cex_transport::RestBody::Untransformed(
                to_retained_input(&data).map_err(|error| error.message())?,
            ))
        },
        base_url: get(config, "baseURL")?
            .as_str()
            .map(|units| String::from_utf16(units))
            .transpose()
            .map_err(|error| error.to_string())?,
        timeout_ms: timeout,
        headers,
        ..Default::default()
    })
}
async fn sign(
    config: &Js,
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    timestamp: f64,
    v3: bool,
) -> Result<WooSignedRequest, String> {
    let next = config.spread_object().map_err(|error| error.message())?;
    let method = method(
        &next,
        if v3 {
            "c.method?.toUpperCase"
        } else {
            "nextConfig.method?.toUpperCase"
        },
    )?;
    if v3 {
        set(&next, "method", Js::str(&method))?;
    }
    let url = get(&next, "url")?;
    if !url.is_nullish() {
        if !url.is_string() {
            return Err("nextConfig?.url?.startsWith is not a function".into());
        }
        let url = string(&url)?;
        if url.starts_with("http") {
            let request = proxy_request(&next, &url)?;
            let prefix = provider.proxy_prefix(RestTransportContext {
                exchange: &ExchangeName::Woo,
                options,
                config: &request,
            });
            if !prefix.is_empty() {
                set(&next, "url", Js::str(&format!("{prefix}/{url}")))?;
            }
        }
    }
    let mutation = matches!(method.as_str(), "DELETE" | "POST" | "PUT");
    let params = get(&next, "params")?;
    let data = get(&next, "data")?;
    let query = if v3 {
        if params.truthy() {
            Some(query(&params, false, false)?)
        } else {
            None
        }
    } else {
        Some(sorted_query(
            if data.truthy() { &data } else { &params },
            true,
        )?)
    };
    if let Some(query) = query
        .as_ref()
        .filter(|query| v3 || (!query.is_empty() && !mutation))
    {
        set(
            &next,
            "url",
            Js::str(&format!("{}?{query}", string(&get(&next, "url")?)?)),
        )?;
        next.delete(&"params".encode_utf16().collect::<Vec<_>>());
    }
    let preimage = if v3 {
        let url = get(&next, "url")?;
        let url = if url.is_nullish() {
            String::new()
        } else {
            string(&url)?
        };
        let body = if data.truthy() {
            let units = safe_cex_exchange::js_runtime::globals::stringify_json(&data)
                .map_err(|error| error.message())?
                .unwrap_or_default();
            String::from_utf16(&units).map_err(|error| error.to_string())?
        } else {
            String::new()
        };
        format!(
            "{}{method}{url}{body}",
            safe_cex_num::js_number_string(timestamp)
        )
    } else {
        format!(
            "{}|{}",
            query.as_deref().unwrap_or(""),
            safe_cex_num::js_number_string(timestamp)
        )
    };
    let signature = sign_payload(
        &SigningOptions {
            key: &options.key,
            secret: &options.secret,
            payload_signer: None,
        },
        ExchangeName::Woo,
        &preimage,
        Encoding::Hex,
    )
    .await?
    .signature;
    let headers = get(&next, "headers")?
        .spread_object()
        .map_err(|error| error.message())?;
    for key in headers.own_keys() {
        if let Ok(name) = String::from_utf16(&key)
            && ["x-api-timestamp", "x-api-signature"]
                .iter()
                .any(|expected| name.eq_ignore_ascii_case(expected))
        {
            headers.delete(&key);
        }
    }
    set(
        &headers,
        "x-api-timestamp",
        Js::str(&safe_cex_num::js_number_string(timestamp)),
    )?;
    set(&headers, "x-api-signature", Js::str(&signature))?;
    if !v3 && mutation {
        set(&next, "data", Js::str(query.as_deref().unwrap_or("")))?;
        next.delete(&"params".encode_utf16().collect::<Vec<_>>());
        let key = headers.own_keys().into_iter().find(|key| {
            String::from_utf16(key).is_ok_and(|key| key.eq_ignore_ascii_case("content-type"))
        });
        let key = key.unwrap_or_else(|| "Content-Type".encode_utf16().collect::<Vec<_>>().into());
        if !matches!(
            headers.get_key(&key).map_err(|error| error.message())?,
            Js::Bool(false)
        ) {
            headers
                .set_key(&key, Js::str("application/x-www-form-urlencoded"))
                .map_err(|error| error.message())?;
        }
    }
    set(&next, "headers", headers)?;
    Ok(WooSignedRequest {
        config: next,
        preimage,
    })
}
pub async fn sign_v1(
    config: &Js,
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    timestamp: f64,
) -> Result<WooSignedRequest, String> {
    sign(config, options, provider, timestamp, false).await
}
pub async fn sign_v3(
    config: &Js,
    options: &ExchangeOptions,
    provider: &dyn TransportProvider,
    timestamp: f64,
) -> Result<WooSignedRequest, String> {
    sign(config, options, provider, timestamp, true).await
}

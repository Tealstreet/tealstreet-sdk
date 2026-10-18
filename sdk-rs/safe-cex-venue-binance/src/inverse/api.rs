//! binanceinverse.api.ts: actual Axios clients over the owning transport/host.
use super::Rt;
use safe_cex_exchange::js_runtime::{
    client::{self, axios_headers_from},
    library::qs_stringify,
    value::{Js, Thrown, arg, from_retained_input},
};
use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY as RETRIED;
use safe_cex_transport::{
    NetworkFailure, RestAdapterVariant, RestFailure, RestRequest, TransportError,
};

fn optional(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}
fn signature_rejection(data: &Js) -> Result<bool, Thrown> {
    if optional(data, "code")?.to_number()? == -1022.0 {
        return Ok(true);
    }
    let message = optional(data, "msg")?;
    let message = if message.is_nullish() {
        Js::str("")
    } else {
        message
    };
    // The source's non-Unicode /i expression folds ASCII only.
    let units = message.to_js_string()?;
    let needle: Vec<_> = "signature for this request is not valid"
        .encode_utf16()
        .collect();
    Ok(units.windows(needle.len()).any(|window| {
        window.iter().zip(&needle).all(|(a, b)| {
            let a = if (65..=90).contains(a) { *a + 32 } else { *a };
            a == *b
        })
    }))
}
fn network_read(error: &Js) -> Result<bool, Thrown> {
    let config = error.get("config")?;
    if config.is_undefined() {
        return Ok(false);
    }
    let method = config.get("method")?;
    let method = if method.is_nullish() {
        Js::Undefined
    } else {
        method.call_method("toLowerCase", vec![])?
    };
    let mut read = method.strict_equals(&Js::str("get")) || method.strict_equals(&Js::str("head"));
    if !read {
        let url = config.get("url")?;
        let path = if url.is_nullish() {
            Js::str("")
        } else {
            url.call_method("split", vec![Js::str("?")])?.get("0")?
        };
        read = path.strict_equals(&Js::str("/dapi/v1/listenKey"));
    }
    if !read || error.get("response")?.truthy() || !error.get("code")?.truthy() {
        return Ok(false);
    }
    let code = error.get("code")?;
    if code.strict_equals(&Js::str("ERR_CANCELED")) || code.strict_equals(&Js::str("ECONNABORTED"))
    {
        return Ok(false);
    }
    let code = error.get("code")?;
    // The installed package compares its deny-list with strict equality. A
    // non-string truthy code is not one of those strings.
    let Some(code) = code.as_str() else {
        return Ok(true);
    };
    let Ok(code) = String::from_utf16(code) else {
        return Ok(true);
    };
    Ok(safe_cex_transport::retry::is_network_error(
        &RestFailure::from_transport(
            TransportError::Network(NetworkFailure::from_code(String::new(), Some(code))),
            RestRequest::default(),
        ),
    ))
}
fn mark_skip(config: Js) -> Result<Js, Thrown> {
    config.set("safeCexSkipRemoteSigning", Js::Bool(true))?;
    Ok(config)
}
fn public_config(config: &Js) -> Result<Js, Thrown> {
    let next = config.spread_object()?;
    next.set(
        "headers",
        axios_headers_from(&Js::object(vec![(
            "Content-Type",
            Js::str("application/json, charset=utf-8"),
        )]))?,
    )?;
    next.set("timeout", Js::Number(20_000.0))?;
    mark_skip(next)
}
fn silent(code: &Js) -> bool {
    [-2011.0, -2013.0, -4059.0, -5022.0]
        .iter()
        .any(|n| code.strict_equals(&Js::Number(*n)))
}

pub(crate) fn create(rt: &Rt, options: Js, public: bool) -> Result<Js, Thrown> {
    let base_url = rt
        .global("BASE_URL")?
        .get(if options.get("testnet")?.truthy() {
            "testnet"
        } else {
            "livenet"
        })?;
    let window = optional(&options.get("extra")?, "recvWindow")?;
    let window = if window.is_nullish() {
        rt.global("RECV_WINDOW")?
    } else {
        window
    };
    let headers = Js::object(vec![(
        "Content-Type",
        Js::str("application/json, charset=utf-8"),
    )]);
    if !public {
        headers.define("X-MBX-APIKEY", options.get("key")?)?;
    }
    let config = Js::object(vec![
        ("baseURL", base_url),
        ("timeout", window),
        ("headers", headers),
        (
            "paramsSerializer",
            Js::object(vec![(
                "serialize",
                Js::function("serialize", |_, a| {
                    Ok(Js::str(&qs_stringify(
                        &arg(&a, 0),
                        &Js::object(vec![("arrayFormat", Js::str("repeat"))]),
                    )?))
                }),
            )]),
        ),
        (
            "transformResponse",
            Js::function("bigIntAsStringJsonParse", |_, a| {
                let input = arg(&a, 0);
                let Some(text) = input.as_str() else {
                    return Ok(input);
                };
                Ok(from_retained_input(
                    safe_cex_transport::how::big_int_as_string_json_parse_utf16(&text),
                ))
            }),
        ),
    ]);
    let xhr = if public {
        client::factory::create_no_proxy(rt.shared.http.clone(), config, rt.base())?
    } else {
        client::factory::create(
            rt.shared.http.clone(),
            &*rt.shared.runtime.transport(),
            config,
            &options,
            rt.base(),
        )?
    };
    xhr.call_method(
        "__configureRetry",
        vec![Js::object(vec![
            ("retries", Js::Number(3.0)),
            (
                "retryCondition",
                Js::function("retryCondition", move |_, a| {
                    let error = arg(&a, 0);
                    if network_read(&error)? {
                        return Ok(Js::Bool(true));
                    }
                    if public {
                        return Ok(Js::Bool(false));
                    }
                    let config = error.get("config")?;
                    if !config.truthy() {
                        return Ok(Js::Bool(false));
                    }
                    let response = error.get("response")?;
                    if !response.truthy()
                        || config.get(RETRIED)?.truthy()
                        || !signature_rejection(&response.get("data")?)?
                    {
                        return Ok(Js::Bool(false));
                    }
                    config.set(RETRIED, Js::Bool(true))?;
                    Ok(Js::Bool(true))
                }),
            ),
        ])],
    )?;
    let weak = std::rc::Rc::downgrade(&rt.shared);
    let request_options = options.clone();
    xhr.get("interceptors")?.get("request")?.call_method(
        "use",
        vec![Js::function("signBinanceRequest", move |_, a| {
            let config = arg(&a, 0);
            if public {
                return public_config(&config);
            }
            if config
                .get("url")?
                .strict_equals(&Js::str("/dapi/v1/listenKey"))
            {
                return mark_skip(config);
            }
            // Array.includes compares against `${config.url}`.
            let url = Js::utf16(&config.get("url")?.to_js_string()?);
            if super::types::PUBLIC_ENDPOINTS
                .iter()
                .any(|path| url.strict_equals(&Js::str(path)))
            {
                return public_config(&config);
            }
            let rt = safe_cex_exchange::js_runtime::scope::scope_of(&weak)?;
            let options = request_options.clone();
            Ok(rt.clone().spawn(Box::pin(async move {
                let window = optional(&options.get("extra")?, "recvWindow")?;
                let window = if window.is_nullish() {
                    Js::Number(super::types::RECV_WINDOW)
                } else {
                    window
                };
                crate::signing::sign_binance_request_native(
                    &config,
                    &options,
                    rt.shared.extra.name.clone(),
                    window,
                    &safe_cex_store::virtual_clock::VIRTUAL_CLOCK,
                    rt.base().signers.payload_signer.as_deref(),
                )
                .await
            })))
        })],
    )?;
    if options.get("tealstreetTunnel")?.truthy() {
        client::factory::install_transport_adapter(
            rt.shared.http.clone(),
            &*rt.shared.runtime.transport(),
            &xhr,
            &options,
            rt.shared.extra.name.clone(),
            RestAdapterVariant::BigInt,
        )?;
    }
    if !public {
        let weak = std::rc::Rc::downgrade(&rt.shared);
        let retry_client = xhr.clone();
        xhr.get("interceptors")?.get("response")?.call_method(
            "use",
            vec![
                Js::function("onFulfilled", move |_, a| {
                    let response = arg(&a, 0);
                    let config = response.get("config")?;
                    if !(!config.is_nullish() && config.get(RETRIED)?.truthy())
                        && signature_rejection(&response.get("data")?)?
                    {
                        let next = config.spread_object()?;
                        next.set(RETRIED, Js::Bool(true))?;
                        return retry_client.call_method("request", vec![next]);
                    }
                    let data = response.get("data")?;
                    if data.truthy()
                        && data.type_of() == "object"
                        && super::ops::contains(&data, "code")?
                        && data.get("code")?.to_number()? < 0.0
                    {
                        let code = data.get("code")?;
                        let message = data.get("msg")?;
                        let message = if message.truthy() {
                            message
                        } else {
                            Js::str(&format!("Binance COIN-M Error: {}", code.to_display()))
                        };
                        if !silent(&code) {
                            safe_cex_exchange::js_runtime::scope::scope_of(&weak)?
                                .call_path("emitAndLogError", vec![message.clone()])?;
                        }
                        let error = Thrown::error(
                            "Error",
                            &String::from_utf16_lossy(&message.to_js_string()?),
                        );
                        error.value.set("code", code)?;
                        error.value.set("response", response)?;
                        error.value.set("__binanceEmitted", Js::Bool(true))?;
                        return Err(error);
                    }
                    Ok(response)
                }),
                {
                    let weak = std::rc::Rc::downgrade(&rt.shared);
                    Js::function("onRejected", move |_, a| {
                        let error = arg(&a, 0);
                        let data = optional(&optional(&error, "response")?, "data")?;
                        let code = optional(&data, "code")?;
                        let message = optional(&data, "msg")?;
                        if message.truthy()
                            && !silent(&code)
                            && !error.get("__binanceEmitted")?.truthy()
                        {
                            error.set("__binanceEmitted", Js::Bool(true))?;
                            safe_cex_exchange::js_runtime::scope::scope_of(&weak)?
                                .call_path("emitAndLogError", vec![data.get("msg")?])?;
                        }
                        Err(Thrown::new(error))
                    })
                },
            ],
        )?;
    }
    Ok(xhr)
}

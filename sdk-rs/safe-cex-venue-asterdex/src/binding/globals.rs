use super::{Rt, Shared, error, scope, text};
use crate::{api_helpers as api, helpers};
use safe_cex_exchange::{
    js_runtime::{
        client,
        globals::{install_builtins, install_enums},
        library::install_library,
        promise::{Promise, await_js},
        value::{Js, Thrown, arg, from_typed, to_json, to_typed},
    },
    runtime::{ExchangeError, ExchangeFuture},
};
use std::rc::{Rc, Weak};

fn failure(message: impl ToString) -> Thrown {
    Thrown::error("Error", &message.to_string())
}
fn coercion(error: safe_cex_num::JsCoercionError) -> Thrown {
    Thrown::error(error.name, error.message)
}
fn spawn(rt: &Rt, body: safe_cex_exchange::js_runtime::promise::JsFuture<'static>) -> Js {
    Js::Promise(Promise::spawn(&rt.exchange_runtime(), body))
}
fn bytes(value: &Js) -> Result<Vec<u8>, Thrown> {
    let values = value
        .array_values()
        .ok_or_else(|| Thrown::type_error("expected a byte array"))?;
    values
        .iter()
        .map(|value| {
            let number = value.to_number()?;
            if !number.is_finite() || number.fract() != 0.0 || !(0.0..=255.0).contains(&number) {
                return Err(Thrown::type_error("invalid byte"));
            }
            Ok(number as u8)
        })
        .collect()
}
fn hex_bytes(value: &Js) -> Result<Vec<u8>, Thrown> {
    let text = text(value)?;
    let hex = text
        .strip_prefix("0x")
        .ok_or_else(|| Thrown::type_error("invalid BytesLike value"))?;
    if !hex.len().is_multiple_of(2) {
        return Err(Thrown::type_error("invalid BytesLike value"));
    }
    hex.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(failure)?;
            u8::from_str_radix(pair, 16).map_err(failure)
        })
        .collect()
}

pub(super) fn build(weak: Weak<Shared>) -> Js {
    let g = Js::object(vec![]);
    install_builtins(&g, weak.clone());
    install_enums(&g);
    install_library(&g, weak.clone());
    let bind = |name: &'static str, function: fn(&Rt, Vec<Js>) -> Result<Js, Thrown>| {
        let weak = weak.clone();
        let _ = g.define(
            name,
            Js::function(name, move |_, args| {
                function(&scope(&weak, Js::Undefined)?, args)
            }),
        );
    };
    for (name, severity) in [("Info", "info"), ("Warning", "warning"), ("Error", "error")] {
        let _ = g.define(name, Js::str(severity));
    }
    let endpoints = Js::object(vec![]);
    for (key, value) in crate::tables::ASTERDEX_ENDPOINTS {
        let mut parts = key.split('.');
        let first = parts.next().unwrap_or_default();
        if !endpoints.has_own(first) {
            let _ = endpoints.define(first, Js::object(vec![]));
        }
        if let Some(second) = parts.next() {
            let _ = endpoints
                .get(first)
                .and_then(|section| section.define(second, Js::str(value)));
        }
    }
    let _ = g.define("ASTERDEX_ENDPOINTS", endpoints);
    bind("safeCexAxiosCreate", |rt, a| {
        let options = arg(&a, 1);
        let typed = if options.is_undefined() {
            rt.base().options()
        } else {
            safe_cex_exchange::js_runtime::base_exchange::exchange_options(&options)?
        };
        client::factory::create(
            (rt.shared.extra.deps.http)(&typed).map_err(error)?,
            &*rt.shared.runtime.transport(),
            arg(&a, 0),
            &options,
            rt.base(),
        )
    });
    bind("axios.create", |rt, a| {
        // Bare axios bypasses safeCexAxiosCreate's hosted signing in the source.
        let settings = arg(&a, 0).spread_object()?;
        settings.define("safeCexSkipRemoteSigning", Js::Bool(true))?;
        Ok(client::create(rt.shared.http.clone(), settings))
    });
    bind("installTransportAdapter", |rt, a| {
        client::factory::install_transport_adapter(
            rt.shared.http.clone(),
            &*rt.shared.runtime.transport(),
            &arg(&a, 0),
            &arg(&a, 1),
            to_typed(&arg(&a, 2))?,
            safe_cex_transport::RestAdapterVariant::Default,
        )?;
        Ok(arg(&a, 0))
    });
    bind("axiosRateLimit", |_, a| {
        let xhr = arg(&a, 0);
        xhr.call_method("__configureRateLimit", vec![arg(&a, 1)])?;
        Ok(xhr)
    });
    bind("axiosRetry", |_, a| {
        let xhr = arg(&a, 0);
        xhr.call_method("__configureRetry", vec![arg(&a, 1)])?;
        Ok(Js::Undefined)
    });
    bind("axiosRetry.isNetworkOrIdempotentRequestError", |_, a| {
        let thrown = arg(&a, 0);
        let config = thrown.get("config")?;
        let response = thrown.get("response")?;
        let request = safe_cex_transport::RestRequest {
            method: text(&config.get("method")?)?,
            url: text(&config.get("url")?)?,
            ..Default::default()
        };
        let native_response = if response.is_nullish() {
            None
        } else {
            Some(Box::new(safe_cex_transport::RestResponse {
                status: response.get("status")?.to_number()? as u16,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Untransformed(
                    safe_cex_exchange::js_runtime::value::to_retained_input(
                        &response.get("data")?,
                    )?,
                ),
            }))
        };
        let code = thrown.get("code")?;
        let failure = safe_cex_transport::RestFailure {
            code: if code.is_nullish() {
                None
            } else {
                Some(text(&code)?)
            },
            message: text(&thrown.get("message")?)?,
            transport_error: None,
            response: native_response,
            request: Box::new(request),
            context: None,
        };
        Ok(Js::Bool(
            safe_cex_transport::retry::is_network_or_idempotent_request_error(&failure),
        ))
    });
    bind("onlyReads", |_, a| {
        let predicate = arg(&a, 0);
        let read = arg(&a, 1);
        Ok(Js::function("onlyReads", move |_, a| {
            let error = arg(&a, 0);
            let config = error.get("config")?;
            if config.is_nullish() || !read.call(Js::Undefined, vec![config])?.truthy() {
                return Ok(Js::Bool(false));
            }
            predicate.call(Js::Undefined, vec![error])
        }))
    });
    bind("isAsterdexRead", |_, a| {
        let config = arg(&a, 0);
        Ok(Js::Bool(api::is_asterdex_read(
            &safe_cex_transport::RestRequest {
                method: text(&config.get("method")?)?,
                url: text(&config.get("url")?)?,
                ..Default::default()
            },
        )))
    });
    bind("createApiWithSecondaryKey", |_, a| {
        let exchange = arg(&a, 1);
        let xhr = exchange.call_method(
            "createXhrWithSecondaryKey",
            vec![arg(&a, 0), arg(&a, 2), Js::Undefined],
        )?;
        Ok(Js::array(vec![
            xhr,
            exchange.get("_primaryXhr")?,
            exchange.get("_secondaryXhr")?,
        ]))
    });
    bind("iteratorFirst", |_, a| arg(&a, 0).get("0"));
    bind("resolveAsterDEXApiVersion", |_, a| {
        from_typed(&helpers::resolve_aster_dex_api_version(
            &safe_cex_exchange::js_runtime::base_exchange::exchange_options(&arg(&a, 0))?,
        ))
    });
    bind("resolveAsterDEXV3AuthContext", |_, a| {
        from_typed(&helpers::resolve_aster_dex_v3_auth_context(
            &safe_cex_exchange::js_runtime::base_exchange::exchange_options(&arg(&a, 0))?,
        ))
    });
    bind("isAsterDEXV3PrivateKey", |_, a| {
        Ok(Js::Bool(helpers::is_aster_dex_v3_private_key(Some(&text(
            &arg(&a, 0),
        )?))))
    });
    bind("stripQueryString", |_, a| {
        Ok(Js::str(api::strip_query_string(&text(&arg(&a, 0))?)))
    });
    bind("isPublicEndpoint", |_, a| {
        Ok(Js::Bool(api::is_public_endpoint(Some(&text(&arg(&a, 0))?))))
    });
    bind("isAsterDEXCodeSignedEndpoint", |_, a| {
        Ok(Js::Bool(api::is_aster_dex_code_signed_endpoint(
            Some(&text(&arg(&a, 0))?),
            Some(&text(&arg(&a, 1))?),
        )))
    });
    bind("getAsterDEXAsterChain", |_, a| {
        Ok(Js::str(api::get_aster_dex_aster_chain(arg(&a, 0).truthy())))
    });
    bind("buildV3Nonce", |rt, a| {
        let clock = rt
            .shared
            .extra
            .clock
            .get()
            .expect("clock initialized by base constructor");
        api::build_v3_nonce(&text(&arg(&a, 0))?, clock.get_current_time_ms())
            .map(|n| Js::str(&n))
            .map_err(failure)
    });
    bind("buildQueryString", |_, a| {
        api::build_query_string(&to_typed(&arg(&a, 0))?)
            .map(|query| Js::str(&query))
            .map_err(coercion)
    });
    bind("buildAsterDEXCodeQueryString", |_, a| {
        api::build_aster_dex_code_query_string(&to_typed(&arg(&a, 0))?)
            .map(|query| Js::str(&query))
            .map_err(coercion)
    });
    bind("buildV3SigningPayload", |_, a| {
        from_typed(&api::build_v3_signing_payload(&to_typed(&arg(&a, 0))?).map_err(coercion)?)
    });
    bind("withoutSigningFields", |_, a| {
        let data = arg(&a, 0);
        let result = Js::object(vec![]);
        if data.truthy() && data.type_of() == "object" {
            for (key, value) in data.own_entries()? {
                if ![
                    "signature",
                    "timestamp",
                    "recvWindow",
                    "user",
                    "signer",
                    "nonce",
                ]
                .iter()
                .any(|field| key.as_ref() == field.encode_utf16().collect::<Vec<_>>().as_slice())
                {
                    result.define_key(&key, value)?;
                }
            }
        }
        Ok(result)
    });
    bind("hasRetriedSignature", |_, a| {
        let config = arg(&a, 0);
        Ok(Js::Bool(
            !config.is_nullish()
                && config
                    .get(safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY)?
                    .truthy(),
        ))
    });
    bind("isBinanceStyleSignatureRejection", |_, a| {
        Ok(Js::Bool(
            safe_cex_transport::signature_rejection::is_binance_style_signature_rejection(
                &safe_cex_exchange::js_runtime::value::to_retained_input(&arg(&a, 0))?,
            )
            .map_err(coercion)?,
        ))
    });
    bind("generateV1Signature", |_, a| {
        let query = text(&arg(&a, 0))?;
        let key = text(&arg(&a, 1))?;
        Ok(Js::str(&safe_cex_crypto::hmac_sha256(
            key.as_bytes(),
            query.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        )))
    });
    bind("generateAsterDEXCodeMessageSignature", |_, a| {
        api::generate_aster_dex_code_message_signature(
            &text(&arg(&a, 0))?,
            &text(&arg(&a, 1))?,
            Some(arg(&a, 2).truthy()),
        )
        .map(|sig| Js::str(&sig))
        .map_err(error)
    });
    bind("ethers.getAddress", |_, a| {
        safe_cex_crypto::ethereum::get_address(&text(&arg(&a, 0))?)
            .map(|value| Js::str(&value))
            .map_err(failure)
    });
    bind("ethers.AbiCoder.encode", |_, a| {
        let values = arg(&a, 1);
        let nonce = num_bigint::BigUint::parse_bytes(text(&values.get("3")?)?.as_bytes(), 10)
            .ok_or_else(|| failure("invalid uint256 nonce"))?
            .to_bytes_be();
        if nonce.len() > 32 {
            return Err(failure("nonce exceeds uint256"));
        }
        let mut padded = [0; 32];
        padded[32 - nonce.len()..].copy_from_slice(&nonce);
        let encoded = safe_cex_crypto::ethereum::encode_string_address_address_uint256(
            &text(&values.get("0")?)?,
            &text(&values.get("1")?)?,
            &text(&values.get("2")?)?,
            &padded,
        )
        .map_err(failure)?;
        from_typed(&encoded)
    });
    bind("ethers.keccak256", |_, a| {
        from_typed(&safe_cex_crypto::ethereum::keccak256(&bytes(&arg(&a, 0))?))
    });
    bind("ethers.getBytes", |_, a| {
        if arg(&a, 0).is_string() {
            from_typed(&hex_bytes(&arg(&a, 0))?)
        } else {
            Ok(arg(&a, 0))
        }
    });
    bind("ethers.hashMessage", |_, a| {
        from_typed(&safe_cex_crypto::ethereum::ethereum_message_hash(&bytes(
            &arg(&a, 0),
        )?))
    });
    bind("new ethers.SigningKey", |_, a| {
        let key = text(&arg(&a, 0))?;
        Ok(Js::object(vec![(
            "sign",
            Js::function("sign", move |_, a| {
                let signature = safe_cex_crypto::ethereum::sign_digest(&bytes(&arg(&a, 0))?, &key)
                    .map_err(failure)?;
                Ok(Js::object(vec![("serialized", Js::str(&signature))]))
            }),
        )]))
    });
    bind("ethers.Signature.from", |_, a| Ok(arg(&a, 0)));
    bind("getAsterDEXErrorDetail", |_, a| {
        api::get_aster_dex_error_detail(Some(&to_json(&arg(&a, 0))?), Some(&to_json(&arg(&a, 1))?))
            .map(|message| Js::str(&message))
            .map_err(coercion)
    });
    bind("shouldSuppressUnknownOrderError", |_, a| {
        let optional = |i| -> Result<Option<serde_json::Value>, Thrown> {
            let item = arg(&a, i);
            if item.is_undefined() {
                Ok(None)
            } else {
                to_json(&item).map(Some)
            }
        };
        Ok(Js::Bool(
            api::should_suppress_unknown_order_error(
                optional(0)?.as_ref(),
                optional(1)?.as_ref(),
                optional(2)?.as_ref(),
            )
            .map_err(coercion)?,
        ))
    });
    bind("appendHttpFailure", |_, a| {
        Ok(Js::str(
            &safe_cex_exchange::error_reporting::append_http_failure(
                Some(&to_json(&arg(&a, 0))?),
                &to_json(&arg(&a, 1))?,
            ),
        ))
    });
    let clock = Js::object(vec![]);
    for name in [
        "start",
        "stop",
        "syncClientServerTime",
        "needsSync",
        "getCurrentTimeMs",
        "getTimeOffset",
    ] {
        let weak = weak.clone();
        let _ = clock.define(
            name,
            Js::function(name, move |_, a| {
                let rt = scope(&weak, Js::Undefined)?;
                let clock = rt
                    .shared
                    .extra
                    .clock
                    .get()
                    .expect("base constructs clock")
                    .clone();
                Ok(match name {
                    "start" => {
                        let op = clock.start(arg(&a, 0).truthy(), to_typed(&arg(&a, 1))?);
                        spawn(
                            &rt,
                            Box::pin(async move {
                                op.await.map_err(error)?;
                                Ok(Js::Undefined)
                            }),
                        )
                    }
                    "stop" => {
                        clock.stop();
                        Js::Undefined
                    }
                    "syncClientServerTime" => {
                        let version = arg(&a, 1);
                        let version = if version.is_undefined() {
                            None
                        } else {
                            Some(to_typed(&version)?)
                        };
                        let op = clock.sync_client_server_time(arg(&a, 0).truthy(), version);
                        spawn(
                            &rt,
                            Box::pin(async move {
                                op.await.map_err(error)?;
                                Ok(Js::Undefined)
                            }),
                        )
                    }
                    "needsSync" => Js::Bool(clock.needs_sync()),
                    "getTimeOffset" => Js::Number(clock.get_time_offset()),
                    _ => Js::Number(clock.get_current_time_ms()),
                })
            }),
        );
    }
    let _ = g.define("asterDEXVirtualClock", clock);
    g
}

struct ClockHttp {
    http: Rc<dyn client::JsHttpHost>,
    runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
}
impl crate::virtual_clock::ClockHttp for ClockHttp {
    fn get(&self, url: String, timeout: u64) -> ExchangeFuture<'static, serde_json::Value> {
        let pending = self.http.send(Js::object(vec![
            ("method", Js::str("get")),
            ("url", Js::str(&url)),
            ("timeout", Js::Number(timeout as f64)),
            ("safeCexSkipRemoteSigning", Js::Bool(true)),
        ]));
        let runtime = self.http.runtime();
        let promise = Promise::spawn(&runtime, pending);
        Box::pin(async move {
            Ok(to_json(
                &await_js(&runtime, Js::Promise(promise))
                    .await?
                    .get("data")?,
            )?)
        })
    }
    fn with_public_cors_proxy(&self, url: &str) -> Result<String, ExchangeError> {
        Ok(safe_cex_transport::TransportRegistry::default()
            .with_public_cors_proxy(url, Some(&self.runtime.transport())))
    }
}
pub(super) fn initialize_clock(rt: &Rt) -> Result<(), Thrown> {
    let clock = crate::virtual_clock::AsterDEXVirtualClock::new(
        rt.base().clone(),
        Rc::new(ClockHttp {
            http: rt.shared.http.clone(),
            runtime: rt.shared.runtime.clone(),
        }),
    );
    let _ = rt.shared.extra.clock.set(clock);
    Ok(())
}

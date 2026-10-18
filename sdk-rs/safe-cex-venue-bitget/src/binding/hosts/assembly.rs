//! Concrete AssemblyHost plus the construction-time globals: BaseExchange,
//! BaseWebSocket, the Axios client and createAPI's imported collaborators.
use crate::assembly::{AssemblyHost, NativeCallback};
use crate::binding::{
    client,
    globals::{arg, shared},
    promise::Promise,
    rt::{Rt, Shared},
    sockets,
    value::{Js, Thrown, error_object, utf16},
};
use safe_cex_exchange::js_runtime::base_exchange::{self, BaseExchangeParts};
use std::rc::{Rc, Weak};

impl AssemblyHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn string(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn property(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        v.get(k)
    }
    fn define(&self, v: &Js, k: &str, field: Js) -> Result<(), Thrown> {
        v.define(k, field)
    }
    fn assign(&self, v: &Js, k: &str, field: Js) -> Result<(), Thrown> {
        v.set(k, field)
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn global(&self, k: &str) -> Result<Js, Thrown> {
        Rt::global(self, k)
    }
    fn member(&self, v: &Js, k: &str) -> Result<Js, Thrown> {
        bound_member(v, k)
    }
    fn invoke(&self, f: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        f.call(Js::Undefined, args)
    }
    fn construct(&self, f: Js, args: Vec<Js>) -> Result<Js, Thrown> {
        f.construct(args, None)
    }
    fn function(&self, body: NativeCallback<Js, Thrown>) -> Js {
        Js::function("", move |_, a| body(a))
    }
    fn async_function(&self, body: NativeCallback<Js, Thrown>) -> Js {
        let runtime = self.exchange_runtime();
        Js::function("", move |_, a| {
            Ok(match body(a) {
                Ok(p @ Js::Promise(_)) => p,
                Ok(v) => Js::Promise(Promise::settled(&runtime, Ok(v))),
                Err(e) => Js::Promise(Promise::settled(&runtime, Err(e))),
            })
        })
    }
    fn spread(&self, v: &Js) -> Result<Js, Thrown> {
        v.spread_object()
    }
    fn bind_mapped(
        &self,
        receiver: &Js,
        owner: &str,
        name: &str,
        targets: &[&str],
    ) -> Result<Js, Thrown> {
        crate::binding::targets::bind(&self.with_this(receiver.clone()), owner, name, targets)
    }
}

pub(crate) use safe_cex_exchange::js_runtime::scope::bound_member;

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// lodash snakeCase/camelCase lookups of get-key `v`.
fn get_key(obj: &Js, key: &str) -> Result<Js, Thrown> {
    if obj.is_nullish() {
        return Ok(Js::Undefined);
    }
    if !obj.is_object() {
        return Err(Thrown::type_error(&format!(
            "Cannot use 'in' operator to search for '{key}' in {}",
            obj.to_display()
        )));
    }
    fn contains(value: &Js, key: &str) -> bool {
        if value.has_own(key) {
            return true;
        }
        match value {
            Js::Object(data) => data
                .proto
                .borrow()
                .as_ref()
                .is_some_and(|p| contains(p, key)),
            Js::Array(_) => {
                key == "length" || crate::binding::builtins::member(value, &utf16(key)).is_some()
            }
            Js::Function(_) => {
                key == "name" || crate::binding::builtins::member(value, &utf16(key)).is_some()
            }
            _ => crate::binding::builtins::member(value, &utf16(key)).is_some(),
        }
    }
    for candidate in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if contains(obj, &candidate) {
            return obj.get(&candidate);
        }
    }
    Ok(Js::Undefined)
}

/// qs.stringify: bracket keys, skips undefined, null -> `k=`, RFC3986 encoding.
fn qs_stringify(value: &Js, encode: bool) -> Result<String, Thrown> {
    fn enc(s: &str, encode: bool) -> String {
        if !encode {
            return s.to_owned();
        }
        let mut out = String::new();
        for b in s.as_bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    out.push(*b as char)
                }
                _ => out.push_str(&format!("%{b:02X}")),
            }
        }
        out
    }
    fn walk(prefix: &str, value: &Js, encode: bool, out: &mut Vec<String>) -> Result<(), Thrown> {
        match value {
            Js::Undefined | Js::Function(_) => {}
            Js::Null => out.push(format!("{}=", enc(prefix, encode))),
            Js::Array(_) => {
                for (i, v) in value.array_values().unwrap_or_default().iter().enumerate() {
                    walk(&format!("{prefix}[{i}]"), v, encode, out)?;
                }
            }
            Js::Object(_) => {
                for (k, v) in value.own_entries()? {
                    walk(
                        &format!("{prefix}[{}]", String::from_utf16_lossy(&k)),
                        &v,
                        encode,
                        out,
                    )?;
                }
            }
            other => out.push(format!(
                "{}={}",
                enc(prefix, encode),
                enc(&text(other)?, encode)
            )),
        }
        Ok(())
    }
    let mut out = vec![];
    if value.is_object() {
        for (k, v) in value.own_entries()? {
            walk(&String::from_utf16_lossy(&k), &v, encode, &mut out)?;
        }
    }
    Ok(out.join("&"))
}

pub(crate) fn to_js_json(value: &Js) -> Result<safe_cex_num::JsJsonValue, Thrown> {
    use safe_cex_num::JsJsonValue as J;
    Ok(match value {
        Js::Undefined | Js::Function(_) => J::Undefined,
        Js::Null => J::Null,
        Js::Bool(b) => J::Bool(*b),
        Js::Number(n) => J::Number(*n),
        Js::String(s) => J::String(String::from_utf16_lossy(s)),
        Js::Array(_) => J::Array(
            value
                .array_values()
                .unwrap_or_default()
                .iter()
                .map(to_js_json)
                .collect::<Result<_, _>>()?,
        ),
        Js::Object(_) => J::Object(
            value
                .own_entries()?
                .into_iter()
                .map(|(k, v)| Ok((String::from_utf16_lossy(&k), to_js_json(&v)?)))
                .collect::<Result<_, Thrown>>()?,
        ),
        _ => J::Object(vec![]),
    })
}

fn client_order_id_valid(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '/' | '_' | '-'))
}

fn is_bitget_signature_rejection(data: &Js) -> Result<bool, Thrown> {
    let code = if data.is_nullish() {
        Js::Undefined
    } else {
        data.get("code")?
    };
    if text(&code)? == "40009" {
        return Ok(true);
    }
    let msg = if data.is_nullish() {
        Js::Undefined
    } else {
        data.get("msg")?
    };
    let msg = if msg.is_nullish() {
        String::new()
    } else {
        text(&msg)?
    };
    Ok(msg.to_lowercase().contains("sign signature error"))
}

/// The BaseExchange instance; the Rust core is kept for `Exchange::base()`.
fn base_exchange(rt: &Rt, name: Js, options: Js, store: Js) -> Result<Js, Thrown> {
    let extra = &rt.shared.extra;
    let parts = BaseExchangeParts {
        name: extra.name.clone(),
        options: extra.options.clone(),
        store: rt.shared.store.clone(),
        settings: crate::binding::exchange::settings(),
        external_public_data: extra.external_public_data.clone(),
    };
    let (this, base) =
        base_exchange::create(&rt.shared.sockets, parts, name, options, store, |_| Ok(()))?;
    let _ = rt.shared.base.set(base);
    Ok(this)
}

/// The secondary ticker sockets are already a typed Rust class; the JS facade
/// creates it once the venue exists and forwards the members the exchange calls.
fn secondary_socket(rt: &Rt, index: f64) -> Js {
    let socket = Js::object_with_class(
        "BitgetSecondaryPublicWebsocket",
        vec![("index", Js::Number(index))],
    );
    let cell: Rc<std::cell::OnceCell<Rc<crate::secondary_public::BitgetSecondaryPublicWebsocket>>> =
        Rc::default();
    let typed = {
        let rt = rt.clone();
        let cell = cell.clone();
        move || -> Result<Rc<crate::secondary_public::BitgetSecondaryPublicWebsocket>, Thrown> {
            if let Some(s) = cell.get() {
                return Ok(s.clone());
            }
            let venue = rt
                .shared
                .sockets
                .venue
                .borrow()
                .as_ref()
                .and_then(Weak::upgrade)
                .ok_or_else(|| Thrown::error("Error", "Bitget exchange is not assembled"))?;
            let s = crate::secondary_public::BitgetSecondaryPublicWebsocket::new(
                1000 + index as u64,
                &venue,
                rt.shared.runtime.clone(),
                index,
            );
            let _ = cell.set(s.clone());
            Ok(s)
        }
    };
    let typed = Rc::new(typed);
    {
        let typed = typed.clone();
        let _ = socket.define(
            "connectAndSubscribe",
            Js::function("connectAndSubscribe", move |_, _| {
                typed()?.connect_and_subscribe()?;
                Ok(Js::Undefined)
            }),
        );
    }
    {
        let cell = cell.clone();
        let _ = socket.define(
            "dispose",
            Js::function("dispose", move |_, _| {
                if let Some(s) = cell.get() {
                    s.base().dispose();
                }
                Ok(Js::Undefined)
            }),
        );
    }
    socket
        .define_accessor(
            "isConnected",
            Some(Rc::new(move |_| {
                Ok(Js::Bool(
                    cell.get().is_some_and(|s| s.base().is_connected()),
                ))
            })),
            None,
        )
        .expect("fresh extensible accessor target");
    socket
}

pub(crate) fn register(g: &Js, weak: Weak<Shared>) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    let w = weak.clone();
    def(
        "BaseExchange",
        Js::constructor("BaseExchange", None, move |_, a| {
            base_exchange(&shared(&w)?, arg(&a, 0), arg(&a, 1), arg(&a, 2))
        }),
    );
    let w = weak.clone();
    def(
        "BaseWebSocket",
        Js::constructor("BaseWebSocket", None, move |_, a| {
            Ok(sockets::base_websocket(
                &shared(&w)?.shared.sockets,
                arg(&a, 0),
            ))
        }),
    );
    let w = weak.clone();
    def(
        "BitgetSecondaryPublicWebsocket",
        Js::constructor("BitgetSecondaryPublicWebsocket", None, move |_, a| {
            Ok(secondary_socket(&shared(&w)?, arg(&a, 1).to_number()?))
        }),
    );
    let w = weak.clone();
    def(
        "safeCexAxiosCreate",
        Js::function("safeCexAxiosCreate", move |_, a| {
            let rt = shared(&w)?;
            client::factory::create(
                rt.shared.http.clone(),
                rt.shared.runtime.transport().as_ref(),
                arg(&a, 0),
                &arg(&a, 1),
                rt.base(),
            )
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
        "networkErrorOnRead",
        Js::function("networkErrorOnRead", |_, _| {
            Ok(Js::function("onlyReads", |_, a| {
                let error = arg(&a, 0);
                let config = error.get("config")?;
                if config.is_undefined() {
                    return Ok(Js::Bool(false));
                }
                let method = config.get("method")?;
                let method = if method.is_nullish() {
                    String::new()
                } else {
                    text(&method)?.to_lowercase()
                };
                if method != "get" && method != "head" {
                    return Ok(Js::Bool(false));
                }
                if error.get("response")?.truthy() {
                    return Ok(Js::Bool(false));
                }
                let code = error.get("code")?;
                if !code.truthy() {
                    return Ok(Js::Bool(false));
                }
                // axios-retry compares codes strictly; non-string codes never
                // match its string deny list and must not be string-coerced.
                let Js::String(code) = code else {
                    return Ok(Js::Bool(true));
                };
                let mut failure = safe_cex_transport::RestFailure::pre_transmit(
                    String::new(),
                    Default::default(),
                );
                failure.code = Some(String::from_utf16_lossy(&code));
                Ok(Js::Bool(safe_cex_transport::retry::is_network_error(
                    &failure,
                )))
            }))
        }),
    );
    // Keep the shared request stamp on the live JS config across both retry
    // paths; the shared client owns replay, timeout and request interceptors.
    def(
        "signatureRejectedResponse",
        Js::function("signatureRejectedResponse", |_, a| {
            let predicate = arg(&a, 0);
            Ok(Js::function("", move |_, a| {
                use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY as KEY;
                let error = arg(&a, 0);
                let config = error.get("config")?;
                if !config.truthy() {
                    return Ok(Js::Bool(false));
                }
                let response = error.get("response")?;
                if !response.truthy() || config.get(KEY)?.truthy() {
                    return Ok(Js::Bool(false));
                }
                if !predicate
                    .call(Js::Undefined, vec![response.get("data")?])?
                    .truthy()
                {
                    return Ok(Js::Bool(false));
                }
                config.set(KEY, Js::Bool(true))?;
                Ok(Js::Bool(true))
            }))
        }),
    );
    def(
        "resendOnSignatureRejection",
        Js::function("resendOnSignatureRejection", |_, a| {
            use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY as KEY;
            let xhr = arg(&a, 0);
            let response = arg(&a, 1);
            let predicate = arg(&a, 2);
            let config = response.get("config")?;
            if (!config.is_nullish() && config.get(KEY)?.truthy())
                || !predicate
                    .call(Js::Undefined, vec![response.get("data")?])?
                    .truthy()
            {
                return Ok(Js::Undefined);
            }
            let stamped = config.spread_object()?;
            stamped.define(KEY, Js::Bool(true))?;
            xhr.call_method("request", vec![stamped])
        }),
    );
    def(
        "isBitgetSignatureRejection",
        Js::function("isBitgetSignatureRejection", |_, a| {
            Ok(Js::Bool(is_bitget_signature_rejection(&arg(&a, 0))?))
        }),
    );
    let w = weak.clone();
    def(
        "installTransportAdapter",
        Js::function("installTransportAdapter", move |_, a| {
            let rt = shared(&w)?;
            client::factory::install_transport_adapter(
                rt.shared.http.clone(),
                rt.shared.runtime.transport().as_ref(),
                &arg(&a, 0),
                &arg(&a, 1),
                crate::binding::value::to_typed(&arg(&a, 2))?,
                safe_cex_transport::RestAdapterVariant::Default,
            )?;
            Ok(Js::Undefined)
        }),
    );
    def(
        "qs.stringify",
        Js::function("stringify", |_, a| {
            let opts = arg(&a, 1);
            let encode = opts.is_nullish()
                || opts.get("encode")?.is_undefined()
                || opts.get("encode")?.truthy();
            Ok(Js::str(&qs_stringify(&arg(&a, 0), encode)?))
        }),
    );
    def(
        "signingQueryTemplate",
        Js::function("signingQueryTemplate", |_, a| {
            Ok(Js::str(&format!(
                "{}?{}",
                text(&arg(&a, 0))?,
                text(&arg(&a, 1))?
            )))
        }),
    );
    def(
        "signingJoin",
        Js::function("signingJoin", |_, a| {
            let mut out = String::new();
            for v in a {
                if !v.is_nullish() {
                    out.push_str(&text(&v)?);
                }
            }
            Ok(Js::str(&out))
        }),
    );
    def(
        "toJsonString",
        Js::function("toJsonString", |_, a| {
            Ok(
                match crate::api::to_json_string(&to_js_json(&arg(&a, 0))?)? {
                    Some(s) => Js::str(&s),
                    None => Js::Undefined,
                },
            )
        }),
    );
    def(
        "createHmac",
        Js::function("createHmac", |_, a| {
            let secret = text(&arg(&a, 1))?;
            Ok(Js::object(vec![(
                "update",
                Js::function("update", move |_, u| {
                    let digest = safe_cex_crypto::hmac_sha256(
                        secret.as_bytes(),
                        text(&arg(&u, 0))?.as_bytes(),
                        safe_cex_crypto::Encoding::Base64,
                    );
                    Ok(Js::object(vec![(
                        "digest",
                        Js::function("digest", move |_, d| {
                            // digest('base64') returns the string; digest() a Buffer-like.
                            if !arg(&d, 0).is_undefined() {
                                return Ok(Js::str(&digest));
                            }
                            let digest = digest.clone();
                            Ok(Js::object(vec![(
                                "toString",
                                Js::function("toString", move |_, _| Ok(Js::str(&digest))),
                            )]))
                        }),
                    )]))
                }),
            )]))
        }),
    );
    def(
        "AxiosHeaders",
        Js::constructor("AxiosHeaders", None, |_, a| arg(&a, 0).spread_object()),
    );
    def(
        "v",
        Js::function("v", |_, a| get_key(&arg(&a, 0), &text(&arg(&a, 1))?)),
    );
    def(
        "appendHttpFailure",
        Js::function("appendHttpFailure", |_, a| {
            let message = arg(&a, 0);
            let text_message = if message.is_string() {
                text(&message)?
            } else if message.is_nullish() {
                String::new()
            } else {
                text(&message)?
            };
            let response = arg(&a, 1);
            let status = if response.is_object() && !response.is_array() {
                response.get("status")?
            } else {
                Js::Undefined
            };
            Ok(Js::str(&match status.as_f64() {
                Some(status) if status >= 400.0 => {
                    let tag = format!("HTTP {}", safe_cex_num::js_number_string(status));
                    if text_message.contains(&tag) {
                        text_message
                    } else {
                        format!("{text_message} ({tag})")
                    }
                }
                _ => text_message,
            }))
        }),
    );
    def(
        "shouldSuppressValidationErrorEmission",
        Js::function("shouldSuppressValidationErrorEmission", |_, a| {
            Ok(Js::Bool(
                arg(&a, 0)
                    .get("suppressBitgetValidationErrorEmission")?
                    .strict_equals(&Js::Bool(true)),
            ))
        }),
    );
    def(
        "shouldSilenceBitgetResponseError",
        Js::function("shouldSilenceBitgetResponseError", |_, a| {
            let message = crate::binding::value::to_json(&arg(&a, 1)).ok();
            let url = arg(&a, 2);
            let url = if url.is_nullish() {
                None
            } else {
                Some(text(&url)?)
            };
            Ok(Js::Bool(crate::api::should_silence_bitget_response_error(
                &text(&arg(&a, 0))?,
                message.as_ref(),
                url.as_deref(),
            )))
        }),
    );
    def(
        "patternClientOrderIdRule",
        Js::function("patternClientOrderIdRule", |_, a| {
            let label = arg(&a, 0);
            let pattern = text(&arg(&a, 1))?;
            Ok(Js::object(vec![
                ("label", label),
                ("requirement", Js::str(&format!("match /{pattern}/"))),
                (
                    "isValid",
                    Js::function("isValid", |_, v| {
                        Ok(Js::Bool(client_order_id_valid(&text(&arg(&v, 0))?)))
                    }),
                ),
            ]))
        }),
    );
    def(
        "typeof",
        Js::function("typeof", |_, a| Ok(Js::str(arg(&a, 0).type_of()))),
    );
    def(
        "WeakSet",
        Js::constructor("WeakSet", None, |_, _| Ok(Js::new_set())),
    );
    let _ = (error_object, utf16);
}

pub(crate) fn client_order_id_rule() -> safe_cex_exchange::client_order_id::ClientOrderIdRule {
    safe_cex_exchange::client_order_id::pattern_client_order_id_rule(
        "Bitget".into(),
        "/^[.A-Z:/a-z0-9_-]{1,32}$/".into(),
        Rc::new(client_order_id_valid),
        Default::default(),
    )
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    use safe_cex_exchange::js_runtime::value::from_json;
    use serde_json::{Value, json};

    #[test]
    fn venue_signature_predicate_matches_actual_source_values_and_getters() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/signature-rejection-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        let mut cases = vec![
            ("undefined", Js::Undefined),
            ("null", Js::Null),
            ("bool", Js::Bool(true)),
            ("number", Js::Number(40009.0)),
            ("empty", Js::object(vec![])),
            ("numeric-code", from_json(&json!({"code":40009}))),
            ("string-code", from_json(&json!({"code":"40009"}))),
            ("array-code", from_json(&json!({"code":[40009]}))),
            (
                "bigint-code",
                Js::object(vec![("code", Js::BigInt(Rc::new(40009.into())))]),
            ),
            ("other-code", from_json(&json!({"code":"400090"}))),
            (
                "message",
                from_json(&json!({"msg":"prefix SIGN SIGNATURE ERROR suffix"})),
            ),
            ("numeric-message", from_json(&json!({"msg":40009}))),
            ("null-message", from_json(&json!({"msg":null}))),
            (
                "array-message",
                from_json(&json!({"msg":["sign signature error"]})),
            ),
            ("long-s", from_json(&json!({"msg":"ſign signature error"}))),
        ];
        for (label, left, right) in [
            ("surrogate-positive", "", "sign signature error"),
            ("surrogate-break", "sign signa", "ture error"),
        ] {
            let units: Vec<_> = left
                .encode_utf16()
                .chain([0xd800])
                .chain(right.encode_utf16())
                .collect();
            cases.push((label, Js::object(vec![("msg", Js::utf16(&units))])));
        }
        for (label, field, result) in [
            ("coerced-code", "code", "40009"),
            ("coerced-message", "msg", "sign signature error"),
        ] {
            let value = Js::object(vec![(
                "toString",
                Js::function("toString", move |_, _| Ok(Js::str(result))),
            )]);
            cases.push((label, Js::object(vec![(field, value)])));
        }
        for (label, field, message) in [
            ("short-circuit", "msg", "unreached"),
            ("code-throws", "code", "code refused"),
            ("message-throws", "msg", "message refused"),
        ] {
            let value = Js::object(if label == "short-circuit" {
                vec![("code", Js::str("40009"))]
            } else {
                vec![]
            });
            value
                .define_accessor(
                    field,
                    Some(Rc::new(move |_| Err(Thrown::error("RangeError", message)))),
                    None,
                )
                .expect("fresh extensible accessor target");
            cases.push((label, value));
        }
        assert_eq!(cases.len(), 22);
        let actual: Vec<_> = cases
            .into_iter()
            .map(
                |(label, value)| match is_bitget_signature_rejection(&value) {
                    Ok(result) => json!({"label":label,"result":result}),
                    Err(error) => {
                        json!({"label":label,"error":error.value.get("name").unwrap().to_display()})
                    }
                },
            )
            .collect();
        assert_eq!(actual, expected);
    }
}

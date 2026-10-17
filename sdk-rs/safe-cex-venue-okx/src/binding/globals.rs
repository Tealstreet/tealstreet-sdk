//! OKX module scope: okx.types.ts tables, the imported mappers and createAPI's
//! collaborators, over the shared builtins, enums and utility library.
use super::ops::{OkxShared, runtime_ops};
use crate::{
    closed_pnl, exchange as okx, income, order_history, position_adapter,
    types::{self, OkxRegion},
};
use safe_cex_exchange::js_runtime::{
    client, globals as shared_globals, library,
    scope::scope_of,
    value::{Js, Thrown, arg, from_typed, to_json, to_typed},
};
use safe_cex_types::Market;
use std::rc::{Rc, Weak};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn coercion(e: safe_cex_types::js_coercion::JsCoercionError) -> Thrown {
    Thrown::error(e.name, e.message)
}

fn wire<T: serde::Serialize>(v: T) -> Js {
    from_typed(&v).unwrap_or(Js::Undefined)
}

fn pairs<T: serde::Serialize + Copy>(table: &[(&str, T)]) -> Js {
    Js::object(table.iter().map(|(k, v)| (*k, wire(*v))).collect())
}

fn region_key(region: OkxRegion) -> &'static str {
    match region {
        OkxRegion::Okx => "okx",
        OkxRegion::Okxeu => "okxeu",
    }
}

fn hosts(urls: [&str; 2]) -> Js {
    Js::object(vec![
        ("livenet", Js::str(urls[0])),
        ("testnet", Js::str(urls[1])),
    ])
}

fn record(v: &Js) -> Result<serde_json::Map<String, serde_json::Value>, Thrown> {
    match to_json(v)? {
        serde_json::Value::Object(map) => Ok(map),
        _ => Ok(serde_json::Map::new()),
    }
}

fn market(v: &Js) -> Result<Option<Market>, Thrown> {
    if v.is_nullish() {
        return Ok(None);
    }
    to_typed(v).map(Some)
}

fn number_callback(f: &Js, args: Vec<Js>) -> f64 {
    f.call(Js::Undefined, args)
        .and_then(|v| v.to_number())
        .unwrap_or(f64::NAN)
}

fn optional_text(f: &Js, args: Vec<Js>) -> Option<String> {
    let v = f.call(Js::Undefined, args).ok()?;
    if v.is_nullish() { None } else { text(&v).ok() }
}

// The installed is-retry-allowed Set membership is strict: no code coercion.
// The real-client source witness covers every denial plus allowed socket codes.
fn network_code_denied(code: &Js) -> bool {
    code.as_str().is_some_and(|code| {
        [
            "ENOTFOUND",
            "ENETUNREACH",
            "UNABLE_TO_GET_ISSUER_CERT",
            "UNABLE_TO_GET_CRL",
            "UNABLE_TO_DECRYPT_CERT_SIGNATURE",
            "UNABLE_TO_DECRYPT_CRL_SIGNATURE",
            "UNABLE_TO_DECODE_ISSUER_PUBLIC_KEY",
            "CERT_SIGNATURE_FAILURE",
            "CRL_SIGNATURE_FAILURE",
            "CERT_NOT_YET_VALID",
            "CERT_HAS_EXPIRED",
            "CRL_NOT_YET_VALID",
            "CRL_HAS_EXPIRED",
            "ERROR_IN_CERT_NOT_BEFORE_FIELD",
            "ERROR_IN_CERT_NOT_AFTER_FIELD",
            "ERROR_IN_CRL_LAST_UPDATE_FIELD",
            "ERROR_IN_CRL_NEXT_UPDATE_FIELD",
            "OUT_OF_MEM",
            "DEPTH_ZERO_SELF_SIGNED_CERT",
            "SELF_SIGNED_CERT_IN_CHAIN",
            "UNABLE_TO_GET_ISSUER_CERT_LOCALLY",
            "UNABLE_TO_VERIFY_LEAF_SIGNATURE",
            "CERT_CHAIN_TOO_LONG",
            "CERT_REVOKED",
            "INVALID_CA",
            "PATH_LENGTH_EXCEEDED",
            "INVALID_PURPOSE",
            "CERT_UNTRUSTED",
            "CERT_REJECTED",
            "HOSTNAME_MISMATCH",
        ]
        .iter()
        .any(|denied| code.iter().copied().eq(denied.encode_utf16()))
    })
}

fn network_error_on_read(is_read: Js) -> Js {
    Js::function("onlyReads", move |_, a| {
        let error = arg(&a, 0);
        let config = error.get("config")?;
        if config.is_undefined() {
            return Ok(Js::Bool(false));
        }
        let read = if is_read.is_undefined() {
            let method = config.get("method")?;
            let method = if method.is_nullish() {
                Js::Undefined
            } else {
                method.call_method("toLowerCase", vec![])?
            };
            method.strict_equals(&Js::str("get")) || method.strict_equals(&Js::str("head"))
        } else {
            is_read.call(Js::Undefined, vec![config])?.truthy()
        };
        if !read || error.get("response")?.truthy() || !error.get("code")?.truthy() {
            return Ok(Js::Bool(false));
        }
        let code = error.get("code")?;
        if code.strict_equals(&Js::str("ERR_CANCELED"))
            || code.strict_equals(&Js::str("ECONNABORTED"))
        {
            return Ok(Js::Bool(false));
        }
        // axios-retry delegates its third code read to is-retry-allowed.
        Ok(Js::Bool(!network_code_denied(&error.get("code")?)))
    })
}

fn tables(g: &Js) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    def("RECV_WINDOW", Js::Number(types::RECV_WINDOW as f64));
    def("BROKER_ID", Js::str(types::BROKER_ID));
    def(
        "BASE_URLS",
        Js::object(
            types::BASE_URLS
                .iter()
                .map(|(r, u)| (region_key(*r), hosts(*u)))
                .collect(),
        ),
    );
    def(
        "BASE_WS_URLS",
        Js::object(
            types::BASE_WS_URLS
                .iter()
                .map(|(r, [public, private, business])| {
                    (
                        region_key(*r),
                        Js::object(vec![
                            ("public", hosts(*public)),
                            ("private", hosts(*private)),
                            ("business", hosts(*business)),
                        ]),
                    )
                })
                .collect(),
        ),
    );
    def(
        "ENDPOINTS",
        Js::object(
            types::ENDPOINTS
                .iter()
                .map(|(k, v)| (*k, Js::str(v)))
                .collect(),
        ),
    );
    let list = |xs: &[&str]| Js::array(xs.iter().map(|x| Js::str(x)).collect());
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        list(&types::GRAFANA_ENDPOINTS_EVERY),
    );
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        list(&types::GRAFANA_ENDPOINTS_PERIODIC),
    );
    def(
        "INTERVAL",
        Js::object(
            types::INTERVAL
                .iter()
                .map(|(k, v)| (*k, Js::str(v)))
                .collect(),
        ),
    );
    def("ORDER_STATUS", pairs(&types::ORDER_STATUS));
    def("ORDER_TYPE", pairs(&types::ORDER_TYPE));
    def("ORDER_SIDE", pairs(&types::ORDER_SIDE));
    def("POSITION_SIDE", pairs(&types::POSITION_SIDE));
    def(
        "REVERSE_ORDER_TYPE",
        Js::object(
            types::REVERSE_ORDER_TYPE
                .iter()
                .map(|(k, v)| {
                    let key = from_typed(k)
                        .ok()
                        .and_then(|k| text(&k).ok())
                        .unwrap_or_default();
                    (key, Js::str(v))
                })
                .collect::<Vec<_>>()
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect(),
        ),
    );
    def(
        "OKX_BILL_TYPE_BY_INCOME_TYPE",
        Js::object(
            income::OKX_BILL_TYPE_BY_INCOME_TYPE
                .iter()
                .map(|(k, v)| (*k, Js::str(v)))
                .collect(),
        ),
    );
}

fn mappers(g: &Js) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    let func =
        |name: &str, f: fn(Vec<Js>) -> Result<Js, Thrown>| Js::function(name, move |_, a| f(a));
    def(
        "isPublicEndpoint",
        func("isPublicEndpoint", |a| {
            let url = arg(&a, 0);
            let url = if url.is_nullish() {
                None
            } else {
                Some(text(&url)?)
            };
            Ok(Js::Bool(crate::api::is_public_endpoint(url.as_deref())))
        }),
    );
    def(
        "getOKXTriggerPxType",
        func("getOKXTriggerPxType", |a| {
            let t = arg(&a, 0);
            let t = if t.is_nullish() {
                None
            } else {
                to_typed(&t).ok()
            };
            Ok(Js::str(okx::get_okx_trigger_px_type(t)))
        }),
    );
    def(
        "parseOKXTriggerPxType",
        func("parseOKXTriggerPxType", |a| {
            let t = arg(&a, 0);
            let t = if t.is_nullish() {
                None
            } else {
                Some(text(&t)?)
            };
            from_typed(&okx::parse_okx_trigger_px_type(t.as_deref()))
        }),
    );
    def(
        "findTickerMarkForCurrency",
        func("findTickerMarkForCurrency", |a| {
            let tickers: Vec<okx::TickerMarkInput> = to_typed(&arg(&a, 0))?;
            Ok(Js::Number(okx::find_ticker_mark_for_currency(
                &tickers,
                &text(&arg(&a, 1))?,
            )))
        }),
    );
    def(
        "mapOkxHistoricalOrder",
        func("mapOkxHistoricalOrder", |a| {
            let to_base = arg(&a, 2);
            let js_market = arg(&a, 1);
            let order = order_history::map_okx_historical_order(
                &record(&arg(&a, 0))?,
                market(&js_market)?.as_ref(),
                |contracts, _, price| {
                    number_callback(
                        &to_base,
                        vec![
                            Js::Number(contracts),
                            js_market.clone(),
                            price.map_or(Js::Undefined, Js::Number),
                        ],
                    )
                },
            )
            .map_err(coercion)?;
            order.map_or(Ok(Js::Null), |o| from_typed(&o))
        }),
    );
    def(
        "mapOkxClosedPosition",
        func("mapOkxClosedPosition", |a| {
            let to_base = arg(&a, 2);
            let js_market = arg(&a, 1);
            let entry = closed_pnl::map_okx_closed_position(
                &record(&arg(&a, 0))?,
                market(&js_market)?.as_ref(),
                |contracts, _, price| {
                    number_callback(
                        &to_base,
                        vec![Js::Number(contracts), js_market.clone(), Js::Number(price)],
                    )
                },
                &text(&arg(&a, 3))?,
            )
            .map_err(coercion)?;
            entry.map_or(Ok(Js::Null), |e| from_typed(&e))
        }),
    );
    def(
        "mapOkxBillToIncomeRecords",
        func("mapOkxBillToIncomeRecords", |a| {
            let resolve = arg(&a, 1);
            let records = income::map_okx_bill_to_income_records(&record(&arg(&a, 0))?, |inst| {
                optional_text(&resolve, vec![Js::str(inst)])
            })
            .map_err(coercion)?;
            from_typed(&records)
        }),
    );
    def(
        "mapOkxPositionHistory",
        func("mapOkxPositionHistory", |a| {
            let deps = arg(&a, 1);
            let (to_base, symbol, settle) = (
                deps.get("toBase")?,
                deps.get("resolveSymbol")?,
                deps.get("resolveSettleCurrency")?,
            );
            let account = deps.get("account")?;
            let account = if account.is_nullish() {
                String::new()
            } else {
                text(&account)?
            };
            let to_base =
                |n: f64, inst: &str| number_callback(&to_base, vec![Js::Number(n), Js::str(inst)]);
            let resolve_symbol = |inst: &str| optional_text(&symbol, vec![Js::str(inst)]);
            let resolve_settle = |inst: &str| optional_text(&settle, vec![Js::str(inst)]);
            let record = position_adapter::map_okx_position_history(
                &record(&arg(&a, 0))?,
                &position_adapter::OkxPositionAdapterDeps {
                    account: &account,
                    to_base: &to_base,
                    resolve_symbol: &resolve_symbol,
                    resolve_settle_currency: &resolve_settle,
                },
            )
            .map_err(coercion)?;
            Ok(record.map_or(Js::Null, |r| {
                js_json(safe_cex_num::JsJsonValue::Object(r.into_iter().collect()))
            }))
        }),
    );
}

fn js_json(v: safe_cex_num::JsJsonValue) -> Js {
    use safe_cex_num::JsJsonValue as J;
    match v {
        J::Undefined => Js::Undefined,
        J::Null => Js::Null,
        J::Bool(b) => Js::Bool(b),
        J::Number(n) => Js::Number(n),
        J::String(s) => Js::str(&s),
        J::Array(xs) => Js::array(xs.into_iter().map(js_json).collect()),
        J::Object(fields) => {
            let o = Js::object(vec![]);
            for (k, v) in fields {
                let _ = o.define(&k, js_json(v));
            }
            o
        }
        J::BigInt(n) => Js::BigInt(Rc::new(n)),
    }
}

/// createAPI's collaborators: the Axios client, retry, signing markers.
fn transport(g: &Js, weak: Weak<OkxShared>) {
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    signature_bindings(g);
    let w = weak.clone();
    def(
        "safeCexAxiosCreate",
        Js::function("safeCexAxiosCreate", move |_, a| {
            let rt = scope_of(&w)?;
            client::factory::create(
                rt.shared.http.clone(),
                &*rt.shared.runtime.transport(),
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
        "networkErrorOnRead",
        Js::function("networkErrorOnRead", |_, a| {
            Ok(network_error_on_read(arg(&a, 0)))
        }),
    );
    def(
        "markSkipRemoteSigning",
        Js::function("markSkipRemoteSigning", |_, a| {
            arg(&a, 0).set("safeCexSkipRemoteSigning", Js::Bool(true))?;
            Ok(Js::Undefined)
        }),
    );
    let w = weak.clone();
    def(
        "installTransportAdapter",
        Js::function("installTransportAdapter", move |_, a| {
            let rt = scope_of(&w)?;
            client::factory::install_transport_adapter(
                rt.shared.http.clone(),
                &*rt.shared.runtime.transport(),
                &arg(&a, 0),
                &arg(&a, 1),
                to_typed(&arg(&a, 2))?,
                safe_cex_transport::RestAdapterVariant::BigInt,
            )?;
            Ok(Js::Undefined)
        }),
    );
    def(
        "AxiosHeaders",
        Js::constructor("AxiosHeaders", |_, a| arg(&a, 0).spread_object()),
    );
    let w = weak;
    def(
        "createAPI",
        Js::function("createAPI", move |_, a| {
            let scope = scope_of(&w)?;
            let h = runtime_ops(&scope);
            crate::history_stream_api::create_api(h, arg(&a, 0), arg(&a, 1))
        }),
    );
}

/// The three imports used by the source API's lazy retry arrow and final
/// response interceptor. Keep the stamp on the actual config object so both
/// retry paths observe it when the full request chain runs again.
fn signature_bindings(g: &Js) {
    use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY as KEY;
    let _ = g.define(
        "isOkxSignatureRejection",
        Js::function("isOkxSignatureRejection", |_, a| {
            crate::api::is_okx_signature_rejection(&to_json(&arg(&a, 0))?)
                .map(Js::Bool)
                .map_err(coercion)
        }),
    );
    let _ = g.define(
        "signatureRejectedResponse",
        Js::function("signatureRejectedResponse", |_, a| {
            let predicate = arg(&a, 0);
            Ok(Js::function("", move |_, a| {
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
    let _ = g.define(
        "resendOnSignatureRejection",
        Js::function("resendOnSignatureRejection", |_, a| {
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
}

/// The whole OKX module scope.
pub(crate) fn build(weak: Weak<OkxShared>) -> Js {
    let g = Js::object(vec![]);
    shared_globals::install_builtins(&g, weak.clone());
    shared_globals::install_enums(&g);
    library::install_library(&g, weak.clone());
    tables(&g);
    mappers(&g);
    transport(&g, weak.clone());
    super::sockets::register(&g, weak);
    g
}

#[cfg(test)]
mod signature_tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[test]
    fn retry_bindings_stamp_the_replayed_config_and_preserve_resend_identity() {
        let globals = Js::object(vec![]);
        signature_bindings(&globals);
        let predicate = globals.get("isOkxSignatureRejection").unwrap();
        let condition = globals
            .call_method("signatureRejectedResponse", vec![predicate.clone()])
            .unwrap();
        let data = Js::object(vec![("code", Js::str("50113"))]);
        let config = Js::object(vec![("url", Js::str("/api/v5/trade/order"))]);
        let response = Js::object(vec![("config", config.clone()), ("data", data.clone())]);
        let error = Js::object(vec![("config", config.clone()), ("response", response)]);
        assert!(
            condition
                .call(Js::Undefined, vec![error.clone()])
                .unwrap()
                .truthy()
        );
        assert!(config.get("safeCexSignatureRetry").unwrap().truthy());
        assert!(!condition.call(Js::Undefined, vec![error]).unwrap().truthy());
        let missing = Js::object(vec![("response", Js::object(vec![("data", data.clone())]))]);
        assert!(
            !condition
                .call(Js::Undefined, vec![missing])
                .unwrap()
                .truthy()
        );

        let calls = Rc::new(Cell::new(0));
        let captured = Rc::new(RefCell::new(Js::Undefined));
        let result = Js::object(vec![("resent", Js::Bool(true))]);
        let (count, next, settled) = (calls.clone(), captured.clone(), result.clone());
        let xhr = Js::object(vec![(
            "request",
            Js::function("request", move |_, a| {
                count.set(count.get() + 1);
                *next.borrow_mut() = arg(&a, 0);
                Ok(settled.clone())
            }),
        )]);
        let original = Js::object(vec![("url", Js::str("/api/v5/trade/order"))]);
        let response = Js::object(vec![("config", original.clone()), ("data", data.clone())]);
        let resent = globals
            .call_method(
                "resendOnSignatureRejection",
                vec![xhr.clone(), response, predicate.clone()],
            )
            .unwrap();
        assert!(
            resent.same_reference(&result),
            "resend result is returned unchanged"
        );
        assert_eq!(calls.get(), 1);
        let stamped = captured.borrow().clone();
        assert!(stamped.get("safeCexSignatureRetry").unwrap().truthy());
        assert_eq!(
            text(&stamped.get("url").unwrap()).unwrap(),
            "/api/v5/trade/order"
        );
        assert!(
            original
                .get("safeCexSignatureRetry")
                .unwrap()
                .is_undefined()
        );
        let response = Js::object(vec![("config", stamped), ("data", data)]);
        assert!(
            globals
                .call_method("resendOnSignatureRejection", vec![xhr, response, predicate])
                .unwrap()
                .is_undefined()
        );
        assert_eq!(calls.get(), 1, "a stamped response is never resent twice");
    }
}

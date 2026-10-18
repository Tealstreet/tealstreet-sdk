//! Module-scope values the WOO source imports or reads: woo.types constants,
//! enums, JS builtins and the construction-time collaborators (createAPI,
//! createApiWithSecondaryKey, rateLimit, getTransport, BaseExchange, BaseWebSocket).
use super::{
    client,
    rt::{Rt, Shared},
    value::{Js, Thrown, arg, from_typed, to_typed},
};
use crate::constants;
use safe_cex_exchange::js_runtime::{
    base_exchange::{self, BaseExchangeParts},
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::scope_of,
    sockets,
};
use safe_cex_types::ExchangeOptions;
use std::rc::Weak;

fn serde_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn table(entries: &[(&str, &str)]) -> Js {
    Js::object(entries.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn enum_table<T: serde::Serialize>(entries: &[(&str, T)]) -> Js {
    let table = Js::object(
        entries
            .iter()
            .map(|(k, v)| (*k, Js::str(&serde_text(v))))
            .collect(),
    );
    // The source exports ordinary objects: unknown keys can inherit the actual
    // realm's Object.prototype members. Preserve those referents as values.
    if let Js::Object(object) = &table {
        *object.proto.borrow_mut() = Some(Js::Builtin(
            safe_cex_num::PrototypeRealm::current().reference(safe_cex_num::ProtoMember::Prototype),
        ));
    }
    table
}

fn strings(values: &[&str]) -> Js {
    Js::array(values.iter().map(|s| Js::str(s)).collect())
}

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

/// The BaseExchange instance; the Rust core is kept for `Exchange::base()`.
fn base_exchange(rt: &Rt, name: Js, options: Js, store: Js) -> Result<Js, Thrown> {
    let extra = &rt.shared.extra;
    let parts = BaseExchangeParts {
        name: extra.name.clone(),
        options: extra.options.clone(),
        store: rt.shared.store.clone(),
        settings: super::exchange::settings(),
        external_public_data: extra.external_public_data.clone(),
    };
    let (this, base) =
        base_exchange::create(&rt.shared.sockets, parts, name, options, store, |_| Ok(()))?;
    let _ = rt.shared.base.set(base);
    Ok(this)
}

// ts: createAPI (woo.api.ts) — the request interceptor signs through the ported
// factory step: public skip, broker injection, remote/keyless bypass, V1/V3 signing.
fn create_api(rt: &Rt, options: Js) -> Result<Js, Thrown> {
    let typed: ExchangeOptions = base_exchange::exchange_options(&options)?;
    let timeout = typed
        .extra
        .as_ref()
        .and_then(|extra| extra.recv_window)
        .unwrap_or(constants::RECV_WINDOW as f64);
    let base_url = constants::BASE_URL
        .iter()
        .find(|(k, _)| {
            *k == if typed.testnet == Some(true) {
                "testnet"
            } else {
                "livenet"
            }
        })
        .map(|(_, v)| *v)
        .unwrap_or_default();
    let xhr = client::factory::create(
        rt.shared.http.clone(),
        &*rt.shared.runtime.transport(),
        Js::object(vec![
            ("baseURL", Js::str(base_url)),
            ("timeout", Js::Number(timeout)),
            (
                "paramsSerializer",
                Js::object(vec![(
                    "serialize",
                    Js::function("serialize", |_, a| {
                        crate::api::sorted_query(&arg(&a, 0), false)
                            .map(|q| Js::str(&q))
                            .map_err(|e| Thrown::error("Error", &e))
                    }),
                )]),
            ),
            (
                "headers",
                Js::object(vec![
                    ("cache-control", Js::str("no-cache")),
                    ("x-api-key", Js::str(&typed.key)),
                ]),
            ),
        ]),
        &options,
        rt.base(),
    )?;
    xhr.call_method(
        "__configureRetry",
        vec![Js::object(vec![
            ("retries", Js::Number(3.0)),
            (
                "retryCondition",
                rt.global("networkErrorOnRead")?
                    .call(Js::Undefined, vec![])?,
            ),
        ])],
    )?;
    if typed.tealstreet_tunnel == Some(true) {
        client::factory::install_transport_adapter(
            rt.shared.http.clone(),
            &*rt.shared.runtime.transport(),
            &xhr,
            &options,
            rt.base().name.clone(),
            safe_cex_transport::RestAdapterVariant::Default,
        )?;
    }
    let runtime = rt.shared.runtime.clone();
    let interceptor = Js::function("", move |_, a| {
        // Native shallow config copies preserve callable and data identities.
        let config = arg(&a, 0);
        // BaseExchange injects executable signer handles into these live options.
        let remote = options.get("remoteSigner")?.truthy();
        let typed = typed.clone();
        let runtime = runtime.clone();
        Ok(Js::Promise(super::promise::Promise::spawn(
            &(runtime.clone() as std::rc::Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
            Box::pin(async move {
                let provider = runtime.transport();
                let next = crate::factory::prepare_attempt(
                    &config,
                    &typed,
                    remote,
                    provider.as_ref(),
                    runtime.now_ms(),
                )
                .await
                .map_err(|e| Thrown::error("Error", &e))?;
                Ok(next)
            }),
        )))
    });
    xhr.get("interceptors")?
        .get("request")?
        .call_method("use", vec![interceptor])?;
    Ok(xhr)
}

// ts: createApiWithSecondaryKey — the secondary client serves only `get`.
fn create_api_with_secondary_key(rt: &Rt, a: Vec<Js>) -> Result<Js, Thrown> {
    let (options, exchange, create, other) = (arg(&a, 0), arg(&a, 1), arg(&a, 2), arg(&a, 3));
    let rate_limit = if other.is_nullish() {
        Js::Undefined
    } else {
        other.get("rateLimit")?
    };
    let mut primary = create.call(Js::Undefined, vec![options.clone(), exchange.clone()])?;
    if rate_limit.is_callable() {
        primary = rate_limit.call(Js::Undefined, vec![primary])?;
    }
    if !options.get("secondary_key")?.truthy() {
        return Ok(Js::array(vec![primary.clone(), primary.clone(), primary]));
    }
    let secondary_options = options.spread_object()?;
    for (k, v) in options.own_entries()? {
        let name = String::from_utf16_lossy(&k);
        if let Some(stripped) = name.strip_prefix("secondary_") {
            secondary_options.set(stripped, v)?;
        }
    }
    let proxy = options.get("secondaryProxyUrl")?;
    let proxy = if proxy.truthy() {
        proxy
    } else if !other.is_nullish() && other.get("proxyUrl")?.truthy() {
        other.get("proxyUrl")?
    } else {
        rt.shared
            .runtime
            .transport()
            .public_cors_proxy()
            .map_or(Js::Undefined, |s| Js::str(&s))
    };
    secondary_options.set("overrideProxyUrl", proxy)?;
    let mut secondary = create.call(Js::Undefined, vec![secondary_options, exchange])?;
    let secondary_limit = if other.is_nullish() {
        Js::Undefined
    } else {
        other.get("secondaryRateLimit")?
    };
    if secondary_limit.is_callable() {
        secondary = secondary_limit.call(Js::Undefined, vec![secondary])?;
    }
    let dynamic = primary.spread_object()?;
    let reader = secondary.clone();
    dynamic.define(
        "get",
        Js::function("get", move |_, args| reader.call_method("get", args)),
    )?;
    Ok(Js::array(vec![dynamic, primary, secondary]))
}

fn get_transport(rt: &Rt) -> Js {
    let provider = rt.shared.runtime.transport();
    Js::object(vec![(
        "requireProxy",
        Js::function("requireProxy", move |_, a| {
            let original = arg(&a, 0);
            let options: ExchangeOptions = base_exchange::exchange_options(&original)?;
            let structural = from_typed(&provider.require_proxy(options))?;
            let result = original.spread_object()?;
            for (key, value) in structural.own_entries()? {
                result.define_key(&key, value)?;
            }
            Ok(result)
        }),
    )])
}

/// Build the module-scope object. `rt` is weak: globals live inside the shared state.
pub(crate) fn build(weak: Weak<Shared>) -> Js {
    let g = Js::object(vec![]);
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };

    // The actual imported journal operations used by the source lifecycle body.
    def(
        "wooOrdersToExecutions",
        Js::function("wooOrdersToExecutions", |_, args| {
            let orders: Vec<safe_cex_types::HistoricalOrder> = to_typed(&arg(&args, 0))?;
            from_typed(&crate::journal::woo_orders_to_executions(&orders))
        }),
    );
    def(
        "reconstructPositionIntervals",
        Js::function("reconstructPositionIntervals", |_, args| {
            use safe_cex_exchange::position_intervals::{
                PositionIntervalOptions, reconstruct_position_intervals,
            };
            let executions: Vec<safe_cex_types::Execution> = to_typed(&arg(&args, 0))?;
            let intervals = reconstruct_position_intervals(
                &executions,
                &mut PositionIntervalOptions::default(),
            )
            .map_err(|message| Thrown::error("Error", &message))?;
            from_typed(&intervals)
        }),
    );
    def(
        "mapFoldedInterval",
        Js::function("mapFoldedInterval", |_, args| {
            use safe_cex_exchange::{
                fold_position_adapter::{FoldPositionAdapterDeps, map_folded_interval},
                position_intervals::PositionInterval,
            };
            let interval: PositionInterval = to_typed(&arg(&args, 0))?;
            // The native adapter's callbacks are infallible. Evaluate the source's
            // fallible resolver only after its refusal guards, before calling it.
            if interval.closed_at.is_none()
                || interval.closed_size.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater)
                || interval.realized_pnl.is_none()
            {
                return Ok(Js::Null);
            }
            let deps = arg(&args, 1);
            let account: String = to_typed(&deps.get("account")?)?;
            let currency =
                deps.call_method("resolveSettleCurrency", vec![Js::str(&interval.symbol)])?;
            let currency: String = to_typed(&currency)?;
            let mut resolve = |_: &str| Ok(currency.clone());
            let record = map_folded_interval(
                &interval,
                &mut FoldPositionAdapterDeps {
                    account: &account,
                    resolve_settle_currency: &mut resolve,
                    resolve_funding: None,
                },
            )?;
            match record {
                Some(record) => from_typed(&record),
                None => Ok(Js::Null),
            }
        }),
    );

    // woo.types.ts
    def("RECV_WINDOW", Js::Number(constants::RECV_WINDOW as f64));
    def("BROKER_ID", Js::str(constants::BROKER_ID));
    def("TESTNET_BROKER_ID", Js::str(constants::TESTNET_BROKER_ID));
    def("BASE_URL", table(constants::BASE_URL));
    def(
        "BASE_WS_URL",
        Js::object(
            constants::BASE_WS_URL
                .iter()
                .map(|(k, v)| (*k, table(v)))
                .collect(),
        ),
    );
    def("ENDPOINTS", table(constants::ENDPOINTS));
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        strings(constants::GRAFANA_ENDPOINTS_EVERY),
    );
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        strings(constants::GRAFANA_ENDPOINTS_PERIODIC),
    );
    def("ORDER_TYPE", enum_table(constants::ORDER_TYPE));
    def("ORDER_SIDE", enum_table(constants::ORDER_SIDE));
    def(
        "ORDER_TRIGGER_TYPE",
        enum_table(constants::ORDER_TRIGGER_TYPE),
    );

    // woo.utils.ts
    def(
        "normalizeSymbol",
        Js::function("normalizeSymbol", |_, a| {
            Ok(Js::str(&crate::utils::normalize_symbol(&text(&arg(
                &a, 0,
            ))?)))
        }),
    );
    def(
        "reverseSymbol",
        Js::function("reverseSymbol", |_, a| {
            Ok(Js::str(&crate::utils::reverse_symbol(&text(&arg(&a, 0))?)))
        }),
    );

    let w = weak.clone();
    def(
        "BaseExchange",
        Js::constructor("BaseExchange", None, move |_, a| {
            base_exchange(&scope_of(&w)?, arg(&a, 0), arg(&a, 1), arg(&a, 2))
        }),
    );
    let w = weak.clone();
    def(
        "createAPI",
        Js::function("createAPI", move |_, a| {
            create_api(&scope_of(&w)?, arg(&a, 0))
        }),
    );
    let w = weak.clone();
    def(
        "createApiWithSecondaryKey",
        Js::function("createApiWithSecondaryKey", move |_, a| {
            create_api_with_secondary_key(&scope_of(&w)?, a)
        }),
    );
    let w = weak.clone();
    def(
        "getTransport",
        Js::function("getTransport", move |_, _| {
            Ok(get_transport(&scope_of(&w)?))
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
                let code = error.get("code")?;
                Ok(Js::Bool(
                    error.get("response")?.is_undefined()
                        && code.truthy()
                        && text(&code)? != "ECONNABORTED",
                ))
            }))
        }),
    );
    let w = weak.clone();
    def(
        "performance",
        Js::object(vec![(
            "now",
            Js::function("now", move |_, _| {
                Ok(Js::Number(scope_of(&w)?.shared.runtime.monotonic_now_ms()))
            }),
        )]),
    );
    let w = weak.clone();
    def(
        "console",
        Js::object(vec![(
            "error",
            Js::function("error", move |_, a| {
                let message = a
                    .iter()
                    .map(|v| v.to_display())
                    .collect::<Vec<_>>()
                    .join(" ");
                scope_of(&w)?.shared.runtime.console("error", &message);
                Ok(Js::Undefined)
            }),
        )]),
    );

    install_enums(&g);
    install_builtins(&g, weak.clone());
    install_library(&g, weak.clone());
    super::hosts::register_all(&g, weak);
    g
}

/// `BaseWebSocket` needs the socket context, which exists only once the shared
/// state does; installed right after construction.
pub(crate) fn install_socket_base(rt: &Rt) {
    let _ = rt.shared.globals.define(
        "BaseWebSocket",
        sockets::base_websocket_class(&rt.shared.sockets),
    );
}

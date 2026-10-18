//! Imported services; source control flow stays in the ported body modules.
use super::{Rt, Shared, error};
use safe_cex_exchange::js_runtime::{
    base_exchange::{self, BaseExchangeParts},
    client,
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::scope_of,
    value::{Js, Thrown, arg, from_typed},
};
use std::rc::{Rc, Weak};

pub(crate) fn build(weak: Weak<Shared>) -> Js {
    let g = Js::object(vec![]);
    install_builtins(&g, weak.clone());
    install_enums(&g);
    install_library(&g, weak.clone());
    let def = |k: &str, v: Js| g.define(k, v).expect("fresh globals");
    let func = |name: &'static str, f: fn(&Rt, Vec<Js>) -> Result<Js, Thrown>| {
        let weak = weak.clone();
        Js::function(name, move |_, a| f(&scope_of(&weak)?, a))
    };
    def(
        "BaseExchange",
        func("BaseExchange", |rt, a| {
            let e = &rt.shared.extra;
            let (value, base) = base_exchange::create(
                &rt.shared.sockets,
                BaseExchangeParts {
                    name: e.name.clone(),
                    options: e.options.clone(),
                    store: rt.shared.store.clone(),
                    settings: super::exchange::settings(),
                    external_public_data: e.external.clone(),
                },
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                |_| Ok(()),
            )?;
            rt.shared
                .base
                .set(base)
                .map_err(|_| error("BaseExchange constructed twice"))?;
            Ok(value)
        }),
    );
    def(
        "safeCexAxiosCreate",
        func("safeCexAxiosCreate", |rt, a| {
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
            let xhr = arg(&a, 0);
            xhr.call_method("__configureRetry", vec![arg(&a, 1)])?;
            Ok(Js::Undefined)
        }),
    );
    def(
        "readsAndPosted",
        Js::function("readsAndPosted", |_, a| Ok(Js::array(a))),
    );
    def(
        "networkErrorOnRead",
        Js::function("networkErrorOnRead", |_, a| {
            let posted = arg(&a, 0);
            Ok(Js::function("retryCondition", move |_, a| {
                let e = arg(&a, 0);
                let c = e.get("config")?;
                if c.is_nullish() {
                    return Ok(Js::Bool(false));
                }
                let method = super::text(&c.get("method")?)?.to_lowercase();
                let is_read = method == "get"
                    || method == "head"
                    || (method == "post"
                        && posted
                            .array_values()
                            .unwrap_or_default()
                            .iter()
                            .any(|p| p.strict_equals(&c.get("url").unwrap_or(Js::Undefined))));
                Ok(Js::Bool(
                    is_read
                        && e.get("response")?.is_undefined()
                        && e.get("code")?.truthy()
                        && !e.get("code")?.strict_equals(&Js::str("ECONNABORTED")),
                ))
            }))
        }),
    );
    def(
        "createPolymarketAPI",
        func("createPolymarketAPI", |rt, a| {
            crate::api_factory::create_polymarket_api(
                Rc::new(rt.clone()),
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                arg(&a, 3),
            )
        }),
    );
    def(
        "createWallet",
        func("createWallet", |rt, a| {
            crate::auth_init::create_wallet(rt, arg(&a, 0))
        }),
    );
    def(
        "jsonParse",
        func("jsonParse", |rt, a| {
            Ok(rt
                .global("JSON.parse")?
                .call(Js::Undefined, a)
                .unwrap_or(Js::Null))
        }),
    );
    def(
        "buildL2Headers",
        func("buildL2Headers", |rt, a| {
            crate::auth_init::build_l2_headers(
                rt,
                arg(&a, 0),
                arg(&a, 1),
                arg(&a, 2),
                arg(&a, 3),
                arg(&a, 4),
                arg(&a, 5),
            )
        }),
    );
    def(
        "createSignedOrder",
        func("createSignedOrder", |rt, a| {
            Ok(rt.spawn(Box::pin(crate::auth_init::create_signed_order(
                Rc::new(rt.clone()),
                arg(&a, 0),
                arg(&a, 1),
            ))))
        }),
    );
    def(
        "signApiKeyDerivationMessage",
        func("signApiKeyDerivationMessage", |rt, a| {
            Ok(
                rt.spawn(Box::pin(crate::auth_init::sign_api_key_derivation_message(
                    Rc::new(rt.clone()),
                    arg(&a, 0),
                    arg(&a, 1),
                    arg(&a, 2),
                ))),
            )
        }),
    );
    for name in [
        "PolymarketAPI",
        "PolymarketPublicWebSocket",
        "PolymarketPrivateWebSocket",
    ] {
        let weak = weak.clone();
        def(
            name,
            Js::constructor(name, None, move |_, a| {
                let rt = scope_of(&weak)?;
                let classes = rt
                    .shared
                    .extra
                    .classes
                    .get()
                    .ok_or_else(|| error("Polymarket classes not initialized"))?;
                match name {
                    "PolymarketAPI" => Ok(crate::classes::PolymarketAPI::new(
                        classes,
                        arg(&a, 0),
                        arg(&a, 1),
                        arg(&a, 2),
                        arg(&a, 3),
                    )?
                    .receiver()),
                    "PolymarketPublicWebSocket" => Ok(
                        crate::classes::PolymarketPublicWebSocket::new(classes, arg(&a, 0))?
                            .receiver(),
                    ),
                    _ => Ok(crate::classes::PolymarketPrivateWebSocket::new(
                        classes,
                        arg(&a, 0),
                        arg(&a, 1),
                    )?
                    .receiver()),
                }
            }),
        );
    }
    def(
        "POLYMARKET_CONFIG",
        from_typed(&crate::constants::POLYMARKET_CONFIG).expect("constant"),
    );
    def(
        "RATE_LIMITS",
        from_typed(&crate::constants::RATE_LIMITS).expect("constant"),
    );
    for (name, entries) in [
        ("ENDPOINTS", crate::constants::ENDPOINTS),
        ("WS_MESSAGE_TYPES", crate::constants::WS_MESSAGE_TYPES),
    ] {
        def(
            name,
            Js::object(entries.iter().map(|(k, v)| (*k, Js::str(v))).collect()),
        );
    }
    for (name, entries) in [
        (
            "ORDER_SIDE_MAP",
            crate::constants::ORDER_SIDE_MAP
                .iter()
                .map(|(k, v)| (*k, from_typed(v).expect("constant")))
                .collect::<Vec<_>>(),
        ),
        (
            "ORDER_TYPE_MAP",
            crate::constants::ORDER_TYPE_MAP
                .iter()
                .map(|(k, v)| (*k, from_typed(v).expect("constant")))
                .collect(),
        ),
        (
            "ORDER_STATUS_MAP",
            crate::constants::ORDER_STATUS_MAP
                .iter()
                .map(|(k, v)| (*k, Js::str(v)))
                .collect(),
        ),
    ] {
        def(name, Js::object(entries));
    }
    def(
        "CLOB_AUTH_TYPES",
        safe_cex_exchange::js_runtime::value::from_json(
            &serde_json::json!({"ClobAuth":[{"name":"address","type":"address"},{"name":"timestamp","type":"string"},{"name":"nonce","type":"uint256"},{"name":"message","type":"string"}]}),
        ),
    );
    super::signing::install(&g, weak);
    g
}

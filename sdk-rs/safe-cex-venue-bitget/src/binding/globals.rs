//! Module-scope values the TS source imports or reads: protocol constants,
//! enums, JS builtins. Host modules register their imported helpers here too.
use super::value::Js;
use crate::protocol;
pub(crate) use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    scope::scope_of as shared,
    value::arg,
};
use std::rc::Weak;

fn enum_object(members: &[(&str, &str)]) -> Js {
    Js::object(members.iter().map(|(k, v)| (*k, Js::str(v))).collect())
}

fn table(entries: &[(&str, &str)]) -> Js {
    enum_object(entries)
}

fn serde_text<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Build the module-scope object. `rt` is weak: globals live inside the shared state.
pub(crate) fn build(rt: Weak<super::rt::Shared>) -> Js {
    let g = Js::object(vec![]);
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };

    // bitget.types.ts
    def("RECV_WINDOW", Js::Number(protocol::RECV_WINDOW as f64));
    def("BROKER_ID", Js::str(protocol::BROKER_ID));
    def("BASE_URL", Js::str(protocol::BASE_URL));
    def("BASE_WS_URL", Js::str(protocol::BASE_WS_URL));
    def(
        "BASE_WS_PRIVATE_URL",
        Js::str(protocol::BASE_WS_PRIVATE_URL),
    );
    def(
        "BASE_WS_PRIVATE_UTA_URL",
        Js::str(protocol::BASE_WS_PRIVATE_UTA_URL),
    );
    def(
        "ENDPOINTS",
        Js::object(
            protocol::ENDPOINTS
                .iter()
                .map(|e| (e.name, Js::str(e.path)))
                .collect(),
        ),
    );
    def(
        "GRAFANA_ENDPOINTS_EVERY",
        Js::array(
            protocol::GRAFANA_ENDPOINTS_EVERY
                .iter()
                .map(|s| Js::str(s))
                .collect(),
        ),
    );
    def(
        "GRAFANA_ENDPOINTS_PERIODIC",
        Js::array(
            protocol::GRAFANA_ENDPOINTS_PERIODIC
                .iter()
                .map(|s| Js::str(s))
                .collect(),
        ),
    );
    def("INTERVAL", table(protocol::INTERVAL));
    def(
        "POSITION_SIDE",
        Js::object(
            protocol::POSITION_SIDE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_STATUS",
        Js::object(
            protocol::ORDER_STATUS
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_TYPE",
        Js::object(
            protocol::ORDER_TYPE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "ORDER_SIDE",
        Js::object(
            protocol::ORDER_SIDE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    def(
        "TIME_IN_FORCE",
        Js::object(
            protocol::TIME_IN_FORCE
                .iter()
                .map(|(k, v)| (*k, Js::str(&serde_text(v))))
                .collect(),
        ),
    );
    // bitget.exchange.ts module constants
    def("BILL_PAGE_LIMIT", Js::Number(100.0));
    def(
        "BILL_MAX_WINDOW_MS",
        Js::Number(30.0 * 24.0 * 60.0 * 60.0 * 1000.0),
    );
    def(
        "BitgetExchange",
        Js::object(vec![("POSITIONING_CACHE_TTL_MS", Js::Number(5000.0))]),
    );
    def(
        "BitgetClientOrderIdPattern",
        Js::str("^[.A-Z:/a-z0-9_-]{1,32}$"),
    );

    install_enums(&g);
    install_builtins(&g, rt);
    g
}

//! Module-scope names the Bybit bodies import or read: JS builtins, the shared
//! library helpers, the types.ts enums, the virtual clock and the fee overlay.
use super::{
    promise::Promise,
    rt::Shared,
    value::{Js, arg, from_typed, to_typed},
};
use safe_cex_exchange::js_runtime::{
    globals::{install_builtins, install_enums},
    library::install_library,
    scope::scope_of,
};
use safe_cex_store::virtual_clock::VirtualClock;
use std::rc::Weak;

pub(crate) fn build(rt: Weak<Shared>, clock: VirtualClock) -> Js {
    let g = Js::object(vec![]);
    install_enums(&g);
    install_builtins(&g, rt.clone());
    install_library(&g, rt.clone());
    let def = |k: &str, v: Js| {
        let _ = g.define(k, v);
    };
    // utils/virtualClock: the one clock the signed clients also read.
    let start_clock = clock.clone();
    let start_rt = rt.clone();
    def(
        "virtualClock",
        Js::object(vec![
            (
                "getCurrentTimeMs",
                Js::function("getCurrentTimeMs", move |_, _| {
                    Ok(Js::Number(clock.get_current_time_ms()))
                }),
            ),
            (
                "start",
                Js::function("start", move |_, _| {
                    let rt = scope_of(&start_rt)?;
                    let clock = start_clock.clone();
                    Ok(Js::Promise(Promise::spawn(
                        &rt.exchange_runtime(),
                        Box::pin(async move {
                            clock.start().await;
                            Ok(Js::Undefined)
                        }),
                    )))
                }),
            ),
        ]),
    );
    // ts: bybitFeeOverlay — null when no rate was returned, else (markets) => markets.
    def(
        "bybitFeeOverlay",
        Js::function("bybitFeeOverlay", |_, a| {
            let fees: Vec<crate::fee_overlay::BybitFeeRate> = to_typed(&arg(&a, 0))?;
            let Some(overlay) = crate::fee_overlay::bybit_fee_overlay(&fees) else {
                return Ok(Js::Null);
            };
            let overlay = std::rc::Rc::new(overlay);
            Ok(Js::function("bybitFeeOverlay", move |_, a| {
                let markets: Vec<safe_cex_types::Market> = to_typed(&arg(&a, 0))?;
                from_typed(&overlay(&markets))
            }))
        }),
    );
    def(
        "ENDPOINTS",
        super::value::from_json(&crate::types::ENDPOINTS),
    );
    g
}

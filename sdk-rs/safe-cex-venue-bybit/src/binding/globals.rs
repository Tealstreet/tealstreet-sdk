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
    let status_rt = rt.clone();
    def(
        "getBybitResponseMeta",
        Js::function("getBybitResponseMeta", move |_, args| {
            let scope = scope_of(&status_rt)?;
            super::members::orders::position_response_meta(&scope, &arg(&args, 0))
        }),
    );
    let json = g.get("JSON").expect("installed JSON builtin");
    def(
        "jsonParse",
        Js::function("jsonParse", move |_, args| {
            Ok(json.call_method("parse", args).unwrap_or(Js::Null))
        }),
    );
    def(
        "RegExp",
        Js::function("RegExp", |_, args| {
            let pattern = arg(&args, 0);
            let flags = arg(&args, 1);
            safe_cex_exchange::js_runtime::regexp::regexp(
                &if pattern.is_undefined() {
                    vec![]
                } else {
                    pattern.to_js_string()?.to_vec()
                },
                &if flags.is_undefined() {
                    vec![]
                } else {
                    flags.to_js_string()?.to_vec()
                },
            )
        }),
    );
    // lodash omit copies enumerable inherited fields before removing literal paths.
    def(
        "omit",
        Js::function("omit", |_, args| {
            let input = arg(&args, 0);
            let request = Js::object(vec![]);
            let mut keys = safe_cex_exchange::js_runtime::for_in::ForInRecord::get(&input);
            while let Some(key) = keys.next_key()? {
                request.define_key(&key, input.get_key(&key)?)?;
            }
            for paths in args.iter().skip(1) {
                for key in paths.array_values().unwrap_or_else(|| vec![paths.clone()]) {
                    request.delete(&key.to_js_string()?)?;
                }
            }
            Ok(request)
        }),
    );
    let income_rt = rt.clone();
    def(
        "fetchBybitIncomeHistory",
        Js::function("fetchBybitIncomeHistory", move |_, args| {
            let rt = scope_of(&income_rt)?;
            Ok(crate::ledger_history::fetch_bybit_income_history(
                std::rc::Rc::new(rt),
                arg(&args, 0),
                arg(&args, 1),
            ))
        }),
    );
    let range_rt = rt.clone();
    def(
        "fetchBybitIncomeHistoryRange",
        Js::function("fetchBybitIncomeHistoryRange", move |_, args| {
            let rt = scope_of(&range_rt)?;
            Ok(crate::ledger_history::fetch_bybit_income_history_range(
                std::rc::Rc::new(rt),
                arg(&args, 0),
                arg(&args, 1),
                arg(&args, 2),
                arg(&args, 3),
            ))
        }),
    );
    def(
        "splitTimeRange",
        Js::function("splitTimeRange", |_, args| {
            Ok(Js::array(
                safe_cex_candle_cache::calendar::split_time_range(
                    arg(&args, 0).to_number()?,
                    arg(&args, 1).to_number()?,
                    arg(&args, 2).to_number()?,
                )
                .iter()
                .map(super::value::from_json)
                .collect(),
            ))
        }),
    );
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
    def(
        "isBybitOrderAlreadyGoneCancelMessage",
        Js::function("isBybitOrderAlreadyGoneCancelMessage", |_, args| {
            let value = arg(&args, 0);
            let value = if value.is_undefined() {
                None
            } else {
                Some(super::value::to_json(&value)?)
            };
            Ok(Js::Bool(
                crate::exchange_helpers::is_bybit_order_already_gone_cancel_message(value.as_ref())
                    .map_err(|message| super::value::Thrown::error("TypeError", &message))?,
            ))
        }),
    );
    def(
        "isUnknownOutcome",
        Js::function("isUnknownOutcome", |_, args| {
            let value = arg(&args, 0);
            if !value.is_object() && !value.is_array() {
                return Ok(Js::Bool(false));
            }
            Ok(Js::Bool(
                safe_cex_store::remaining_utils::is_unknown_outcome(&super::value::to_json(
                    &value,
                )?)
                .map_err(|message| super::value::Thrown::error("TypeError", &message))?,
            ))
        }),
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
    for (key, value) in [
        (
            "HYROTRADER_SIMULATOR_URL",
            &*crate::types::HYROTRADER_SIMULATOR_URL,
        ),
        (
            "HYROTRADER_SIMULATOR_WS_URL",
            &*crate::types::HYROTRADER_SIMULATOR_WS_URL,
        ),
        ("HYROTRADER_PROD_URL", &*crate::types::HYROTRADER_PROD_URL),
        (
            "HYROTRADER_PROD_WS_URL",
            &*crate::types::HYROTRADER_PROD_WS_URL,
        ),
        ("INTERVAL", &*crate::types::INTERVAL),
        ("ORDER_SIDE", &*crate::types::ORDER_SIDE),
        ("POSITION_SIDE", &*crate::types::POSITION_SIDE),
        ("ORDER_STATUS", &*crate::types::ORDER_STATUS),
        ("ORDER_TYPE", &*crate::types::ORDER_TYPE),
        ("ORDER_TIME_IN_FORCE", &*crate::types::ORDER_TIME_IN_FORCE),
        (
            "ORDER_TIME_IN_FORCE_OUTBOUND",
            &*crate::types::ORDER_TIME_IN_FORCE_OUTBOUND,
        ),
        ("ORDER_TRIGGER_TYPE", &*crate::types::ORDER_TRIGGER_TYPE),
        (
            "POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE",
            &*crate::types::POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
        ),
    ] {
        def(key, super::value::from_json(value));
    }
    g
}

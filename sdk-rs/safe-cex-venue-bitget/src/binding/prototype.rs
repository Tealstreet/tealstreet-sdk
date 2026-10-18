//! TS prototype methods (not class fields, so absent from the assembled own
//! properties) installed on the receiver, each running its ported Rust body.
use super::{
    globals::arg,
    promise::await_js,
    rt::{Rt, Variant},
    value::{Js, Thrown, from_typed, to_typed},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{MarginType, PositionMode};

fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}

fn report(this: &Js, message: &str) -> Result<(), ExchangeError> {
    this.call_method("emitAndLogError", vec![Js::str(message)])
        .map(|_| ())
        .map_err(ExchangeError::from)
}

/// Resolve `{ positionMode, marginType }` through the receiver's resolveSymbolPositioning.
async fn positioning_field(
    rt: Rt,
    this: Js,
    symbol: String,
    field: &'static str,
) -> Result<Option<Js>, ExchangeError> {
    let resolved = await_js(
        &rt.exchange_runtime(),
        this.call_method("resolveSymbolPositioning", vec![Js::str(&symbol)])?,
    )
    .await?;
    let value = if resolved.is_nullish() {
        Js::Undefined
    } else {
        resolved.get(field)?
    };
    Ok((!value.is_nullish()).then_some(value))
}

pub(crate) fn install(rt: &Rt) {
    let this = rt.this.clone();
    // Own class fields win over prototype members, as in JS lookup.
    let def = |name: &str, f: Js| {
        if !this.has_own(name) {
            let _ = this.define(name, f);
        }
    };

    // ts: BitgetExchange#resolveSymbolPositioning
    {
        let rt = rt.clone();
        def(
            "resolveSymbolPositioning",
            Js::function("resolveSymbolPositioning", move |_, a| {
                let rt = rt.clone();
                let symbol = arg(&a, 0);
                Ok(rt.clone().spawn(Box::pin(async move {
                    crate::runtime_orchestration::resolve_symbol_positioning(&rt, symbol).await
                })))
            }),
        );
    }
    // ts: BitgetExchange.getOrFetchPositionMode / getOrFetchMarginType
    for (name, field) in [
        ("getOrFetchPositionMode", "positionMode"),
        ("getOrFetchMarginType", "marginType"),
    ] {
        let rt = rt.clone();
        def(
            name,
            Js::function(name, move |_, a| {
                let rt = rt.clone();
                let symbol = text(&arg(&a, 0))?;
                Ok(rt.clone().spawn(Box::pin(async move {
                    let this = rt.this.clone();
                    let uses_uta = this.get("usesUtaRouting")?.truthy();
                    let resolve =
                        |s: &str| positioning_field(rt.clone(), this.clone(), s.to_owned(), field);
                    let result = if field == "positionMode" {
                        let mode = crate::positioning::get_or_fetch_position_mode(
                            &symbol,
                            |s| {
                                let fut = resolve(s);
                                async move {
                                    fut.await?
                                        .map(|v| {
                                            to_typed::<PositionMode>(&v)
                                                .map_err(ExchangeError::from)
                                        })
                                        .transpose()
                                }
                            },
                            || uses_uta,
                            |m| report(&this, m),
                        )
                        .await?;
                        from_typed(&mode)?
                    } else {
                        let margin = crate::positioning::get_or_fetch_margin_type(
                            &symbol,
                            |s| {
                                let fut = resolve(s);
                                async move {
                                    fut.await?
                                        .map(|v| {
                                            to_typed::<MarginType>(&v).map_err(ExchangeError::from)
                                        })
                                        .transpose()
                                }
                            },
                            || uses_uta,
                            |m| report(&this, m),
                        )
                        .await?;
                        from_typed(&margin)?
                    };
                    Ok(result)
                })))
            }),
        );
    }
    // ts: BitgetExchange#_resolveMarket
    {
        let rt = rt.clone();
        def(
            "_resolveMarket",
            Js::function("_resolveMarket", move |_, a| {
                let symbol = text(&arg(&a, 0))?;
                let margin = text(&rt.this.get("apiMarginCoin")?)?;
                let store = rt.shared.store.clone();
                let markets = store.markets();
                let by_symbol = store.markets_by_symbol();
                let found =
                    crate::market::resolve_market(&symbol, &margin, &markets, |s| by_symbol.get(s));
                match found {
                    Some(market) => from_typed(market),
                    None => Ok(Js::Undefined),
                }
            }),
        );
    }
    // ts: BitgetExchange.calculateUpnlFromLastPrice
    {
        let rt = rt.clone();
        def(
            "calculateUpnlFromLastPrice",
            Js::function("calculateUpnlFromLastPrice", move |_, a| {
                let positions: Vec<safe_cex_types::Position> = to_typed(&arg(&a, 0))?;
                let store = rt.shared.store.clone();
                let tickers = store.tickers_by_symbol();
                Ok(Js::Number(crate::pnl::calculate_upnl_from_last_price(
                    &positions,
                    |s| tickers.get(s).map(|t| t.last),
                )))
            }),
        );
    }
    if rt.shared.extra.variant == Variant::Inverse {
        // ts: BitgetInverseExchange.getMarginCoinForSymbol
        let store = rt.shared.store.clone();
        def(
            "getMarginCoinForSymbol",
            Js::function("getMarginCoinForSymbol", move |_, a| {
                let symbol = text(&arg(&a, 0))?;
                let by_symbol = store.markets_by_symbol();
                Ok(Js::str(&crate::market::get_margin_coin_for_symbol(
                    &symbol,
                    |s| by_symbol.get(s),
                )))
            }),
        );
        // ts: BitgetInverseExchange#generateUuid
        let runtime = rt.exchange_runtime();
        def(
            "generateUuid",
            Js::function("generateUuid", move |_, _| {
                Ok(Js::str(&crate::inverse_logic::generate_uuid(|| {
                    runtime.random()
                })))
            }),
        );
    }
}

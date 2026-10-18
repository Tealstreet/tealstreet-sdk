use super::values as v;
use super::*;
use crate::account_actions::{self as a, WeexActionMethod, WeexActionValue};
fn fields(values: Option<Vec<(&'static str, WeexActionValue)>>) -> Js {
    values
        .map(|fields| {
            Js::object(
                fields
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            match v {
                                WeexActionValue::String(s) => Js::utf16(&s),
                                WeexActionValue::Strings(a) => {
                                    Js::array(a.into_iter().map(|s| Js::utf16(&s)).collect())
                                }
                            },
                        )
                    })
                    .collect(),
            )
        })
        .unwrap_or(Js::Undefined)
}
pub(super) fn runtime() -> a::WeexAccountActionRuntime<Host, Js, Js, Js, Js, Thrown> {
    a::WeexAccountActionRuntime {
        request: |h, request| {
            let config = Js::object(std::vec![]);
            let params = fields(request.params);
            if !params.is_undefined() {
                config.define("params", params)?;
            }
            let body = fields(request.body);
            let pending = match request.method {
                WeexActionMethod::Post => h
                    .root("xhr")?
                    .call_method("post", std::vec![Js::str(request.endpoint), body, config])?,
                WeexActionMethod::Delete => {
                    if !body.is_undefined() {
                        config.define("data", body)?;
                    }
                    h.root("xhr")?
                        .call_method("delete", std::vec![Js::str(request.endpoint), config])?
                }
            };
            Ok(Box::pin(async move {
                await_js(&h.0.exchange_runtime(), pending).await?;
                Ok(())
            }))
        },
        promise_all: |_, a, b| {
            Ok(Box::pin(async move {
                futures::future::try_join(a, b).await?;
                Ok(())
            }))
        },
        is_algo_order: |_, order| {
            Ok(crate::order_routing::is_algo_order(
                to_typed(&order.get("type")?).ok(),
                &v::units(&order.get("id")?)?,
            ))
        },
        order_id: |_, order| v::units(&order.get("id")?),
        order_symbol: |_, order| v::units(&order.get("symbol")?),
        orders: |h| {
            h.root("store.orders")?
                .array_values()
                .ok_or_else(|| Thrown::type_error("orders is not iterable"))
        },
        update_orders_status: |h, ids, status| {
            h.call(
                "store.updateOrdersStatus",
                std::vec![
                    Js::array(
                        ids.into_iter()
                            .map(|id| Js::object(std::vec![("id", Js::utf16(&id))]))
                            .collect(),
                    ),
                    from_typed(&status)?,
                ],
            )?;
            Ok(())
        },
        market: |h, symbol| {
            let market = h.root("store.marketsBySymbol")?.get_key(symbol)?;
            Ok(market.truthy().then_some(market))
        },
        leverage_min: |_, market| {
            market
                .get("limits")?
                .get("leverage")?
                .get("min")?
                .to_number()
        },
        leverage_max: |_, market| {
            market
                .get("limits")?
                .get("leverage")?
                .get("max")?
                .to_number()
        },
        config: |h, symbol, refetch| {
            let pending = h.call(
                "getOrFetchSymbolConfig",
                std::vec![Js::utf16(symbol), Js::Bool(refetch)],
            )?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        margin_type: |_, config| to_typed(&config.get("marginType")?),
        position_mode: |_, config| to_typed(&config.get("positionMode")?),
        isolated_long_leverage: |_, config| config.get("isolatedLongLeverage")?.to_number(),
        isolated_short_leverage: |_, config| config.get("isolatedShortLeverage")?.to_number(),
        position_symbols: |h| {
            h.root("store.positions")?
                .array_values()
                .ok_or_else(|| Thrown::type_error("positions is not iterable"))?
                .into_iter()
                .map(|row| v::units(&row.get("symbol")?))
                .collect()
        },
        market_symbols: |h| {
            h.root("store.markets")?
                .array_values()
                .ok_or_else(|| Thrown::type_error("markets is not iterable"))?
                .into_iter()
                .map(|row| v::units(&row.get("symbol")?))
                .collect()
        },
        cache_with_mode: |h, symbol, config, mode| {
            let value = config.spread_object()?;
            value.define("positionMode", from_typed(&mode)?)?;
            h.root("symbolSettings")?
                .call_method("set", std::vec![Js::utf16(symbol), value])?;
            Ok(())
        },
        set_account_mode: |h, mode| h.this().set("accountPositionMode", from_typed(&mode)?),
        set_hedged: |h, hedged| {
            h.call(
                "store.setSetting",
                std::vec![Js::str("isHedged"), Js::Bool(hedged)],
            )?;
            Ok(())
        },
        fetch_positions: |h| {
            let pending = h.call("fetchPositions", std::vec![])?;
            Ok(Box::pin(await_js(&h.0.exchange_runtime(), pending)))
        },
        update_positions: |h, positions| {
            h.call(
                "store.update",
                std::vec![Js::object(std::vec![("positions", positions)])],
            )?;
            Ok(())
        },
        new_error: |_, message| Thrown::error("Error", &String::from_utf16_lossy(&message)),
    }
}

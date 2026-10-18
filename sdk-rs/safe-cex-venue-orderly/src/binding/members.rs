use super::{
    host::{Host, R, V, v},
    text,
};
use crate::classes::Return;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_typed, to_typed};
use std::collections::HashMap;
fn margin_cache(receiver: &Js) -> R<HashMap<String, safe_cex_types::MarginType>> {
    let map = receiver.get("marginTypeBySymbol")?;
    let Js::Map(entries) = map else {
        return Err(Thrown::type_error("marginTypeBySymbol must be a Map"));
    };
    entries
        .borrow()
        .iter()
        .map(|(key, value)| Ok((text(key)?, to_typed(value)?)))
        .collect()
}
pub(super) fn invoke(
    host: &Host,
    receiver: V,
    name: &'static str,
    args: Vec<V>,
) -> R<Return<Host>> {
    let arg = |i| args.get(i).cloned().unwrap_or_else(|| v(Js::Undefined));
    let mapped = match name {
        "dispose" => Some(crate::runtime_actions::dispose(host, receiver.clone())?),
        "mapPosition" => Some(crate::data_mappers::map_position(
            host,
            receiver.clone(),
            arg(0),
            arg(1),
        )?),
        "mapLimitOrder" => Some(crate::data_mappers::map_limit(
            host,
            receiver.clone(),
            arg(0),
        )?),
        "mapAlgoOrder" => Some(crate::data_mappers::map_algo(
            host,
            receiver.clone(),
            arg(0),
        )?),
        "listenOHLCV" => Some(crate::data_mappers::listen_candles(
            host,
            receiver.clone(),
            arg(0),
            arg(1),
        )?),
        "listenOrderBook" => Some(crate::data_mappers::listen_book(
            host,
            receiver.clone(),
            arg(0),
            arg(1),
        )?),
        "mapMarginType" => {
            let value = arg(0);
            let raw = if value.is_string() {
                Some(text(&value)?)
            } else {
                None
            };
            Some(v(from_typed(&crate::margin::map_margin_type(
                raw.as_deref(),
            ))?))
        }
        "toOrderlyMarginMode" => Some(v(Js::str(crate::margin::to_orderly_margin_mode(to_typed(
            &arg(0),
        )?)))),
        "getCurrentMarginType" => Some(v(from_typed(&crate::margin::get_current_margin_type(
            &text(&arg(0))?,
            &margin_cache(&receiver)?,
            host.rt()?.base().store.as_ref(),
        ))?)),
        "resolveMarginTypeForOrder" => Some(v(from_typed(
            &crate::margin::resolve_margin_type_for_order(
                &to_typed(&arg(0))?,
                &margin_cache(&receiver)?,
                host.rt()?.base().store.as_ref(),
            )
            .map_err(|e| Thrown::error("Error", &e))?,
        )?)),
        "createOrderlyPositionId" => Some(v(Js::str(&crate::margin::create_orderly_position_id(
            &text(&arg(0))?,
            to_typed(&arg(1))?,
            to_typed(&arg(2))?,
            to_typed(&arg(3))?,
        )))),
        "isAlgoOrder" => Some(v(Js::Bool(crate::utils::is_algo_order(to_typed(&arg(0))?)))),
        _ => None,
    };
    if let Some(value) = mapped {
        return Ok(Return::Value(value));
    }
    let host = host.clone();
    Ok(Return::Future(Box::pin(async move {
        let arg = |i| args.get(i).cloned().unwrap_or_else(|| v(Js::Undefined));
        match name {
            "start" => crate::runtime_actions::start(&host, receiver).await,
            "tick" => crate::runtime_actions::tick(&host, receiver, arg(0)).await,
            "getPositionMode" => crate::account_order_reads::position_mode(&host, receiver).await,
            "getAccount" => crate::account_order_reads::account(&host, receiver).await,
            "validateAccount" => crate::account_order_reads::validate(&host, receiver).await,
            "fetchAndUpdateAccountLeverage" => {
                crate::account_order_reads::leverage(&host, receiver).await
            }
            "fetchAndUpdateMarginTypes" => {
                crate::account_order_reads::margin_types(&host, receiver).await
            }
            "fetchMarkets" => crate::data_mappers::markets(&host, receiver).await,
            "fetchTickers" => crate::data_mappers::tickers(&host, receiver).await,
            "fetchPositions" => crate::data_mappers::positions(&host, receiver).await,
            "fetchOrders" => crate::account_order_reads::orders(&host, receiver).await,
            "fetchLimitOrders" => crate::account_order_reads::limit_orders(&host, receiver).await,
            "fetchAlgoOrders" => crate::account_order_reads::algo_orders(&host, receiver).await,
            "fetchOHLCV" => crate::data_mappers::candles(&host, receiver, arg(0)).await,
            "setLeverage" => {
                crate::trading_actions::set_leverage(&host, receiver, arg(0), arg(1)).await
            }
            "setAllLeverage" => {
                crate::trading_actions::set_all_leverage(&host, receiver, arg(0)).await
            }
            "changeMarginType" => {
                crate::trading_actions::change_margin(&host, receiver, arg(0), arg(1)).await
            }
            "placeOrder" => crate::runtime_actions::place_order(&host, receiver, arg(0)).await,
            "updateOrder" => crate::runtime_actions::update_order(&host, receiver, arg(0)).await,
            "updateAlgoOrder" => crate::trading_actions::update_algo(&host, receiver, arg(0)).await,
            "cancelOrders" => crate::trading_actions::cancel_orders(&host, receiver, arg(0)).await,
            "cancelAlgoOrder" => crate::trading_actions::cancel_algo(&host, receiver, arg(0)).await,
            "cancelAllOrders" => crate::trading_actions::cancel_all(&host, receiver).await,
            "cancelSymbolOrders" => {
                crate::trading_actions::cancel_symbol(&host, receiver, arg(0)).await
            }
            "placeAlgoOrder" => crate::runtime_actions::place_algo(&host, receiver, arg(0)).await,
            "placePositionalAlgoOrder" => {
                crate::runtime_actions::place_positional(&host, receiver, arg(0)).await
            }
            "getPositionSideForOrder" => {
                let options = to_typed(&arg(0))?;
                let mode = crate::utils::get_position_side_for_order(&options, || async {
                    let promise = receiver.call_method("getPositionMode", vec![])?;
                    to_typed(&safe_cex_exchange::js_runtime::promise::await_js(promise).await?)
                })
                .await?;
                Ok(v(mode.map_or(Js::Undefined, Js::str)))
            }
            _ => Err(Thrown::error(
                "Error",
                &format!("Unknown Orderly member {name}"),
            )),
        }
    })))
}

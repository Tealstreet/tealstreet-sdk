//! BlofinExchange's mode utilities against the shared Store and an explicit host cache.
use crate::utility::UtilityInput;
use safe_cex_num::js_truthy;
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_types::{
    MarginType, OrderSide, OrderType, PositionMode, model_utils::create_position_id,
};
use std::{cell::Cell, collections::HashSet, future::Future};

#[derive(Debug, Default)]
pub struct AccountModes {
    pub position_mode: Cell<Option<PositionMode>>,
    pub margin_type: Cell<Option<MarginType>>,
}

pub fn apply_confirmed_position_mode(
    modes: &AccountModes,
    store: &dyn Store,
    position_mode: PositionMode,
) -> Result<(), StoreMutationError> {
    modes.position_mode.set(Some(position_mode));
    // Observe the prepared update before Store normalization can mask helper errors.
    match prepare_confirmed_mode_update(store, position_mode)? {
        Some(update) => store.update(update, false),
        None => Ok(()),
    }
}

fn prepare_confirmed_mode_update(
    store: &dyn Store,
    position_mode: PositionMode,
) -> Result<Option<StoreUpdate>, safe_cex_types::StoreReadError> {
    let mut update = StoreUpdate::default();
    {
        let leverage = store.fetched_leverage()?;
        if !leverage.is_empty() {
            update.fetched_leverage = Some(
                leverage
                    .iter()
                    .map(|(symbol, leverage)| {
                        let mut leverage = leverage.clone();
                        leverage.position_mode = Some(position_mode);
                        (symbol.clone(), leverage)
                    })
                    .collect(),
            );
        }
    }
    {
        let positions = store.positions()?;
        if !positions.is_empty() {
            let mut ids = HashSet::new();
            update.positions = Some(
                positions
                    .iter()
                    .map(|position| {
                        let mut position = position.clone();
                        position.id = create_position_id(
                            &position.symbol,
                            position.side.into_value(),
                            Some(position_mode),
                        );
                        position.position_mode = (Some(position_mode)).into();
                        position
                    })
                    .filter(|position| ids.insert(position.id.clone()))
                    .collect(),
            );
        }
    }
    // All Store read guards have been released before returning to update/callbacks.
    if update.fetched_leverage.is_none() && update.positions.is_none() {
        Ok(None)
    } else {
        Ok(Some(update))
    }
}

// ts: applyConfirmedMarginType. Flat positions and fetched leverage follow the
// confirmed type so the margin selector does not snap back.
pub fn apply_confirmed_margin_type(
    modes: &AccountModes,
    store: &dyn Store,
    margin_type: MarginType,
) -> Result<(), StoreMutationError> {
    modes.margin_type.set(Some(margin_type));
    let mut update = StoreUpdate::default();
    {
        let leverage = store.fetched_leverage()?;
        if !leverage.is_empty() {
            update.fetched_leverage = Some(
                leverage
                    .iter()
                    .map(|(symbol, leverage)| {
                        let mut leverage = leverage.clone();
                        leverage.margin_type = Some(margin_type);
                        (symbol.clone(), leverage)
                    })
                    .collect(),
            );
        }
    }
    {
        let positions = store.positions()?;
        let flat_update = positions
            .iter()
            .any(|position| position.contracts == 0.0 && position.margin_type != Some(margin_type));
        if flat_update {
            update.positions = Some(
                positions
                    .iter()
                    .map(|position| {
                        let mut position = position.clone();
                        if position.contracts == 0.0 {
                            position.margin_type = (Some(margin_type)).into();
                        }
                        position
                    })
                    .collect(),
            );
        }
    }
    if update.fetched_leverage.is_none() && update.positions.is_none() {
        return Ok(());
    }
    store.update(update, false)
}

pub fn after_method_called_in_worker(
    modes: &AccountModes,
    store: &dyn Store,
    result: Option<&UtilityInput>,
    method: &str,
    first_arg: Option<&UtilityInput>,
) -> Result<(), StoreMutationError> {
    let enabled = js_truthy(first_arg.map(UtilityInput::json));
    if method == "changePositionMode" {
        let confirmed = result
            .map(UtilityInput::json)
            .filter(|result| result.is_object() || result.is_array())
            .and_then(|result| result.get("positionMode"))
            .filter(|mode| js_truthy(Some(mode)));
        let mode = match confirmed {
            Some(mode) => match mode.as_str() {
                Some("hedged") => PositionMode::Hedged,
                Some("oneway") => PositionMode::OneWay,
                _ => {
                    return Err(StoreMutationError::InvalidOperation(
                        "Blofin worker result has an unsupported positionMode",
                    ));
                }
            },
            None if enabled => PositionMode::Hedged,
            None => PositionMode::OneWay,
        };
        apply_confirmed_position_mode(modes, store, mode)?;
    } else if method == "changeMarginType" {
        apply_confirmed_margin_type(
            modes,
            store,
            if enabled {
                MarginType::Isolated
            } else {
                MarginType::Cross
            },
        )?;
    }
    Ok(())
}

pub struct PositionSideOptions<'a> {
    pub symbol: &'a str,
    pub side: safe_cex_types::ModelField<OrderSide>,
    pub order_type: safe_cex_types::ModelField<OrderType>,
    pub reduce_only: bool,
}

pub async fn get_position_side<E, F: Future<Output = Result<Option<PositionMode>, E>>>(
    options: &PositionSideOptions<'_>,
    get_or_fetch_position_mode: impl FnOnce(&str) -> F,
) -> Result<&'static str, E> {
    if get_or_fetch_position_mode(options.symbol).await? == Some(PositionMode::OneWay) {
        return Ok("net");
    }
    let mut side = if options.side == OrderSide::Buy {
        "long"
    } else {
        "short"
    };
    if matches!(
        options.order_type.into_value(),
        Some(OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss)
    ) || options.reduce_only
    {
        side = if side == "long" { "short" } else { "long" };
    }
    Ok(side)
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_num::numeric_ingress::js_from_str;
    #[test]
    fn native_prepared_mode_update_dedupes_before_store_normalization() {
        let store = DefaultStore::new();
        let positions=js_from_str(r#"[
            {"id":"BTCUSDT:long","symbol":"BTCUSDT","side":"long","positionMode":"hedged","entryPrice":101.5,"notional":17,"leverage":3,"unrealizedPnl":5,"contracts":1,"liquidationPrice":11},
            {"id":"BTCUSDT:short","symbol":"BTCUSDT","side":"short","positionMode":"hedged","entryPrice":205.5,"notional":37,"leverage":7,"unrealizedPnl":9,"contracts":2,"liquidationPrice":23}
        ]"#).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            Store::positions(&store).unwrap().len(),
            2,
            "independent_input_floor"
        );
        let prepared = prepare_confirmed_mode_update(&store, PositionMode::OneWay).unwrap();
        let native = prepared.unwrap().positions.unwrap();
        assert_eq!(native.len(), 1, "native_prepared_update_first_id_dedupe");
        assert_eq!(native[0].id, "BTCUSDT");
        assert_eq!(native[0].position_mode, Some(PositionMode::OneWay));
        assert_eq!(
            native[0].entry_price, 101.5,
            "native_prepared_update_first_collision"
        );
        assert_eq!(native[0].notional, 17.0);
        assert_eq!(native[0].leverage, 3.0);
        assert_eq!(native[0].contracts, 1.0);
        assert_eq!(
            Store::positions(&store).unwrap().len(),
            2,
            "prepare_does_not_write_store"
        );
    }
}

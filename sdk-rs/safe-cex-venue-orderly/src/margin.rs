//! Stateless margin helpers from OrderlyExchange; use the shared MarginType.
use safe_cex_types::{MarginType, OrderSide, PlaceOrderOpts, PositionMode, PositionSide};

pub fn map_margin_type(raw_margin_type: Option<&str>) -> MarginType {
    if raw_margin_type.is_some_and(|value| value.to_uppercase() == "ISOLATED") {
        MarginType::Isolated
    } else {
        MarginType::Cross
    }
}

pub fn to_orderly_margin_mode(margin_type: MarginType) -> &'static str {
    if margin_type == MarginType::Isolated {
        "ISOLATED"
    } else {
        "CROSS"
    }
}

pub fn create_orderly_position_id(
    symbol: &str,
    side: PositionSide,
    position_mode: PositionMode,
    margin_type: MarginType,
) -> String {
    let base_id =
        safe_cex_types::model_utils::create_position_id(symbol, Some(side), Some(position_mode));
    if margin_type == MarginType::Isolated {
        format!("{base_id}:isolated")
    } else {
        base_id
    }
}

/// The exchange owns the cache; read positions from the canonical Store only on a cache miss.
pub fn get_current_margin_type(
    symbol: &str,
    margin_type_by_symbol: &std::collections::HashMap<String, MarginType>,
    store: &dyn safe_cex_store::Store,
) -> Result<MarginType, safe_cex_types::StoreReadError> {
    if let Some(cached) = margin_type_by_symbol.get(symbol) {
        return Ok(*cached);
    }
    let positions = store.positions()?;
    let mut active_margin_types = Vec::new();
    for position in positions
        .iter()
        .filter(|position| position.symbol == symbol && position.contracts > 0.0)
    {
        if !active_margin_types.contains(&position.margin_type) {
            active_margin_types.push(position.margin_type);
        }
    }
    Ok(if active_margin_types.len() == 1 {
        active_margin_types[0].unwrap_or(MarginType::Cross)
    } else {
        MarginType::Cross
    })
}

pub fn resolve_margin_type_for_order(
    opts: &PlaceOrderOpts,
    margin_type_by_symbol: &std::collections::HashMap<String, MarginType>,
    store: &dyn safe_cex_store::Store,
) -> Result<MarginType, safe_cex_exchange::runtime::ExchangeError> {
    let cached = margin_type_by_symbol.get(&opts.symbol).copied();
    if !opts.reduce_only.unwrap_or(false) {
        return match cached {
            Some(cached) => Ok(cached),
            None => Ok(get_current_margin_type(
                &opts.symbol,
                margin_type_by_symbol,
                store,
            )?),
        };
    }
    let closing_side = if opts.side == OrderSide::Buy {
        PositionSide::Short
    } else {
        PositionSide::Long
    };
    let candidates: Vec<Option<MarginType>> = store
        .positions()?
        .iter()
        .filter(|position| {
            position.symbol == opts.symbol
                && position.side == closing_side
                && position.contracts > 0.0
        })
        .map(|position| position.margin_type.into_value())
        .collect();
    if let Some(cached) = cached {
        let selected = candidates
            .iter()
            .filter(|margin| **margin == Some(cached))
            .count();
        if selected == 1 {
            return Ok(cached);
        }
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].unwrap_or(MarginType::Cross));
    }
    if candidates.len() > 1 {
        return Err(safe_cex_exchange::runtime::ExchangeError::new(format!(
            "Orderly reduce-only order for {} is ambiguous across Cross and Isolated positions; select the intended margin mode first",
            opts.symbol
        )));
    }
    match cached {
        Some(cached) => Ok(cached),
        None => Ok(get_current_margin_type(
            &opts.symbol,
            margin_type_by_symbol,
            store,
        )?),
    }
}

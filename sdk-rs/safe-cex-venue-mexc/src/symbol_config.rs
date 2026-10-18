//! Pure symbol settings selection used by the MEXC leverage override.
use crate::{
    exchange_helpers::{find_open_position, native_position_id},
    utils::{normalize_retained_margin_type, to_number},
};
use safe_cex_num::{JsCoercionError, JsJsonValue, js_number_string};
use safe_cex_types::{
    MarginType, Market, OrderSide, PlaceOrderOpts, Position, PositionMode, PositionSide,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MexcSymbolConfig {
    pub margin_type: MarginType,
    pub buy_leverage: f64,
    pub sell_leverage: f64,
    pub position_mode: PositionMode,
}

fn leverage(value: f64, fallback: f64) -> Result<f64, JsCoercionError> {
    // Source toNumber stringifies even an already-numeric leverage, including -0.
    to_number(
        Some(&Value::String(js_number_string(value))),
        Some(fallback),
    )
}

// ts: MexcExchange.deriveSymbolConfig
pub fn derive_symbol_config(
    symbol: &str,
    positions: &[Position],
    existing: Option<&MexcSymbolConfig>,
    account_mode: PositionMode,
) -> Result<MexcSymbolConfig, JsCoercionError> {
    let positions: Vec<_> = positions
        .iter()
        .filter(|p| p.symbol == symbol && p.contracts != 0.0 && !p.contracts.is_nan())
        .collect();
    let position = positions.first().copied();
    let position_mode = position
        .and_then(|p| (p.position_mode).into_value())
        .or_else(|| existing.map(|c| c.position_mode))
        .unwrap_or(account_mode);
    let long = positions.iter().find(|p| p.side == PositionSide::Long);
    let short = positions.iter().find(|p| p.side == PositionSide::Short);
    let one_way = if position_mode == PositionMode::OneWay {
        position.map(|p| leverage(p.leverage, 20.0)).transpose()?
    } else {
        None
    };
    let margin = position
        .and_then(|p| (p.margin_type).into_value())
        .or_else(|| existing.map(|c| c.margin_type))
        .map(|margin| {
            safe_cex_types::retained_json::RetainedJsonInput::Text(
                match margin {
                    MarginType::Isolated => "isolated",
                    MarginType::Cross => "cross",
                }
                .encode_utf16()
                .collect(),
            )
        });
    let margin = position
        .and_then(|p| p.info.as_ref())
        .and_then(|info| info.get("openType"))
        .filter(|v| !v.is_nullish())
        .or(margin.as_ref());
    Ok(MexcSymbolConfig {
        margin_type: normalize_retained_margin_type(margin)?,
        buy_leverage: match long {
            Some(p) => leverage(p.leverage, existing.map_or(20.0, |c| c.buy_leverage))?,
            None => one_way
                .or_else(|| existing.map(|c| c.buy_leverage))
                .unwrap_or(20.0),
        },
        sell_leverage: match short {
            Some(p) => leverage(p.leverage, existing.map_or(20.0, |c| c.sell_leverage))?,
            None => one_way
                .or_else(|| existing.map(|c| c.sell_leverage))
                .unwrap_or(20.0),
        },
        position_mode,
    })
}

// ts: MexcExchange.getLeverageForOrder
pub fn get_leverage_for_order(
    opts: &PlaceOrderOpts,
    positions: &[Position],
    existing: Option<&MexcSymbolConfig>,
    account_mode: PositionMode,
) -> Result<f64, JsCoercionError> {
    let derived;
    let config = match existing {
        Some(config) => config,
        None => {
            derived = derive_symbol_config(&opts.symbol, positions, None, account_mode)?;
            &derived
        }
    };
    Ok(if opts.reduce_only.unwrap_or(false) {
        if opts.side == OrderSide::Sell {
            config.buy_leverage
        } else {
            config.sell_leverage
        }
    } else if opts.side == OrderSide::Buy {
        config.buy_leverage
    } else {
        config.sell_leverage
    })
}

// ts: MexcExchange.formatChangeLeveragePayload
pub fn format_change_leverage_payload(
    symbol: &str,
    market: &Market,
    position_type: u8,
    leverage: f64,
    margin_type: MarginType,
    include_open_type_with_position_id: bool,
    positions: &[Position],
) -> Result<JsJsonValue, JsCoercionError> {
    let side = if position_type == 1 {
        PositionSide::Long
    } else {
        PositionSide::Short
    };
    let position = find_open_position(symbol, side, positions);
    let position_id = position
        .map(native_position_id)
        .transpose()?
        .unwrap_or_default();
    let open_type = match margin_type {
        MarginType::Isolated => 1,
        MarginType::Cross => 2,
    };
    if !position_id.is_empty() {
        let mut payload = vec![
            ("positionId".to_owned(), JsJsonValue::String(position_id)),
            ("leverage".to_owned(), JsJsonValue::Number(leverage)),
        ];
        if include_open_type_with_position_id {
            payload.push(("openType".to_owned(), JsJsonValue::Number(open_type as f64)));
        }
        Ok(JsJsonValue::Object(payload))
    } else {
        Ok(JsJsonValue::Object(vec![
            ("symbol".to_owned(), JsJsonValue::String(market.id.clone())),
            (
                "positionType".to_owned(),
                JsJsonValue::Number(position_type as f64),
            ),
            ("leverage".to_owned(), JsJsonValue::Number(leverage)),
            ("openType".to_owned(), JsJsonValue::Number(open_type as f64)),
        ]))
    }
}

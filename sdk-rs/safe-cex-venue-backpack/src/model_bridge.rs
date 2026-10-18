//! Typed SDK projection preserves native numeric fields before the wire boundary.
use crate::native_value::NativeValue;
use crate::{
    adapter::prepared_json, native_value::Value, order_actions as actions, rest_reads as reads,
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::*;
fn text(s: &[u16]) -> Result<String, ExchangeError> {
    String::from_utf16(s).map_err(|e| ExchangeError::new(e.to_string()))
}
pub fn options(o: &actions::Options) -> Result<PreparedOrderOpts, ExchangeError> {
    Ok(PreparedOrderOpts {
        symbol: text(&o.symbol)?,
        r#type: o.r#type,
        side: o.side,
        amount: o.amount,
        price: o.price,
        trigger_price: o.trigger_price,
        stop_loss: o.stop_loss,
        take_profit: o.take_profit,
        reduce_only: o.reduce_only.into_property(),
        time_in_force: o.time_in_force,
        trigger_type: None,
        tp_trigger_type: o.tp_trigger_type,
        sl_trigger_type: o.sl_trigger_type,
        client_order_id: o.client_order_id.as_deref().map(text).transpose()?,
        max_slippage_pct: None,
        close: None,
        implied_volatility: None,
    })
}
pub fn action(o: PlaceOrderOpts) -> actions::Options {
    actions::Options {
        symbol: o.symbol.encode_utf16().collect(),
        r#type: o.r#type.into(),
        side: o.side.into(),
        amount: o.amount,
        price: o.price,
        trigger_price: o.trigger_price,
        take_profit: o.take_profit,
        stop_loss: o.stop_loss,
        time_in_force: o.time_in_force,
        reduce_only: o.reduce_only.unwrap_or(false).into(),
        tp_trigger_type: o.tp_trigger_type,
        sl_trigger_type: o.sl_trigger_type,
        client_order_id: o.client_order_id.map(|s| s.encode_utf16().collect()),
        ..Default::default()
    }
}
pub fn market(m: reads::Market<Value>) -> Result<Market, ExchangeError> {
    Ok(Market {
        id: text(&m.id)?,
        symbol: text(&m.symbol)?,
        base: text(&m.base)?,
        quote: m
            .quote
            .as_deref()
            .map(text)
            .transpose()?
            .unwrap_or_default(),
        pretty_symbol: None,
        active: m.active,
        precision: MarketPrecision {
            amount: m.precision[0],
            price: m.precision[1],
        },
        limits: MarketLimits {
            amount: MarketLimitsAmount {
                min: m.limits[0][0],
                max: m.limits[0][1],
            },
            leverage: MarketLimitsLeverage {
                min: m.limits[3][0],
                max: m.limits[3][1],
            },
            price: Some(MarketLimitsPrice {
                min: m.limits[1][0],
            }),
        },
        contract_size: Some(m.contract_size),
        is_expiring: None,
        is_inverse: None,
        is_spot: Some(m.is_spot),
        is_perpetual: None,
        is_read_only: None,
        info: Some(prepared_json(&m.info)?),
        taker_fee: None,
        maker_fee: None,
        option: None,
    })
}
pub fn position(p: reads::Position) -> Result<Position, ExchangeError> {
    Ok(Position {
        id: text(&p.id)?,
        symbol: text(&p.symbol)?,
        side: (p.side).into(),
        entry_price: p.entry_price,
        notional: p.notional,
        leverage: p.leverage,
        margin_type: (Some(p.margin_type)).into(),
        position_mode: (Some(p.position_mode)).into(),
        unrealized_pnl: p.unrealized_pnl,
        realized_pnl: (None).into(),
        contracts: p.contracts,
        liquidation_price: p.liquidation_price,
        margin_size: (None).into(),
        info: (None).into(),
        extra: (None).into(),
        last_update: (Some(p.last_update)).into(),
        option: (None).into(),
    })
}
pub fn ticker(t: reads::Ticker<Value>) -> Result<Ticker, ExchangeError> {
    Ok(Ticker {
        id: text(&t.id)?,
        symbol: text(&t.symbol)?,
        bid: t.bid,
        ask: t.ask,
        last: t.last,
        mark: t.mark,
        index: t.index,
        mid_price: None,
        percentage: t.percentage,
        open_interest: t.open_interest,
        funding_rate: t.funding_rate,
        funding_time: t.funding_time.number()?,
        volume: t.volume,
        quote_volume: t.quote_volume,
        timestamp: None,
        info: Some(prepared_json(&t.info)?),
    })
}
pub fn candle(c: reads::Candle) -> Candle {
    Candle {
        timestamp: c.timestamp,
        open: c.open,
        high: c.high,
        low: c.low,
        close: c.close,
        volume: c.volume,
        last_side: None,
    }
}
pub fn order(u: crate::private_events::OrderUpdate) -> Result<Order, ExchangeError> {
    Ok(Order {
        client_order_id: (None).into(),
        id: text(&u.id)?,
        parent_id: (None).into(),
        status: (u.status).into(),
        symbol: text(&u.symbol)?,
        r#type: (u.order_type).into(),
        side: (u.side).into(),
        price: (u.price).into(),
        amount: u.amount,
        filled: u.filled,
        remaining: u.remaining,
        reduce_only: u.reduce_only.into(),
        close: (None).into(),
        info: (None).into(),
        time_in_force: (Some(u.time_in_force)).into(),
        last_update: u.last_update,
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    })
}
pub fn private_position(
    u: crate::private_events::PositionUpdate,
) -> Result<Position, ExchangeError> {
    Ok(Position {
        id: text(&u.id)?,
        symbol: text(&u.symbol)?,
        side: (u.side).into(),
        entry_price: u.entry_price,
        notional: u.notional,
        leverage: u.leverage,
        margin_type: (Some(u.margin_type)).into(),
        position_mode: (Some(u.position_mode)).into(),
        unrealized_pnl: u.unrealized_pnl,
        realized_pnl: (None).into(),
        contracts: u.contracts,
        liquidation_price: u.liquidation_price,
        margin_size: (None).into(),
        info: (None).into(),
        extra: (None).into(),
        last_update: (Some(u.last_update)).into(),
        option: (None).into(),
    })
}

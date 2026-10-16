//! Family membership and virtual attached TP/SL IDs.

// ts: BITGET_LIKE_EXCHANGES
pub const BITGET_LIKE_EXCHANGES: &[&str] = &["bitget", "bitgetusdc", "bitgetinverse"];

// ts: isBitgetLikeExchange
pub fn is_bitget_like_exchange(exchange: Option<&str>) -> bool {
    exchange.is_some_and(|name| BITGET_LIKE_EXCHANGES.contains(&name))
}

// ts: isBitgetVirtualAttachedTpSlOrderId
pub fn is_bitget_virtual_attached_tp_sl_order_id(id: Option<&str>) -> bool {
    id.is_some_and(|id| id.ends_with("__stop_loss") || id.ends_with("__take_profit"))
}

// ts: isBitgetVirtualAttachedTpSlOrder
pub fn is_bitget_virtual_attached_tp_sl_order(
    order_id: Option<&str>,
    exchange_name: Option<&str>,
) -> bool {
    is_bitget_like_exchange(exchange_name) && is_bitget_virtual_attached_tp_sl_order_id(order_id)
}

use safe_cex_types::PlaceOrderOpts;

// ts: BinancePMExchange.getOrderPositionSide
pub fn get_order_position_side(is_hedged: bool, opts: &PlaceOrderOpts) -> &'static str {
    crate::order_logic::get_order_position_side(is_hedged, opts)
}

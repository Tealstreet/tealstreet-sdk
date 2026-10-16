//! Exact suffix handling from BitunixExchange's virtual TP/SL helpers.
/// Return the source leg string; TP is checked before SL.
pub fn get_virtual_tpsl_leg(order_id: &str) -> Option<&'static str> {
    if order_id.ends_with("__tp") {
        return Some("tp");
    }
    if order_id.ends_with("__sl") {
        return Some("sl");
    }
    None
}

/// Remove exactly one recognized suffix, preserving the complete prefix.
pub fn get_virtual_tpsl_parent_order_id(order_id: &str) -> &str {
    let Some(leg) = get_virtual_tpsl_leg(order_id) else {
        return order_id;
    };
    &order_id[..order_id.len() - (2 + leg.len())]
}

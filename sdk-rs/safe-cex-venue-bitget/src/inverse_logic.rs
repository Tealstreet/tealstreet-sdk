//! Pure decisions belonging to the inverse variant.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderType, PositionMode};

// ts: BitgetInverseExchange.getInverseAlgoOrderSide
pub fn get_inverse_algo_order_side(
    order_type: OrderType,
    side: OrderSide,
    mode: PositionMode,
) -> Result<&'static str, ExchangeError> {
    // The two TS methods have identical bodies; the oracle executes each source independently.
    crate::order_logic::get_algo_order_side(order_type, side, mode)
}

// ts: BitgetInverseExchange.generateUuid
/// The caller supplies Math.random's [0, 1) operation (the shared runtime's random).
pub fn generate_uuid(mut random: impl FnMut() -> f64) -> String {
    let mut result = String::with_capacity(36);
    for c in "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".chars() {
        if c == 'x' || c == 'y' {
            let r = (random() * 16.0) as u32;
            let value = if c == 'x' { r } else { (r & 0x3) | 0x8 };
            result.push(char::from_digit(value, 16).expect("Math.random is in [0, 1)"));
        } else {
            result.push(c);
        }
    }
    result
}

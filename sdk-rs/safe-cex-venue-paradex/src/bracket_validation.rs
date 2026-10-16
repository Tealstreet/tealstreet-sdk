//! Source TP/SL comparisons retain JavaScript numeric truthiness and warning order.
use safe_cex_num::js_number_string;
use safe_cex_types::OrderSide;

// ts: ParadexExchange#validateTpSlPrices
pub fn validate_tp_sl_prices<E>(
    side: OrderSide,
    price: Option<f64>,
    take_profit: Option<f64>,
    stop_loss: Option<f64>,
    log: impl FnOnce(String, &'static str) -> Result<(), E>,
) -> Result<bool, E> {
    let truthy = |value: f64| value != 0.0 && !value.is_nan();
    let Some(entry) = price.filter(|price| truthy(*price)) else {
        return Ok(true);
    };
    let long = side == OrderSide::Buy;
    if let Some(tp) = take_profit.filter(|price| truthy(*price)) {
        if (long && tp <= entry) || (!long && tp >= entry) {
            log(
                format!(
                    "Invalid TP for {}: {} must be {} entry {}",
                    if long { "long" } else { "short" },
                    js_number_string(tp),
                    if long { ">" } else { "<" },
                    js_number_string(entry)
                ),
                "warning",
            )?;
            return Ok(false);
        }
    }
    if let Some(sl) = stop_loss.filter(|price| truthy(*price)) {
        if (long && sl >= entry) || (!long && sl <= entry) {
            log(
                format!(
                    "Invalid SL for {}: {} must be {} entry {}",
                    if long { "long" } else { "short" },
                    js_number_string(sl),
                    if long { "<" } else { ">" },
                    js_number_string(entry)
                ),
                "warning",
            )?;
            return Ok(false);
        }
    }
    Ok(true)
}

//! Source symbol transformations; see manifest notes for representation/cache boundaries.
pub fn normalize_symbol(value: &str) -> String {
    let mut parts = value.split('_');
    let _prefix = parts.next();
    let base_asset = parts.next().unwrap_or("undefined");
    let quote_asset = parts.next().unwrap_or("undefined");
    format!("{base_asset}{quote_asset}")
}

pub fn reverse_symbol(value: &str) -> String {
    let asset = value.replacen("USDC", "", 1);
    format!("PERP_{asset}_USDC")
}

pub fn is_algo_order(order_type: safe_cex_types::OrderType) -> bool {
    use safe_cex_types::OrderType;
    order_type == OrderType::StopLoss
        || order_type == OrderType::TakeProfit
        || order_type == OrderType::TrailingStopLoss
}

/// The caller supplies the exchange's awaited position-mode getter.
pub async fn get_position_side_for_order<E, F>(
    opts: &safe_cex_types::PlaceOrderOpts,
    get_position_mode: impl FnOnce() -> F,
) -> Result<Option<&'static str>, E>
where
    F: std::future::Future<Output = Result<safe_cex_types::PositionMode, E>>,
{
    use safe_cex_types::{OrderSide, PositionMode};
    let position_mode = get_position_mode().await?;
    let reduce_only = opts.reduce_only;
    let mut position_side = None;
    if position_mode == PositionMode::Hedged {
        if reduce_only.unwrap_or(false) {
            if opts.side == OrderSide::Buy {
                position_side = Some("SHORT");
            } else {
                position_side = Some("LONG");
            }
        } else if opts.side == OrderSide::Buy {
            position_side = Some("LONG");
        } else {
            position_side = Some("SHORT");
        }
    }
    Ok(position_side)
}

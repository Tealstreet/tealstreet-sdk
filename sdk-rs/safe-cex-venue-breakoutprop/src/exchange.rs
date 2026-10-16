//! Standalone source helpers; concrete ownership/session/polling are in lifecycle and polling.
use safe_cex_num::number_from_string;

pub fn parse_dx_session_timeout_ms(timeout: &str) -> Result<f64, String> {
    let parts: Vec<f64> = timeout.split(':').map(number_from_string).collect();
    if parts.len() != 3 || parts.iter().any(|part| !part.is_finite() || *part < 0.0) {
        return Err(format!("Invalid DXtrade session timeout: {timeout}"));
    }
    let (hours, minutes, seconds) = (parts[0], parts[1], parts[2]);
    if minutes >= 60.0 || seconds >= 60.0 {
        return Err(format!("Invalid DXtrade session timeout: {timeout}"));
    }
    Ok((hours * 3600.0 + minutes * 60.0 + seconds) * 1000.0)
}

// Source takes a TS enum but runtime strings reach its explicit rejection branch.
pub fn to_dx_order_type(order_type: &str) -> Result<&'static str, String> {
    if order_type == "limit" {
        return Ok("LIMIT");
    }
    if order_type == "market" {
        return Ok("MARKET");
    }
    if order_type == "stop_market" || order_type == "take_profit_market" {
        return Ok("STOP");
    }
    Err(format!(
        "Breakout Prop does not support {order_type} orders"
    ))
}

//! WOO IOC pricing and rejection predicates; transport dispatch is separate.
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use safe_cex_types::{Market, OrderSide, Ticker};
use serde_json::Value;

// ts: isPositiveFinitePrice
pub fn is_positive_finite_price(price: Option<f64>) -> bool {
    price.is_some_and(|p| p.is_finite() && p > 0.0)
}
fn nonnegative(value: f64) -> f64 {
    if value.is_nan() {
        value
    } else {
        value.max(0.0)
    }
}
// ts: applyMarketIocSlippage
pub fn apply_market_ioc_slippage(price: f64, side: OrderSide, slippage_pct: f64) -> f64 {
    let factor = nonnegative(slippage_pct) / 100.0;
    price
        * if side == OrderSide::Buy {
            1.0 + factor
        } else {
            1.0 - factor
        }
}
fn fair_price(ticker: Option<&Ticker>) -> Option<f64> {
    ticker.and_then(|t| {
        [Some(t.mark), Some(t.index), t.mid_price, Some(t.last)]
            .into_iter()
            .flatten()
            .find(|v| is_positive_finite_price(Some(*v)))
    })
}
// ts: chooseMarketIocOrderPrice
pub fn choose_market_ioc_order_price(
    ticker: Option<&Ticker>,
    side: OrderSide,
    slippage_pct: f64,
    price_range_pct: f64,
    touch_price: Option<f64>,
) -> Option<f64> {
    let quote =
        touch_price.or_else(|| ticker.map(|t| if side == OrderSide::Buy { t.ask } else { t.bid }));
    if let Some(fair) = fair_price(ticker) {
        let crossing = apply_market_ioc_slippage(fair, side, slippage_pct);
        if let Some(quote) = quote.filter(|v| is_positive_finite_price(Some(*v))) {
            let factor = nonnegative(price_range_pct) / 100.0;
            let within = if side == OrderSide::Buy {
                quote <= fair * (1.0 + factor)
            } else {
                quote >= fair * (1.0 - factor)
            };
            let misses = if side == OrderSide::Buy {
                crossing < quote
            } else {
                crossing > quote
            };
            if within && misses {
                return Some(quote);
            }
        }
        return Some(crossing);
    }
    quote
        .filter(|v| is_positive_finite_price(Some(*v)))
        .or_else(|| {
            ticker
                .map(|t| if side == OrderSide::Buy { t.bid } else { t.ask })
                .filter(|v| is_positive_finite_price(Some(*v)))
        })
        .map(|price| apply_market_ioc_slippage(price, side, slippage_pct))
}
// ts: getWooMarketPriceRangePct
pub fn get_woo_market_price_range_pct(market: Option<&Market>) -> Result<f64, JsCoercionError> {
    let range = safe_cex_num::js_retained_number(
        market
            .and_then(|m| m.info.as_ref())
            .and_then(|v| v.get("price_range")),
    )?;
    Ok(if range.is_finite() && range > 0.0 {
        range * 100.0
    } else {
        1.0
    })
}
fn message(error: Option<&Value>) -> Result<String, JsCoercionError> {
    let nested = error
        .and_then(|v| v.get("response"))
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("message"));
    let selected = nested.filter(|v| js_truthy(Some(v))).or_else(|| {
        error
            .and_then(|v| v.get("message"))
            .filter(|v| js_truthy(Some(v)))
    });
    js_string(selected.or(Some(&Value::String(String::new()))))
}
// ts: isWooRateLimitError
pub fn is_woo_rate_limit_error(error: Option<&Value>) -> Result<bool, JsCoercionError> {
    let response = error.and_then(|v| v.get("response"));
    let equals = |value: Option<&Value>, expected| value.and_then(Value::as_f64) == Some(expected);
    if equals(response.and_then(|v| v.get("status")), 429.0)
        || equals(
            response
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("code")),
            -1003.0,
        )
        || equals(
            response
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("error_code")),
            90003.0,
        )
    {
        return Ok(true);
    }
    Ok(message(error)?.to_lowercase().contains("rate limit"))
}
// ts: isWooPositionModeAlreadySetError
pub fn is_woo_position_mode_already_set_error(
    error: Option<&Value>,
) -> Result<bool, JsCoercionError> {
    Ok(message(error)?
        .to_lowercase()
        .contains("position mode remains unchanged"))
}
// ts: isPostOnlyCrossingPrice
pub fn is_post_only_crossing_price(side: OrderSide, price: f64, touch_price: f64) -> bool {
    if side == OrderSide::Buy {
        price >= touch_price
    } else {
        price <= touch_price
    }
}
// ts: isPostOnlyCrossingFairPrice
pub fn is_post_only_crossing_fair_price(
    side: OrderSide,
    price: f64,
    ticker: Option<&Ticker>,
) -> Option<(f64, bool)> {
    fair_price(ticker).map(|fair| (fair, is_post_only_crossing_price(side, price, fair)))
}

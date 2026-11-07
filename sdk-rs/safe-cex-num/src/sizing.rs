//! Native sizing, currency and account leverage calculations.
use crate::{adjust, fixed_number, handle_rounding_error, math_round};
use safe_cex_types::{Balance, Market};
fn contract_size(market: &Market) -> f64 {
    market
        .contract_size
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(1.0)
}
fn inverse(market: &Market) -> bool {
    market.is_inverse == Some(true)
}
fn quanto(market: &Market) -> bool {
    market
        .info
        .as_ref()
        .and_then(|value| value.get("isQuanto"))
        .is_some_and(safe_cex_types::retained_json::RetainedJsonInput::is_truthy)
}
fn present(value: Option<f64>, message: &str) -> Option<f64> {
    match value.filter(|v| *v != 0.0 && !v.is_nan()) {
        Some(v) => Some(v),
        None => {
            eprintln!("{message}");
            None
        }
    }
}
fn default_precision(value: f64) -> f64 {
    if value.is_nan() {
        return 0.0;
    }
    let x = value.abs();
    let digits = if x == 0.0 {
        1
    } else if x >= 1000.0 {
        2
    } else if x >= 10.0 {
        3
    } else if x >= 1.0 {
        4
    } else if x >= 1e-4 {
        5
    } else if x >= 1e-5 {
        6
    } else if x >= 1e-6 {
        7
    } else if x >= 1e-7 {
        8
    } else if x >= 1e-8 {
        9
    } else if x >= 1e-9 {
        10
    } else if x >= 1e-10 {
        11
    } else {
        12
    };
    fixed_number(value, digits)
}
pub fn convert_from_quote_size_to_native(size: f64, market: &Market, price: f64) -> f64 {
    adjust(
        if inverse(market) { size } else { size / price } / contract_size(market),
        market.precision.amount,
    )
}
pub fn convert_from_base_size_to_native(size: f64, market: &Market, price: f64) -> f64 {
    adjust(
        if inverse(market) { size * price } else { size } / contract_size(market),
        market.precision.amount,
    )
}
pub fn convert_from_native_size_to_quote(size: f64, market: &Market, price: f64) -> f64 {
    let units = size * contract_size(market);
    default_precision(if inverse(market) {
        units
    } else {
        units * price
    })
}
pub fn convert_from_native_size_to_base(size: f64, market: &Market, price: f64) -> f64 {
    let units = size * contract_size(market);
    default_precision(if inverse(market) {
        units / price
    } else {
        units
    })
}
pub fn convert_margin_to_size_currency(
    margin: f64,
    market: &Market,
    price: f64,
    btc_price: Option<f64>,
) -> f64 {
    if quanto(market) {
        let Some(btc) = present(btc_price, "btcPrice is required for quanto markets") else {
            return 0.0;
        };
        convert_from_quote_size_to_native(handle_rounding_error(margin * btc), market, price)
    } else if inverse(market) {
        convert_from_base_size_to_native(margin, market, price)
    } else {
        convert_from_quote_size_to_native(margin, market, price)
    }
}
pub fn convert_size_to_margin_currency(
    size: f64,
    market: &Market,
    price: f64,
    btc_price: Option<f64>,
) -> f64 {
    if quanto(market) {
        let Some(btc) = present(btc_price, "btcPrice is required for quanto markets") else {
            return 0.0;
        };
        handle_rounding_error(convert_from_native_size_to_quote(size, market, price) / btc)
    } else if inverse(market) {
        convert_from_native_size_to_base(size, market, price)
    } else {
        convert_from_native_size_to_quote(size, market, price)
    }
}
pub fn convert_from_margin_to_quote(
    margin: f64,
    market: &Market,
    price: Option<f64>,
    btc_price: Option<f64>,
) -> f64 {
    if quanto(market) {
        let Some(btc) = present(btc_price, "btcPrice is required for quanto markets") else {
            return 0.0;
        };
        handle_rounding_error(margin * btc)
    } else if inverse(market) {
        let Some(price) = present(price, "price is required for inverse markets") else {
            return 0.0;
        };
        handle_rounding_error(margin * price)
    } else {
        margin
    }
}
pub fn get_size_currency(market: Option<&Market>) -> &str {
    let Some(m) = market else {
        return "";
    };
    if contract_size(m) != 1.0 {
        "Contracts"
    } else if inverse(m) {
        &m.quote
    } else {
        &m.base
    }
}
pub fn get_margin_currency<'a>(market: Option<&'a Market>, fallback: &'a str) -> &'a str {
    let Some(m) = market else {
        return fallback;
    };
    if quanto(m) {
        "BTC"
    } else if m.is_spot == Some(true) {
        ""
    } else {
        let v = if inverse(m) { &m.base } else { &m.quote };
        if v.is_empty() { fallback } else { v }
    }
}
pub fn get_contract_type(market: Option<&Market>) -> &'static str {
    match market {
        Some(m) if inverse(m) => "Inverse",
        Some(m) if quanto(m) => "Quanto",
        _ => "Linear",
    }
}
pub fn calculate_pnl_in_margin_currency(
    size: f64,
    entry: f64,
    exit: f64,
    is_long: bool,
    market: &Market,
    btc_price: Option<f64>,
) -> f64 {
    let units = size * contract_size(market);
    let pnl = if quanto(market) {
        let Some(btc) = present(btc_price, "btcPrice is required for quanto markets") else {
            return 0.0;
        };
        units * (exit - entry) / btc
    } else if inverse(market) {
        units * (1.0 / entry - 1.0 / exit)
    } else {
        units * (exit - entry)
    };
    handle_rounding_error(if is_long { pnl } else { -pnl })
}
pub fn convert_margin_risk_to_native_size(
    risk: f64,
    entry: f64,
    stop: f64,
    market: &Market,
    btc_price: Option<f64>,
) -> f64 {
    let raw = if inverse(market) {
        risk / (1.0 / entry - 1.0 / stop).abs() / contract_size(market)
    } else {
        convert_from_margin_to_quote(risk, market, Some(entry), btc_price)
            / (entry - stop).abs()
            / contract_size(market)
    };
    let quotient = raw / market.precision.amount;
    let nearest = math_round(quotient);
    let exact = (quotient - nearest).abs() <= quotient.abs().max(1.0) * 1e-12;
    handle_rounding_error(if exact { nearest } else { quotient.floor() } * market.precision.amount)
}
pub fn calculate_position_account_leverage(notional: f64, balance: f64) -> f64 {
    if balance <= 0.0 {
        0.0
    } else {
        notional.abs() / balance
    }
}
pub fn calculate_total_account_leverage(notionals: &[f64], balance: f64) -> f64 {
    if balance <= 0.0 {
        0.0
    } else {
        calculate_total_notional(notionals) / balance
    }
}
pub fn calculate_total_notional(notionals: &[f64]) -> f64 {
    notionals.iter().fold(0.0, |sum, v| sum + v.abs())
}
/// A borrowed source row, preserving the account or currency object shape.
#[derive(Clone, Copy, serde::Serialize)]
#[serde(untagged)]
pub enum BalanceSlice<'a> {
    Account(&'a Balance),
    Currency(&'a safe_cex_types::CurrencyBalance),
}
impl<'a> BalanceSlice<'a> {
    pub fn as_balance(self) -> Option<&'a Balance> {
        match self {
            Self::Account(balance) => Some(balance),
            Self::Currency(currency) => currency.as_balance(),
        }
    }
}
pub fn get_balance_slice<'a>(
    balance: Option<&'a Balance>,
    market: Option<&Market>,
) -> Option<BalanceSlice<'a>> {
    let balance = balance?;
    let Some(market) = market else {
        return Some(BalanceSlice::Account(balance));
    };
    let currency = get_margin_currency(Some(market), "");
    if currency.is_empty() {
        return Some(BalanceSlice::Account(balance));
    }
    Some(
        balance
            .currencies
            .as_ref()
            .and_then(|v| v.get(currency))
            .map(BalanceSlice::Currency)
            .unwrap_or(BalanceSlice::Account(balance)),
    )
}
pub fn get_total_balance_from_balance(balance: Option<&Balance>, market: Option<&Market>) -> f64 {
    let total = get_balance_slice(balance, market)
        .and_then(BalanceSlice::as_balance)
        .map(|v| v.total)
        .unwrap_or(0.0);
    if total == 0.0 || total.is_nan() {
        0.0
    } else {
        total
    }
}

/// Convert base-denominated size to native linear contracts; inverse conversion is separate.
pub fn base_to_native_contracts(market: &Market, base_amount: f64) -> f64 {
    base_amount / market.contract_size.filter(|v| *v > 0.0).unwrap_or(1.0)
}
pub fn get_ticker_predicted_best_ask(
    ticker: &safe_cex_types::Ticker,
    market: Option<&Market>,
) -> f64 {
    if ticker.ask != 0.0 && !ticker.ask.is_nan() {
        return ticker.ask;
    }
    if ticker.last != 0.0
        && !ticker.last.is_nan()
        && let Some(price) = market
            .map(|m| m.precision.price)
            .filter(|v| *v != 0.0 && !v.is_nan())
    {
        return ticker.last + price;
    }
    ticker.last
}
pub fn get_ticker_predicted_best_bid(
    ticker: &safe_cex_types::Ticker,
    market: Option<&Market>,
) -> f64 {
    if ticker.bid != 0.0 && !ticker.bid.is_nan() {
        return ticker.bid;
    }
    if ticker.last != 0.0
        && !ticker.last.is_nan()
        && let Some(price) = market
            .map(|m| m.precision.price)
            .filter(|v| *v != 0.0 && !v.is_nan())
    {
        let result = ticker.last - price;
        return if result.is_nan() {
            result
        } else {
            result.max(0.0)
        };
    }
    ticker.last
}

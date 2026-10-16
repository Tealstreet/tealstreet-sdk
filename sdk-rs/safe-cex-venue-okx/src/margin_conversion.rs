// The host performs String(value ?? '') with its original coercion and errors.
pub fn to_finite_number<E>(string_value: impl FnOnce() -> Result<Vec<u16>, E>) -> Result<f64, E> {
    let units = string_value()?;
    // Surrogates cannot be decimal-prefix units or JS whitespace.
    let parsed = safe_cex_num::parse_float(&String::from_utf16_lossy(&units));
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

// None means absent/falsy; conversion owns String(value || '').toUpperCase().
pub fn get_contract_currency<V, E>(
    read_value: impl FnOnce() -> Result<Option<V>, E>,
    is_truthy: impl FnOnce(&V) -> bool,
    string_uppercase: impl FnOnce(Option<V>) -> Result<Vec<u16>, E>,
) -> Result<Vec<u16>, E> {
    let value = read_value()?;
    string_uppercase(value.filter(is_truthy))
}

pub fn margin_to_usd<E>(
    finite_amount: impl FnOnce() -> Result<f64, E>,
    has_market: bool,
    margin_currency: Option<&[u16]>,
    uppercase_margin: impl FnOnce(&[u16]) -> Result<Vec<u16>, E>,
    is_inverse: impl FnOnce() -> Result<bool, E>,
    uppercase_base: impl FnOnce() -> Result<Vec<u16>, E>,
    reference_price: impl FnOnce() -> Result<f64, E>,
) -> Result<f64, E> {
    let amount = finite_amount()?;
    let Some(currency) = margin_currency.filter(|s| !s.is_empty()) else {
        return Ok(amount);
    };
    if !has_market {
        return Ok(amount);
    }
    let currency = uppercase_margin(currency)?;
    if is_inverse()? && currency == uppercase_base()? {
        let price = reference_price()?;
        if price == 0.0 || price.is_nan() {
            return Ok(0.0);
        }
        return Ok(amount * price);
    }
    Ok(amount)
}

pub fn usd_to_margin<E>(
    finite_amount: impl FnOnce() -> Result<f64, E>,
    has_market: bool,
    margin_currency: Option<&[u16]>,
    uppercase_margin: impl FnOnce(&[u16]) -> Result<Vec<u16>, E>,
    is_inverse: impl FnOnce() -> Result<bool, E>,
    uppercase_base: impl FnOnce() -> Result<Vec<u16>, E>,
    reference_price: impl FnOnce() -> Result<f64, E>,
) -> Result<f64, E> {
    let amount = finite_amount()?;
    let Some(currency) = margin_currency.filter(|s| !s.is_empty()) else {
        return Ok(amount);
    };
    if !has_market {
        return Ok(amount);
    }
    let currency = uppercase_margin(currency)?;
    if is_inverse()? && currency == uppercase_base()? {
        let price = reference_price()?;
        if price == 0.0 || price.is_nan() {
            return Ok(0.0);
        }
        return Ok(amount / price);
    }
    Ok(amount)
}

pub fn contracts_to_base<E>(
    contracts: f64,
    contract_size: impl FnOnce() -> Result<Option<f64>, E>,
    contract_currency: impl FnOnce() -> Result<Vec<u16>, E>,
    is_inverse: impl FnOnce() -> Result<bool, E>,
    uppercase_quote: impl FnOnce() -> Result<Vec<u16>, E>,
    reference_price: impl FnOnce() -> Result<f64, E>,
) -> Result<f64, E> {
    let size = contract_size()?
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(1.0);
    let currency = contract_currency()?;
    if is_inverse()? && currency == uppercase_quote()? {
        let price = reference_price()?;
        if price == 0.0 || price.is_nan() {
            return Ok(0.0);
        }
        return Ok((contracts * size) / price);
    }
    Ok(safe_cex_num::multiply(contracts, size))
}

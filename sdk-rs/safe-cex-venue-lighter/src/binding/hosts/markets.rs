//! MarketReadHost on Bound.
use crate::{
    binding::{
        ops::{Arithmetic, JsIterator, arithmetic_value, spread_with_value},
        realm::Bound,
        value::{to_account, to_js, to_signing},
    },
    exchange_market_reads::{ForOfBody, ForOfStep, MarketReadHost},
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};

/// Number::exponentiate, including the NaN/±1**±Infinity cases powf differs on.
fn exponentiate(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() {
        return f64::NAN;
    }
    if exponent == 0.0 {
        return 1.0;
    }
    if base.is_nan() || (base.abs() == 1.0 && exponent.is_infinite()) {
        return f64::NAN;
    }
    base.powf(exponent)
}

impl MarketReadHost for Bound {
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Add, left, right)
    }
    fn number_power(&self, base: f64, exponent: f64) -> f64 {
        exponentiate(base, exponent)
    }
    fn function_value(&self, name: &str) -> SigningResult<AccountValue> {
        self.realm
            .global(name)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn for_of(&self, array: AccountValue, body: ForOfBody) -> SigningResult<Option<AccountValue>> {
        let iterator = JsIterator::open(&to_js(&array)).map_err(to_signing)?;
        while let Some(value) = iterator.step().map_err(to_signing)? {
            if let ForOfStep::Break(result) = body(to_account(&value))? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }
    fn spread_override(
        &self,
        source: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        spread_with_value(&source, fields)
    }
}

//! HistoricalFillsHost on Bound.
use crate::{
    binding::{
        ops::{Arithmetic, Relation, arithmetic_value, for_of, relation_bool, spread_values},
        realm::Bound,
        value::{native_to_js, to_account, to_js, to_signing},
    },
    exchange_historical_fills::{FillBinary, FillVisitor, HistoricalFillsHost},
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::SigningResult,
};
use safe_cex_exchange::js_runtime::regexp::regexp;
use safe_cex_exchange::js_runtime::value::Thrown;

impl HistoricalFillsHost for Bound {
    fn require_options_binding(&self, value: &AccountValue) -> SigningResult<()> {
        let value = to_js(value);
        if value.is_nullish() {
            return Err(to_signing(Thrown::type_error(&format!(
                "Cannot destructure property 'symbol' of 'opts' as it is {}.",
                value.to_display()
            ))));
        }
        Ok(())
    }
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), value))
    }
    fn binary(
        &self,
        op: FillBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        let relation = match op {
            FillBinary::Multiply => return arithmetic_value(Arithmetic::Multiply, left, right),
            FillBinary::Divide => return arithmetic_value(Arithmetic::Divide, left, right),
            FillBinary::Subtract => return arithmetic_value(Arithmetic::Subtract, left, right),
            FillBinary::LessThan => Relation::Less,
            FillBinary::LessOrEqual => Relation::LessOrEqual,
            FillBinary::GreaterThan => Relation::Greater,
        };
        relation_bool(relation, left, right)
    }
    fn regex(&self, pattern: &[u16], flags: &[u16]) -> SigningResult<AccountValue> {
        regexp(pattern, flags)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn for_each(&self, iterable: AccountValue, visitor: FillVisitor) -> SigningResult<()> {
        for_of(&iterable, |item| visitor(item))
    }
    fn spread(&self, iterable: AccountValue) -> SigningResult<Vec<AccountValue>> {
        spread_values(&iterable)
    }
}

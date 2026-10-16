//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::{Arithmetic, JsIterator, arithmetic_value, spread_with_value},
        realm::Bound,
        value::{native_to_js, to_account, to_js, to_signing},
    },
    exchange_runtime_initialization::RuntimeInitializationHost,
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::SigningResult,
};

impl RuntimeInitializationHost for Bound {
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), promise))
    }
    fn spread_override(
        &self,
        source: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        spread_with_value(&source, fields)
    }
    fn second_binding(&self, value: AccountValue) -> SigningResult<AccountValue> {
        let iterator = JsIterator::open(&to_js(&value)).map_err(to_signing)?;
        let _elided = iterator.step().map_err(to_signing)?;
        Ok(iterator
            .step()
            .map_err(to_signing)?
            .map_or(AccountValue::Undefined, |v| to_account(&v)))
    }
    fn add(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Add, a, b)
    }
}

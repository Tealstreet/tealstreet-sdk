//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::{Arithmetic, Relation, arithmetic_value, relation_value},
        realm::Bound,
        value::{native_to_js, to_account},
    },
    exchange_account_cache::AccountCacheHost,
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::SigningResult,
};

impl AccountCacheHost for Bound {
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), promise))
    }
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Subtract, left, right)
    }
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Add, left, right)
    }
    fn less(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::Less, left, right)
    }
}

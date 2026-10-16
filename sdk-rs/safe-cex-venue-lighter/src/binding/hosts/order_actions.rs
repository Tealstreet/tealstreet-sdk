//! Host family on Bound; see the trait docs for each JS operation.
use super::order_mapping::{account, js};
use crate::binding::value::to_signing;
use crate::{
    binding::{
        ops::{
            Arithmetic, JsIterator, Relation, arithmetic_value, copy_data, has_property,
            relation_value, spread_values,
        },
        realm::Bound,
        value::{native_to_js, to_account, to_js},
    },
    exchange_account_access::AccountAccessCall,
    exchange_order_actions::{OrderActionHost, OrderActionIterator},
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, utf16};
use std::rc::Rc;

impl OrderActionIterator for JsIterator {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        Ok(self.step().map_err(to_signing)?.map(|v| to_account(&v)))
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        error
    }
}

impl OrderActionHost for Bound {
    fn object_binding_pair(
        &self,
        value: AccountValue,
        keys: (&str, &str),
    ) -> SigningResult<(AccountValue, AccountValue)> {
        let object = to_js(&value);
        if object.is_nullish() {
            let shown = if matches!(object, Js::Null) {
                "object null"
            } else {
                "undefined"
            };
            let what = object.to_display();
            return js(Err(Thrown::type_error(&format!(
                "Cannot destructure property '{}' of '{shown}' as it is {what}.",
                keys.0
            ))));
        }
        let first = account(object.get(keys.0))?;
        let second = account(object.get(keys.1))?;
        Ok((first, second))
    }
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), value))
    }
    fn has(&self, value: &AccountValue, key: &str) -> SigningResult<bool> {
        let object = to_js(value);
        if !object.is_object() {
            return js(Err(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for '{key}' in {}",
                object.to_display()
            ))));
        }
        Ok(has_property(&object, &utf16(key)))
    }
    fn spread(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        account(copy_data(&[to_js(source)]))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        js(to_js(object).define(key, to_js(&value)))
    }
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn OrderActionIterator>> {
        Ok(Box::new(js(JsIterator::open(&to_js(&value)))?))
    }
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        callee.call(&spread_values(&value)?)
    }
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Subtract, a, b)
    }
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Divide, a, b)
    }
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Multiply, a, b)
    }
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::Greater, a, b)
    }
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::LessOrEqual, a, b)
    }
    fn greater_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::GreaterOrEqual, a, b)
    }
}

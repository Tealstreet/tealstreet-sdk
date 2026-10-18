//! Host family on Bound; see the trait docs for each JS operation.
use super::order_mapping::{account, js};
use crate::binding::value::to_signing;
use crate::{
    binding::{
        ops::{
            Arithmetic, JsIterator, Relation, arithmetic_value, relation_value, spread_with_value,
        },
        realm::Bound,
        value::{native_to_js, to_account, to_js, to_thrown},
    },
    exchange_ohlcv::{OhlcvHost, OhlcvSelfCallback},
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::SigningResult,
};
use safe_cex_exchange::js_runtime::regexp::regexp;
use safe_cex_exchange::js_runtime::value::{FunctionData, Js, Thrown, utf16};
use std::{
    cell::OnceCell,
    rc::{Rc, Weak},
};

impl OhlcvHost for Bound {
    fn self_function(&self, callback: OhlcvSelfCallback) -> SigningResult<AccountValue> {
        // A weak self slot: the function sees its own identity without a cycle.
        let slot: Rc<OnceCell<Weak<FunctionData>>> = Rc::default();
        let own = slot.clone();
        let function = Js::function("", move |_, args| {
            let callee = own
                .get()
                .and_then(Weak::upgrade)
                .map(Js::Function)
                .ok_or_else(|| Thrown::error("ReferenceError", "callee is no longer bound"))?;
            let args: Vec<_> = args.iter().map(to_account).collect();
            callback(to_account(&callee), &args)
                .map(|v| to_js(&v))
                .map_err(to_thrown)
        });
        if let Js::Function(data) = &function {
            let _ = slot.set(Rc::downgrade(data));
        }
        Ok(to_account(&function))
    }
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), promise))
    }
    fn call_value(
        &self,
        callee: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        account(to_js(&callee).call(Js::Undefined, args.iter().map(to_js).collect()))
    }
    fn regexp(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue> {
        account(regexp(&utf16(pattern), &utf16(flags)))
    }
    fn interval_binding(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)> {
        let iterator = js(JsIterator::open(&to_js(&value)))?;
        // `[, amount, unit]`: the elision still steps; built-ins have no `return`.
        iterator.step().map_err(to_signing)?;
        let amount = iterator
            .step()
            .map_err(to_signing)?
            .unwrap_or(Js::Undefined);
        let unit = iterator
            .step()
            .map_err(to_signing)?
            .unwrap_or(Js::Undefined);
        Ok((to_account(&amount), to_account(&unit)))
    }
    fn spread_override(
        &self,
        value: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        spread_with_value(&value, fields)
    }
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Subtract, a, b)
    }
    fn add(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Add, a, b)
    }
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Multiply, a, b)
    }
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Divide, a, b)
    }
    fn less(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::Less, a, b)
    }
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::LessOrEqual, a, b)
    }
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::Greater, a, b)
    }
}

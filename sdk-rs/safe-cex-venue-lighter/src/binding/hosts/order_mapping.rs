//! Host family on Bound; see the trait docs for each JS operation.
//! Also holds the result adapters the ohlcv, order action and live helper families share.
use crate::{
    binding::{
        ops::{Arithmetic, JsIterator, Relation, arithmetic_value, copy_data, relation_bool},
        realm::Bound,
        value::{to_account, to_js, to_signing},
    },
    exchange_order_mapping::{OrderMapBinary, OrderMapIterator, OrderMappingHost},
    sdk_account_api::AccountValue,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::regexp::regexp;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, utf16};

pub(super) fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}

pub(super) fn account(result: Result<Js, Thrown>) -> SigningResult<AccountValue> {
    result.map(|v| to_account(&v)).map_err(to_signing)
}

impl OrderMapIterator for JsIterator {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        Ok(self.step().map_err(to_signing)?.map(|v| to_account(&v)))
    }
    fn close_return(&mut self) -> SigningResult<()> {
        Ok(())
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        error
    }
}

impl OrderMappingHost for Bound {
    fn iterator(&self, value: &AccountValue) -> SigningResult<Box<dyn OrderMapIterator>> {
        Ok(Box::new(js(JsIterator::open(&to_js(value)))?))
    }
    fn copy_data_properties(&self, sources: &[AccountValue]) -> SigningResult<AccountValue> {
        account(copy_data(&sources.iter().map(to_js).collect::<Vec<_>>()))
    }
    fn binary(
        &self,
        op: OrderMapBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        let arithmetic = match op {
            OrderMapBinary::Add => Arithmetic::Add,
            OrderMapBinary::Subtract => Arithmetic::Subtract,
            OrderMapBinary::Multiply => Arithmetic::Multiply,
            OrderMapBinary::Divide => Arithmetic::Divide,
            OrderMapBinary::LessOrEqual => {
                return relation_bool(Relation::LessOrEqual, left, right);
            }
            OrderMapBinary::GreaterThan => return relation_bool(Relation::Greater, left, right),
            OrderMapBinary::GreaterOrEqual => {
                return relation_bool(Relation::GreaterOrEqual, left, right);
            }
        };
        arithmetic_value(arithmetic, left, right)
    }
    fn regex_literal(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue> {
        account(regexp(&utf16(pattern), &utf16(flags)))
    }
}

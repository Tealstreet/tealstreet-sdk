//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::{JsIterator, spread_with_value, to_string},
        realm::Bound,
        value::{to_account, to_js, to_signing, to_thrown},
    },
    exchange_reconciliation::{ReconciliationCallback, ReconciliationHost},
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::value::Js;

impl ReconciliationHost for Bound {
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        TradingHost::array(self, values)
    }
    fn pair(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)> {
        let iterator = JsIterator::open(&to_js(&value)).map_err(to_signing)?;
        let next = || -> SigningResult<AccountValue> {
            Ok(iterator
                .step()
                .map_err(to_signing)?
                .map_or(AccountValue::Undefined, |v| to_account(&v)))
        };
        let first = next()?;
        let second = next()?;
        Ok((first, second))
    }
    fn spread_override(
        &self,
        value: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        spread_with_value(&value, fields)
    }
    fn callback(&self, value: ReconciliationCallback) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::function("", move |_, _| {
            value().map(|v| to_js(&v)).map_err(to_thrown)
        })))
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        TradingHost::error_value(self, error)
    }
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        to_string(&to_js(&value))
            .map(|s| s.to_vec())
            .map_err(to_signing)
    }
}

//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::JsIterator,
        realm::Bound,
        value::{to_account, to_js, to_signing},
    },
    exchange_account_lookup::{AccountLookupHost, AccountLookupIterator},
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};
use std::rc::Rc;

impl AccountLookupIterator for JsIterator {
    fn next(&self) -> SigningResult<Option<AccountValue>> {
        Ok(self.step().map_err(to_signing)?.map(|v| to_account(&v)))
    }
    fn close(&self, completion: SigningResult<AccountValue>) -> SigningResult<AccountValue> {
        completion
    }
}

impl AccountLookupHost for Bound {
    fn append_spread(&self, array: &AccountValue, source: AccountValue) -> SigningResult<()> {
        let target = to_js(array);
        let iterator = JsIterator::open(&to_js(&source)).map_err(to_signing)?;
        while let Some(value) = iterator.step().map_err(to_signing)? {
            target.push(value).map_err(to_signing)?;
        }
        Ok(())
    }
    fn iterator(&self, source: AccountValue) -> SigningResult<Rc<dyn AccountLookupIterator>> {
        Ok(Rc::new(
            JsIterator::open(&to_js(&source)).map_err(to_signing)?,
        ))
    }
}

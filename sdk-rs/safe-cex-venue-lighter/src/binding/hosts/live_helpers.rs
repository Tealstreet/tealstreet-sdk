//! Host family on Bound; see the trait docs for each JS operation.
use super::order_mapping::{account, js};
use crate::{
    binding::{
        ops::{Relation, negate, relation_bool},
        realm::Bound,
        value::{to_account, to_js},
    },
    exchange_live_helpers::LiveHelperHost,
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};

impl LiveHelperHost for Bound {
    fn class_constructor(&self) -> AccountValue {
        // Before finish_class_binding this is TDZ; undefined makes the member read throw.
        self.realm
            .global("LighterExchange")
            .map(|v| to_account(&v))
            .unwrap_or(AccountValue::Undefined)
    }
    fn global_value(&self, name: &str) -> SigningResult<AccountValue> {
        account(self.realm.global(name))
    }
    fn less_than(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        relation_bool(Relation::Less, left, right)
    }
    fn negate(&self, value: AccountValue) -> SigningResult<AccountValue> {
        account(negate(&to_js(&value)))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        js(to_js(object).define(key, to_js(&value)))
    }
}

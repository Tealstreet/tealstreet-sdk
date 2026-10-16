//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::{Arithmetic, Relation, arithmetic_value, for_of, relation_bool},
        realm::Bound,
    },
    exchange_batch_placement::{BatchBinary, BatchPlacementHost, BatchVisitor},
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};

impl BatchPlacementHost for Bound {
    fn binary(
        &self,
        op: BatchBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        match op {
            BatchBinary::Add => arithmetic_value(Arithmetic::Add, left, right),
            BatchBinary::Multiply => arithmetic_value(Arithmetic::Multiply, left, right),
            BatchBinary::Remainder => arithmetic_value(Arithmetic::Remainder, left, right),
            BatchBinary::LessThan => relation_bool(Relation::Less, left, right),
            BatchBinary::GreaterThan => relation_bool(Relation::Greater, left, right),
        }
    }
    fn for_each(&self, value: AccountValue, visitor: BatchVisitor) -> SigningResult<()> {
        for_of(&value, |item| visitor(item))
    }
}

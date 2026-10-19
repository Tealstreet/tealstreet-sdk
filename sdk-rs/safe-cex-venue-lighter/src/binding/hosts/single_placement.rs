//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::{
            Arithmetic, JsIterator, Relation, arithmetic_value, copy_data, for_of, relation_bool,
            spread_values, to_string,
        },
        realm::Bound,
        value::{to_account, to_js, to_signing},
    },
    exchange_account_access::AccountAccessCall,
    exchange_single_placement::{
        PlacementBinary, PlacementIterator, PlacementVisitor, SinglePlacementHost,
    },
    sdk_account_api::AccountValue,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::value::{Js, Prop, Str, Thrown, utf16};
use std::rc::Rc;

impl PlacementIterator for JsIterator {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        Ok(self.step().map_err(to_signing)?.map(|v| to_account(&v)))
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        let _ = self.close();
        error
    }
}

/// Property storage the runtime cannot hold must fail loudly, never drop the write.
fn unsupported_store(target: &Js, key: &[u16]) -> Thrown {
    Thrown::type_error(&format!(
        "Cannot store property '{}' on {}",
        String::from_utf16_lossy(key),
        target.to_display()
    ))
}

fn own_props(target: &Js) -> Option<&std::cell::RefCell<indexmap::IndexMap<Str, Prop>>> {
    match target {
        Js::Object(o) => Some(&o.props),
        Js::Function(f) => Some(&f.props),
        _ => None,
    }
}

fn strict_put(target: &Js, key: &[u16], value: Js) -> Result<(), Thrown> {
    match target {
        Js::Undefined | Js::Null => target.set_key(key, value),
        Js::Bool(_) | Js::Number(_) | Js::BigInt(_) | Js::String(_) => {
            Err(Thrown::type_error(&format!(
                "Cannot create property '{}' on {} '{}'",
                String::from_utf16_lossy(key),
                target.type_of(),
                target.to_display()
            )))
        }
        Js::Array(_) => target.set_key(key, value),
        Js::Object(_) | Js::Function(_) => {
            let getter_only = own_props(target).is_some_and(|p| {
                matches!(p.borrow().get(key), Some(Prop::Accessor { set: None, .. }))
            });
            if getter_only {
                return Err(Thrown::type_error(&format!(
                    "Cannot set property {} of #<Object> which has only a getter",
                    String::from_utf16_lossy(key)
                )));
            }
            target.set_key(key, value)
        }
        _ => Err(unsupported_store(target, key)),
    }
}

impl SinglePlacementHost for Bound {
    fn binary(
        &self,
        op: PlacementBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        let arithmetic = match op {
            PlacementBinary::Add => Arithmetic::Add,
            PlacementBinary::Subtract => Arithmetic::Subtract,
            PlacementBinary::Multiply => Arithmetic::Multiply,
            PlacementBinary::Divide => Arithmetic::Divide,
            PlacementBinary::Remainder => Arithmetic::Remainder,
            PlacementBinary::LessThan => return relation_bool(Relation::Less, left, right),
            PlacementBinary::LessOrEqual => {
                return relation_bool(Relation::LessOrEqual, left, right);
            }
            PlacementBinary::GreaterThan => return relation_bool(Relation::Greater, left, right),
        };
        arithmetic_value(arithmetic, left, right)
    }
    fn spread_object(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        copy_data(&[to_js(source)])
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        let target = to_js(object);
        let key = utf16(key);
        let storable = matches!(&target, Js::Object(_) | Js::Function(_) | Js::Array(_));
        if !storable {
            return Err(to_signing(unsupported_store(&target, &key)));
        }
        target.define_key(&key, to_js(&value)).map_err(to_signing)
    }
    fn set_index(
        &self,
        object: &AccountValue,
        key: AccountValue,
        value: AccountValue,
    ) -> SigningResult<()> {
        let target = to_js(object);
        let key = to_string(&to_js(&key)).map_err(to_signing)?;
        strict_put(&target, &key, to_js(&value)).map_err(to_signing)
    }
    fn for_each(&self, value: AccountValue, visitor: PlacementVisitor) -> SigningResult<()> {
        for_of(&value, |item| visitor(item))
    }
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn PlacementIterator>> {
        let iterator = JsIterator::open(&to_js(&value)).map_err(to_signing)?;
        Ok(Box::new(iterator))
    }
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        callee.call(&spread_values(&value)?)
    }
}

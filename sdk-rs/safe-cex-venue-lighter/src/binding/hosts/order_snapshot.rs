//! OrderSnapshotHost on Bound and the cancelled-order `[id, timestamp]` cursor.
use crate::{
    binding::{
        ops::{Arithmetic, JsIterator, Relation, arithmetic_value, has_property, relation_value},
        realm::Bound,
        value::{native_to_js, to_account, to_js, to_signing},
    },
    exchange_order_snapshot::{CancelledOrderEntries, CandidateVisitor, OrderSnapshotHost},
    sdk_account_api::AccountValue,
    transaction_promise::NativePromise,
    transaction_signing::{SigningResult, TransactionSigningError},
};
use safe_cex_exchange::js_runtime::regexp::regexp;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, utf16};

/// `for (const [id, timestamp] of value)`; intrinsic iterators have no
/// `return`, so closing on a throw keeps the original completion.
struct Entries {
    outer: JsIterator,
}

impl CancelledOrderEntries for Entries {
    fn next_entry(&mut self) -> SigningResult<Option<(AccountValue, AccountValue)>> {
        let Some(pair) = self.outer.step().map_err(to_signing)? else {
            return Ok(None);
        };
        let inner = JsIterator::open(&pair).map_err(to_signing)?;
        let id = inner.step().map_err(to_signing)?.unwrap_or(Js::Undefined);
        let timestamp = inner.step().map_err(to_signing)?.unwrap_or(Js::Undefined);
        Ok(Some((to_account(&id), to_account(&timestamp))))
    }
    fn close_on_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        error
    }
}

const ERROR_CONSTRUCTORS: [&str; 4] = ["Error", "TypeError", "SyntaxError", "RangeError"];

impl Bound {
    /// Intrinsic constructors the runtime models without prototype objects.
    fn intrinsic_instance(&self, value: &Js, constructor: &Js) -> Option<bool> {
        let is = |name: &str| {
            self.realm
                .global(name)
                .is_ok_and(|g| g.same_reference(constructor))
        };
        for name in ERROR_CONSTRUCTORS {
            if is(name) {
                let Js::Object(o) = value else {
                    return Some(false);
                };
                if o.class != "Error" {
                    return Some(false);
                }
                return Some(
                    name == "Error"
                        || value
                            .get("name")
                            .is_ok_and(|n| n.strict_equals(&Js::str(name))),
                );
            }
        }
        if is("Map") {
            return Some(matches!(value, Js::Map(_)));
        }
        if is("Set") {
            return Some(matches!(value, Js::Set(_)));
        }
        None
    }
    /// InstanceofOperator without Symbol.hasInstance (the runtime has no symbols).
    fn instance_of_js(&self, value: &Js, constructor: &Js) -> Result<bool, Thrown> {
        if !constructor.is_object() {
            return Err(Thrown::type_error(
                "Right-hand side of 'instanceof' is not an object",
            ));
        }
        if !constructor.is_callable() {
            return Err(Thrown::type_error(
                "Right-hand side of 'instanceof' is not callable",
            ));
        }
        if let Some(hit) = self.intrinsic_instance(value, constructor) {
            return Ok(hit);
        }
        if !value.is_object() {
            return Ok(false);
        }
        let prototype = constructor.get("prototype")?;
        if !prototype.is_object() {
            return Err(Thrown::type_error(&format!(
                "Function has non-object prototype '{}' in instanceof check",
                prototype.to_display()
            )));
        }
        let mut current = match value {
            Js::Object(o) => o.proto.borrow().clone(),
            _ => None,
        };
        while let Some(object) = current {
            if object.same_reference(&prototype) {
                return Ok(true);
            }
            current = match &object {
                Js::Object(o) => o.proto.borrow().clone(),
                _ => None,
            };
        }
        Ok(false)
    }
}

impl OrderSnapshotHost for Bound {
    fn visit_until(
        &self,
        iterable: AccountValue,
        visitor: CandidateVisitor,
    ) -> SigningResult<Option<AccountValue>> {
        let iterator = JsIterator::open(&to_js(&iterable)).map_err(to_signing)?;
        while let Some(value) = iterator.step().map_err(to_signing)? {
            if let Some(result) = visitor(to_account(&value))? {
                return Ok(Some(result));
            }
        }
        Ok(None)
    }
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        to_account(&native_to_js(&self.realm.exchange_runtime(), promise))
    }
    fn regex(&self, pattern: &[u16], flags: &[u16]) -> SigningResult<AccountValue> {
        regexp(pattern, flags)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn has_key(&self, object: &AccountValue, key: &str) -> SigningResult<bool> {
        let object = to_js(object);
        if !object.is_object() {
            return Err(to_signing(Thrown::type_error(&format!(
                "Cannot use 'in' operator to search for '{key}' in {}",
                object.to_display()
            ))));
        }
        Ok(has_property(&object, &utf16(key)))
    }
    fn global_value(&self, name: &str) -> SigningResult<AccountValue> {
        self.realm
            .global(name)
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
    fn instance_of(&self, value: &AccountValue, constructor: AccountValue) -> SigningResult<bool> {
        self.instance_of_js(&to_js(value), &to_js(&constructor))
            .map_err(to_signing)
    }
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        arithmetic_value(Arithmetic::Subtract, left, right)
    }
    fn greater_than(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::Greater, left, right)
    }
    fn greater_or_equal(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        relation_value(Relation::GreaterOrEqual, left, right)
    }
    fn cancelled_entries(
        &self,
        value: AccountValue,
    ) -> SigningResult<Box<dyn CancelledOrderEntries>> {
        let outer = JsIterator::open(&to_js(&value)).map_err(to_signing)?;
        Ok(Box::new(Entries { outer }))
    }
}

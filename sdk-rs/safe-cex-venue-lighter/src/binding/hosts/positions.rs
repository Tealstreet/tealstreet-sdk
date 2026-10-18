//! PositionReadHost on Bound, the live for-in cursor, and the receiver-bound
//! Array.prototype.find used by ExchangeStateHost::positions_find.
use crate::{
    binding::{
        ops::{has_property, spread, to_string},
        realm::Bound,
        value::{to_account, to_js, to_signing},
    },
    exchange_positions::{PositionForIn, PositionReadHost},
    exchange_state::PositionFinder,
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};
use safe_cex_exchange::js_runtime::value::{Js, Str, Thrown};
use std::{collections::VecDeque, rc::Rc};

/// Strict-mode PutValue on an already-evaluated base and property key.
fn put(base: &Js, key: &[u16], value: Js) -> Result<(), Thrown> {
    let name = String::from_utf16_lossy(key);
    match base {
        Js::Object(_) | Js::Function(_) | Js::Array(_) => base.set_key(key, value),
        Js::Map(_) | Js::Set(_) | Js::Promise(_) => Err(Thrown::type_error(&format!("Cannot create property '{name}' on {}",base.to_display()))),
        primitive => Err(Thrown::type_error(&format!(
            "Cannot create property '{name}' on {} '{}'",
            primitive.type_of(),
            primitive.to_display()
        ))),
    }
}

/// Enumerable string keys for for-in: own keys first, then each prototype's,
/// with shadowed names reported once.
fn for_in_keys(value: &Js) -> Vec<Str> {
    let mut out: Vec<Str> = vec![];
    let mut current = Some(value.clone());
    while let Some(object) = current {
        for key in object.own_keys() {
            if !out.contains(&key) {
                out.push(key);
            }
        }
        current = match &object {
            Js::Object(o) => o.proto.borrow().clone(),
            _ => None,
        };
    }
    out
}

/// A key deleted before it is reached is skipped, as EnumerateObjectProperties requires.
struct ForIn {
    object: Js,
    keys: VecDeque<Str>,
}

impl PositionForIn for ForIn {
    fn next_key(&mut self) -> SigningResult<Option<AccountValue>> {
        while let Some(key) = self.keys.pop_front() {
            if has_property(&self.object, &key) {
                return Ok(Some(AccountValue::String(key.to_vec())));
            }
        }
        Ok(None)
    }
}

/// Array.prototype.find bound to its receiver: length read once, live index Gets.
#[allow(dead_code)] // reached through ExchangeStateHost::positions_find once Bound implements it
pub(crate) struct ArrayFind {
    receiver: Js,
}

/// Resolves `positions.find`, failing as V8 does when it is not callable. The
/// predicate is native, so only the intrinsic Array find can be honored.
#[allow(dead_code)]
pub(crate) fn positions_find(positions: &Js) -> Result<Rc<dyn PositionFinder>, Thrown> {
    let find = positions.get("find")?;
    if !find.is_callable() {
        return Err(Thrown::type_error("this.positions.find is not a function"));
    }
    if !positions.is_array() {
        return Err(Thrown::type_error(
            "this.positions.find is not the intrinsic Array find; a native predicate cannot cross into it",
        ));
    }
    Ok(Rc::new(ArrayFind {
        receiver: positions.clone(),
    }))
}

impl PositionFinder for ArrayFind {
    fn find(
        &self,
        predicate: &mut dyn FnMut(AccountValue) -> SigningResult<AccountValue>,
    ) -> SigningResult<AccountValue> {
        let length = self.receiver.get("length").map_err(to_signing)?;
        let length = length.to_number().map_err(to_signing)?;
        let length = if length.is_nan() || length <= 0.0 {
            0
        } else {
            length.min(9_007_199_254_740_991.0) as usize
        };
        for index in 0..length {
            let value = self.receiver.get(&index.to_string()).map_err(to_signing)?;
            let value = to_account(&value);
            if predicate(value.clone())?.truthy() {
                return Ok(value);
            }
        }
        Ok(AccountValue::Undefined)
    }
}

impl PositionReadHost for Bound {
    fn assign_index(
        &self,
        object: &AccountValue,
        key: AccountValue,
        value: AccountValue,
    ) -> SigningResult<()> {
        let base = to_js(object);
        let key = to_js(&key);
        if base.is_nullish() {
            let setting = if key.is_object() {
                String::new()
            } else {
                let key = key.to_js_string().map_err(to_signing)?;
                format!(" (setting '{}')", String::from_utf16_lossy(&key))
            };
            return Err(to_signing(Thrown::type_error(&format!(
                "Cannot set properties of {}{setting}",
                base.to_display()
            ))));
        }
        let key = to_string(&key).map_err(to_signing)?;
        put(&base, &key, to_js(&value)).map_err(to_signing)
    }
    fn for_in(&self, object: &AccountValue) -> SigningResult<Box<dyn PositionForIn>> {
        let object = to_js(object);
        let keys = if object.is_nullish() {
            VecDeque::new()
        } else {
            for_in_keys(&object).into()
        };
        Ok(Box::new(ForIn { object, keys }))
    }
    fn spread_array(&self, arrays: &[AccountValue]) -> SigningResult<AccountValue> {
        let mut values = vec![];
        for array in arrays {
            values.extend(spread(&to_js(array)).map_err(to_signing)?);
        }
        Ok(to_account(&Js::array(values)))
    }
}

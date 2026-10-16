//! Host family on Bound; see the trait docs for each JS operation.
use crate::{
    binding::{
        ops::to_string,
        realm::{Bound, JsCall},
        value::{to_js, to_signing},
    },
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_error_recovery::{ErrorRecoveryCall, ExchangeErrorRecoveryHost},
    sdk_account_api::AccountValue,
    transaction_signing::SigningResult,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

impl ErrorRecoveryCall for JsCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        AccountAccessCall::call(self, args)
    }
}

/// `value instanceof Error` over js_runtime errors, which are class-tagged
/// objects (js_runtime's Error constructor builds no prototype chain).
fn instance_of(value: &Js, error: &Js) -> Result<bool, Thrown> {
    if !error.is_callable() {
        return Err(Thrown::type_error(
            "Right-hand side of 'instanceof' is not callable",
        ));
    }
    let prototype = error.get("prototype")?;
    let mut current = match value {
        Js::Object(_) => Some(value.clone()),
        _ => return Ok(false),
    };
    while let Some(Js::Object(object)) = current {
        if object.class == "Error" {
            return Ok(true);
        }
        let parent = object.proto.borrow().clone();
        if let Some(parent) = &parent
            && prototype.is_object()
            && parent.same_reference(&prototype)
        {
            return Ok(true);
        }
        current = parent;
    }
    Ok(false)
}

/// /timeout of \d+ms exceeded/i: non-unicode /i folds ASCII letters only.
fn matches_timeout(text: &[u16]) -> bool {
    let fold = |unit: u16| {
        if (b'A' as u16..=b'Z' as u16).contains(&unit) {
            unit + 32
        } else {
            unit
        }
    };
    let equal = |at: &[u16], literal: &str| {
        at.len() >= literal.len() && literal.bytes().zip(at).all(|(b, &u)| fold(u) == b as u16)
    };
    let digit = |unit: &u16| (b'0' as u16..=b'9' as u16).contains(unit);
    (0..text.len()).any(|start| {
        let rest = &text[start..];
        if !equal(rest, "timeout of ") {
            return false;
        }
        let rest = &rest["timeout of ".len()..];
        let digits = rest.iter().take_while(|u| digit(u)).count();
        digits > 0 && equal(&rest[digits..], "ms exceeded")
    })
}

impl ExchangeErrorRecoveryHost for Bound {
    fn owner(&self) -> AccountValue {
        ExchangeAccountAccessHost::owner(self)
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        ExchangeAccountAccessHost::get(self, object, key)
    }
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        ExchangeAccountAccessHost::set(self, object, key, value)
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        let callee = self.realm.global(name).map_err(to_signing)?;
        Ok(Rc::new(JsCall {
            callee,
            this: Js::Undefined,
        }))
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>> {
        let this = to_js(object);
        let callee = this.get(key).map_err(to_signing)?;
        Ok(Rc::new(JsCall { callee, this }))
    }
    fn instance_of_error(&self, value: &AccountValue) -> SigningResult<bool> {
        let error = self.realm.global("Error").map_err(to_signing)?;
        instance_of(&to_js(value), &error).map_err(to_signing)
    }
    fn timeout_regex_test(&self, value: &AccountValue) -> SigningResult<bool> {
        let text = to_string(&to_js(value)).map_err(to_signing)?;
        Ok(matches_timeout(&text))
    }
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        to_string(&to_js(&value)).map(|s| s.to_vec()).map_err(to_signing)
    }
}

//! Host view of one LighterPrivateWebsocket instance: every owner, parent and
//! global operation the ported bodies perform, over the live JS object.
use super::super::{
    realm::Realm,
    value::{js_to_native, to_account, to_js, to_signing, to_thrown},
};
use crate::{
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::SigningResult,
    ws_private_class::{PrivateSocketCall, PrivateSocketHost, PrivateSocketListener},
    ws_private_recovery::{PrivateRecoveryHost, RecoveryCall, RecoveryCallback, RecoveryTimerCall},
    ws_private_warnings::{PrivateStreamWarningParams, PrivateWarningFlags, PrivateWarningHost},
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use std::rc::Rc;

/// Imports the bodies call by bare name; everything else is an owner path.
const IMPORTED_CALLEES: [&str; 5] = [
    "clearTimeout",
    "clearInterval",
    "setTimeout",
    "setInterval",
    "resolveTransportRouting",
];

pub(super) struct Owner {
    pub(super) realm: Realm,
    pub(super) this: Js,
}

/// A callee read now and called later with its source receiver.
pub(super) struct Captured {
    name: String,
    callee: Js,
    this: Js,
}

impl Captured {
    fn invoke(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        if !self.callee.is_callable() {
            return Err(to_signing(Thrown::type_error(&format!(
                "{} is not a function",
                self.name
            ))));
        }
        self.callee
            .call(self.this.clone(), args.iter().map(to_js).collect())
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
}

impl PrivateSocketCall for Captured {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke(args)
    }
}

impl RecoveryCall for Captured {
    fn call(&self, args: &[AccountValue]) -> SigningResult<()> {
        self.invoke(args).map(|_| ())
    }
}

/// A captured global setTimeout/setInterval taking a native callback.
pub(super) struct TimerCall(Js);

impl RecoveryTimerCall for TimerCall {
    fn call(&self, callback: RecoveryCallback, delay: f64) -> SigningResult<AccountValue> {
        let function = Js::function("", move |_, _| {
            callback().map(|()| Js::Undefined).map_err(to_thrown)
        });
        self.0
            .call(Js::Undefined, vec![function, Js::Number(delay)])
            .map(|v| to_account(&v))
            .map_err(to_signing)
    }
}

fn js<T>(result: Result<T, Thrown>) -> SigningResult<T> {
    result.map_err(to_signing)
}

impl Owner {
    fn walk(&self, path: &str) -> Result<(Js, Js), Thrown> {
        let (prefix, last) = match path.rsplit_once('.') {
            Some((prefix, last)) => (Some(prefix), last),
            None => (None, path),
        };
        let mut receiver = self.this.clone();
        for segment in prefix.into_iter().flat_map(|p| p.split('.')) {
            receiver = receiver.get(segment)?;
        }
        let value = receiver.get(last)?;
        Ok((receiver, value))
    }
    fn read(&self, path: &str) -> Result<Js, Thrown> {
        self.walk(path).map(|(_, v)| v)
    }
    fn write(&self, path: &str, value: Js) -> Result<(), Thrown> {
        match path.rsplit_once('.') {
            Some((prefix, last)) => {
                let mut receiver = self.this.clone();
                for segment in prefix.split('.') {
                    receiver = receiver.get(segment)?;
                }
                receiver.set(last, value)
            }
            None => self.this.set(path, value),
        }
    }
    pub(super) fn capture(&self, name: &str) -> Result<Captured, Thrown> {
        let (callee, this) = if let Some(member) = name.strip_prefix("super.") {
            let prototype = self.realm.global("BaseWebSocket")?.get("prototype")?;
            (prototype.get(member)?, self.this.clone())
        } else if IMPORTED_CALLEES.contains(&name) {
            (self.realm.global(name)?, Js::Undefined)
        } else {
            let (receiver, callee) = self.walk(name)?;
            (callee, receiver)
        };
        Ok(Captured {
            name: name.to_owned(),
            callee,
            this,
        })
    }
    fn call_global(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let callee = self.realm.global(name)?;
        if !callee.is_callable() {
            return Err(Thrown::type_error(&format!("{name} is not a function")));
        }
        callee.call(Js::Undefined, args)
    }
    fn string(&self, value: Js) -> Result<Vec<u16>, Thrown> {
        value.to_js_string().map(|s| s.to_vec())
    }
    fn optional_bool(&self, path: &str) -> SigningResult<Option<bool>> {
        let value = js(self.read(path))?;
        Ok((!value.is_nullish()).then(|| value.truthy()))
    }
}

impl PrivateRecoveryHost for Owner {
    fn boolean(&self, name: &str) -> SigningResult<bool> {
        if let Some(method) = name.strip_suffix("()") {
            return js(self.capture(method))?.invoke(&[]).map(|v| v.truthy());
        }
        js(self.read(name)).map(|v| v.truthy())
    }
    fn number(&self, name: &str) -> SigningResult<f64> {
        js(self.read(name).and_then(|v| v.to_number()))
    }
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()> {
        js(self.write(name, Js::Number(value)))
    }
    fn timer(&self, name: &str) -> SigningResult<AccountValue> {
        js(self.read(name)).map(|v| to_account(&v))
    }
    fn set_timer(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        js(self.write(name, to_js(&value)))
    }
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn RecoveryCall>> {
        Ok(Rc::new(js(Owner::capture(self, name))?))
    }
    fn capture_set_timeout(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>> {
        Ok(Rc::new(TimerCall(js(self.realm.global("setTimeout"))?)))
    }
    fn now(&self) -> SigningResult<f64> {
        js(self
            .call_global("virtualClock.getCurrentTimeMs", vec![])
            .and_then(|v| v.to_number()))
    }
}

impl PrivateSocketHost for Owner {
    fn value(&self, name: &str) -> SigningResult<AccountValue> {
        js(self.read(name)).map(|v| to_account(&v))
    }
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()> {
        js(self.write(name, to_js(&value)))
    }
    fn method(&self, name: &str) -> SigningResult<Rc<dyn PrivateSocketCall>> {
        Ok(Rc::new(js(self.capture(name))?))
    }
    fn binding(&self, name: &str) -> SigningResult<AccountValue> {
        js(self.realm.global(name)).map(|v| to_account(&v))
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        js(to_js(object).get(key)).map(|v| to_account(&v))
    }
    fn call(
        &self,
        callee: AccountValue,
        receiver: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let callee = to_js(&callee);
        Captured {
            name: callee.to_display(),
            callee,
            this: to_js(&receiver),
        }
        .invoke(args)
    }
    fn function(&self, body: PrivateSocketListener) -> SigningResult<AccountValue> {
        let function = Js::function("", move |_, args| {
            let args: Vec<_> = args.iter().map(to_account).collect();
            body(&args).map(|v| to_js(&v)).map_err(to_thrown)
        });
        Ok(to_account(&function))
    }
    fn capture_set_interval(&self) -> SigningResult<Rc<dyn RecoveryTimerCall>> {
        Ok(Rc::new(TimerCall(js(self.realm.global("setInterval"))?)))
    }
    fn date_now(&self) -> SigningResult<f64> {
        js(self
            .call_global("Date.now", vec![])
            .and_then(|v| v.to_number()))
    }
    fn iso_string(&self, ms: f64) -> SigningResult<Vec<u16>> {
        js(super::imports::iso_string(ms))
    }
    fn to_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        js(self.string(to_js(&value)))
    }
    fn to_number(&self, value: AccountValue) -> SigningResult<f64> {
        js(to_js(&value).to_number())
    }
    fn json_parse(&self, data: AccountValue) -> SigningResult<AccountValue> {
        js(self.call_global("jsonParse", vec![to_js(&data)])).map(|v| to_account(&v))
    }
    fn json_stringify(&self, value: AccountValue) -> SigningResult<AccountValue> {
        js(self.call_global("JSON.stringify", vec![to_js(&value)])).map(|v| to_account(&v))
    }
    fn object(&self, entries: Vec<(&'static str, AccountValue)>) -> SigningResult<AccountValue> {
        Ok(to_account(&Js::object(
            entries.iter().map(|(k, v)| (*k, to_js(v))).collect(),
        )))
    }
    fn spread_with(
        &self,
        source: AccountValue,
        key: &'static str,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        let copy = js(to_js(&source).spread_object())?;
        js(copy.define(key, to_js(&value)))?;
        Ok(to_account(&copy))
    }
    fn own_key_count(&self, value: AccountValue) -> SigningResult<f64> {
        let keys = js(self.call_global("Object.keys", vec![to_js(&value)]))?;
        js(keys.get("length").and_then(|v| v.to_number()))
    }
    fn is_array(&self, value: &AccountValue) -> SigningResult<bool> {
        Ok(to_js(value).is_array())
    }
    fn strict_equals(&self, left: &AccountValue, right: &AccountValue) -> bool {
        to_js(left).strict_equals(&to_js(right))
    }
    fn promise_runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.realm.0.extra.microtasks.clone()
    }
}

impl PrivateWarningHost for Owner {
    fn now_ms(&self) -> SigningResult<f64> {
        PrivateRecoveryHost::now(self)
    }
    fn network(&self) -> SigningResult<Vec<u16>> {
        js(self
            .read("parent.lighterDeploymentProfile.network")
            .and_then(|v| self.string(v)))
    }
    fn account_index(&self) -> SigningResult<Vec<u16>> {
        let index = js(self.capture("parent.getPrivateWsAccountIndex"))?.invoke(&[])?;
        js(self.string(to_js(&index)))
    }
    fn rate_limit_guidance(&self) -> SigningResult<Vec<u16>> {
        let message = js(self.capture("parent.getAccountRateLimitGuidanceMessage"))?.invoke(&[])?;
        js(self.string(to_js(&message)))
    }
    fn supports_ws_proxy(&self) -> SigningResult<bool> {
        js(self.read("parent.lighterDeploymentProfile.supportsWsProxy")).map(|v| v.truthy())
    }
    fn ws_route_option(&self) -> SigningResult<Option<bool>> {
        self.optional_bool("parent.options.tealstreetWsRoute")
    }
    fn testnet_option(&self) -> SigningResult<Option<bool>> {
        self.optional_bool("parent.options.testnet")
    }
    fn resolve_private_host(&self) -> SigningResult<AccountValue> {
        let resolve = js(self.capture("resolveTransportRouting"))?;
        let name = PrivateSocketHost::value(self, "parent.name")?;
        let options = PrivateSocketHost::value(self, "parent.options")?;
        let routing = to_js(&resolve.invoke(&[name, options])?);
        js(routing.get("wsPrivate").and_then(|w| w.get("host"))).map(|v| to_account(&v))
    }
    fn log_warning(&self, message: &[u16]) -> SigningResult<()> {
        let log = js(self.capture("parent.log"))?;
        log.invoke(&[
            AccountValue::String(message.to_vec()),
            AccountValue::String("warning".encode_utf16().collect()),
        ])
        .map(|_| ())
    }
    fn emit_warning(&self, message: &[u16], force_toast: bool) -> SigningResult<()> {
        let emit = js(self.capture("parent.emitter.emit"))?;
        let options = Js::object(vec![("forceToast", Js::Bool(force_toast))]);
        emit.invoke(&[
            AccountValue::String("warning".encode_utf16().collect()),
            AccountValue::String(message.to_vec()),
            to_account(&options),
        ])
        .map(|_| ())
    }
}

impl PrivateWarningFlags for Owner {
    fn readonly_fallback(&self) -> SigningResult<bool> {
        js(self.read("useReadonlyFallback")).map(|v| v.truthy())
    }
    fn readonly_failures(&self) -> SigningResult<f64> {
        js(self
            .read("readonlyFallbackFailures")
            .and_then(|v| v.to_number()))
    }
}

/// The `params` object getPrivateStreamErrorWarningMessage receives.
pub(super) struct Params(pub(super) Js);

impl PrivateStreamWarningParams for Params {
    fn policy_restriction(&self) -> SigningResult<bool> {
        js(self.0.get("isPolicyRestrictionError")).map(|v| v.truthy())
    }
    fn rate_limit(&self) -> SigningResult<bool> {
        js(self.0.get("isRateLimitError")).map(|v| v.truthy())
    }
    fn error_message(&self) -> SigningResult<Vec<u16>> {
        js(self
            .0
            .get("errMsg")
            .and_then(|v| v.to_js_string())
            .map(|s| s.to_vec()))
    }
}

/// Await any JS value as a native promise on the realm's microtasks.
pub(super) fn awaited(realm: &Realm, value: Js) -> NativePromise<AccountValue> {
    js_to_native(
        &realm.exchange_runtime(),
        realm.0.extra.microtasks.clone(),
        value,
    )
}

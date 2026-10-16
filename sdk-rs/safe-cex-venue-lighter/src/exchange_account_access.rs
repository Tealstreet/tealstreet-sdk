//! Account identity/lookup and signer preflight bodies. Required live JS hosts
//! retain Get/Set, callee and receiver order, import bindings, object/throw
//! identity, UTF16, actual Error construction and queued await. Full exchange,
//! HTTP, signer and JS engine bindings are separate work. The imported index
//! parser has its source ordinary Number-or-undefined result domain; ENDPOINTS
//! is the ordinary unmodified source table.
use crate::{
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub trait AccountAccessCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub trait ExchangeAccountAccessHost {
    fn owner(&self) -> AccountValue;
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()>;
    /// Capture actual callee and receiver before args, including noncallables.
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>>;
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>>;
    /// Fresh ordinary object with precisely these ordered own fields.
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue>;
    /// Pure infallible projection retaining the actual thrown value/identity.
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError;
    /// Internal async-return resolution, including live Get(then), inherited
    /// thenables and adoption. Getter/call errors reject resolver; primitive
    /// and ordinary nonthenable values settle without extra microtasks.
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>);
    fn runtime(&self) -> Rc<dyn TransactionRuntime>;
    /// Full PromiseResolve/thenable assimilation with queued continuation.
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn is_undefined(v: &AccountValue) -> bool {
    matches!(v, AccountValue::Undefined)
}
fn optional(
    h: &dyn ExchangeAccountAccessHost,
    v: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if v.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(v, key)
    }
}
fn owner_call(
    h: &dyn ExchangeAccountAccessHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn fail(h: &dyn ExchangeAccountAccessHost, message: AccountValue) -> SigningResult<AccountValue> {
    let error = h.global("Error")?.construct(&[message])?;
    Err(h.throw_value(error))
}
fn returned(
    h: &dyn ExchangeAccountAccessHost,
    value: SigningResult<AccountValue>,
) -> NativePromise<AccountValue> {
    let (promise, resolver) = NativePromise::pending(h.runtime());
    match value {
        Ok(value) => h.resolve_return(value, resolver),
        Err(error) => resolver.settle(Err(error)),
    }
    promise
}
fn start(
    h: Rc<dyn ExchangeAccountAccessHost>,
    work: impl FnOnce() -> SigningResult<NativePromise<AccountValue>>,
) -> NativePromise<AccountValue> {
    match work() {
        Ok(p) => p,
        Err(e) => returned(h.as_ref(), Err(e)),
    }
}
fn after(
    h: Rc<dyn ExchangeAccountAccessHost>,
    value: AccountValue,
    next: impl FnOnce(AccountValue) -> SigningResult<AccountValue> + 'static,
) -> SigningResult<NativePromise<AccountValue>> {
    let awaited = h.await_value(value)?;
    let (p, r) = NativePromise::pending(h.runtime());
    awaited.subscribe(move |v| match v.and_then(|v| next((*v).clone())) {
        Ok(value) => h.resolve_return(value, r),
        Err(error) => r.settle(Err(error)),
    });
    Ok(p)
}
// ts: LighterExchange#extractAccountFromResponse
pub fn extract_account_from_response(
    h: &dyn ExchangeAccountAccessHost,
    payload: &AccountValue,
) -> SigningResult<AccountValue> {
    if matches!(
        optional(h, payload, "success")?,
        AccountValue::Boolean(true)
    ) && optional(h, payload, "data")?.truthy()
    {
        let parse = h.member(&h.owner(), "parseAccountIndex")?;
        let data = h.get(payload, "data")?;
        let candidate = h.get(&data, "accountIndex")?;
        let account_index = parse.call(&[candidate])?;
        let data = h.get(payload, "data")?;
        let referrer = h.get(&data, "referrer")?;
        return h.literal(&[("accountIndex", account_index), ("referrer", referrer)]);
    }
    if matches!(optional(h,payload,"code")?,AccountValue::Number(n) if n==200.0) {
        let is_array = h.global("Array.isArray")?;
        if is_array
            .call(&[optional(h, payload, "accounts")?])?
            .truthy()
        {
            let accounts = h.get(payload, "accounts")?;
            let first = h.get(&accounts, "0")?;
            let parse = h.member(&h.owner(), "parseAccountIndex")?;
            let mut candidate = optional(h, &first, "account_index")?;
            if candidate.is_nullish() {
                candidate = optional(h, &first, "index")?;
            }
            let account_index = parse.call(&[candidate])?;
            let referrer = optional(h, &first, "referrer")?;
            return h.literal(&[("accountIndex", account_index), ("referrer", referrer)]);
        }
    }
    let message = optional(h, payload, "message")?;
    let message = if message.truthy() {
        message
    } else {
        let error = optional(h, payload, "error")?;
        optional(h, &error, "message")?
    };
    h.literal(&[("errorMessage", message)])
}
// ts: LighterExchange#fetchParsedAccountLookup
pub fn fetch_parsed_account_lookup(
    h: Rc<dyn ExchangeAccountAccessHost>,
    params: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let client = h.get(&h.owner(), "lighterApiClient")?;
        let get = h.member(&client, "get")?;
        let options = if params.truthy() {
            h.literal(&[("params", params.clone())])?
        } else {
            AccountValue::Undefined
        };
        let payload = get.call(&[text("/api/v1/account"), options])?;
        let live = h.clone();
        after(h, payload, move |payload| {
            let parsed = owner_call(live.as_ref(), "extractAccountFromResponse", &[payload])?;
            if is_undefined(&live.get(&parsed, "accountIndex")?) {
                // Source captures Error constructor BEFORE reading errorMessage.
                let constructor = live.global("Error")?;
                let message = live.get(&parsed, "errorMessage")?;
                let error = constructor.construct(&[if message.truthy() {
                    message
                } else {
                    text("Failed to resolve Lighter account index")
                }])?;
                return Err(live.throw_value(error));
            }
            let index = live.get(&parsed, "accountIndex")?;
            let referrer = live.get(&parsed, "referrer")?;
            live.literal(&[("accountIndex", index), ("referrer", referrer)])
        })
    })
}
// ts: LighterExchange#ensureAccountIndex
pub fn ensure_account_index(h: Rc<dyn ExchangeAccountAccessHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !is_undefined(&h.get(&h.owner(), "accountIndex")?) {
            return Ok(returned(h.as_ref(), h.get(&h.owner(), "accountIndex")));
        }
        let account = owner_call(h.as_ref(), "fetchAccountInfo", &[])?;
        let live = h.clone();
        after(h, account, move |account| {
            let index = live.get(&account, "accountIndex")?;
            live.set(&live.owner(), "accountIndex", index)?;
            live.get(&live.owner(), "accountIndex")
        })
    })
}
// ts: LighterExchange#getValidatedApiKeyIndex
pub fn get_validated_api_key_index(
    h: &dyn ExchangeAccountAccessHost,
) -> SigningResult<AccountValue> {
    let parse = h.global("parseLighterApiKeyIndex")?;
    let opts = h.get(&h.owner(), "options")?;
    let index = h.get(&opts, "apiKeyIndex")?;
    let index = parse.call(&[index])?;
    let invalid = if is_undefined(&index) {
        true
    } else {
        let AccountValue::Number(n) = &index else {
            unreachable!("source parser Number result")
        };
        *n < 0.0 || *n > 255.0
    };
    if invalid {
        return fail(
            h,
            text("Invalid API key index. Expected an integer from 0 to 255."),
        );
    }
    Ok(index)
}
// ts: LighterExchange.getPrivateWsAccountIndex
pub fn get_private_ws_account_index(
    h: &dyn ExchangeAccountAccessHost,
) -> SigningResult<AccountValue> {
    if !is_undefined(&h.get(&h.owner(), "accountIndex")?) {
        let index = h.get(&h.owner(), "accountIndex")?;
        return h.member(&index, "toString")?.call(&[]);
    }
    let stringify = h.global("toLighterApiKeyIndexString")?;
    let opts = h.get(&h.owner(), "options")?;
    let index = h.get(&opts, "apiKeyIndex")?;
    stringify.call(&[index])
}
// ts: LighterExchange.getPrivateWsApiKeyIndex
pub fn get_private_ws_api_key_index(
    h: &dyn ExchangeAccountAccessHost,
) -> SigningResult<AccountValue> {
    let stringify = h.global("toLighterApiKeyIndexString")?;
    let opts = h.get(&h.owner(), "options")?;
    let index = h.get(&opts, "apiKeyIndex")?;
    stringify.call(&[index])
}
// ts: LighterExchange.getAccount
pub fn get_account(h: Rc<dyn ExchangeAccountAccessHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let account = owner_call(h.as_ref(), "fetchAccountInfo", &[])?;
        let live = h.clone();
        after(h, account, move |account| {
            if is_undefined(&live.get(&live.owner(), "accountIndex")?) {
                let index = live.get(&account, "accountIndex")?;
                live.set(&live.owner(), "accountIndex", index)?;
            }
            let index = live.get(&account, "accountIndex")?;
            let user_id = live.member(&index, "toString")?.call(&[])?;
            let affiliate_id = live.get(&account, "referrer")?;
            live.literal(&[("userId", user_id), ("affiliateId", affiliate_id)])
        })
    })
}
// ts: LighterExchange#warmupSignerRuntime
pub fn warmup_signer_runtime(h: Rc<dyn ExchangeAccountAccessHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if !h.global("isWasmReady")?.call(&[])?.truthy() {
            let init = h.global("initWasm")?.call(&[])?;
            let live = h.clone();
            after(h, init, move |_| {
                live.global("getCryptoApi")?.call(&[])?;
                Ok(AccountValue::Undefined)
            })
        } else {
            h.global("getCryptoApi")?.call(&[])?;
            Ok(returned(h.as_ref(), Ok(AccountValue::Undefined)))
        }
    })
}

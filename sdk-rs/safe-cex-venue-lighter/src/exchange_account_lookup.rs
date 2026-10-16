//! Ordered account lookup bodies. Full JS/import/array/iterator/Promise hosts
//! remain required bindings; no exchange, HTTP client or JS engine is built here.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::SigningResult,
};
use std::{cell::RefCell, rc::Rc};

pub trait AccountLookupIterator {
    /// Actual IteratorStepValue; errors here do not trigger IteratorClose.
    fn next(&self) -> SigningResult<Option<AccountValue>>;
    /// Full IteratorClose with the supplied return/throw completion precedence.
    fn close(&self, completion: SigningResult<AccountValue>) -> SigningResult<AccountValue>;
}
pub trait AccountLookupHost: TradingHost {
    /// Append one actual Array-literal SpreadElement, using the full iterator.
    fn append_spread(&self, array: &AccountValue, source: AccountValue) -> SigningResult<()>;
    /// Actual GetIterator capture, preserving live next/return and original throws.
    fn iterator(&self, source: AccountValue) -> SigningResult<Rc<dyn AccountLookupIterator>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn optional(
    h: &dyn AccountLookupHost,
    value: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(value, key)
    }
}
fn undefined(v: &AccountValue) -> bool {
    matches!(v, AccountValue::Undefined)
}
fn call(
    h: &dyn AccountLookupHost,
    name: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), name)?.call(args)
}
type Continuation = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Continuation),
    Again(Box<dyn FnOnce() -> SigningResult<Step>>),
}
fn wait(
    value: SigningResult<AccountValue>,
    next: impl FnOnce(SigningResult<AccountValue>) -> SigningResult<Step> + 'static,
) -> SigningResult<Step> {
    match value {
        Ok(value) => Ok(Step::Await(value, Box::new(next))),
        Err(e) => next(Err(e)),
    }
}
fn drive(
    h: Rc<dyn AccountLookupHost>,
    resolver: PromiseResolver<AccountValue>,
    mut step: SigningResult<Step>,
) {
    loop {
        match step {
            Err(e) => {
                resolver.settle(Err(e));
                return;
            }
            Ok(Step::Done(value)) => {
                h.resolve_return(value, resolver);
                return;
            }
            Ok(Step::Again(next)) => step = next(),
            Ok(Step::Await(value, next)) => match h.await_value(value) {
                Err(e) => step = next(Err(e)),
                Ok(p) => {
                    p.subscribe(move |v| drive(h, resolver, next(v.map(|v| (*v).clone()))));
                    return;
                }
            },
        }
    }
}
fn start(
    h: Rc<dyn AccountLookupHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, resolver) = NativePromise::pending(h.runtime());
    drive(h, resolver, work());
    p
}
fn payload_array(
    h: &dyn AccountLookupHost,
    payload: &AccountValue,
    nested: bool,
    key: &str,
) -> SigningResult<AccountValue> {
    let check = h.global("Array.isArray")?;
    let value = if nested {
        optional(h, &optional(h, payload, "data")?, key)?
    } else {
        optional(h, payload, key)?
    };
    if check.call(&[value])?.truthy() {
        if nested {
            let data = h.get(payload, "data")?;
            h.get(&data, key)
        } else {
            h.get(payload, key)
        }
    } else {
        h.array(&[])
    }
}
// ts: LighterExchange#fetchAccountFromL1AddressLookup
pub fn fetch_account_from_l1_address_lookup(
    h: Rc<dyn AccountLookupHost>,
    address: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let client = h.get(&h.owner(), "lighterApiClient")?;
        let get = h.member(&client, "get")?;
        let endpoint = h.constant("ENDPOINTS.ACCOUNTS_BY_L1_ADDRESS")?;
        let params = h.literal(&[("l1_address", address)])?;
        let config = h.literal(&[("params", params)])?;
        let fetched = get.call(&[endpoint, config])?;
        let live = h.clone();
        wait(Ok(fetched), move |payload| {
            let payload = payload?;
            let accounts = live.array(&[])?;
            for (nested, key) in [
                (false, "sub_accounts"),
                (false, "accounts"),
                (true, "sub_accounts"),
                (true, "accounts"),
            ] {
                let source = payload_array(live.as_ref(), &payload, nested, key)?;
                live.append_spread(&accounts, source)?;
            }
            let first = live.get(&accounts, "0")?;
            let parse = live.member(&live.owner(), "parseAccountIndex")?;
            let mut raw = optional(live.as_ref(), &first, "account_index")?;
            if raw.is_nullish() {
                raw = optional(live.as_ref(), &first, "index")?;
            }
            let index = parse.call(&[raw])?;
            if undefined(&index) {
                let ctor = live.global("Error")?;
                let mut message = optional(live.as_ref(), &payload, "message")?;
                if !message.truthy() {
                    message = optional(
                        live.as_ref(),
                        &optional(live.as_ref(), &payload, "error")?,
                        "message",
                    )?;
                }
                if !message.truthy() {
                    message = text("Failed to resolve Lighter account index by L1 address");
                }
                return Err(live.throw_value(ctor.construct(&[message])?));
            }
            let referrer = optional(live.as_ref(), &first, "referrer")?;
            Ok(Step::Done(live.literal(&[
                ("accountIndex", index),
                ("referrer", referrer),
            ])?))
        })
    })
}
fn trim_option(h: &dyn AccountLookupHost, key: &str) -> SigningResult<AccountValue> {
    let options = h.get(&h.owner(), "options")?;
    let value = h.get(&options, key)?;
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.member(&value, "trim")?.call(&[])
    }
}
fn push_lookup(
    h: &dyn AccountLookupHost,
    array: &AccountValue,
    by: &str,
    value: AccountValue,
) -> SigningResult<()> {
    let push = h.member(array, "push")?;
    push.call(&[h.literal(&[("by", text(by)), ("value", value)])?])?;
    Ok(())
}
fn push_addresses(
    h: &dyn AccountLookupHost,
    addresses: AccountValue,
    candidates: &AccountValue,
) -> SigningResult<()> {
    let iter = h.iterator(addresses)?;
    while let Some(address) = iter.next()? {
        if let Err(e) = push_lookup(h, candidates, "l1_address", address) {
            return iter.close(Err(e)).map(|_| ());
        }
    }
    Ok(())
}
struct LookupState {
    h: Rc<dyn AccountLookupHost>,
    addresses: AccountValue,
    last_error: RefCell<AccountValue>,
}
fn finish_lookup(s: Rc<LookupState>) -> SigningResult<Step> {
    let ctor = s.h.global("Error")?;
    let message = s.last_error.borrow().clone();
    let message = if message.truthy() {
        let mut units = s.h.template_string(message)?;
        units.extend(". Provide your Lighter account index in Application ID.".encode_utf16());
        AccountValue::String(units)
    } else {
        text(
            "Failed to resolve Lighter account index. Provide your Lighter account index in Application ID.",
        )
    };
    Err(s.h.throw_value(ctor.construct(&[message])?))
}
fn run_candidates(
    s: Rc<LookupState>,
    iter: Rc<dyn AccountLookupIterator>,
    fallback: bool,
) -> SigningResult<Step> {
    let Some(candidate) = iter.next()? else {
        if fallback {
            return finish_lookup(s);
        }
        let next = s.h.iterator(s.addresses.clone())?;
        return run_candidates(s, next, true);
    };
    let fetched = call(
        s.h.as_ref(),
        if fallback {
            "fetchAccountFromL1AddressLookup"
        } else {
            "fetchParsedAccountLookup"
        },
        &[candidate],
    );
    wait(fetched, move |result| match result {
        Ok(value) => Ok(Step::Done(iter.close(Ok(value))?)),
        Err(error) => {
            let update = (|| {
                let message = optional(s.h.as_ref(), &s.h.error_value(error), "message")?;
                if message.truthy() {
                    s.last_error.replace(message);
                }
                Ok(())
            })();
            if let Err(e) = update {
                return iter.close(Err(e)).map(Step::Done);
            }
            // Synchronous lookup throws continue eagerly without recursive stack growth.
            Ok(Step::Again(Box::new(move || {
                run_candidates(s, iter, fallback)
            })))
        }
    })
}
// ts: LighterExchange#fetchAccountInfo
pub fn fetch_account_info(h: Rc<dyn AccountLookupHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let key = trim_option(h.as_ref(), "key")?;
        let application_id = trim_option(h.as_ref(), "applicationId")?;
        let configured = h
            .global("parseLighterPositiveIntegerString")?
            .call(&[application_id.clone()])?;
        let parse = h.global("parseLighterApiKeyIndex")?;
        let options = h.get(&h.owner(), "options")?;
        let legacy = parse.call(&[h.get(&options, "apiKeyIndex")?])?;
        let candidates = h.array(&[])?;
        let uniq = h.global("uniq")?;
        let app_address = call(h.as_ref(), "extractL1AddressCandidate", &[application_id])?;
        let key_address = call(h.as_ref(), "extractL1AddressCandidate", &[key])?;
        let addresses = h.array(&[app_address, key_address])?;
        let filter = h.member(&addresses, "filter")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            live.global("Boolean")?
                .call(&[args.first().cloned().unwrap_or(AccountValue::Undefined)])
        }))?;
        let addresses = uniq.call(&[filter.call(&[callback])?])?;
        if !undefined(&configured) {
            push_lookup(h.as_ref(), &candidates, "index", configured)?;
        }
        push_addresses(h.as_ref(), addresses.clone(), &candidates)?;
        if !undefined(&legacy) {
            let push = h.member(&candidates, "push")?;
            let value = h.global("String")?.call(&[legacy])?;
            push.call(&[h.literal(&[("by", text("index")), ("value", value)])?])?;
        }
        let iter = h.iterator(candidates)?;
        run_candidates(
            Rc::new(LookupState {
                h: h.clone(),
                addresses,
                last_error: RefCell::new(AccountValue::Undefined),
            }),
            iter,
            false,
        )
    })
}

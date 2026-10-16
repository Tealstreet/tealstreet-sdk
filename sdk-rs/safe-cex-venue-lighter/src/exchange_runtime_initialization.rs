//! Account tier, signer initialization, validation and startup bodies.
use crate::{
    exchange_trading_preflight::TradingHost,
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, PromiseResolver},
    transaction_signing::{SigningResult, TransactionSigningError},
};
use std::rc::Rc;
pub trait RuntimeInitializationHost: TradingHost {
    /// Infallible realm binding retaining this exact Promise handle, queued
    /// settlement and adoption; it must not copy or eagerly unwrap its value.
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue;
    /// Fresh CopyDataProperties plus ordered own data overrides.
    fn spread_override(
        &self,
        source: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue>;
    /// Actual [, second] binding, including elision and IteratorClose.
    fn second_binding(&self, value: AccountValue) -> SigningResult<AccountValue>;
    fn add(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
}
fn get(h: &dyn RuntimeInitializationHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&h.owner(), key)
}
fn set(h: &dyn RuntimeInitializationHost, key: &str, value: AccountValue) -> SigningResult<()> {
    h.set(&h.owner(), key, value)
}
fn call(
    h: &dyn RuntimeInitializationHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn method(
    h: &dyn RuntimeInitializationHost,
    value: &AccountValue,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(value, key)?.call(args)
}
fn optional(
    h: &dyn RuntimeInitializationHost,
    value: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        h.get(value, key)
    }
}
fn eq(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        _ => false,
    }
}
fn push(
    h: &dyn RuntimeInitializationHost,
    buf: &mut Vec<u16>,
    value: AccountValue,
) -> SigningResult<()> {
    buf.extend(h.template_string(value)?);
    Ok(())
}
fn error(h: &dyn RuntimeInitializationHost, message: AccountValue) -> SigningResult<Step> {
    let value = h.global("Error")?.construct(&[message])?;
    Err(h.throw_value(value))
}
type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
type Catch = Rc<dyn Fn(TransactionSigningError) -> SigningResult<Step>>;
fn caught(step: SigningResult<Step>, handler: Catch) -> SigningResult<Step> {
    match step {
        Err(e) => handler(e),
        Ok(Step::Await(v, next)) => Ok(Step::Await(v, Box::new(move |v| caught(next(v), handler)))),
        done => done,
    }
}
fn finally(h: Rc<dyn RuntimeInitializationHost>, step: SigningResult<Step>) -> SigningResult<Step> {
    match step {
        Ok(Step::Await(v, next)) => Ok(Step::Await(v, Box::new(move |v| finally(h, next(v))))),
        done => {
            set(
                h.as_ref(),
                "signerAdapterInitPromise",
                AccountValue::Undefined,
            )?;
            done
        }
    }
}
fn drive(
    h: Rc<dyn RuntimeInitializationHost>,
    r: PromiseResolver<AccountValue>,
    step: SigningResult<Step>,
) {
    match step {
        Err(e) => r.settle(Err(e)),
        Ok(Step::Done(v)) => h.resolve_return(v, r),
        Ok(Step::Await(v, next)) => match h.await_value(v) {
            Err(e) => drive(h, r, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, r, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn RuntimeInitializationHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    drive(h, r, work());
    p
}
fn loaded_update(
    h: &dyn RuntimeInitializationHost,
    fields: &[(&str, AccountValue)],
    flags: &[(&str, AccountValue)],
) -> SigningResult<()> {
    let store = get(h, "store")?;
    let update = h.member(&store, "update")?;
    let store = get(h, "store")?;
    let loaded = h.get(&store, "loaded")?;
    let loaded = h.spread_override(loaded, flags)?;
    let mut fields = fields.to_vec();
    fields.push(("loaded", loaded));
    update.call(&[h.literal(&fields)?])?;
    Ok(())
}
fn warning(
    h: &dyn RuntimeInitializationHost,
    e: TransactionSigningError,
    prefix: &str,
) -> SigningResult<Step> {
    let e = h.error_value(e);
    let log = h.member(&h.owner(), "log")?;
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    let part = optional(h, &e, "message")?;
    let part = if part.truthy() {
        part
    } else {
        h.global("String")?.call(&[e])?
    };
    push(h, &mut message, part)?;
    log.call(&[
        AccountValue::String(message),
        h.constant("LogSeverity.Warning")?,
    ])?;
    Ok(Step::Done(AccountValue::Undefined))
}
// ts: LighterExchange.changeAccountTier
pub fn change_account_tier(
    h: Rc<dyn RuntimeInitializationHost>,
    new_tier: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let index = call(h.as_ref(), "ensureAccountIndex", &[])?;
        Ok(Step::Await(
            index,
            Box::new(move |index| {
                let index = index?;
                let auth = call(
                    h.as_ref(),
                    "getPrivateWsAuthToken",
                    &[AccountValue::Number(60.0)],
                )?;
                Ok(Step::Await(
                    auth,
                    Box::new(move |auth| {
                        let auth = auth?;
                        if !auth.truthy() {
                            return error(
                                h.as_ref(),
                                text(
                                    "Lighter auth token unavailable. Please wait for the account signer to finish loading and try again.",
                                ),
                            );
                        }
                        let params = h.global("URLSearchParams")?;
                        let account = method(h.as_ref(), &index, "toString", &[])?;
                        let body = params.construct(&[h.literal(&[
                            ("account_index", account),
                            ("new_tier", new_tier.clone()),
                        ])?])?;
                        let headers = h.literal(&[
                            ("Content-Type", text("application/x-www-form-urlencoded")),
                            ("authorization", auth),
                        ])?;
                        let config = h.literal(&[
                            ("headers", headers),
                            ("__suppressExchangeError", AccountValue::Boolean(true)),
                        ])?;
                        let client = get(h.as_ref(), "lighterApiClient")?;
                        let post = h.member(&client, "post")?;
                        let endpoint = h.constant("ENDPOINTS.CHANGE_ACCOUNT_TIER")?;
                        let encoded = method(h.as_ref(), &body, "toString", &[])?;
                        let payload = post.call(&[endpoint, encoded, config])?;
                        Ok(Step::Await(
                            payload,
                            Box::new(move |payload| {
                                let payload = payload?;
                                if !eq(
                                    &optional(h.as_ref(), &payload, "code")?,
                                    &AccountValue::Number(200.0),
                                ) {
                                    let ctor = h.global("Error")?;
                                    let message = optional(h.as_ref(), &payload, "message")?;
                                    let message = if message.truthy() {
                                        message
                                    } else {
                                        let mut s = "Failed to switch Lighter account tier to "
                                            .encode_utf16()
                                            .collect::<Vec<_>>();
                                        push(h.as_ref(), &mut s, new_tier)?;
                                        AccountValue::String(s)
                                    };
                                    return Err(h.throw_value(ctor.construct(&[message])?));
                                }
                                set(h.as_ref(), "accountLimits", AccountValue::Undefined)?;
                                set(
                                    h.as_ref(),
                                    "accountLimitsNextFetchAt",
                                    AccountValue::Number(0.0),
                                )?;
                                let balance = call(h.as_ref(), "fetchRawBalance", &[])?;
                                Ok(Step::Await(
                                    balance,
                                    Box::new(move |balance| {
                                        let balance = balance?;
                                        if !get(h.as_ref(), "isDisposed")?.truthy() {
                                            loaded_update(
                                                h.as_ref(),
                                                &[("balance", balance)],
                                                &[("balance", AccountValue::Boolean(true))],
                                            )?;
                                        }
                                        Ok(Step::Done(AccountValue::Undefined))
                                    }),
                                ))
                            }),
                        ))
                    }),
                ))
            }),
        ))
    })
}
fn create_adapter(
    h: Rc<dyn RuntimeInitializationHost>,
    index: AccountValue,
    api_key: AccountValue,
) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let create = h.global("createLighterSignerAdapter")?;
        let request = h.literal(&[
            ("exchangeName", get(h.as_ref(), "name")?),
            ("options", get(h.as_ref(), "options")?),
            ("apiClient", get(h.as_ref(), "lighterApiClient")?),
            ("accountIndex", index),
            ("apiKeyIndex", api_key),
        ])?;
        let value = create.call(&[request])?;
        Ok(Step::Await(
            value,
            Box::new(|result| Ok(Step::Done(result?))),
        ))
    })
}
fn init_adapter(
    h: Rc<dyn RuntimeInitializationHost>,
    index: AccountValue,
    api_key: AccountValue,
) -> SigningResult<Step> {
    let work = (|| {
        let warmup = call(h.as_ref(), "warmupSignerRuntime", &[])?;
        let live = h.clone();
        Ok(Step::Await(
            warmup,
            Box::new(move |warmup| {
                warmup?;
                let first = live.promise_value(create_adapter(
                    live.clone(),
                    index.clone(),
                    api_key.clone(),
                ));
                let assign = live.clone();
                let attempt = Ok(Step::Await(
                    first,
                    Box::new(move |adapter| {
                        set(assign.as_ref(), "signerAdapter", adapter?)?;
                        Ok(Step::Done(AccountValue::Undefined))
                    }),
                ));
                let retry = live.clone();
                let attempt = caught(
                    attempt,
                    Rc::new(move |e| {
                        let original = e.clone();
                        let e = retry.error_value(e);
                        let string = retry.global("String")?;
                        let message = optional(retry.as_ref(), &e, "message")?;
                        let message = if message.truthy() {
                            message
                        } else if e.truthy() {
                            e
                        } else {
                            text("")
                        };
                        let message = string.call(&[message])?;
                        let message = method(retry.as_ref(), &message, "toLowerCase", &[])?;
                        let wasm = method(
                            retry.as_ref(),
                            &message,
                            "includes",
                            &[text("wasm not initialized")],
                        )?;
                        if !wasm.truthy() {
                            return Err(original);
                        }
                        let log = retry.member(&retry.owner(), "log")?;
                        log.call(&[text("Lighter signer init hit WASM-not-initialized race; retrying once after explicit runtime warmup."),retry.constant("LogSeverity.Warning")?])?;
                        let warmed = call(retry.as_ref(), "warmupSignerRuntime", &[])?;
                        let retry = retry.clone();
                        let index = index.clone();
                        let api_key = api_key.clone();
                        Ok(Step::Await(
                            warmed,
                            Box::new(move |warmed| {
                                warmed?;
                                let adapter = retry.promise_value(create_adapter(
                                    retry.clone(),
                                    index,
                                    api_key,
                                ));
                                Ok(Step::Await(
                                    adapter,
                                    Box::new(move |adapter| {
                                        set(retry.as_ref(), "signerAdapter", adapter?)?;
                                        Ok(Step::Done(AccountValue::Undefined))
                                    }),
                                ))
                            }),
                        ))
                    }),
                );
                finish_adapter_success(live, attempt)
            }),
        ))
    })();
    let error_h = h.clone();
    let handled = caught(
        work,
        Rc::new(move |e| {
            set(error_h.as_ref(), "signerAdapter", AccountValue::Undefined)?;
            let normalize = error_h.member(&error_h.owner(), "normalizeSignerErrorMessage")?;
            let message = normalize.call(&[error_h.error_value(e)])?;
            set(error_h.as_ref(), "signerInitErrorMessage", message)?;
            let now = error_h.global("virtualClock.getCurrentTimeMs")?.call(&[])?;
            let issue = error_h
                .member(&error_h.owner(), "isWasmRuntimeIssueMessage")?
                .call(&[get(error_h.as_ref(), "signerInitErrorMessage")?])?;
            if issue.truthy() && !error_h.global("isWasmReady")?.call(&[])?.truthy() {
                set(
                    error_h.as_ref(),
                    "signerRetryBlockedUntil",
                    AccountValue::Number(0.0),
                )?;
            } else {
                let until = error_h.add(
                    now,
                    error_h.constant("LighterExchange.SIGNER_RETRY_COOLDOWN_MS")?,
                )?;
                set(error_h.as_ref(), "signerRetryBlockedUntil", until)?;
            }
            let ctor = error_h.global("Error")?;
            let mut message = "Lighter signer initialization failed: "
                .encode_utf16()
                .collect::<Vec<_>>();
            push(
                error_h.as_ref(),
                &mut message,
                get(error_h.as_ref(), "signerInitErrorMessage")?,
            )?;
            Err(error_h.throw_value(ctor.construct(&[AccountValue::String(message)])?))
        }),
    );
    finally(h, handled)
}
fn finish_adapter_success(
    h: Rc<dyn RuntimeInitializationHost>,
    step: SigningResult<Step>,
) -> SigningResult<Step> {
    match step {
        Ok(Step::Await(v, next)) => Ok(Step::Await(
            v,
            Box::new(move |v| finish_adapter_success(h, next(v))),
        )),
        Ok(Step::Done(_)) => {
            set(
                h.as_ref(),
                "signerInitErrorMessage",
                AccountValue::Undefined,
            )?;
            set(
                h.as_ref(),
                "signerRetryBlockedUntil",
                AccountValue::Number(0.0),
            )?;
            Ok(Step::Done(get(h.as_ref(), "signerAdapter")?))
        }
        Err(e) => Err(e),
    }
}
// ts: LighterExchange#ensureSignerAdapter
pub fn ensure_signer_adapter(h: Rc<dyn RuntimeInitializationHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        if get(h.as_ref(), "signerAdapter")?.truthy() {
            return Ok(Step::Done(get(h.as_ref(), "signerAdapter")?));
        }
        if get(h.as_ref(), "signerAdapterInitPromise")?.truthy() {
            let p = get(h.as_ref(), "signerAdapterInitPromise")?;
            return Ok(Step::Await(p, Box::new(|v| Ok(Step::Done(v?)))));
        }
        let opts = get(h.as_ref(), "options")?;
        if !h.get(&opts, "secret")?.truthy() {
            return error(h.as_ref(), text("Lighter private key is missing"));
        }
        if call(h.as_ref(), "shouldShortCircuitSignerRetry", &[])?.truthy() {
            let message = call(h.as_ref(), "getSignerRetryBlockMessage", &[])?;
            call(h.as_ref(), "warnSignerUnavailable", &[message.clone()])?;
            return error(h.as_ref(), message);
        }
        let index = call(h.as_ref(), "ensureAccountIndex", &[])?;
        Ok(Step::Await(
            index,
            Box::new(move |index| {
                let index = index?;
                let api_key = call(h.as_ref(), "getValidatedApiKeyIndex", &[])?;
                let live = h.clone();
                let promise = start(h.clone(), move || init_adapter(live, index, api_key));
                // ts-bug: LighterExchange#ensureSignerAdapter — synchronous cleanup precedes this assignment.
                set(
                    h.as_ref(),
                    "signerAdapterInitPromise",
                    h.promise_value(promise),
                )?;
                let pending = get(h.as_ref(), "signerAdapterInitPromise")?;
                Ok(Step::Await(pending, Box::new(|v| Ok(Step::Done(v?)))))
            }),
        ))
    })
}
fn validation_error(
    h: &dyn RuntimeInitializationHost,
    e: TransactionSigningError,
) -> SigningResult<Step> {
    let e = h.error_value(e);
    let response = optional(h, &e, "response")?;
    let data = optional(h, &response, "data")?;
    let message = optional(h, &data, "message")?;
    let message = if message.truthy() {
        message
    } else {
        optional(h, &e, "message")?
    };
    call(h, "emitAndLogError", &[message.clone()])?;
    for (needle, result) in [
        (
            "invalid account l1 address",
            "Invalid L1 address for account lookup. For Lighter, the Public Key field is an API key, not an L1 address. Enter account index in Application ID.",
        ),
        (
            "not found",
            "Lighter account lookup failed. API Key Index is not account index. Enter your account index in Application ID.",
        ),
    ] {
        let string = h.global("String")?.call(&[if message.truthy() {
            message.clone()
        } else {
            text("")
        }])?;
        let lower = method(h, &string, "toLowerCase", &[])?;
        if method(h, &lower, "includes", &[text(needle)])?.truthy() {
            return Ok(Step::Done(h.literal(&[("error", text(result))])?));
        }
    }
    Ok(Step::Done(h.literal(&[(
        "error",
        if message.truthy() {
            message
        } else {
            text("Invalid API key or credentials")
        },
    )])?))
}
// ts: LighterExchange.validateAccount
pub fn validate_account(h: Rc<dyn RuntimeInitializationHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let work = (|| {
            if call(h.as_ref(), "getValidatedApiKeyIndex", &[]).is_err() {
                return Ok(Step::Done(h.literal(&[(
                    "error",
                    text("Invalid API key index. Expected an integer from 0 to 255."),
                )])?));
            }
            let account = call(h.as_ref(), "fetchAccountInfo", &[])?;
            let live = h.clone();
            Ok(Step::Await(
                account,
                Box::new(move |account| {
                    let account = account?;
                    set(
                        live.as_ref(),
                        "accountIndex",
                        live.get(&account, "accountIndex")?,
                    )?;
                    let opts = get(live.as_ref(), "options")?;
                    if !live.get(&opts, "secret")?.truthy() {
                        return Ok(Step::Done(live.literal(&[("error", text(""))])?));
                    }
                    let precheck = (|| {
                        if !live.global("isWasmReady")?.call(&[])?.truthy() {
                            let init = live.global("initWasm")?.call(&[])?;
                            let ready = live.clone();
                            Ok(Step::Await(
                                init,
                                Box::new(move |result| {
                                    result?;
                                    ready.global("getCryptoApi")?.call(&[])?;
                                    Ok(Step::Done(AccountValue::Undefined))
                                }),
                            ))
                        } else {
                            live.global("getCryptoApi")?.call(&[])?;
                            Ok(Step::Done(AccountValue::Undefined))
                        }
                    })();
                    let warn = live.clone();
                    let precheck = caught(
                        precheck,
                        Rc::new(move |e| {
                            warning(
                                warn.as_ref(),
                                e,
                                "WASM signer precheck warning during validateAccount: ",
                            )
                        }),
                    );
                    finish_validation(live, precheck)
                }),
            ))
        })();
        let errors = h.clone();
        caught(work, Rc::new(move |e| validation_error(errors.as_ref(), e)))
    })
}
fn finish_validation(
    h: Rc<dyn RuntimeInitializationHost>,
    step: SigningResult<Step>,
) -> SigningResult<Step> {
    match step {
        Ok(Step::Await(v, next)) => Ok(Step::Await(
            v,
            Box::new(move |v| finish_validation(h, next(v))),
        )),
        Ok(Step::Done(_)) => Ok(Step::Done(h.literal(&[("error", text(""))])?)),
        Err(e) => Err(e),
    }
}
fn option(h: &dyn RuntimeInitializationHost, key: &str) -> SigningResult<AccountValue> {
    h.get(&get(h, "options")?, key)
}
fn market_loaded(
    h: Rc<dyn RuntimeInitializationHost>,
    markets: AccountValue,
) -> SigningResult<AccountValue> {
    if get(h.as_ref(), "isDisposed")?.truthy() {
        return h.array(&[]);
    }
    let log = h.member(&h.owner(), "log")?;
    let mut message = "Loaded ".encode_utf16().collect::<Vec<_>>();
    push(h.as_ref(), &mut message, h.get(&markets, "length")?)?;
    message.extend(" Lighter markets".encode_utf16());
    log.call(&[AccountValue::String(message)])?;
    let store = get(h.as_ref(), "store")?;
    let update = h.member(&store, "update")?;
    let map = h.member(&markets, "map")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let market = arg(args, 0);
        live.literal(&[
            ("id", live.get(&market, "id")?),
            ("symbol", live.get(&market, "symbol")?),
            ("bid", AccountValue::Number(0.0)),
            ("ask", AccountValue::Number(0.0)),
            ("last", AccountValue::Number(0.0)),
            ("volume", AccountValue::Number(0.0)),
            ("quoteVolume", AccountValue::Number(0.0)),
            ("mark", AccountValue::Number(0.0)),
            ("index", AccountValue::Number(0.0)),
            ("percentage", AccountValue::Number(0.0)),
            ("openInterest", AccountValue::Number(0.0)),
            ("fundingRate", AccountValue::Number(0.0)),
            ("fundingTime", AccountValue::Number(0.0)),
        ])
    }))?;
    let tickers = map.call(&[callback])?;
    let store = get(h.as_ref(), "store")?;
    let loaded = h.get(&store, "loaded")?;
    let loaded = h.spread_override(
        loaded,
        &[
            ("markets", AccountValue::Boolean(true)),
            ("tickers", AccountValue::Boolean(true)),
        ],
    )?;
    update.call(&[h.literal(&[
        ("markets", markets.clone()),
        ("tickers", tickers),
        ("loaded", loaded),
    ])?])?;
    Ok(markets)
}
fn positions_loaded(
    h: Rc<dyn RuntimeInitializationHost>,
    positions: AccountValue,
) -> SigningResult<AccountValue> {
    if get(h.as_ref(), "isDisposed")?.truthy() {
        return h.array(&[]);
    }
    let store = get(h.as_ref(), "store")?;
    h.member(&store, "addOrUpdatePositionsFull")?
        .call(&[positions.clone()])?;
    loaded_update(
        h.as_ref(),
        &[],
        &[
            ("positions", AccountValue::Boolean(true)),
            ("orders", AccountValue::Boolean(true)),
        ],
    )?;
    Ok(positions)
}
fn launch_tick(h: Rc<dyn RuntimeInitializationHost>) -> SigningResult<Step> {
    let looping = h.global("loop")?;
    let live = h.clone();
    let callback = h.function(Rc::new(move |_| call(live.as_ref(), "tick", &[])))?;
    let delay = call(h.as_ref(), "getInitialTickDelay", &[])?;
    looping.call(&[callback, delay])?;
    Ok(Step::Done(AccountValue::Undefined))
}
fn finish_start_tick(
    h: Rc<dyn RuntimeInitializationHost>,
    step: SigningResult<Step>,
) -> SigningResult<Step> {
    match step {
        Ok(Step::Await(v, next)) => Ok(Step::Await(
            v,
            Box::new(move |v| finish_start_tick(h, next(v))),
        )),
        Ok(Step::Done(continue_tick)) => {
            if continue_tick.truthy() {
                launch_tick(h)
            } else {
                Ok(Step::Done(AccountValue::Undefined))
            }
        }
        Err(e) => Err(e),
    }
}
fn start_private_snapshot(h: Rc<dyn RuntimeInitializationHost>) -> SigningResult<Step> {
    let work = (|| {
        let all = h.global("Promise.all")?;
        let fetch = h.member(&h.owner(), "fetchPositions")?;
        let positions = fetch.call(&[
            AccountValue::Undefined,
            h.literal(&[("allowRecentAccountSnapshot", AccountValue::Boolean(true))])?,
        ])?;
        let then = h.member(&positions, "then")?;
        let live = h.clone();
        let callback = h.function(Rc::new(move |args| {
            positions_loaded(live.clone(), arg(args, 0))
        }))?;
        let positions = then.call(&[callback])?;
        let balance = h
            .member(&h.owner(), "fetchRawBalance")?
            .call(&[h.literal(&[
                ("allowRecentAccountSnapshot", AccountValue::Boolean(true)),
                ("includeAccountLimits", AccountValue::Boolean(false)),
            ])?])?;
        let values = all.call(&[h.array(&[positions, balance])?])?;
        let live = h.clone();
        Ok(Step::Await(
            values,
            Box::new(move |values| {
                let balance = live.second_binding(values?)?;
                if get(live.as_ref(), "isDisposed")?.truthy() {
                    return Ok(Step::Done(AccountValue::Boolean(false)));
                }
                loaded_update(
                    live.as_ref(),
                    &[("balance", balance)],
                    &[("balance", AccountValue::Boolean(true))],
                )?;
                Ok(Step::Done(AccountValue::Boolean(true)))
            }),
        ))
    })();
    let recover = h.clone();
    let work = caught(
        work,
        Rc::new(move |e| {
            call(
                recover.as_ref(),
                "handlePrivateSnapshotFetchError",
                &[recover.error_value(e)],
            )?;
            Ok(Step::Done(AccountValue::Boolean(true)))
        }),
    );
    finish_start_tick(h, work)
}
fn after_public_ready(
    h: Rc<dyn RuntimeInitializationHost>,
    public_only: bool,
    signer_warmup: AccountValue,
) -> SigningResult<Step> {
    call(h.as_ref(), "log", &[text("Ready to trade on Lighter")])?;
    if public_only {
        loaded_update(
            h.as_ref(),
            &[],
            &[
                ("balance", AccountValue::Boolean(true)),
                ("positions", AccountValue::Boolean(true)),
                ("orders", AccountValue::Boolean(true)),
            ],
        )?;
        return Ok(Step::Done(AccountValue::Undefined));
    }
    Ok(Step::Await(
        signer_warmup,
        Box::new(move |warmup| {
            warmup?;
            if get(h.as_ref(), "signerAdapter")?.truthy() {
                let socket = get(h.as_ref(), "privateWebsocket")?;
                h.member(&socket, "connectAndSubscribe")?.call(&[])?;
            } else {
                let log = h.member(&h.owner(), "log")?;
                log.call(&[
                    text("Lighter private websocket disabled (signer unavailable)"),
                    h.constant("LogSeverity.Warning")?,
                ])?;
            }
            start_private_snapshot(h)
        }),
    ))
}
// ts: LighterExchange.start
pub fn start_exchange(h: Rc<dyn RuntimeInitializationHost>) -> NativePromise<AccountValue> {
    start(h.clone(), || {
        let store = get(h.as_ref(), "store")?;
        h.member(&store, "setSetting")?
            .call(&[text("isHedged"), AccountValue::Boolean(false)])?;
        let external = call(h.as_ref(), "initExternalPublicData", &[])?;
        Ok(Step::Await(
            external,
            Box::new(move |external| {
                let external = external?;
                // ts-bug: LighterExchange.start — numeric API key index zero selects public-only mode.
                let public_only = option(h.as_ref(), "publicDataOnly")?.truthy()
                    || (!option(h.as_ref(), "secret")?.truthy()
                        && !option(h.as_ref(), "remoteSigner")?.truthy())
                    || !option(h.as_ref(), "apiKeyIndex")?.truthy();
                let signer_warmup = if !public_only && option(h.as_ref(), "secret")?.truthy() {
                    let signer = call(h.as_ref(), "ensureSignerAdapter", &[])?;
                    let catch = h.member(&signer, "catch")?;
                    let live = h.clone();
                    let callback = h.function(Rc::new(move |args| {
                        warning(
                            live.as_ref(),
                            live.throw_value(arg(args, 0)),
                            "Lighter signer unavailable at startup: ",
                        )?;
                        Ok(AccountValue::Undefined)
                    }))?;
                    catch.call(&[callback])?
                } else {
                    h.global("Promise.resolve")?
                        .call(&[AccountValue::Undefined])?
                };
                if !external.truthy() {
                    let markets = call(h.as_ref(), "fetchMarkets", &[])?;
                    let then = h.member(&markets, "then")?;
                    let live = h.clone();
                    let callback = h.function(Rc::new(move |args| {
                        market_loaded(live.clone(), arg(args, 0))
                    }))?;
                    let markets = then.call(&[callback])?;
                    Ok(Step::Await(
                        markets,
                        Box::new(move |markets| {
                            markets?;
                            if get(h.as_ref(), "isDisposed")?.truthy() {
                                return Ok(Step::Done(AccountValue::Undefined));
                            }
                            let socket = get(h.as_ref(), "publicWebsocket")?;
                            h.member(&socket, "connectAndSubscribe")?.call(&[])?;
                            after_public_ready(h, public_only, signer_warmup)
                        }),
                    ))
                } else {
                    let warmed = signer_warmup.clone();
                    Ok(Step::Await(
                        warmed,
                        Box::new(move |warmup| {
                            warmup?;
                            if get(h.as_ref(), "isDisposed")?.truthy() {
                                return Ok(Step::Done(AccountValue::Undefined));
                            }
                            after_public_ready(h, public_only, signer_warmup)
                        }),
                    ))
                }
            }),
        ))
    })
}

//! Builder-approval fee resolution, imported client construction and account reads.
use crate::{
    account_tier::LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE, types::RECV_WINDOW,
};
use crate::{transaction_promise::NativePromise, transaction_signing::TransactionSigningError};
use std::{future::Future, pin::Pin, rc::Rc};
type F<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
/// run_async starts eagerly and retains the owned task through settlement, even without observers.
pub struct LighterApprovalReadRuntime<H, V, E> {
    pub run_async: fn(&H, Pin<Box<dyn Future<Output = Result<V, E>>>>) -> NativePromise<V>,
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call: fn(&H, V, Vec<V>) -> Result<V, E>,
    pub call_async: for<'a> fn(&'a H, V, Vec<V>) -> Result<F<'a, V, E>, E>,
    pub construct: fn(&H, &'static str, V) -> Result<V, E>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub undefined: fn() -> V,
    pub number: fn(f64) -> V,
    pub text: fn(&str) -> V,
    pub truthy: fn(&V) -> bool,
    pub is_number: fn(&V) -> bool,
    pub finite: fn(&V) -> bool,
    pub is_object: fn(&V) -> bool,
    pub is_string: fn(&V) -> bool,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub nullish: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub same: fn(&V, &V) -> bool,
    pub copy: fn(&H, &V) -> Result<V, E>,
    pub spread: fn(&H, &V, V) -> Result<(), E>,
    pub extend_array: fn(&H, &V, V) -> Result<(), E>,
    pub map: fn(&H, V, &mut dyn FnMut(V) -> Result<V, E>) -> Result<V, E>,
    pub filter: fn(&H, V, &mut dyn FnMut(V) -> Result<bool, E>) -> Result<V, E>,
    pub default_caps: fn(&H) -> Result<V, E>,
    pub base_url: fn(&H, V) -> Result<V, E>,
    pub deployment_profile: fn(&H, V) -> Result<V, E>,
    pub wasm_ready: fn(&H) -> Result<bool, E>,
    pub init_wasm: for<'a> fn(&'a H) -> Result<F<'a, V, E>, E>,
    pub crypto_api: fn(&H) -> Result<V, E>,
    pub now: fn(&H) -> Result<f64, E>,
    pub bigint: fn(&H, V) -> Result<V, E>,
    pub negative_one: fn() -> V,
    pub to_bigint: fn(&H, V) -> Result<V, E>,
    pub to_number: fn(&H, V) -> Result<V, E>,
    pub positive_integer: fn(&H, &V) -> Result<V, E>,
    pub l1_address: fn(&H, &V) -> Result<bool, E>,
    pub extract_tier: fn(&H, V) -> Result<V, E>,
    pub eligible_tier: fn(&H, V) -> Result<bool, E>,
    pub nonzero_fees: fn(&H, V) -> Result<bool, E>,
    pub nonnegative_bigint: fn(&H, V) -> Result<bool, E>,
    pub new_error: fn(&H, &'static str) -> E,
    pub is_error: fn(&H, &E) -> Result<bool, E>,
    pub error_value: fn(&E) -> V,
    pub warn_tier_lookup_failure: fn(&H, &E),
}
fn eager<'a, V: 'a, E: 'a>(mut f: F<'a, V, E>) -> F<'a, V, E> {
    match f
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(x) => Box::pin(std::future::ready(x)),
        std::task::Poll::Pending => Box::pin(js_await(f)),
    }
}
async fn js_await<T>(f: impl Future<Output = T>) -> T {
    let mut suspended = false;
    std::future::poll_fn(|cx| {
        if suspended {
            std::task::Poll::Ready(())
        } else {
            suspended = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
    f.await
}
fn optional<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    v: &V,
    k: &str,
) -> Result<V, E> {
    if (r.nullish)(v) {
        Ok((r.undefined)())
    } else {
        (r.get)(h, v, k)
    }
}
fn fallback<H, V, E>(
    r: &LighterApprovalReadRuntime<H, V, E>,
    v: V,
    default: impl FnOnce() -> V,
) -> V {
    if (r.nullish)(&v) { default() } else { v }
}
fn method<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    v: &V,
    k: &'static str,
    args: Vec<V>,
) -> Result<V, E> {
    let callback = (r.method)(h, v, k)?;
    (r.call)(h, callback, args)
}
fn client_options<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    opts: &V,
) -> Result<V, E> {
    let url = (r.base_url)(
        h,
        (r.object)(vec![
            ("exchangeName", (r.get)(h, opts, "exchangeName")?),
            ("overrideBaseUrl", (r.get)(h, opts, "baseUrl")?),
            ("testnet", (r.get)(h, opts, "testnet")?),
        ]),
    )?;
    Ok((r.object)(vec![
        ("baseUrl", url),
        ("timeout", (r.number)(RECV_WINDOW as f64)),
        (
            "headers",
            (r.object)(vec![("Content-Type", (r.text)("application/json"))]),
        ),
    ]))
}
pub fn resolve_approval_fee_caps<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    params: &V,
) -> Result<V, E> {
    let base = if (r.is_number)(&(r.get)(h, params, "feeValue")?)
        && (r.finite)(&(r.get)(h, params, "feeValue")?)
    {
        (r.object)(vec![
            ("maxPerpsMakerFee", (r.get)(h, params, "feeValue")?),
            ("maxPerpsTakerFee", (r.get)(h, params, "feeValue")?),
            ("maxSpotMakerFee", (r.get)(h, params, "feeValue")?),
            ("maxSpotTakerFee", (r.get)(h, params, "feeValue")?),
        ])
    } else {
        (r.default_caps)(h)?
    };
    let result = (r.copy)(h, &base)?;
    (r.spread)(h, &result, (r.get)(h, params, "feeCaps")?)?;
    Ok(result)
}
pub fn create_approval_client<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    opts: &V,
) -> Result<V, E> {
    (r.construct)(h, "LighterApiClient", client_options(h, r, opts)?)
}
fn ensure_wasm_ready_body<'a, H, V: 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        if !(r.wasm_ready)(h)? {
            js_await((r.init_wasm)(h)?).await?;
        }
        Ok((r.undefined)())
    }))
}
pub fn create_approval_transaction_client<H, V, E>(
    h: &H,
    r: &LighterApprovalReadRuntime<H, V, E>,
    params: &V,
) -> Result<V, E> {
    let http = (r.get)(h, params, "apiClient")?;
    let crypto = (r.crypto_api)(h)?;
    let profile = (r.deployment_profile)(
        h,
        (r.object)(vec![
            ("exchangeName", (r.get)(h, params, "exchangeName")?),
            ("testnet", (r.get)(h, params, "testnet")?),
        ]),
    )?;
    let chain = (r.get)(h, &profile, "signerChainId")?;
    (r.construct)(
        h,
        "TransactionClient",
        (r.object)(vec![
            ("httpClient", http),
            ("crypto", crypto),
            ("chainId", chain),
            ("privateKey", (r.get)(h, params, "apiPrivateKey")?),
            ("accountIndex", (r.get)(h, params, "accountIndex")?),
            ("apiKeyIndex", (r.get)(h, params, "apiKeyIndex")?),
        ]),
    )
}
fn network<H, V, E>(h: &H, r: &LighterApprovalReadRuntime<H, V, E>, params: &V) -> Result<V, E> {
    Ok((r.object)(vec![
        ("baseUrl", (r.get)(h, params, "baseUrl")?),
        ("exchangeName", (r.get)(h, params, "exchangeName")?),
        ("testnet", (r.get)(h, params, "testnet")?),
    ]))
}
fn fetch_account_limits_for_approval_body<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
    params: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        js_await(ensure_wasm_ready_body(h, r)).await?;
        let client = create_approval_client(h, r, &network(h, r, &params)?)?;
        let tx = create_approval_transaction_client(
            h,
            r,
            &(r.object)(vec![
                ("accountIndex", (r.get)(h, &params, "accountIndex")?),
                ("apiClient", client.clone()),
                ("apiKeyIndex", (r.get)(h, &params, "apiKeyIndex")?),
                ("apiPrivateKey", (r.get)(h, &params, "apiPrivateKey")?),
                ("exchangeName", (r.get)(h, &params, "exchangeName")?),
                ("testnet", (r.get)(h, &params, "testnet")?),
            ]),
        )?;
        let deadline = (r.bigint)(h, (r.number)(((r.now)(h)? / 1000.0).floor() + 60.0))?;
        let auth = (r.method)(h, &tx, "getAuthToken")?;
        let token = js_await((r.call_async)(h, auth, vec![deadline])?).await?;
        let get = (r.method)(h, &client, "get")?;
        let index = (r.get)(h, &params, "accountIndex")?;
        let config = (r.object)(vec![
            (
                "params",
                (r.object)(vec![(
                    "account_index",
                    method(h, r, &index, "toString", vec![])?,
                )]),
            ),
            ("headers", (r.object)(vec![("authorization", token)])),
        ]);
        (r.call_async)(h, get, vec![(r.text)("/api/v1/accountLimits"), config])?.await
    }))
}
fn assert_partner_attribution_tier_eligible_for_approval_body<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
    params: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        if !(r.nonzero_fees)(h, (r.get)(h, &params, "feeCaps")?)?
            || (r.is_undefined)(&(r.get)(h, &params, "apiKeyIndex")?)
            || !(r.truthy)(&(r.get)(h, &params, "apiPrivateKey")?)
        {
            return Ok((r.undefined)());
        }
        let outcome: Result<(), E> = async {
            let limits = js_await(fetch_account_limits_for_approval_body(
                h,
                r,
                (r.object)(vec![
                    ("accountIndex", (r.get)(h, &params, "accountIndex")?),
                    ("apiKeyIndex", (r.get)(h, &params, "apiKeyIndex")?),
                    ("apiPrivateKey", (r.get)(h, &params, "apiPrivateKey")?),
                    ("baseUrl", (r.get)(h, &params, "baseUrl")?),
                    ("exchangeName", (r.get)(h, &params, "exchangeName")?),
                    ("testnet", (r.get)(h, &params, "testnet")?),
                ]),
            ))
            .await?;
            let tier = (r.extract_tier)(h, limits)?;
            if (r.truthy)(&tier) && !(r.eligible_tier)(h, tier)? {
                return Err((r.new_error)(
                    h,
                    LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE,
                ));
            }
            Ok(())
        }
        .await;
        if let Err(error) = outcome {
            if (r.is_error)(h, &error)?
                && (r.same)(
                    &(r.get)(h, &(r.error_value)(&error), "message")?,
                    &(r.text)(LIGHTER_PARTNER_ATTRIBUTION_APPROVAL_TIER_REQUIREMENT_MESSAGE),
                )
            {
                return Err(error);
            }
            (r.warn_tier_lookup_failure)(h, &error);
        }
        Ok((r.undefined)())
    }))
}
fn resolve_account_index_body<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
    params: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let identifier = (r.get)(h, &params, "accountIdentifier")?;
        let trimmed = method(h, r, &identifier, "trim", vec![])?;
        let candidate = (r.positive_integer)(h, &trimmed)?;
        if (r.truthy)(&candidate) {
            return (r.bigint)(h, candidate);
        }
        if !(r.l1_address)(h, &trimmed)? {
            return Err((r.new_error)(
                h,
                "Lighter account identifier must be a numeric account index or an L1 wallet address",
            ));
        }
        let client = create_approval_client(h, r, &params)?;
        let get = (r.method)(h, &client, "get")?;
        let payload = js_await((r.call_async)(
            h,
            get,
            vec![
                (r.text)("/api/v1/accountsByL1Address"),
                (r.object)(vec![("params", (r.object)(vec![("l1_address", trimmed)]))]),
            ],
        )?)
        .await?;
        let accounts = (r.array)(vec![]);
        for (nested, key) in [
            (false, "sub_accounts"),
            (false, "accounts"),
            (true, "sub_accounts"),
            (true, "accounts"),
        ] {
            let selected = if nested {
                optional(h, r, &optional(h, r, &payload, "data")?, key)?
            } else {
                optional(h, r, &payload, key)?
            };
            let source = if (r.is_array)(h, &selected)? {
                if nested {
                    (r.get)(h, &(r.get)(h, &payload, "data")?, key)?
                } else {
                    (r.get)(h, &payload, key)?
                }
            } else {
                (r.array)(vec![])
            };
            (r.extend_array)(h, &accounts, source)?;
        }
        let first = (r.get)(h, &accounts, "0")?;
        let account_index = optional(h, r, &first, "account_index")?;
        let raw = if (r.nullish)(&account_index) {
            optional(h, r, &first, "index")?
        } else {
            account_index
        };
        let resolved = (r.to_bigint)(h, raw)?;
        if (r.is_undefined)(&resolved) {
            return Err((r.new_error)(
                h,
                "Failed to resolve Lighter account index from L1 address",
            ));
        }
        Ok(resolved)
    }))
}
fn fetch_account_snapshot_body<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
    params: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let client = create_approval_client(h, r, &network(h, r, &params)?)?;
        let get = (r.method)(h, &client, "get")?;
        let index = (r.get)(h, &params, "accountIndex")?;
        let payload = js_await((r.call_async)(
            h,
            get,
            vec![
                (r.text)("/api/v1/account"),
                (r.object)(vec![(
                    "params",
                    (r.object)(vec![
                        ("by", (r.text)("index")),
                        ("value", method(h, r, &index, "toString", vec![])?),
                    ]),
                )]),
            ],
        )?)
        .await?;
        let array = optional(h, r, &payload, "accounts")?;
        let account = if (r.is_array)(h, &array)? {
            (r.get)(h, &(r.get)(h, &payload, "accounts")?, "0")?
        } else {
            (r.undefined)()
        };
        if !(r.truthy)(&account) || !(r.is_object)(&account) {
            return Err((r.new_error)(
                h,
                "Lighter account lookup returned no account",
            ));
        }
        let approved = (r.get)(h, &account, "approved_integrators")?;
        let raw = if (r.is_array)(h, &approved)? {
            (r.get)(h, &account, "approved_integrators")?
        } else {
            (r.array)(vec![])
        };
        let account_index = (r.get)(h, &params, "accountIndex")?;
        let tier = (r.extract_tier)(h, account.clone())?;
        let address = (r.get)(h, &account, "l1_address")?;
        let address = if (r.is_string)(&address) {
            (r.get)(h, &account, "l1_address")?
        } else {
            (r.undefined)()
        };
        let mapped = (r.map)(h, raw, &mut |item| {
            let id = (r.get)(h, &item, "account_index")?;
            let id = if (r.nullish)(&id) {
                (r.get)(h, &item, "index")?
            } else {
                id
            };
            Ok((r.object)(vec![
                (
                    "accountIndex",
                    fallback(r, (r.to_bigint)(h, id)?, r.negative_one),
                ),
                (
                    "approvalExpiry",
                    fallback(
                        r,
                        (r.to_number)(h, (r.get)(h, &item, "approval_expiry")?)?,
                        || (r.number)(0.0),
                    ),
                ),
                (
                    "maxPerpsTakerFee",
                    fallback(
                        r,
                        (r.to_number)(h, (r.get)(h, &item, "max_perps_taker_fee")?)?,
                        || (r.number)(0.0),
                    ),
                ),
                (
                    "maxPerpsMakerFee",
                    fallback(
                        r,
                        (r.to_number)(h, (r.get)(h, &item, "max_perps_maker_fee")?)?,
                        || (r.number)(0.0),
                    ),
                ),
                (
                    "maxSpotTakerFee",
                    fallback(
                        r,
                        (r.to_number)(h, (r.get)(h, &item, "max_spot_taker_fee")?)?,
                        || (r.number)(0.0),
                    ),
                ),
                (
                    "maxSpotMakerFee",
                    fallback(
                        r,
                        (r.to_number)(h, (r.get)(h, &item, "max_spot_maker_fee")?)?,
                        || (r.number)(0.0),
                    ),
                ),
            ]))
        })?;
        let filtered = (r.filter)(h, mapped, &mut |item| {
            (r.nonnegative_bigint)(h, (r.get)(h, &item, "accountIndex")?)
        })?;
        Ok((r.object)(vec![
            ("accountIndex", account_index),
            ("accountTier", tier),
            ("l1Address", address),
            ("approvedIntegrators", filtered),
        ]))
    }))
}
fn get_next_nonce_body<'a, H, V: Clone + 'a, E: 'a>(
    h: &'a H,
    r: &'a LighterApprovalReadRuntime<H, V, E>,
    params: V,
) -> F<'a, V, E> {
    eager(Box::pin(async move {
        let client = create_approval_client(h, r, &network(h, r, &params)?)?;
        let next = (r.method)(h, &client, "getNextNonce")?;
        (r.call_async)(
            h,
            next,
            vec![
                (r.get)(h, &params, "accountIndex")?,
                (r.get)(h, &params, "apiKeyIndex")?,
            ],
        )?
        .await
    }))
}

pub fn ensure_wasm_ready<H: 'static, V: 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move { ensure_wasm_ready_body(h.as_ref(), r.as_ref()).await }),
    )
}

pub fn fetch_account_limits_for_approval<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
    params: V,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move {
            fetch_account_limits_for_approval_body(h.as_ref(), r.as_ref(), params).await
        }),
    )
}

pub fn assert_partner_attribution_tier_eligible_for_approval<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
    params: V,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move {
            assert_partner_attribution_tier_eligible_for_approval_body(
                h.as_ref(),
                r.as_ref(),
                params,
            )
            .await
        }),
    )
}

pub fn resolve_account_index<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
    params: V,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move { resolve_account_index_body(h.as_ref(), r.as_ref(), params).await }),
    )
}

pub fn fetch_account_snapshot<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
    params: V,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move { fetch_account_snapshot_body(h.as_ref(), r.as_ref(), params).await }),
    )
}

pub fn get_next_nonce<H: 'static, V: Clone + 'static>(
    h: Rc<H>,
    r: Rc<LighterApprovalReadRuntime<H, V, TransactionSigningError>>,
    params: V,
) -> NativePromise<V> {
    let owner = h.clone();
    (r.run_async)(
        &owner,
        Box::pin(async move { get_next_nonce_body(h.as_ref(), r.as_ref(), params).await }),
    )
}

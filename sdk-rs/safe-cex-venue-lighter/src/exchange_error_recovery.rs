//! Exchange error normalization, signer retry/warning and snapshot failure bodies.
//! Native values/UTF16 are retained at required live JS property, callee, receiver,
//! instanceof, regex and coercion hosts. Out-of-type state/clock/Math primitives
//! use JS primitive operators; object values throw TypeError (no host ToPrimitive here).
use crate::{
    sdk_account_api::{AccountValue, JsBinary, JsRelation, primitive_binary, primitive_relation},
    transaction_signing::SigningResult,
};
use std::rc::Rc;
const STANDARD: &str = "Lighter Standard account rate limit temporarily active. Standard accounts have very small API limits; please wait before retrying or switch the Lighter account tier to Plus or Premium if this persists.";
const ACCOUNT: &str =
    "Lighter rate limit temporarily active for this account/session. Please wait before retrying.";
const UNKNOWN: &str = "Lighter rate limit temporarily active for this account/session. Please wait before retrying; if this persists, check your Lighter account tier and connectivity route.";
pub trait ErrorRecoveryCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub trait ExchangeErrorRecoveryHost {
    fn owner(&self) -> AccountValue;
    /// Actual live JS Get/strict Set, including original throws and native identity.
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()>;
    /// Capture actual callee AND receiver before argument evaluation; defer
    /// noncallable Call failure until after args. Dotted globals include member lookup.
    fn global(&self, name: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>>;
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn ErrorRecoveryCall>>;
    /// Actual Error global lookup and complete instanceof operator, including
    /// Symbol.hasInstance/prototype traps and original errors; not shape inference.
    fn instance_of_error(&self, value: &AccountValue) -> SigningResult<bool>;
    /// Actual fresh /timeout of \d+ms exceeded/i literal, test method capture/call
    /// with regex receiver and value, JS coercion and original errors. Reflective
    /// regex policy and literal resource behavior are required host contracts.
    fn timeout_regex_test(&self, value: &AccountValue) -> SigningResult<bool>;
    /// Full source template substitution ToString, with native UTF16/coercion throws.
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>>;
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(s.encode_utf16().collect())
}
fn eq(value: &AccountValue, s: &str) -> bool {
    matches!(value,AccountValue::String(v) if *v==s.encode_utf16().collect::<Vec<_>>())
}
fn call(
    h: &dyn ExchangeErrorRecoveryHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    h.member(&h.owner(), key)?.call(args)
}
fn optional(
    h: &dyn ExchangeErrorRecoveryHost,
    value: &AccountValue,
    keys: &[&str],
) -> SigningResult<AccountValue> {
    let mut value = value.clone();
    for key in keys {
        if value.is_nullish() {
            return Ok(AccountValue::Undefined);
        }
        value = h.get(&value, key)?;
    }
    Ok(value)
}
fn fallback_empty(value: AccountValue) -> AccountValue {
    if value.truthy() { value } else { text("") }
}
fn now(h: &dyn ExchangeErrorRecoveryHost) -> SigningResult<AccountValue> {
    h.global("virtualClock.getCurrentTimeMs")?.call(&[])
}
// ts: LighterExchange.getAccountRateLimitGuidanceMessage
pub fn get_account_rate_limit_guidance_message(
    h: &dyn ExchangeErrorRecoveryHost,
) -> SigningResult<AccountValue> {
    let tier = call(h, "getCachedAccountTier", &[])?;
    Ok(text(if eq(&tier, "standard") {
        STANDARD
    } else if eq(&tier, "premium") || eq(&tier, "plus") {
        ACCOUNT
    } else {
        UNKNOWN
    }))
}
// ts: LighterExchange#normalizeSignerErrorMessage
pub fn normalize_signer_error_message(
    h: &dyn ExchangeErrorRecoveryHost,
    error: &AccountValue,
) -> SigningResult<AccountValue> {
    if h.instance_of_error(error)? && h.get(error, "message")?.truthy() {
        return h.get(error, "message");
    }
    h.global("String")?.call(&[fallback_empty(error.clone())])
}
// ts: LighterExchange#shouldShortCircuitSignerRetry
pub fn should_short_circuit_signer_retry(h: &dyn ExchangeErrorRecoveryHost) -> SigningResult<bool> {
    let now = now(h)?;
    let issue = h.member(&h.owner(), "isWasmRuntimeIssueMessage")?;
    let reason = fallback_empty(h.get(&h.owner(), "signerInitErrorMessage")?);
    let issue = issue.call(&[reason])?.truthy();
    if issue && !h.global("isWasmReady")?.call(&[])?.truthy() {
        return Ok(true);
    }
    primitive_relation(
        JsRelation::Less,
        &now,
        &h.get(&h.owner(), "signerRetryBlockedUntil")?,
    )
}
// ts: LighterExchange#getSignerRetryBlockMessage
pub fn get_signer_retry_block_message(
    h: &dyn ExchangeErrorRecoveryHost,
) -> SigningResult<AccountValue> {
    let now = now(h)?;
    let max = h.global("Math.max")?;
    let until = h.get(&h.owner(), "signerRetryBlockedUntil")?;
    let difference = primitive_binary(JsBinary::Subtract, &until, &now)?;
    let remaining = max.call(&[AccountValue::Number(0.0), difference])?;
    let issue = h.member(&h.owner(), "isWasmRuntimeIssueMessage")?;
    let message = fallback_empty(h.get(&h.owner(), "signerInitErrorMessage")?);
    let issue = issue.call(&[message])?.truthy();
    let hint = if issue && !h.global("isWasmReady")?.call(&[])?.truthy() {
        "WASM runtime is still unavailable."
            .encode_utf16()
            .collect()
    } else if primitive_relation(JsRelation::Greater, &remaining, &AccountValue::Number(0.0))? {
        let ceil = h.global("Math.ceil")?;
        let seconds = ceil.call(&[primitive_binary(
            JsBinary::Divide,
            &remaining,
            &AccountValue::Number(1000.0),
        )?])?;
        let mut hint: Vec<u16> = "Retrying is paused for ".encode_utf16().collect();
        hint.extend(h.template_string(seconds)?);
        hint.extend("s.".encode_utf16());
        hint
    } else {
        "Retrying is temporarily paused.".encode_utf16().collect()
    };
    let reason = if h.get(&h.owner(), "signerInitErrorMessage")?.truthy() {
        let mut reason: Vec<u16> = " Last init failure: ".encode_utf16().collect();
        reason.extend(h.template_string(h.get(&h.owner(), "signerInitErrorMessage")?)?);
        reason
    } else {
        vec![]
    };
    let mut result: Vec<u16> =
        "Lighter signer unavailable; execution is disabled until signer runtime recovers. "
            .encode_utf16()
            .collect();
    result.extend(hint);
    result.extend(reason);
    Ok(AccountValue::String(result))
}
// ts: LighterExchange#warnSignerUnavailable
pub fn warn_signer_unavailable(
    h: &dyn ExchangeErrorRecoveryHost,
    message: &[u16],
) -> SigningResult<()> {
    let now = now(h)?;
    let last = h.get(&h.owner(), "lastSignerWarningAt")?;
    let elapsed = primitive_binary(JsBinary::Subtract, &now, &last)?;
    if primitive_relation(JsRelation::Less, &elapsed, &AccountValue::Number(10000.0))? {
        return Ok(());
    }
    h.set(&h.owner(), "lastSignerWarningAt", now)?;
    call(
        h,
        "log",
        &[AccountValue::String(message.to_vec()), text("warning")],
    )?;
    Ok(())
}
// ts: LighterExchange#extractOrderErrorMessage
pub fn extract_order_error_message(
    h: &dyn ExchangeErrorRecoveryHost,
    error: &AccountValue,
) -> SigningResult<AccountValue> {
    if matches!(error, AccountValue::String(_)) {
        return Ok(error.clone());
    }
    if h.instance_of_error(error)? && h.get(error, "message")?.truthy() {
        return h.get(error, "message");
    }
    let mut candidate = optional(h, error, &["response", "data", "error", "message"])?;
    if candidate.is_nullish() {
        candidate = optional(h, error, &["response", "data", "message"])?
    }
    if candidate.is_nullish() {
        candidate = optional(h, error, &["response", "data", "error"])?
    }
    if candidate.is_nullish() {
        candidate = optional(h, error, &["message"])?
    }
    if !candidate.is_nullish() {
        return h.global("String")?.call(&[candidate]);
    }
    match h
        .global("JSON.stringify")
        .and_then(|json| json.call(&[error.clone()]))
    {
        Ok(value) => Ok(value),
        Err(_) => h.global("String")?.call(&[error.clone()]),
    }
}
// ts: LighterExchange#isTransientLighterRestTransportError
pub fn is_transient_lighter_rest_transport_error(
    h: &dyn ExchangeErrorRecoveryHost,
    error: &AccountValue,
) -> SigningResult<bool> {
    let stringify = h.global("String")?;
    let mut code = optional(h, error, &["code"])?;
    if code.is_nullish() {
        code = optional(h, error, &["response", "data", "code"])?
    }
    if code.is_nullish() {
        code = text("")
    }
    let code = stringify.call(&[code])?;
    let code = h.member(&code, "toUpperCase")?.call(&[])?;
    let message = call(h, "extractOrderErrorMessage", &[error.clone()])?;
    let message = h.member(&message, "toLowerCase")?.call(&[])?;
    Ok(eq(&code, "NO_RESPONSE")
        || eq(&code, "ECONNABORTED")
        || eq(&code, "ERR_NETWORK")
        || eq(&message, "no response received from server")
        || eq(&message, "network error")
        || h.timeout_regex_test(&message)?)
}
// ts: LighterExchange#handlePrivateSnapshotFetchError
pub fn handle_private_snapshot_fetch_error(
    h: &dyn ExchangeErrorRecoveryHost,
    error: &AccountValue,
) -> SigningResult<()> {
    if !call(h, "isTransientLighterRestTransportError", &[error.clone()])?.truthy() {
        let emit = h.member(&h.owner(), "emitAndLogError")?;
        let value = if h.instance_of_error(error)? || matches!(error, AccountValue::String(_)) {
            error.clone()
        } else {
            call(h, "extractOrderErrorMessage", &[error.clone()])?
        };
        emit.call(&[value])?;
        return Ok(());
    }
    let now = now(h)?;
    if primitive_relation(
        JsRelation::Greater,
        &h.get(&h.owner(), "lastTransientSnapshotWarningAt")?,
        &AccountValue::Number(0.0),
    )? && primitive_relation(
        JsRelation::Less,
        &primitive_binary(
            JsBinary::Subtract,
            &now,
            &h.get(&h.owner(), "lastTransientSnapshotWarningAt")?,
        )?,
        &AccountValue::Number(60000.0),
    )? {
        return Ok(());
    }
    h.set(&h.owner(), "lastTransientSnapshotWarningAt", now)?;
    let log = h.member(&h.owner(), "log")?;
    let value = call(h, "extractOrderErrorMessage", &[error.clone()])?;
    let mut message: Vec<u16> =
        "[Lighter] Private snapshot fetch skipped after transient REST transport failure: "
            .encode_utf16()
            .collect();
    message.extend(h.template_string(value)?);
    log.call(&[AccountValue::String(message), text("warning")])?;
    Ok(())
}

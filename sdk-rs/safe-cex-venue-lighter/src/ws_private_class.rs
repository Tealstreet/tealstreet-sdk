//! LighterPrivateWebsocket class assembly plus its socket lifecycle bodies
//! (connect, close, message dispatch, idle watchdog and diagnostics loop).
//! Every owner, parent, socket and global operation goes through the host so
//! live getters, receivers and thrown values keep their source identity.
use crate::{
    sdk_account_api::AccountValue,
    transaction_promise::{NativePromise, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_policy::{
        LIGHTER_POLICY_REASON_RE, calculate_lighter_private_reconnect_delay,
        classify_lighter_private_close,
    },
    ws_private,
    ws_private_feed_helpers::{self, LighterPrivateFeedRuntime},
    ws_private_recovery::{self, PrivateRecoveryHost, RecoveryCallback},
    ws_private_stream_handlers::{self, LighterPrivateStreamRuntime},
    ws_private_warnings::{
        self, GEORESTRICTED_MESSAGE, PrivateStreamWarningParams, PrivateWarningFlags,
        PrivateWarningHost, PrivateWarningState, ReadonlyWarningTimes,
    },
};
use safe_cex_num::{coercion::parse_int, js_number_string, math_round};
use std::rc::Rc;

const PRIVATE_STREAM_WARNING_AFTER_MS: f64 = 90_000.0;
const PRIVATE_STREAM_WARNING_COOLDOWN_MS: f64 = 60_000.0;
const RATE_LIMIT_COOLDOWN_MS: f64 = 60_000.0;
const RATE_LIMIT_RECONNECT_DELAY_MS: f64 = 30_000.0;
const EARLY_CLOSE_WINDOW_MS: f64 = 10_000.0;
const DIAGNOSTICS_INTERVAL_MS: f64 = 5_000.0;

/// A callee captured before its arguments are evaluated, bound to its source
/// receiver (owner, parent, emitter, or undefined for globals/imports).
pub trait PrivateSocketCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub type PrivateSocketListener = Rc<dyn Fn(&[AccountValue]) -> SigningResult<AccountValue>>;
/// Live socket owner operations. Dotted names ("parent.isDisposed",
/// "parent.emitter.emit") walk the owner's live properties on every use.
pub trait PrivateSocketHost: PrivateRecoveryHost {
    fn value(&self, name: &str) -> SigningResult<AccountValue>;
    fn set_value(&self, name: &str, value: AccountValue) -> SigningResult<()>;
    /// Owner/parent member or captured global/import (clearTimeout,
    /// clearInterval, resolveTransportRouting), Get now and Call later.
    fn method(&self, name: &str) -> SigningResult<Rc<dyn PrivateSocketCall>>;
    /// Module binding read (WS_MIN_RECONNECTION_DELAY).
    fn binding(&self, name: &str) -> SigningResult<AccountValue>;
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    fn call(
        &self,
        callee: AccountValue,
        receiver: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
    /// A fresh source arrow function value wrapping the listener body.
    fn function(&self, body: PrivateSocketListener) -> SigningResult<AccountValue>;
    fn capture_set_interval(&self)
    -> SigningResult<Rc<dyn ws_private_recovery::RecoveryTimerCall>>;
    fn date_now(&self) -> SigningResult<f64>;
    /// new Date(ms).toISOString(), including its RangeError for invalid dates.
    fn iso_string(&self, ms: f64) -> SigningResult<Vec<u16>>;
    fn to_string(&self, value: AccountValue) -> SigningResult<Vec<u16>>;
    fn to_number(&self, value: AccountValue) -> SigningResult<f64>;
    /// The imported @tealstreet/json-parse jsonParse.
    fn json_parse(&self, data: AccountValue) -> SigningResult<AccountValue>;
    /// JSON.stringify; undefined when the value is not serializable.
    fn json_stringify(&self, value: AccountValue) -> SigningResult<AccountValue>;
    fn object(&self, entries: Vec<(&'static str, AccountValue)>) -> SigningResult<AccountValue>;
    /// `{ ...source, [key]: value }` with ordinary CopyDataProperties.
    fn spread_with(
        &self,
        source: AccountValue,
        key: &'static str,
        value: AccountValue,
    ) -> SigningResult<AccountValue>;
    /// Object.keys(value).length.
    fn own_key_count(&self, value: AccountValue) -> SigningResult<f64>;
    fn is_array(&self, value: &AccountValue) -> SigningResult<bool>;
    fn strict_equals(&self, left: &AccountValue, right: &AccountValue) -> bool;
    fn promise_runtime(&self) -> Rc<dyn TransactionRuntime>;
}

fn text(value: &str) -> AccountValue {
    AccountValue::String(value.encode_utf16().collect())
}
fn units(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}
fn invoke(
    host: &dyn PrivateSocketHost,
    name: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    host.method(name)?.call(args)
}
fn number(host: &dyn PrivateSocketHost, value: AccountValue) -> SigningResult<f64> {
    match value {
        AccountValue::Number(n) => Ok(n),
        other => host.to_number(other),
    }
}
fn set_bool(host: &dyn PrivateSocketHost, name: &str, value: bool) -> SigningResult<()> {
    host.set_value(name, AccountValue::Boolean(value))
}
/// `value?.key`
fn optional_get(
    host: &dyn PrivateSocketHost,
    value: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        host.get(value, key)
    }
}
fn string_units(value: &AccountValue) -> Option<Vec<u16>> {
    match value {
        AccountValue::String(s) => Some(s.clone()),
        _ => None,
    }
}
fn starts(value: &[u16], prefix: &str) -> bool {
    value.starts_with(&units(prefix))
}
fn equals(value: &[u16], word: &str) -> bool {
    value == units(word).as_slice()
}
fn lower(value: &[u16]) -> String {
    String::from_utf16_lossy(value).to_lowercase()
}
fn log_units(host: &dyn PrivateSocketHost, message: Vec<u16>, severity: &str) -> SigningResult<()> {
    host.method("parent.log")?
        .call(&[AccountValue::String(message), text(severity)])
        .map(|_| ())
}
fn clear_timer(host: &dyn PrivateSocketHost, field: &str) -> SigningResult<()> {
    if host.value(field)?.truthy() {
        let clear = host.method("clearTimeout")?;
        clear.call(&[host.value(field)?])?;
        host.set_value(field, AccountValue::Undefined)?;
    }
    Ok(())
}
fn concat(parts: &[&[u16]]) -> Vec<u16> {
    parts.concat()
}

// ts: LighterPrivateWebsocket.checkLastReceived
pub fn check_last_received(host: &dyn PrivateSocketHost) -> SigningResult<()> {
    let now = host.date_now()?;
    let diff = now - host.number("lastReceived")?;
    let ping_threshold = number(host, invoke(host, "getIdlePingThresholdMs", &[])?)?;
    let reconnect_threshold = number(host, invoke(host, "getIdleReconnectThresholdMs", &[])?)?;
    let last_received_time = host.iso_string(host.number("lastReceived")?)?;
    if diff > reconnect_threshold {
        let message = concat(&[
            &units(&format!(
                "[Lighter][PWS] No private stream messages for {}ms (last at ",
                js_number_string(diff)
            )),
            &last_received_time,
            &units("); reconnecting."),
        ]);
        log_units(host, message, "warning")?;
        let clear = host.method("clearTimeout")?;
        clear.call(&[host.value("lastReceivedTimer")?])?;
        host.set_number("lastReceived", 0.0)?;
        set_bool(host, "pingSent", false)?;
        host.set_number("lastIdlePingAt", 0.0)?;
        invoke(host, "reconnect", &[])?;
        return Ok(());
    }
    if diff > ping_threshold {
        let retry_interval = number(host, invoke(host, "getPingIntervalMs", &[])?)?;
        let should_retry = !host.value("pingSent")?.truthy()
            || now - host.number("lastIdlePingAt")? >= retry_interval;
        if should_retry {
            let message = concat(&[
                &units(&format!(
                    "[Lighter][PWS] Private stream idle for {}ms (last at ",
                    js_number_string(diff)
                )),
                &last_received_time,
                &units("); sending ping."),
            ]);
            log_units(host, message, "info")?;
            // Source wraps the ping attempt in an empty catch.
            let _ignored: SigningResult<()> = (|| {
                let previous = host.value("pingAt")?;
                invoke(host, "ping", &[])?;
                if !host.strict_equals(&host.value("pingAt")?, &previous) {
                    set_bool(host, "pingSent", true)?;
                    host.set_number("lastIdlePingAt", now)?;
                }
                Ok(())
            })();
        }
        return Ok(());
    }
    if diff <= ping_threshold {
        set_bool(host, "pingSent", false)?;
        host.set_number("lastIdlePingAt", 0.0)?;
    }
    Ok(())
}

fn diagnostics_tick(host: &dyn PrivateSocketHost) -> SigningResult<()> {
    if host.value("isDisposed")?.truthy() || !host.value("isConnected")?.truthy() {
        return Ok(());
    }
    let now = host.now()?;
    let since_connect = now - host.number("lastConnectTime")?;
    if host.number("accountStreamMessageCount")? == 0.0
        && since_connect > PRIVATE_STREAM_WARNING_AFTER_MS
        && !host.value("hasWarnedNoAccountStream")?.truthy()
        && !invoke(host, "isWsRateLimited", &[])?.truthy()
    {
        set_bool(host, "hasWarnedNoAccountStream", true)?;
        let seconds = js_number_string(math_round(since_connect / 1000.0));
        let route = invoke(host, "isWsProxyRouteEnabled", &[])?;
        let route = host.to_string(route)?;
        let fallback = host.value("useReadonlyFallback")?;
        let fallback = host.to_string(fallback)?;
        let message = concat(&[
            &units(&format!(
                "[Lighter][PWS] Connected but no private account stream messages after {seconds}s. wsRoute="
            )),
            &route,
            &units(" readonlyFallback="),
            &fallback,
            &units("."),
        ]);
        log_units(host, message, "warning")?;
        if !host.value("hasWarnedNoAccountStreamToast")?.truthy() {
            set_bool(host, "hasWarnedNoAccountStreamToast", true)?;
            let emit = host.method("parent.emitter.emit")?;
            let warning = if invoke(host, "isWsProxyRouteEnabled", &[])?.truthy() {
                "Lighter private WS route is connected but not receiving account updates. Proxy infra may be unavailable; private updates may rely on REST reconciliation."
            } else {
                "Lighter private websocket is connected but not receiving account updates. Private updates may rely on REST reconciliation in this environment."
            };
            let options = host.object(vec![("forceToast", AccountValue::Boolean(true))])?;
            emit.call(&[text("warning"), text(warning), options])?;
        }
    }
    if host.number("lastAccountStreamMessageAt")? > 0.0 {
        let stream_age = now - host.number("lastAccountStreamMessageAt")?;
        let should_warn = stream_age > PRIVATE_STREAM_WARNING_AFTER_MS
            && (host.number("lastStaleAccountStreamWarningAt")? == 0.0
                || now - host.number("lastStaleAccountStreamWarningAt")?
                    > PRIVATE_STREAM_WARNING_COOLDOWN_MS);
        if should_warn {
            host.set_number("lastStaleAccountStreamWarningAt", now)?;
            log_units(
                host,
                units(&format!(
                    "[Lighter][PWS] Private account stream stale ({}s since last message).",
                    js_number_string(math_round(stream_age / 1000.0))
                )),
                "warning",
            )?;
        }
    }
    invoke(host, "maybeRecoverPrivateStream", &[]).map(|_| ())
}

// ts: LighterPrivateWebsocket#beginDiagnosticsLoop
pub fn begin_diagnostics_loop(host: Rc<dyn PrivateSocketHost>) -> SigningResult<()> {
    if host.value("diagnosticsInterval")?.truthy() {
        let clear = host.method("clearInterval")?;
        clear.call(&[host.value("diagnosticsInterval")?])?;
    }
    for flag in [
        "hasWarnedNoAccountStream",
        "hasWarnedNoAccountStreamToast",
        "hasWarnedProxyStreamErrorToast",
    ] {
        set_bool(host.as_ref(), flag, false)?;
    }
    for field in [
        "lastStaleAccountStreamWarningAt",
        "accountStreamMessageCount",
        "lastAccountStreamMessageAt",
    ] {
        host.set_number(field, 0.0)?;
    }
    let set_interval = host.capture_set_interval()?;
    let owner = host.clone();
    let callback: RecoveryCallback = Rc::new(move || diagnostics_tick(owner.as_ref()));
    let id = set_interval.call(callback, DIAGNOSTICS_INTERVAL_MS)?;
    host.set_value("diagnosticsInterval", id)
}

fn connection_error(
    host: &dyn PrivateSocketHost,
    mode: &str,
    event: &AccountValue,
) -> SigningResult<AccountValue> {
    let log = host.method("parent.log")?;
    let mut detail = optional_get(host, event, "message")?;
    if !detail.truthy() {
        let error = optional_get(host, event, "error")?;
        detail = optional_get(host, &error, "message")?;
    }
    if !detail.truthy() {
        detail = text("");
    }
    let mut message = units("[Lighter][PWS] Connection error");
    if detail.truthy() {
        message.extend(units(": "));
        message.extend(host.to_string(detail)?);
    }
    message.extend(units(&format!(" (mode: {mode})")));
    log.call(&[AccountValue::String(message), text("warning")])?;
    Ok(AccountValue::Undefined)
}

fn connect(host: &Rc<dyn PrivateSocketHost>) -> SigningResult<()> {
    let h = host.as_ref();
    if h.value("isDisposed")?.truthy() {
        return Ok(());
    }
    let use_proxy = invoke(h, "isWsProxyRouteEnabled", &[])?.truthy();
    let base_url = if use_proxy {
        let resolve = h.method("resolveTransportRouting")?;
        let routing = resolve.call(&[h.value("parent.name")?, h.value("parent.options")?])?;
        let ws_private = h.get(&routing, "wsPrivate")?;
        h.get(&ws_private, "host")?
    } else {
        h.value("parent.lighterDeploymentProfile.wsUrl")?
    };
    let use_readonly = h.value("useReadonlyFallback")?.truthy() && !use_proxy;
    let ws_url = if use_readonly {
        let base = h.to_string(base_url)?;
        let separator = if base.contains(&(b'?' as u16)) {
            "&"
        } else {
            "?"
        };
        AccountValue::String(concat(&[&base, &units(separator), &units("readonly=true")]))
    } else {
        base_url
    };
    let mode = if use_proxy {
        "proxy-route"
    } else if use_readonly {
        "readonly-fallback"
    } else {
        "direct"
    };
    let log = h.method("parent.log")?;
    let attempt = js_number_string(h.number("failedHandshakes")? + 1.0);
    let url_text = h.to_string(ws_url.clone())?;
    let message = concat(&[
        &units(&format!(
            "[Lighter][PWS] Connecting ({mode}, attempt #{attempt}): "
        )),
        &url_text,
    ]);
    log.call(&[AccountValue::String(message), text("info")])?;
    set_bool(h, "openedSinceLastConnect", false)?;
    let ws = invoke(h, "createWebsocket", &[ws_url])?;
    h.set_value("ws", ws)?;
    for (event, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let ws = h.value("ws")?;
        let add = h.get(&ws, "addEventListener")?;
        let listener = h.value(handler)?;
        h.call(add, ws, &[text(event), listener])?;
    }
    let ws = h.value("ws")?;
    let add = h.get(&ws, "addEventListener")?;
    let owner = host.clone();
    let listener = h.function(Rc::new(move |args: &[AccountValue]| {
        let event = args.first().cloned().unwrap_or(AccountValue::Undefined);
        connection_error(owner.as_ref(), mode, &event)
    }))?;
    h.call(add, ws, &[text("error"), listener])?;
    Ok(())
}

// ts: LighterPrivateWebsocket.connectAndSubscribe
pub fn connect_and_subscribe(host: Rc<dyn PrivateSocketHost>) -> NativePromise<AccountValue> {
    let result = connect(&host).map(|()| AccountValue::Undefined);
    NativePromise::settled(host.promise_runtime(), result)
}

// ts: LighterPrivateWebsocket.onClose
pub fn on_close(host: Rc<dyn PrivateSocketHost>, event: AccountValue) -> SigningResult<()> {
    let h = host.as_ref();
    let record = h.method("recordCloseDiagnostics")?;
    let code = number(h, optional_get(h, &event, "code")?)?;
    let code = if code.is_finite() {
        AccountValue::Number(number(h, optional_get(h, &event, "code")?)?)
    } else {
        AccountValue::Undefined
    };
    let options = h.object(vec![("syntheticIfNoSocket", AccountValue::Boolean(true))])?;
    record.call(&[code, options])?;
    for name in [
        "stopDiagnosticsLoop",
        "clearStreamRecoveryReconnectTimeout",
        "clearRateLimitResubscribeTimeout",
    ] {
        invoke(h, name, &[])?;
    }
    let close_code = number(h, optional_get(h, &event, "code")?)?;
    let close_reason = if string_units(&optional_get(h, &event, "reason")?).is_some() {
        string_units(&h.get(&event, "reason")?).unwrap_or_default()
    } else {
        Vec::new()
    };
    let failed_before_open = !h.value("openedSinceLastConnect")?.truthy();
    let now = h.now()?;
    let time_since_open = if h.number("lastConnectTime")? > 0.0 {
        now - h.number("lastConnectTime")?
    } else {
        f64::INFINITY
    };
    let early_post_open_no_stream = h.value("openedSinceLastConnect")?.truthy()
        && time_since_open <= EARLY_CLOSE_WINDOW_MS
        && h.number("accountStreamMessageCount")? == 0.0;
    let classification = classify_lighter_private_close(
        Some(close_code),
        &String::from_utf16_lossy(&close_reason),
        failed_before_open,
        early_post_open_no_stream,
    );
    if classification.is_handshake_failure {
        let failed = h.number("failedHandshakes")?;
        h.set_number("failedHandshakes", failed + 1.0)?;
        if classification.is_readonly_fallback_signal {
            let failures = h.number("readonlyFallbackFailures")?;
            h.set_number("readonlyFallbackFailures", failures + 1.0)?;
        }
        if !classification.is_auth_issue
            && !invoke(h, "isWsProxyRouteEnabled", &[])?.truthy()
            && !h.value("useReadonlyFallback")?.truthy()
            && classification.is_readonly_fallback_signal
            && h.number("readonlyFallbackFailures")? >= 3.0
        {
            set_bool(h, "useReadonlyFallback", true)?;
            set_bool(h, "hasWarnedReadonlyFallback", true)?;
            invoke(
                h,
                "emitReadonlyFallbackWarning",
                &[AccountValue::Boolean(true)],
            )?;
        }
    }
    if classification.is_rate_limited {
        h.set_number("wsRateLimitedUntil", now + RATE_LIMIT_COOLDOWN_MS)?;
    }
    set_bool(h, "openedSinceLastConnect", false)?;
    h.set_number("lastReceived", 0.0)?;
    h.set_number("lastIdlePingAt", 0.0)?;
    clear_timer(h, "lastReceivedTimer")?;
    let parent = h.value("parent")?;
    let sync = h.get(&parent, "syncSocketHealthToStore")?;
    if !sync.is_nullish() {
        h.call(sync, parent, &[])?;
    }
    if close_code != 1000.0 && !h.value("parent.isDisposed")?.truthy() {
        let log = h.method("parent.log")?;
        let mut message = units(&format!(
            "[Lighter][PWS] WebSocket closed (code {}",
            js_number_string(close_code)
        ));
        if !close_reason.is_empty() {
            message.extend(units(", reason: "));
            message.extend(&close_reason);
        }
        message.extend(units(&format!(
            ", handshakes failed: {})",
            js_number_string(h.number("failedHandshakes")?)
        )));
        log.call(&[AccountValue::String(message), text("warning")])?;
    }
    if h.value("reconnecting")?.truthy() || h.value("parent.isDisposed")?.truthy() {
        return Ok(());
    }
    set_bool(h, "reconnecting", true)?;
    clear_timer(h, "pingTimeoutId")?;
    clear_timer(h, "pongMissTimeoutId")?;
    for (event, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let ws = h.value("ws")?;
        if ws.is_nullish() {
            continue;
        }
        let remove = h.get(&ws, "removeEventListener")?;
        if remove.is_nullish() {
            continue;
        }
        let listener = h.value(handler)?;
        h.call(remove, ws, &[text(event), listener])?;
    }
    h.set_value("ws", AccountValue::Undefined)?;
    let count = h.number("onCloseCount")?;
    h.set_number("onCloseCount", count + 1.0)?;
    if h.number("onCloseCount")? > 100.0 {
        log_units(
            h,
            units("[Lighter][PWS] onClose count > 100, stopping reconnection"),
            "error",
        )?;
        return Ok(());
    }
    let base_delay = number(h, h.binding("WS_MIN_RECONNECTION_DELAY")?)?;
    let failed_handshakes = h.number("failedHandshakes")?;
    let ws_rate_limited = invoke(h, "isWsRateLimited", &[])?.truthy();
    let delay = calculate_lighter_private_reconnect_delay(
        base_delay,
        failed_handshakes,
        classification.should_backoff,
        classification.is_rate_limited,
        ws_rate_limited,
        RATE_LIMIT_RECONNECT_DELAY_MS,
    );
    log_units(
        h,
        units(&format!(
            "[Lighter][PWS] Reconnecting in {}ms...",
            js_number_string(math_round(delay))
        )),
        "info",
    )?;
    let set_timeout = h.capture_set_timeout()?;
    let owner = host.clone();
    let callback: RecoveryCallback = Rc::new(move || {
        let h = owner.as_ref();
        if h.value("parent.isDisposed")?.truthy() {
            return Ok(());
        }
        set_bool(h, "reconnecting", false)?;
        h.set_number("lastCloseTime", h.date_now()?)?;
        invoke(h, "connectAndSubscribe", &[]).map(|_| ())
    });
    set_timeout.call(callback, delay)?;
    Ok(())
}

fn is_account_all(channel: &[u16]) -> bool {
    equals(channel, "account_all")
        || starts(channel, "account_all:")
        || starts(channel, "account_all/")
}
fn is_account_all_orders(channel: &[u16]) -> bool {
    equals(channel, "account_all_orders")
        || starts(channel, "account_all_orders:")
        || starts(channel, "account_all_orders/")
}
fn nonempty_keys(host: &dyn PrivateSocketHost, value: &AccountValue) -> SigningResult<bool> {
    Ok(value.truthy() && host.own_key_count(value.clone())? > 0.0)
}
fn coalesce(
    first: AccountValue,
    second: impl FnOnce() -> SigningResult<AccountValue>,
) -> SigningResult<AccountValue> {
    if first.is_nullish() {
        second()
    } else {
        Ok(first)
    }
}

fn dispatch_account_all(
    host: &dyn PrivateSocketHost,
    json: &AccountValue,
    channel: &[u16],
) -> SigningResult<()> {
    let envelope = optional_get(host, json, "data")?;
    let envelope_orders = optional_get(host, &envelope, "orders")?;
    let envelope_positions = optional_get(host, &envelope, "positions")?;
    let envelope_trades = optional_get(host, &envelope, "trades")?;
    let envelope_account = optional_get(host, &envelope, "account")?;
    let envelope_balances = optional_get(host, &envelope, "balances")?;
    let has_array_payloads = envelope.truthy()
        && (host.is_array(&envelope_orders)?
            || host.is_array(&envelope_positions)?
            || host.is_array(&host.get(&envelope, "fills")?)?);
    let by_orders = coalesce(optional_get(host, json, "orders")?, || Ok(envelope_orders))?;
    let by_positions = coalesce(optional_get(host, json, "positions")?, || {
        Ok(envelope_positions)
    })?;
    let by_trades = coalesce(optional_get(host, json, "trades")?, || Ok(envelope_trades))?;
    let has_balance = coalesce(optional_get(host, json, "account")?, || {
        coalesce(optional_get(host, json, "balances")?, || {
            coalesce(envelope_account, || Ok(envelope_balances))
        })
    })?
    .truthy();
    let has_by_market = nonempty_keys(host, &by_orders)?
        || nonempty_keys(host, &by_positions)?
        || nonempty_keys(host, &by_trades)?;
    if has_array_payloads || has_by_market || has_balance {
        invoke(
            host,
            "noteAccountStreamActivity",
            &[text("account_all"), AccountValue::String(channel.to_vec())],
        )?;
        invoke(host, "parent.invalidateAccountSnapshotCache", &[])?;
        invoke(host, "throttleFetchBalance", &[AccountValue::Number(500.0)])?;
        if invoke(host, "shouldUseRestOrderReconciliation", &[])?.truthy() {
            let throttle = host.method("throttleFetchOrders")?;
            let options = host.object(vec![("skipIfRecent", AccountValue::Boolean(true))])?;
            throttle.call(&[AccountValue::Number(500.0), options])?;
        }
    }
    if has_array_payloads && envelope.truthy() {
        let handler = host.method("handleAccountTopic")?;
        let message = host.spread_with(json.clone(), "data", envelope)?;
        handler.call(&[message])?;
    } else {
        let source = if has_by_market && envelope.truthy() {
            envelope
        } else {
            json.clone()
        };
        invoke(host, "handleAccountAllTopic", &[source])?;
    }
    Ok(())
}

fn error_message(host: &dyn PrivateSocketHost, json: &AccountValue) -> SigningResult<Vec<u16>> {
    let error = optional_get(host, json, "error")?;
    if let AccountValue::String(message) = error {
        return Ok(message);
    }
    if matches!(error, AccountValue::Object(_)) {
        let code = host.get(&error, "code")?;
        let code = if code.is_nullish() {
            units("?")
        } else {
            host.to_string(code)?
        };
        let detail = host.get(&error, "message")?;
        let detail = if detail.is_nullish() {
            host.json_stringify(error.clone())?
        } else {
            detail
        };
        let detail = host.to_string(detail)?;
        return Ok(concat(&[&units("["), &code, &units("] "), &detail]));
    }
    let message = optional_get(host, json, "message")?;
    let message = if message.truthy() {
        message
    } else {
        text("unknown private websocket error")
    };
    host.to_string(message)
}

/// Returns true when the source body returned early from the error branch.
fn dispatch_error(
    host: &dyn PrivateSocketHost,
    json: &AccountValue,
    is_orders_channel: bool,
) -> SigningResult<bool> {
    let error = optional_get(host, json, "error")?;
    let raw_code = if matches!(error, AccountValue::Object(_)) {
        host.get(&error, "code")?
    } else {
        optional_get(host, json, "code")?
    };
    let raw_code = if raw_code.is_nullish() {
        text("")
    } else {
        raw_code
    };
    let parsed_code = parse_int(
        &String::from_utf16_lossy(&host.to_string(raw_code)?),
        Some(10),
    );
    let message = error_message(host, json)?;
    let normalized = lower(&message);
    if parsed_code == 30003.0 || normalized.contains("already subscribed") {
        if host.value("hasPendingAccountAllOrdersSubscribe")?.truthy()
            || is_orders_channel
            || normalized.contains("account_all_orders")
        {
            set_bool(host, "hasPendingAccountAllOrdersSubscribe", false)?;
            set_bool(host, "hasReceivedAccountAllOrdersAck", true)?;
        }
        log_units(
            host,
            concat(&[
                &units(
                    "[Lighter][PWS] Ignoring benign duplicate subscription error from private stream: ",
                ),
                &message,
            ]),
            "info",
        )?;
        return Ok(true);
    }
    let invalid_channel = normalized.contains("invalid channel");
    let rate_limited =
        normalized.contains("too many websocket messages") || normalized.contains("rate limit");
    let policy_restricted = LIGHTER_POLICY_REASON_RE(&String::from_utf16_lossy(&message));
    let now = host.now()?;
    if rate_limited {
        invoke(
            host,
            "notePrivateStreamRateLimitError",
            &[AccountValue::Number(now)],
        )?;
        set_bool(host, "hasPendingAccountAllOrdersSubscribe", false)?;
        host.set_number("wsRateLimitedUntil", now + RATE_LIMIT_COOLDOWN_MS)?;
        log_units(
            host,
            units(&format!(
                "[Lighter][PWS] Server rate-limited private WS messages. Applying {}s cooldown.",
                js_number_string(RATE_LIMIT_COOLDOWN_MS / 1000.0)
            )),
            "warning",
        )?;
        invoke(host, "scheduleRateLimitResubscribe", &[])?;
    }
    if invalid_channel
        && host.value("hasPendingAccountAllOrdersSubscribe")?.truthy()
        && !host.value("hasReceivedAccountAllOrdersAck")?.truthy()
    {
        set_bool(host, "accountAllOrdersChannelEnabled", false)?;
        set_bool(host, "hasPendingAccountAllOrdersSubscribe", false)?;
        if !host.value("hasWarnedOrdersChannelUnsupported")?.truthy() {
            set_bool(host, "hasWarnedOrdersChannelUnsupported", true)?;
            log_units(
                host,
                units(
                    "[Lighter][PWS] account_all_orders channel appears unsupported in this environment. Continuing with REST order reconciliation.",
                ),
                "warning",
            )?;
        }
        return Ok(true);
    }
    log_units(
        host,
        concat(&[
            &units("[Lighter][PWS] Received error from private stream: "),
            &message,
        ]),
        "warning",
    )?;
    if !host.value("hasWarnedProxyStreamErrorToast")?.truthy() {
        let warning = host.method("getPrivateStreamErrorWarningMessage")?;
        let params = host.object(vec![
            ("errMsg", AccountValue::String(message.clone())),
            ("isRateLimitError", AccountValue::Boolean(rate_limited)),
            (
                "isPolicyRestrictionError",
                AccountValue::Boolean(policy_restricted),
            ),
        ])?;
        let toast = warning.call(&[params])?;
        let rate_limit_message = invoke(host, "getRateLimitWarningMessage", &[])?;
        let force = !rate_limited
            || !host.strict_equals(&toast, &rate_limit_message)
            || invoke(
                host,
                "shouldForceToastPrivateStreamRateLimit",
                &[AccountValue::Number(now)],
            )?
            .truthy();
        if force {
            set_bool(host, "hasWarnedProxyStreamErrorToast", true)?;
            let emit = host.method("parent.emitter.emit")?;
            let options = host.object(vec![("forceToast", AccountValue::Boolean(true))])?;
            emit.call(&[text("warning"), toast, options])?;
        }
    }
    Ok(false)
}

fn type_is(host: &dyn PrivateSocketHost, json: &AccountValue, word: &str) -> SigningResult<bool> {
    Ok(string_units(&optional_get(host, json, "type")?).is_some_and(|t| equals(&t, word)))
}

// ts: LighterPrivateWebsocket.onMessage
pub fn on_message(host: &dyn PrivateSocketHost, event: AccountValue) -> SigningResult<()> {
    let data = host.get(&event, "data")?;
    if host.value("isDisposed")?.truthy() {
        return Ok(());
    }
    invoke(host, "trackLastReceived", &[])?;
    invoke(host, "emitWsDiagnosticReceive", &[data.clone()])?;
    let json = host.json_parse(data)?;
    let channel = string_units(&optional_get(host, &json, "channel")?).unwrap_or_default();
    let all_channel = is_account_all(&channel);
    let orders_channel = is_account_all_orders(&channel);
    if all_channel {
        dispatch_account_all(host, &json, &channel)?;
    }
    if orders_channel {
        let envelope = optional_get(host, &json, "data")?;
        let by_market = coalesce(optional_get(host, &json, "orders")?, || {
            optional_get(host, &envelope, "orders")
        })?;
        let orders = invoke(host, "flattenOrdersByMarket", &[by_market])?;
        if number(host, host.get(&orders, "length")?)? > 0.0 {
            invoke(
                host,
                "noteAccountStreamActivity",
                &[
                    text("account_all_orders"),
                    AccountValue::String(channel.clone()),
                ],
            )?;
            invoke(host, "handleOrderTopic", &[orders])?;
        }
    }
    let message_type = string_units(&optional_get(host, &json, "type")?).unwrap_or_default();
    let read_channel = || -> SigningResult<Vec<u16>> {
        Ok(string_units(&host.get(&json, "channel")?).unwrap_or_default())
    };
    if (equals(&message_type, "subscribed") || starts(&message_type, "subscribed/"))
        && string_units(&optional_get(host, &json, "channel")?).is_some()
        && (starts(&read_channel()?, "account_all/")
            || starts(&read_channel()?, "account_all:")
            || equals(&read_channel()?, "account_all")
            || starts(&read_channel()?, "account_all_orders/")
            || starts(&read_channel()?, "account_all_orders:")
            || equals(&read_channel()?, "account_all_orders"))
    {
        if starts(&read_channel()?, "account_all_orders/")
            || starts(&read_channel()?, "account_all_orders:")
            || equals(&read_channel()?, "account_all_orders")
        {
            set_bool(host, "hasPendingAccountAllOrdersSubscribe", false)?;
            set_bool(host, "hasReceivedAccountAllOrdersAck", true)?;
        }
        let log = host.method("parent.log")?;
        let acknowledged = host.to_string(host.get(&json, "channel")?)?;
        let message = concat(&[
            &units("[Lighter][PWS] Subscription acknowledged for "),
            &acknowledged,
            &units(" (type="),
            &message_type,
            &units(")."),
        ]);
        log.call(&[AccountValue::String(message), text("info")])?;
    }
    if starts(&message_type, "update/") && (all_channel || orders_channel) {
        let message = concat(&[
            &units("[Lighter][PWS] Streaming update received (type="),
            &message_type,
            &units(")."),
        ]);
        log_units(host, message, "info")?;
    }
    if (type_is(host, &json, "error")? || optional_get(host, &json, "error")?.truthy())
        && dispatch_error(host, &json, orders_channel)?
    {
        return Ok(());
    }
    if type_is(host, &json, "ping")? {
        let send = host.method("send")?;
        let pong = host.object(vec![
            ("type", text("pong")),
            ("timestamp", AccountValue::Number(host.now()?)),
        ])?;
        send.call(&[host.json_stringify(pong)?])?;
    }
    if type_is(host, &json, "pong")? {
        let handle = host.method("handlePongEvent")?;
        handle.call(&[invoke(host, "getPingIntervalMs", &[])?])?;
    }
    Ok(())
}

/// Bindings for the class's ported member families, scoped to the invocation's
/// actual owner. The generic feed/stream runtimes carry their own host `H`.
pub trait PrivateWebsocketContexts<H> {
    fn socket(&self, owner: AccountValue) -> Rc<dyn PrivateSocketHost>;
    fn warnings(&self, owner: AccountValue) -> Rc<dyn PrivateWarningHost>;
    fn warning_flags(&self, owner: AccountValue) -> Rc<dyn PrivateWarningFlags>;
    fn warning_params(&self, params: AccountValue) -> Rc<dyn PrivateStreamWarningParams>;
    /// The class-wide readonlyFallbackWarningAtByAccount map.
    fn readonly_warning_times(&self) -> ReadonlyWarningTimes;
    fn feed(&self) -> Rc<LighterPrivateFeedRuntime<H, AccountValue>>;
    fn stream(&self) -> Rc<LighterPrivateStreamRuntime<H, AccountValue>>;
    fn flatten_by_market_payload(
        &self,
        payload: &AccountValue,
        include_single_objects: bool,
    ) -> SigningResult<AccountValue>;
    fn empty_array(&self) -> AccountValue;
    fn type_error(&self, message: &str) -> TransactionSigningError;
    fn promise_value(&self, value: NativePromise<AccountValue>) -> AccountValue;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateWebsocketMethodMetadata {
    pub name: &'static str,
    pub length: usize,
    pub asynchronous: bool,
}

/// Instance arrow fields in source declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateWebsocketMethod {
    ShouldEnforcePongMissReconnect,
    GetRateLimitWarningMessage,
    GetReadonlyFallbackWarningKey,
    EmitReadonlyFallbackWarning,
    IsReadonlyFallbackActive,
    ShouldUseRestOrderReconciliation,
    IsWsProxyRouteEnabled,
    ShouldUseGeorestrictedConnectivityWarning,
    GetPrivateStreamErrorWarningMessage,
    GetPingIntervalMs,
    GetIdlePingThresholdMs,
    GetIdleReconnectThresholdMs,
    GetWatchdogStaleThresholdMs,
    IsWsRateLimited,
    NotePrivateStreamRateLimitError,
    ClearPrivateStreamRateLimitErrorState,
    ShouldForceToastPrivateStreamRateLimit,
    CheckLastReceived,
    BeginDiagnosticsLoop,
    StopDiagnosticsLoop,
    ClearStreamRecoveryReconnectTimeout,
    ClearRateLimitResubscribeTimeout,
    ScheduleRateLimitResubscribe,
    ScheduleStreamRecoveryReconnectFallback,
    MaybeRecoverPrivateStream,
    ThrottleFetchOrders,
    ThrottleFetchPositions,
    ThrottleFetchBalance,
    ResolveExplicitlyClosedPositionUpdates,
    FlattenByMarketPayload,
    FlattenOrdersByMarket,
    ScheduleThrottledTask,
    SendSubscribePayload,
    NoteAccountStreamActivity,
    IsRecentStreamEvent,
    NormalizeOrderStatus,
    IsUserCancelledReason,
    ConnectAndSubscribe,
    OnOpen,
    OnClose,
    OnMessage,
    Ping,
    HandleAccountTopic,
    HandleAccountAllTopic,
    HandleOrderTopic,
    HandlePositionTopic,
    HandleFillsTopic,
    AuthenticateAndSubscribe,
    Dispose,
}
impl PrivateWebsocketMethod {
    pub const ALL: [Self; 49] = [
        Self::ShouldEnforcePongMissReconnect,
        Self::GetRateLimitWarningMessage,
        Self::GetReadonlyFallbackWarningKey,
        Self::EmitReadonlyFallbackWarning,
        Self::IsReadonlyFallbackActive,
        Self::ShouldUseRestOrderReconciliation,
        Self::IsWsProxyRouteEnabled,
        Self::ShouldUseGeorestrictedConnectivityWarning,
        Self::GetPrivateStreamErrorWarningMessage,
        Self::GetPingIntervalMs,
        Self::GetIdlePingThresholdMs,
        Self::GetIdleReconnectThresholdMs,
        Self::GetWatchdogStaleThresholdMs,
        Self::IsWsRateLimited,
        Self::NotePrivateStreamRateLimitError,
        Self::ClearPrivateStreamRateLimitErrorState,
        Self::ShouldForceToastPrivateStreamRateLimit,
        Self::CheckLastReceived,
        Self::BeginDiagnosticsLoop,
        Self::StopDiagnosticsLoop,
        Self::ClearStreamRecoveryReconnectTimeout,
        Self::ClearRateLimitResubscribeTimeout,
        Self::ScheduleRateLimitResubscribe,
        Self::ScheduleStreamRecoveryReconnectFallback,
        Self::MaybeRecoverPrivateStream,
        Self::ThrottleFetchOrders,
        Self::ThrottleFetchPositions,
        Self::ThrottleFetchBalance,
        Self::ResolveExplicitlyClosedPositionUpdates,
        Self::FlattenByMarketPayload,
        Self::FlattenOrdersByMarket,
        Self::ScheduleThrottledTask,
        Self::SendSubscribePayload,
        Self::NoteAccountStreamActivity,
        Self::IsRecentStreamEvent,
        Self::NormalizeOrderStatus,
        Self::IsUserCancelledReason,
        Self::ConnectAndSubscribe,
        Self::OnOpen,
        Self::OnClose,
        Self::OnMessage,
        Self::Ping,
        Self::HandleAccountTopic,
        Self::HandleAccountAllTopic,
        Self::HandleOrderTopic,
        Self::HandlePositionTopic,
        Self::HandleFillsTopic,
        Self::AuthenticateAndSubscribe,
        Self::Dispose,
    ];
    pub fn metadata(self) -> PrivateWebsocketMethodMetadata {
        let (name, length, asynchronous) = match self {
            Self::ShouldEnforcePongMissReconnect => ("shouldEnforcePongMissReconnect", 0, false),
            Self::GetRateLimitWarningMessage => ("getRateLimitWarningMessage", 0, false),
            Self::GetReadonlyFallbackWarningKey => ("getReadonlyFallbackWarningKey", 0, false),
            Self::EmitReadonlyFallbackWarning => ("emitReadonlyFallbackWarning", 0, false),
            Self::IsReadonlyFallbackActive => ("isReadonlyFallbackActive", 0, false),
            Self::ShouldUseRestOrderReconciliation => {
                ("shouldUseRestOrderReconciliation", 0, false)
            }
            Self::IsWsProxyRouteEnabled => ("isWsProxyRouteEnabled", 0, false),
            Self::ShouldUseGeorestrictedConnectivityWarning => {
                ("shouldUseGeorestrictedConnectivityWarning", 0, false)
            }
            Self::GetPrivateStreamErrorWarningMessage => {
                ("getPrivateStreamErrorWarningMessage", 1, false)
            }
            Self::GetPingIntervalMs => ("getPingIntervalMs", 0, false),
            Self::GetIdlePingThresholdMs => ("getIdlePingThresholdMs", 0, false),
            Self::GetIdleReconnectThresholdMs => ("getIdleReconnectThresholdMs", 0, false),
            Self::GetWatchdogStaleThresholdMs => ("getWatchdogStaleThresholdMs", 0, false),
            Self::IsWsRateLimited => ("isWsRateLimited", 0, false),
            Self::NotePrivateStreamRateLimitError => ("notePrivateStreamRateLimitError", 1, false),
            Self::ClearPrivateStreamRateLimitErrorState => {
                ("clearPrivateStreamRateLimitErrorState", 0, false)
            }
            Self::ShouldForceToastPrivateStreamRateLimit => {
                ("shouldForceToastPrivateStreamRateLimit", 1, false)
            }
            Self::CheckLastReceived => ("checkLastReceived", 0, false),
            Self::BeginDiagnosticsLoop => ("beginDiagnosticsLoop", 0, false),
            Self::StopDiagnosticsLoop => ("stopDiagnosticsLoop", 0, false),
            Self::ClearStreamRecoveryReconnectTimeout => {
                ("clearStreamRecoveryReconnectTimeout", 0, false)
            }
            Self::ClearRateLimitResubscribeTimeout => {
                ("clearRateLimitResubscribeTimeout", 0, false)
            }
            Self::ScheduleRateLimitResubscribe => ("scheduleRateLimitResubscribe", 0, false),
            Self::ScheduleStreamRecoveryReconnectFallback => {
                ("scheduleStreamRecoveryReconnectFallback", 0, false)
            }
            Self::MaybeRecoverPrivateStream => ("maybeRecoverPrivateStream", 0, false),
            Self::ThrottleFetchOrders => ("throttleFetchOrders", 0, false),
            Self::ThrottleFetchPositions => ("throttleFetchPositions", 0, false),
            Self::ThrottleFetchBalance => ("throttleFetchBalance", 0, false),
            Self::ResolveExplicitlyClosedPositionUpdates => {
                ("resolveExplicitlyClosedPositionUpdates", 1, false)
            }
            Self::FlattenByMarketPayload => ("flattenByMarketPayload", 1, false),
            Self::FlattenOrdersByMarket => ("flattenOrdersByMarket", 1, false),
            Self::ScheduleThrottledTask => ("scheduleThrottledTask", 4, false),
            Self::SendSubscribePayload => ("sendSubscribePayload", 2, false),
            Self::NoteAccountStreamActivity => ("noteAccountStreamActivity", 2, false),
            Self::IsRecentStreamEvent => ("isRecentStreamEvent", 1, false),
            Self::NormalizeOrderStatus => ("normalizeOrderStatus", 1, false),
            Self::IsUserCancelledReason => ("isUserCancelledReason", 1, false),
            Self::ConnectAndSubscribe => ("connectAndSubscribe", 0, true),
            Self::OnOpen => ("onOpen", 0, false),
            Self::OnClose => ("onClose", 1, false),
            Self::OnMessage => ("onMessage", 1, false),
            Self::Ping => ("ping", 0, false),
            Self::HandleAccountTopic => ("handleAccountTopic", 1, false),
            Self::HandleAccountAllTopic => ("handleAccountAllTopic", 1, false),
            Self::HandleOrderTopic => ("handleOrderTopic", 1, false),
            Self::HandlePositionTopic => ("handlePositionTopic", 1, false),
            Self::HandleFillsTopic => ("handleFillsTopic", 1, false),
            Self::AuthenticateAndSubscribe => ("authenticateAndSubscribe", 0, true),
            Self::Dispose => ("dispose", 0, false),
        };
        PrivateWebsocketMethodMetadata {
            name,
            length,
            asynchronous,
        }
    }
    /// Called by the realm with the arrow's traced lexical owner.
    pub fn invoke<H: 'static>(
        self,
        contexts: &dyn PrivateWebsocketContexts<H>,
        owner: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let socket = contexts.socket(owner.clone());
        let recovery: Rc<dyn PrivateRecoveryHost> = socket.clone();
        let boolean = |name: &str| recovery.boolean(name);
        let undefined = |result: SigningResult<()>| result.map(|()| AccountValue::Undefined);
        let string = |result: SigningResult<Vec<u16>>| result.map(AccountValue::String);
        Ok(match self {
            Self::ShouldEnforcePongMissReconnect => {
                AccountValue::Boolean(ws_private::should_enforce_pong_miss_reconnect())
            }
            Self::GetRateLimitWarningMessage => {
                string(ws_private_warnings::get_rate_limit_warning_message(
                    contexts.warnings(owner).as_ref(),
                ))?
            }
            Self::GetReadonlyFallbackWarningKey => {
                string(ws_private_warnings::get_readonly_fallback_warning_key(
                    contexts.warnings(owner).as_ref(),
                ))?
            }
            Self::EmitReadonlyFallbackWarning => {
                undefined(ws_private_warnings::emit_readonly_fallback_warning(
                    contexts.warnings(owner).as_ref(),
                    &contexts.readonly_warning_times(),
                    arg(args, 0).truthy(),
                ))?
            }
            Self::IsReadonlyFallbackActive => AccountValue::Boolean(
                ws_private::is_readonly_fallback_active(boolean("useReadonlyFallback")?),
            ),
            Self::ShouldUseRestOrderReconciliation => {
                let ack = boolean("hasReceivedAccountAllOrdersAck")?;
                let fallback = ack && boolean("useReadonlyFallback")?;
                AccountValue::Boolean(ws_private::should_use_rest_order_reconciliation(
                    ack, fallback,
                ))
            }
            Self::IsWsProxyRouteEnabled => AccountValue::Boolean(
                ws_private_warnings::is_ws_proxy_route_enabled(contexts.warnings(owner).as_ref())?,
            ),
            Self::ShouldUseGeorestrictedConnectivityWarning => AccountValue::Boolean(
                ws_private_warnings::should_use_georestricted_connectivity_warning(
                    contexts.warnings(owner.clone()).as_ref(),
                    contexts.warning_flags(owner).as_ref(),
                    arg(args, 0).truthy(),
                )?,
            ),
            Self::GetPrivateStreamErrorWarningMessage => string(
                ws_private_warnings::get_private_stream_error_warning_message(
                    contexts.warnings(owner.clone()).as_ref(),
                    contexts.warning_flags(owner).as_ref(),
                    contexts.warning_params(arg(args, 0)).as_ref(),
                ),
            )?,
            Self::GetPingIntervalMs => AccountValue::Number(ws_private::get_ping_interval_ms(
                boolean("useReadonlyFallback")?,
            )),
            Self::GetIdlePingThresholdMs => AccountValue::Number(
                ws_private::get_idle_ping_threshold_ms(boolean("useReadonlyFallback")?),
            ),
            Self::GetIdleReconnectThresholdMs => AccountValue::Number(
                ws_private::get_idle_reconnect_threshold_ms(boolean("useReadonlyFallback")?),
            ),
            Self::GetWatchdogStaleThresholdMs => AccountValue::Number(
                ws_private::get_watchdog_stale_threshold_ms(boolean("useReadonlyFallback")?),
            ),
            Self::IsWsRateLimited => {
                let now = recovery.now()?;
                AccountValue::Boolean(ws_private::is_ws_rate_limited(
                    now,
                    recovery.number("wsRateLimitedUntil")?,
                ))
            }
            Self::NotePrivateStreamRateLimitError => {
                let now = number(socket.as_ref(), arg(args, 0))?;
                let mut state = PrivateWarningState {
                    rate_limit_error_count: recovery.number("privateStreamRateLimitErrorCount")?,
                    ..PrivateWarningState::default()
                };
                let first = state.rate_limit_error_count == 0.0;
                state.note_private_stream_rate_limit_error(now);
                if first {
                    recovery.set_number(
                        "privateStreamRateLimitFirstSeenAt",
                        state.rate_limit_first_seen_at,
                    )?;
                }
                recovery.set_number(
                    "privateStreamRateLimitErrorCount",
                    state.rate_limit_error_count,
                )?;
                AccountValue::Undefined
            }
            Self::ClearPrivateStreamRateLimitErrorState => {
                let mut state = PrivateWarningState::default();
                state.clear_private_stream_rate_limit_error_state();
                recovery.set_number(
                    "privateStreamRateLimitFirstSeenAt",
                    state.rate_limit_first_seen_at,
                )?;
                recovery.set_number(
                    "privateStreamRateLimitErrorCount",
                    state.rate_limit_error_count,
                )?;
                AccountValue::Undefined
            }
            Self::ShouldForceToastPrivateStreamRateLimit => {
                let now = number(socket.as_ref(), arg(args, 0))?;
                let count = recovery.number("privateStreamRateLimitErrorCount")?;
                let first_seen = if count > 1.0 {
                    recovery.number("privateStreamRateLimitFirstSeenAt")?
                } else {
                    0.0
                };
                AccountValue::Boolean(ws_private::should_force_toast_private_stream_rate_limit(
                    now, count, first_seen,
                ))
            }
            Self::CheckLastReceived => undefined(check_last_received(socket.as_ref()))?,
            Self::BeginDiagnosticsLoop => undefined(begin_diagnostics_loop(socket))?,
            Self::StopDiagnosticsLoop => undefined(ws_private_recovery::stop_diagnostics_loop(
                recovery.as_ref(),
            ))?,
            Self::ClearStreamRecoveryReconnectTimeout => undefined(
                ws_private_recovery::clear_stream_recovery_reconnect_timeout(recovery.as_ref()),
            )?,
            Self::ClearRateLimitResubscribeTimeout => undefined(
                ws_private_recovery::clear_rate_limit_resubscribe_timeout(recovery.as_ref()),
            )?,
            Self::ScheduleRateLimitResubscribe => undefined(
                ws_private_recovery::schedule_rate_limit_resubscribe(recovery),
            )?,
            Self::ScheduleStreamRecoveryReconnectFallback => undefined(
                ws_private_recovery::schedule_stream_recovery_reconnect_fallback(recovery),
            )?,
            Self::MaybeRecoverPrivateStream => undefined(
                ws_private_recovery::maybe_recover_private_stream(recovery.as_ref()),
            )?,
            Self::ThrottleFetchOrders => ws_private_feed_helpers::throttle_fetch_orders(
                &contexts.feed(),
                &owner,
                arg(args, 0),
                arg(args, 1),
            )?,
            Self::ThrottleFetchPositions => ws_private_feed_helpers::throttle_fetch_positions(
                &contexts.feed(),
                &owner,
                arg(args, 0),
            )?,
            Self::ThrottleFetchBalance => ws_private_feed_helpers::throttle_fetch_balance(
                &contexts.feed(),
                &owner,
                arg(args, 0),
            )?,
            Self::ResolveExplicitlyClosedPositionUpdates => {
                ws_private_feed_helpers::resolve_explicitly_closed_position_updates(
                    &contexts.feed(),
                    &owner,
                    &arg(args, 0),
                )?
            }
            Self::FlattenByMarketPayload => {
                contexts.flatten_by_market_payload(&arg(args, 0), arg(args, 1).truthy())?
            }
            Self::FlattenOrdersByMarket => {
                let payload = arg(args, 0);
                if !matches!(payload, AccountValue::Object(_)) {
                    contexts.empty_array()
                } else {
                    invoke(
                        socket.as_ref(),
                        "flattenByMarketPayload",
                        &[payload, AccountValue::Boolean(true)],
                    )?
                }
            }
            Self::ScheduleThrottledTask => {
                let key = match string_units(&arg(args, 0)) {
                    Some(key) if equals(&key, "ordersThrottleTimeout") => "ordersThrottleTimeout",
                    Some(key) if equals(&key, "positionsThrottleTimeout") => {
                        "positionsThrottleTimeout"
                    }
                    Some(key) if equals(&key, "balanceThrottleTimeout") => "balanceThrottleTimeout",
                    _ => {
                        return Err(contexts
                            .type_error("scheduleThrottledTask key is not a ThrottleTimeoutKey"));
                    }
                };
                let feed = contexts.feed();
                let task_feed = feed.clone();
                let task = arg(args, 2);
                ws_private_feed_helpers::schedule_throttled_task(
                    &feed,
                    &owner,
                    key,
                    arg(args, 1),
                    Box::new(move || (task_feed.call_async)(&task_feed.host, task, vec![])),
                    arg(args, 3),
                )?
            }
            Self::SendSubscribePayload => ws_private_feed_helpers::send_subscribe_payload(
                &contexts.feed(),
                &owner,
                arg(args, 0),
                arg(args, 1).truthy(),
            )?,
            Self::NoteAccountStreamActivity => {
                let topic = socket.to_string(arg(args, 0))?;
                let channel = socket.to_string(arg(args, 1))?;
                undefined(ws_private_recovery::note_account_stream_activity(
                    recovery.as_ref(),
                    &topic,
                    &channel,
                ))?
            }
            Self::IsRecentStreamEvent => {
                let AccountValue::Number(timestamp) = arg(args, 0) else {
                    return Ok(AccountValue::Boolean(false));
                };
                if !timestamp.is_finite() || timestamp <= 0.0 {
                    AccountValue::Boolean(false)
                } else {
                    let wall_now = socket.date_now()?;
                    let virtual_now = recovery.now()?;
                    AccountValue::Boolean(ws_private::is_recent_stream_event(
                        timestamp,
                        wall_now,
                        virtual_now,
                        recovery.number("lastConnectTime")?,
                    ))
                }
            }
            Self::NormalizeOrderStatus => {
                ws_private_feed_helpers::normalize_order_status(&contexts.feed(), &arg(args, 0))?
            }
            Self::IsUserCancelledReason => AccountValue::Boolean(
                ws_private_feed_helpers::is_user_cancelled_reason(&contexts.feed(), arg(args, 0))?,
            ),
            Self::ConnectAndSubscribe => contexts.promise_value(connect_and_subscribe(socket)),
            Self::OnOpen => ws_private_stream_handlers::on_open(&contexts.stream(), &owner)?,
            Self::OnClose => undefined(on_close(socket, arg(args, 0)))?,
            Self::OnMessage => undefined(on_message(socket.as_ref(), arg(args, 0)))?,
            Self::Ping => ws_private_stream_handlers::ping(&contexts.stream(), &owner)?,
            Self::HandleAccountTopic => ws_private_stream_handlers::handle_account_topic(
                &contexts.stream(),
                &owner,
                &arg(args, 0),
            )?,
            Self::HandleAccountAllTopic => ws_private_stream_handlers::handle_account_all_topic(
                &contexts.stream(),
                &owner,
                &arg(args, 0),
            )?,
            Self::HandleOrderTopic => ws_private_stream_handlers::handle_order_topic(
                &contexts.stream(),
                &owner,
                arg(args, 0),
            )?,
            Self::HandlePositionTopic => ws_private_stream_handlers::handle_position_topic(
                &contexts.stream(),
                &owner,
                arg(args, 0),
            )?,
            Self::HandleFillsTopic => ws_private_stream_handlers::handle_fills_topic(
                &contexts.stream(),
                &owner,
                arg(args, 0),
            )?,
            Self::AuthenticateAndSubscribe => contexts.promise_value(
                ws_private_stream_handlers::authenticate_and_subscribe(&contexts.stream(), &owner),
            ),
            Self::Dispose => undefined(ws_private_recovery::dispose(recovery.as_ref()))?,
        })
    }
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}

pub type PrivateWebsocketConstructor<H> = fn(
    &dyn PrivateWebsocketRealm<H>,
    AccountValue,
    AccountValue,
    &[AccountValue],
) -> SigningResult<AccountValue>;
/// Language operations for the derived class over the source BaseWebSocket.
pub trait PrivateWebsocketRealm<H>: PrivateWebsocketContexts<H> {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue>;
    /// Establish the derived constructor, prototype chain and inner class binding.
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: PrivateWebsocketConstructor<H>,
    ) -> SigningResult<AccountValue>;
    /// Arrow field closing over the lexical owner; async arrows return the
    /// body's promise directly.
    fn arrow(
        &self,
        owner: &AccountValue,
        body: PrivateWebsocketMethod,
    ) -> SigningResult<AccountValue>;
    /// DefineField: writable/enumerable/configurable data property.
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    /// JS view of the same map `readonly_warning_times` hands the warning body.
    fn readonly_warning_map(&self) -> SigningResult<AccountValue>;
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()>;
    /// Source BaseWebSocket constructor via super(...args), with its receiver.
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
}

const STATIC_NUMBERS_BEFORE_MAP: [(&str, f64); 1] =
    [("READONLY_FALLBACK_WARNING_COOLDOWN_MS", 30_000.0)];
const STATIC_NUMBERS_BEFORE_ALIAS: [(&str, f64); 8] = [
    ("STANDARD_PING_INTERVAL_MS", 10_000.0),
    ("DEGRADED_PING_INTERVAL_MS", 20_000.0),
    ("STANDARD_IDLE_PING_THRESHOLD_MS", 20_000.0),
    ("DEGRADED_IDLE_PING_THRESHOLD_MS", 45_000.0),
    ("STANDARD_IDLE_RECONNECT_THRESHOLD_MS", 180_000.0),
    ("DEGRADED_IDLE_RECONNECT_THRESHOLD_MS", 180_000.0),
    ("RATE_LIMIT_RECONNECT_DELAY_MS", 30_000.0),
    ("RATE_LIMIT_COOLDOWN_MS", 60_000.0),
];
const STATIC_NUMBERS_AFTER_ALIAS: [(&str, f64); 9] = [
    ("RATE_LIMIT_RESUBSCRIBE_BUFFER_MS", 2_000.0),
    ("RATE_LIMIT_PING_INTERVAL_MS", 20_000.0),
    ("RATE_LIMIT_PING_SKIP_LOG_COOLDOWN_MS", 15_000.0),
    ("PRIVATE_STREAM_WARNING_AFTER_MS", 90_000.0),
    ("PRIVATE_STREAM_WARNING_COOLDOWN_MS", 60_000.0),
    ("NO_STREAM_RECOVERY_AFTER_MS", 180_000.0),
    ("STALE_STREAM_RECOVERY_AFTER_MS", 120_000.0),
    ("RECOVERY_ATTEMPT_COOLDOWN_MS", 180_000.0),
    ("RECOVERY_RECONNECT_GRACE_MS", 20_000.0),
];
const EARLY_CLOSE_STATIC: (&str, f64) = ("EARLY_CLOSE_WINDOW_MS", 10_000.0);

enum Field {
    Data(&'static str, AccountValue),
    Arrow(PrivateWebsocketMethod),
}
fn method_at(index: usize) -> Field {
    Field::Arrow(PrivateWebsocketMethod::ALL[index])
}
fn instance_fields() -> Vec<Field> {
    let number = |name, value| Field::Data(name, AccountValue::Number(value));
    let boolean = |name, value| Field::Data(name, AccountValue::Boolean(value));
    let undefined = |name| Field::Data(name, AccountValue::Undefined);
    let mut fields = vec![
        boolean("recoversAfterOffline", false),
        boolean("treatAnyMessageAsPong", true),
        method_at(0),
        number("lastConnectTime", 0.0),
        undefined("ordersThrottleTimeout"),
        undefined("positionsThrottleTimeout"),
        undefined("balanceThrottleTimeout"),
        undefined("diagnosticsInterval"),
        boolean("openedSinceLastConnect", false),
        boolean("useReadonlyFallback", false),
        number("failedHandshakes", 0.0),
        boolean("hasWarnedReadonlyFallback", false),
        boolean("hasWarnedNoAccountStream", false),
        boolean("hasWarnedNoAccountStreamToast", false),
        boolean("hasWarnedProxyStreamErrorToast", false),
        boolean("hasWarnedOrdersChannelUnsupported", false),
        number("lastStaleAccountStreamWarningAt", 0.0),
        number("accountStreamMessageCount", 0.0),
        number("lastAccountStreamMessageAt", 0.0),
        boolean("accountAllOrdersChannelEnabled", true),
        boolean("hasPendingAccountAllOrdersSubscribe", false),
        boolean("hasReceivedAccountAllOrdersAck", false),
        number("wsRateLimitedUntil", 0.0),
        number("lastStreamRecoveryAttemptAt", 0.0),
        number("readonlyFallbackFailures", 0.0),
        undefined("streamRecoveryReconnectTimeout"),
        undefined("rateLimitResubscribeTimeout"),
        number("privateStreamRateLimitFirstSeenAt", 0.0),
        number("privateStreamRateLimitErrorCount", 0.0),
        number("lastRateLimitedPingAt", 0.0),
        number("lastRateLimitedPingSkipLogAt", 0.0),
        number("lastIdlePingAt", 0.0),
        Field::Data(
            "readonlyFallbackWarningMessage",
            text(GEORESTRICTED_MESSAGE),
        ),
    ];
    fields.extend((1..PrivateWebsocketMethod::ALL.len()).map(method_at));
    fields
}

// ts: LighterPrivateWebsocket
pub struct LighterPrivateWebsocket;
impl LighterPrivateWebsocket {
    pub fn define<H: 'static>(realm: &dyn PrivateWebsocketRealm<H>) -> SigningResult<AccountValue> {
        let base = realm.binding_value("BaseWebSocket")?;
        let class =
            realm.create_derived_class(base, "LighterPrivateWebsocket", 0, Self::construct::<H>)?;
        let number =
            |key: &str, value: f64| realm.define_data(&class, key, AccountValue::Number(value));
        for (key, value) in STATIC_NUMBERS_BEFORE_MAP {
            number(key, value)?;
        }
        realm.define_data(
            &class,
            "readonlyFallbackWarningAtByAccount",
            realm.readonly_warning_map()?,
        )?;
        for (key, value) in STATIC_NUMBERS_BEFORE_ALIAS {
            number(key, value)?;
        }
        let cooldown = realm.get(&class, "RATE_LIMIT_COOLDOWN_MS")?;
        realm.define_data(&class, "RATE_LIMIT_FORCE_TOAST_AFTER_MS", cooldown)?;
        for (key, value) in STATIC_NUMBERS_AFTER_ALIAS {
            number(key, value)?;
        }
        number(EARLY_CLOSE_STATIC.0, EARLY_CLOSE_STATIC.1)?;
        realm.finish_class_binding(&class)?;
        Ok(class)
    }
    /// Implicit derived constructor: `super(...args)` then field initializers.
    pub fn construct<H: 'static>(
        realm: &dyn PrivateWebsocketRealm<H>,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let owner = realm.construct_base(class, new_target, args)?;
        Self::initialize_fields(realm, &owner)?;
        Ok(owner)
    }
    pub fn initialize_fields<H: 'static>(
        realm: &dyn PrivateWebsocketRealm<H>,
        owner: &AccountValue,
    ) -> SigningResult<()> {
        for field in instance_fields() {
            match field {
                Field::Data(key, value) => realm.define_data(owner, key, value)?,
                Field::Arrow(method) => {
                    let value = realm.arrow(owner, method)?;
                    realm.define_data(owner, method.metadata().name, value)?;
                }
            }
        }
        Ok(())
    }
}

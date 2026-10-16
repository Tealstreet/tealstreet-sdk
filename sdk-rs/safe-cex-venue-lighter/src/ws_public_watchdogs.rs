//! Public per-topic stream watchdog/recovery/disposal bodies. Full websocket,
//! imported helper and JavaScript record/iterator/object/timer operations are
//! required live hosts. Typed topics/options and ordinary readonly constants;
//! Object.keys/entries retain ordinary string-key/entry-pair result domains;
//! reflective String/Array/Object intrinsic replacements yielding other types,
//! full owner/runtime construction and resource-allocation parity are unproved.
use crate::{
    sdk_account_api::{AccountValue, type_error},
    transaction_signing::SigningResult,
};
use std::rc::Rc;
const BOOK_DELAY: f64 = 90_000.0;
const CANDLE_DELAY: f64 = 120_000.0;
const CANDLE_COOLDOWN: f64 = 120_000.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchdogRecord {
    Topics,
    Handlers,
    BookStale,
    CandleNoData,
    BookAttempts,
    BookRecovery,
}
pub trait WatchdogRecordAccess {
    /// Actual [[Get]], strict assignment and delete, preserving UTF16 keys,
    /// prototype behavior, proxy/getter/setter interception and original throws.
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue>;
    fn set(&self, key: &[u16], value: AccountValue) -> SigningResult<()>;
    fn delete(&self, key: &[u16]) -> SigningResult<()>;
}
pub trait WatchdogCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue>;
}
pub type WatchdogCallback = Rc<dyn Fn() -> SigningResult<()>>;
pub trait WatchdogTimerCall {
    fn call(&self, callback: WatchdogCallback, delay: f64) -> SigningResult<AccountValue>;
}
/// Captured Object.keys/values with actual Object receiver. Call occurs after
/// owner argument evaluation, then actual GetIterator/for-of with live yields,
/// next failures and IteratorClose on body errors, preserving throw precedence.
pub trait WatchdogValuesCall {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
}
/// Captured Object.entries with actual receiver, enumeration own enumerable
/// string keys/value Gets in JS order, then full outer iteration AND each entry's
/// source two-element destructuring/IteratorClose before invoking the visitor.
pub trait WatchdogEntriesCall {
    fn for_of(
        &self,
        record: Rc<dyn WatchdogRecordAccess>,
        visit: &mut dyn FnMut(Vec<u16>, AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
}
pub trait PublicWatchdogHost {
    fn record(&self, kind: WatchdogRecord) -> SigningResult<Rc<dyn WatchdogRecordAccess>>;
    /// Assign a fresh source {} with intrinsic ordinary prototype; keep old aliases.
    fn replace_empty(&self, kind: WatchdogRecord) -> SigningResult<()>;
    fn flag(&self, name: &str) -> SigningResult<bool>;
    fn set_flag(&self, name: &str, value: bool) -> SigningResult<()>;
    fn number(&self, name: &str) -> SigningResult<f64>;
    fn set_number(&self, name: &str, value: f64) -> SigningResult<()>;
    /// Capture actual owner/parent/import/global callee AND receiver before args;
    /// defer noncallable Call failures until args have been evaluated. Imported
    /// helpers use their original bindings and full UTF16/native values, not an
    /// existing UTF8-only helper substitute.
    fn capture(&self, name: &str) -> SigningResult<Rc<dyn WatchdogCall>>;
    fn capture_timeout(&self) -> SigningResult<Rc<dyn WatchdogTimerCall>>;
    fn capture_values(&self, name: &str) -> SigningResult<Rc<dyn WatchdogValuesCall>>;
    fn capture_entries(&self) -> SigningResult<Rc<dyn WatchdogEntriesCall>>;
    /// Ordinary Array.isArray, then live value[0] only for arrays (including holes).
    /// None means non-array only; empty/sparse arrays yield Some(Undefined).
    fn array_first(&self, value: &AccountValue) -> SigningResult<Option<AccountValue>>;
    /// Full source property Get on native primitives/objects (including nullish
    /// TypeError), without JSON approximation. Callers apply optional chaining.
    fn property(&self, value: &AccountValue, name: &str) -> SigningResult<AccountValue>;
    fn now(&self) -> SigningResult<f64>;
    /// Fresh source ordinary object literal with ordered own data properties.
    /// Prototype/descriptor/heap-allocation resource behavior is required host
    /// policy, not independently proved by the native fixture.
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue>;
    /// Actual parent.store.marketsBySymbol lookup before opts.symbol evaluation.
    fn markets_by_symbol(&self) -> SigningResult<Rc<dyn WatchdogRecordAccess>>;
}
pub trait WatchdogOptions {
    fn symbol(&self) -> SigningResult<Vec<u16>>;
    fn interval(&self) -> SigningResult<AccountValue>;
}
fn text(value: &[u16]) -> AccountValue {
    AccountValue::String(value.to_vec())
}
fn call(host: &dyn PublicWatchdogHost, name: &str, args: &[AccountValue]) -> SigningResult<()> {
    host.capture(name)?.call(args)?;
    Ok(())
}
fn handlers_length(host: &dyn PublicWatchdogHost, topic: &[u16]) -> SigningResult<AccountValue> {
    let handlers = host.record(WatchdogRecord::Handlers)?.get(topic)?;
    if handlers.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        host.property(&handlers, "length")
    }
}
struct ArmPlan {
    kind: WatchdogRecord,
    clear: &'static str,
    recover: &'static str,
    delay: f64,
    reason: &'static str,
}
const BOOK_STALE_ARM: ArmPlan = ArmPlan {
    kind: WatchdogRecord::BookStale,
    clear: "clearOrderBookStaleTimeout",
    recover: "attemptOrderBookRecovery",
    delay: BOOK_DELAY,
    reason: "no order book messages after 90000ms",
};
const CANDLE_NO_DATA_ARM: ArmPlan = ArmPlan {
    kind: WatchdogRecord::CandleNoData,
    clear: "clearOhlcvNoDataTimeout",
    recover: "attemptOhlcvRecovery",
    delay: CANDLE_DELAY,
    reason: "no OHLCV messages after 120000ms",
};
fn arm(
    host: Rc<dyn PublicWatchdogHost>,
    plan: &'static ArmPlan,
    topic: &[u16],
    channel: Option<AccountValue>,
) -> SigningResult<()> {
    let ArmPlan {
        kind,
        clear,
        recover,
        delay,
        reason,
    } = *plan;
    call(host.as_ref(), clear, &[text(topic)])?;
    // Source assignment LHS owner record is captured before setTimeout lookup.
    let lhs = host.record(kind)?;
    let timer = host.capture_timeout()?;
    let owner = host.clone();
    let key = topic.to_vec();
    let callback: WatchdogCallback = Rc::new(move || {
        owner.record(kind)?.delete(&key)?;
        if owner.flag("isDisposed")? || owner.flag("reconnecting")? {
            return Ok(());
        }
        let mut args = vec![text(&key)];
        if let Some(channel) = &channel {
            args.push(channel.clone());
        }
        args.push(AccountValue::String(reason.encode_utf16().collect()));
        call(owner.as_ref(), recover, &args)
    });
    let id = timer.call(callback, delay)?;
    lhs.set(topic, id)
}
// ts: LighterPublicWebsocket#armOrderBookStaleTimeout
pub fn arm_order_book_stale_timeout(
    host: Rc<dyn PublicWatchdogHost>,
    topic: &[u16],
    channel: AccountValue,
) -> SigningResult<()> {
    arm(host, &BOOK_STALE_ARM, topic, Some(channel))
}
// ts: LighterPublicWebsocket#isOrderBookTopic
pub fn is_order_book_topic(topic: &[u16]) -> bool {
    topic.starts_with(&"order_book:".encode_utf16().collect::<Vec<_>>())
        || topic.starts_with(&"order_book/".encode_utf16().collect::<Vec<_>>())
}
// ts: LighterPublicWebsocket#armExistingOrderBookTimeouts
pub fn arm_existing_order_book_timeouts(host: &dyn PublicWatchdogHost) -> SigningResult<()> {
    let entries = host.capture_entries()?;
    let topics = host.record(WatchdogRecord::Topics)?;
    entries.for_of(topics, &mut |topic, value| {
        if !host
            .capture("isOrderBookTopic")?
            .call(&[text(&topic)])?
            .truthy()
        {
            return Ok(());
        }
        let channel = host.array_first(&value)?.unwrap_or(value);
        if !channel.truthy() {
            return Ok(());
        }
        call(host, "armOrderBookStaleTimeout", &[text(&topic), channel])
    })
}
// ts: LighterPublicWebsocket#armOhlcvNoDataTimeout
pub fn arm_ohlcv_no_data_timeout(
    host: Rc<dyn PublicWatchdogHost>,
    topic: &[u16],
) -> SigningResult<()> {
    arm(host, &CANDLE_NO_DATA_ARM, topic, None)
}
// ts: LighterPublicWebsocket#armExistingOhlcvNoDataTimeouts
pub fn arm_existing_ohlcv_no_data_timeouts(host: &dyn PublicWatchdogHost) -> SigningResult<()> {
    let keys = host.capture_values("Object.keys")?;
    let topics = host.record(WatchdogRecord::Topics)?;
    keys.for_of(topics, &mut |topic| {
        // `topic.startsWith(...)`: only a String key has the method here.
        let AccountValue::String(topic) = topic else {
            return Err(type_error("topic.startsWith is not a function"));
        };
        if !topic.starts_with(&"candle/".encode_utf16().collect::<Vec<_>>())
            || !handlers_length(host, &topic)?.truthy()
        {
            return Ok(());
        }
        call(host, "armOhlcvNoDataTimeout", &[text(&topic)])
    })
}
fn recovery_log(
    host: &dyn PublicWatchdogHost,
    topic: &[u16],
    reason: &[u16],
    promote: bool,
) -> SigningResult<()> {
    let log = host.capture("parent.log")?;
    let prefix = if promote {
        "[Lighter] Public WS OHLCV recovery promoting route to proxy for "
    } else {
        "[Lighter] Public WS OHLCV recovery probing via reconnect for "
    };
    let mut message: Vec<u16> = prefix.encode_utf16().collect();
    message.extend_from_slice(topic);
    message.extend(" (".encode_utf16());
    message.extend_from_slice(reason);
    message.extend(").".encode_utf16());
    log.call(&[
        text(&message),
        AccountValue::String("warning".encode_utf16().collect()),
    ])?;
    Ok(())
}
// ts: LighterPublicWebsocket#attemptOhlcvRecovery
pub fn attempt_ohlcv_recovery(
    host: &dyn PublicWatchdogHost,
    topic: &[u16],
    reason: &[u16],
) -> SigningResult<()> {
    let now = host.now()?;
    let attempt = host.capture("shouldAttemptLighterOhlcvRecovery")?;
    let boolean = host.capture("Boolean")?;
    let length = handlers_length(host, topic)?;
    let has_handlers = boolean.call(&[length])?;
    let last = host.number("lastOhlcvRecoveryReconnectAt")?;
    let params = host.literal(&[
        ("hasTopicHandlers", has_handlers),
        ("nowMs", AccountValue::Number(now)),
        ("lastRecoveryReconnectAtMs", AccountValue::Number(last)),
        ("recoveryCooldownMs", AccountValue::Number(CANDLE_COOLDOWN)),
    ])?;
    if !attempt.call(&[params])?.truthy() {
        return Ok(());
    }
    let promote = host.capture("shouldPromoteLighterOhlcvRecoveryToProxy")?;
    let enabled = host.capture("isWsProxyRouteEnabled")?.call(&[])?;
    let direct = host.flag("useDirectFallbackRoute")?;
    let connected = host.flag("connectedViaProxyRoute")?;
    let params = host.literal(&[
        ("isWsProxyRouteEnabled", enabled),
        ("useProxyRoute", AccountValue::Boolean(!direct)),
        ("connectedViaProxyRoute", AccountValue::Boolean(connected)),
    ])?;
    let promote = promote.call(&[params])?.truthy();
    if promote {
        host.set_flag("useProxyRoute", true)?;
        host.set_flag("useDirectFallbackRoute", false)?;
    }
    recovery_log(host, topic, reason, promote)?;
    host.set_number("lastOhlcvRecoveryReconnectAt", now)?;
    call(host, "reconnect", &[])
}
// ts: LighterPublicWebsocket.maybeRecoverOhlcvStream
pub fn maybe_recover_ohlcv_stream(
    host: &dyn PublicWatchdogHost,
    opts: &dyn WatchdogOptions,
    reason: Option<&[u16]>,
) -> SigningResult<()> {
    let reason = reason
        .map(<[u16]>::to_vec)
        .unwrap_or_else(|| "stale OHLCV stream".encode_utf16().collect());
    let markets = host.markets_by_symbol()?;
    let symbol = opts.symbol()?;
    let market = markets.get(&symbol)?;
    if !market.truthy() {
        return Ok(());
    }
    let build = host.capture("buildLighterCandleTopic")?;
    let id = host.property(&market, "id")?;
    let resolve = host.capture("resolveLighterInterval")?;
    let interval = opts.interval()?;
    let resolution = resolve.call(&[interval])?;
    let topic = build.call(&[id, resolution])?;
    call(host, "attemptOhlcvRecovery", &[topic, text(&reason)])
}
// ts: LighterPublicWebsocket.dispose
pub fn dispose(host: &dyn PublicWatchdogHost) -> SigningResult<()> {
    for name in [
        "clearTickerRestFallbackWatchdog",
        "stopTickerRestFallback",
        "clearPendingSubscribeTimeouts",
    ] {
        call(host, name, &[])?;
    }
    let values = host.capture_values("Object.values")?;
    let record = host.record(WatchdogRecord::CandleNoData)?;
    values.for_of(record, &mut |id| {
        if id.truthy() {
            call(host, "clearTimeout", &[id])
        } else {
            Ok(())
        }
    })?;
    host.replace_empty(WatchdogRecord::CandleNoData)?;
    for name in [
        "clearAllOrderBookInitialDataTimeouts",
        "clearAllOrderBookStaleTimeouts",
        "stopAllOrderBookRestFallback",
    ] {
        call(host, name, &[])?;
    }
    host.replace_empty(WatchdogRecord::BookAttempts)?;
    host.replace_empty(WatchdogRecord::BookRecovery)?;
    call(host, "super.dispose", &[])
}

//! Public-stream cleanup bodies at explicit ECMAScript record/array/timer hosts.
//! The websocket owner and timer scheduling remain separate pending work.
use crate::{sdk_account_api::AccountValue, transaction_signing::SigningResult};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CleanupRecord {
    OhlcvNoData,
    OrderBookStale,
    OrderBookInitialData,
    RestIntervals,
    RestInFlight,
    RestCooldown,
}
/// Source [[Get]], strict delete, and Object.values. Values includes own enumerable
/// string properties in ECMAScript order, with the original Get/throw/identity
/// behavior; inherited properties participate in get but not values.
pub trait CleanupRecordAccess {
    fn get(&self, key: &[u16]) -> SigningResult<AccountValue>;
    fn delete(&self, key: &[u16]) -> SigningResult<()>;
    fn values(&self) -> SigningResult<Vec<AccountValue>>;
}
/// Source array.length and receiver-bound .forEach(callback). The host must
/// preserve the actual method lookup/call, snapshot length, skip missing indices,
/// and read present values live before invoking each callback synchronously.
pub trait CleanupArrayAccess {
    fn length(&self) -> SigningResult<AccountValue>;
    fn for_each(
        &self,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
}
/// Capture the actual Object.values callee and Object receiver before the owner
/// argument is read. This is a deferred call attempt, including a noncallable
/// callee: argument evaluation still precedes its Call TypeError.
pub trait CleanupValuesCall {
    /// Call the captured callee/receiver with record, then perform ECMAScript
    /// GetIterator/for-of, including live yields, next failures and IteratorClose
    /// on visit errors with source throw precedence. Never replace this with an
    /// immutable Vec loop when the host exposes a custom iterator.
    fn for_of(
        &self,
        record: Rc<dyn CleanupRecordAccess>,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
}
pub trait PublicCleanupHost {
    fn object_values_callback(&self) -> SigningResult<Rc<dyn CleanupValuesCall>>;
    /// Get the current owner field every time; a timer callback can replace it.
    fn record(&self, kind: CleanupRecord) -> SigningResult<Rc<dyn CleanupRecordAccess>>;
    /// Assign a fresh source {} with its ordinary Object.prototype, preserving
    /// old record aliases and original setter failures.
    fn replace_empty_record(&self, kind: CleanupRecord) -> SigningResult<()>;
    fn pending_subscriptions(&self) -> SigningResult<Rc<dyn CleanupArrayAccess>>;
    /// Assign a fresh source [], preserving old array aliases/setter failures.
    fn replace_empty_pending_subscriptions(&self) -> SigningResult<()>;
    /// Source global timer operations, preserving argument identity and failures.
    fn clear_timeout(&self, value: &AccountValue) -> SigningResult<()>;
    fn clear_interval(&self, value: &AccountValue) -> SigningResult<()>;
}
fn clear_topic_timeout(
    host: &dyn PublicCleanupHost,
    kind: CleanupRecord,
    topic: &[u16],
) -> SigningResult<()> {
    let timeout_id = host.record(kind)?.get(topic)?;
    if !timeout_id.truthy() {
        return Ok(());
    }
    host.clear_timeout(&timeout_id)?;
    // Deliberately reacquire the current owner record after the host call.
    host.record(kind)?.delete(topic)
}
// ts: LighterPublicWebsocket#clearOhlcvNoDataTimeout
pub fn clear_ohlcv_no_data_timeout(
    host: &dyn PublicCleanupHost,
    topic: &[u16],
) -> SigningResult<()> {
    clear_topic_timeout(host, CleanupRecord::OhlcvNoData, topic)
}
// ts: LighterPublicWebsocket#clearOrderBookStaleTimeout
pub fn clear_order_book_stale_timeout(
    host: &dyn PublicCleanupHost,
    topic: &[u16],
) -> SigningResult<()> {
    clear_topic_timeout(host, CleanupRecord::OrderBookStale, topic)
}
// ts: LighterPublicWebsocket#clearOrderBookInitialDataTimeout
pub fn clear_order_book_initial_data_timeout(
    host: &dyn PublicCleanupHost,
    topic: &[u16],
) -> SigningResult<()> {
    clear_topic_timeout(host, CleanupRecord::OrderBookInitialData, topic)
}
fn clear_all_timeouts(host: &dyn PublicCleanupHost, kind: CleanupRecord) -> SigningResult<()> {
    let values = host.object_values_callback()?;
    let record = host.record(kind)?;
    values.for_of(record, &mut |timeout_id| {
        if timeout_id.truthy() {
            host.clear_timeout(&timeout_id)
        } else {
            Ok(())
        }
    })?;
    host.replace_empty_record(kind)
}
// ts: LighterPublicWebsocket#clearAllOrderBookInitialDataTimeouts
pub fn clear_all_order_book_initial_data_timeouts(
    host: &dyn PublicCleanupHost,
) -> SigningResult<()> {
    clear_all_timeouts(host, CleanupRecord::OrderBookInitialData)
}
// ts: LighterPublicWebsocket#clearAllOrderBookStaleTimeouts
pub fn clear_all_order_book_stale_timeouts(host: &dyn PublicCleanupHost) -> SigningResult<()> {
    clear_all_timeouts(host, CleanupRecord::OrderBookStale)
}
// ts: LighterPublicWebsocket#stopOrderBookRestFallback
pub fn stop_order_book_rest_fallback(
    host: &dyn PublicCleanupHost,
    topic: &[u16],
) -> SigningResult<()> {
    let interval_id = host.record(CleanupRecord::RestIntervals)?.get(topic)?;
    if interval_id.truthy() {
        host.clear_interval(&interval_id)?;
        host.record(CleanupRecord::RestIntervals)?.delete(topic)?;
    }
    host.record(CleanupRecord::RestInFlight)?.delete(topic)
}
// ts: LighterPublicWebsocket#stopAllOrderBookRestFallback
pub fn stop_all_order_book_rest_fallback(host: &dyn PublicCleanupHost) -> SigningResult<()> {
    let values = host.object_values_callback()?;
    let record = host.record(CleanupRecord::RestIntervals)?;
    values.for_of(record, &mut |interval_id| {
        if interval_id.truthy() {
            host.clear_interval(&interval_id)
        } else {
            Ok(())
        }
    })?;
    host.replace_empty_record(CleanupRecord::RestIntervals)?;
    host.replace_empty_record(CleanupRecord::RestInFlight)?;
    host.replace_empty_record(CleanupRecord::RestCooldown)
}
// ts: LighterPublicWebsocket#clearPendingSubscribeTimeouts
pub fn clear_pending_subscribe_timeouts(host: &dyn PublicCleanupHost) -> SigningResult<()> {
    if !host.pending_subscriptions()?.length()?.truthy() {
        return Ok(());
    }
    host.pending_subscriptions()?
        .for_each(&mut |timeout_id| host.clear_timeout(&timeout_id))?;
    host.replace_empty_pending_subscriptions()
}

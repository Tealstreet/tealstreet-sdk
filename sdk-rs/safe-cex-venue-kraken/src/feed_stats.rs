//! Idle-feed bodies consume a lazy source for-in enumeration of handler-array lengths.
use safe_cex_exchange::runtime::ExchangeError;

pub fn futures_clear_feed_stats_if_idle<F: ?Sized>(
    feed: &F,
    lengths: impl IntoIterator<Item = Result<Option<usize>, ExchangeError>>,
    cleanup: impl FnOnce(&F) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    for length in lengths {
        if length?.is_some_and(|length| length > 0) {
            return Ok(());
        }
    }
    cleanup(feed)
}

pub fn spot_clear_feed_stats_if_idle<F: ?Sized>(
    feed: &F,
    lengths: impl IntoIterator<Item = Result<Option<usize>, ExchangeError>>,
    cleanup: impl FnOnce(&F) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    for length in lengths {
        if length?.is_some_and(|length| length > 0) {
            return Ok(());
        }
    }
    cleanup(feed)
}

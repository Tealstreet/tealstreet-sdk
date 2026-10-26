//! Startup read retry policy, public discovery and private gates alike. Venues
//! rebuild requests to refresh signatures.
use crate::log::code;
use crate::{Cx, Level, TimerId};

struct Read<K> {
    key: K,
    retry: crate::read_scheduler::RetryState,
    timer: Option<TimerId>,
    due: f64,
}

pub struct Backoff<K> {
    reads: Vec<Read<K>>,
    /// End of the latest rate-limit/ban window; later failures wait it out too.
    ban_until: f64,
}
impl<K> Default for Backoff<K> {
    fn default() -> Self {
        Self {
            reads: Vec::new(),
            ban_until: 0.0,
        }
    }
}
impl<K: PartialEq> Backoff<K> {
    pub fn waiting(&self, key: &K) -> bool {
        self.reads
            .iter()
            .any(|read| &read.key == key && read.timer.is_some())
    }

    /// No read has failed without a later success.
    pub fn idle(&self) -> bool {
        self.reads.is_empty()
    }

    /// At most one retry per independent read; no write request enters this API.
    pub fn failed<P>(&mut self, cx: &mut Cx<'_, P>, key: K) {
        self.failed_after(cx, key, 0.0);
    }

    /// As `failed`, waiting at least `minimum_ms`: a venue rate-limit or ban
    /// window must not be retried into, since bans escalate and block trading.
    pub fn failed_after<P>(&mut self, cx: &mut Cx<'_, P>, key: K, minimum_ms: f64) {
        let minimum = if minimum_ms.is_finite() {
            minimum_ms.max(0.0)
        } else {
            0.0
        };
        // A ban covers the whole host: reads already waiting, and any read that
        // fails before it ends, wait it out too.
        let until = (cx.now + minimum).max(self.ban_until);
        self.ban_until = until;
        let minimum = until - cx.now;
        for read in &mut self.reads {
            if let Some(timer) = read.timer
                && read.due < until
            {
                cx.clear_timer(timer);
                read.timer = Some(cx.set_timer(minimum));
                read.due = until;
            }
        }
        let index = match self.reads.iter().position(|read| read.key == key) {
            Some(index) => index,
            None => {
                self.reads.push(Read {
                    key,
                    retry: crate::read_scheduler::RetryState::default(),
                    timer: None,
                    due: 0.0,
                });
                self.reads.len() - 1
            }
        };
        let read = &mut self.reads[index];
        if read.timer.is_some() {
            return;
        }
        let delay = read
            .retry
            .failed(|nominal| cx.jittered(nominal))
            .max(minimum);
        read.timer = Some(cx.set_timer(delay));
        read.due = cx.now + delay;
        cx.notice(
            Level::Warn,
            code::REQUEST_FAILED,
            "Startup read failed; retry scheduled",
        );
    }

    pub fn succeeded<P>(&mut self, cx: &mut Cx<'_, P>, key: &K) {
        if let Some(index) = self.reads.iter().position(|read| &read.key == key)
            && let Some(timer) = self.reads.swap_remove(index).timer
        {
            cx.clear_timer(timer);
        }
    }
}
impl<K: Clone> Backoff<K> {
    pub fn on_timer(&mut self, timer: TimerId) -> Option<K> {
        let read = self
            .reads
            .iter_mut()
            .find(|read| read.timer == Some(timer))?;
        read.timer = None;
        Some(read.key.clone())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiscoveryRead {
    Markets,
    Tickers,
}
impl DiscoveryRead {
    fn name(self) -> &'static str {
        match self {
            Self::Markets => "markets",
            Self::Tickers => "tickers",
        }
    }
}

/// Public startup remains recoverable throughout an outage. Six failed reads
/// surface one degradation receipt; retries continue on the existing schedule.
#[derive(Default)]
pub struct PublicRetries {
    retry: Backoff<DiscoveryRead>,
    degraded: bool,
    ready: bool,
}
impl PublicRetries {
    pub fn failed_after<P>(
        &mut self,
        cx: &mut Cx<'_, P>,
        read: DiscoveryRead,
        minimum_ms: f64,
        error: crate::VenueError,
    ) {
        self.ready(cx);
        let waiting = self.retry.waiting(&read);
        self.retry.failed_after(cx, read, minimum_ms);
        if waiting || self.ready || self.degraded {
            return;
        }
        let attempts = self
            .retry
            .reads
            .iter()
            .find(|r| r.key == read)
            .unwrap()
            .retry
            .failures();
        if attempts < 6 {
            return;
        }
        self.degraded = true;
        let mut fields = vec![
            ("read", read.name().into()),
            ("attempts", attempts.into()),
            (
                "kind",
                error
                    .kind
                    .unwrap_or(crate::ErrorKind::VenueUnavailable)
                    .as_str()
                    .into(),
            ),
        ];
        if let Some(code) = error.venue_code {
            fields.push(("venueCode", code.into()));
        }
        if let Some(message) = error.venue_message {
            fields.push(("venueMessage", message.into()));
        }
        if minimum_ms.is_finite() && minimum_ms > 0.0 {
            fields.push(("retryAfterMs", minimum_ms.into()));
        }
        cx.event(
            Level::Warn,
            code::STARTUP_DEGRADED,
            format!("Public startup delayed; still retrying: {}", error.message),
            fields,
        );
    }

    pub fn succeeded<P>(&mut self, cx: &mut Cx<'_, P>, read: DiscoveryRead) {
        self.retry.succeeded(cx, &read);
        self.ready(cx);
    }

    /// Call after committing usable data, including a partial catalogue.
    /// Repair retries remain independent of public startup readiness.
    pub fn ready<P>(&mut self, cx: &mut Cx<'_, P>) {
        if !self.ready && cx.store.loaded().markets && cx.store.loaded().tickers {
            self.ready = true;
            if std::mem::take(&mut self.degraded) {
                cx.notice(
                    Level::Info,
                    code::STARTUP_RECOVERED,
                    "Public startup recovered",
                );
            }
        }
    }

    pub fn on_timer(&mut self, timer: TimerId) -> Option<DiscoveryRead> {
        self.retry.on_timer(timer)
    }
}

/// Only parsed envelope error fields may enter diagnostics, never a raw body.
pub fn public_response_error(
    status: u16,
    venue_code: Option<&str>,
    venue_message: Option<&str>,
) -> crate::VenueError {
    let kind = if matches!(status, 418 | 429) {
        crate::ErrorKind::RateLimited
    } else {
        crate::ErrorKind::VenueUnavailable
    };
    let message = match (venue_code, venue_message.filter(|s| !s.is_empty())) {
        (Some(code), Some(message)) => format!("Public discovery {code}: {message}"),
        (Some(code), None) => format!("Public discovery {code}"),
        (None, _) if !(200..300).contains(&status) => format!("Public discovery HTTP {status}"),
        _ => "Malformed public discovery response".into(),
    };
    let mut error = crate::VenueError::new(message).kind(kind);
    if let Some(code) = venue_code {
        error = error.venue_code(code);
        if let Some(message) = venue_message {
            error = error.venue_message(message);
        }
    }
    error
}

/// Longest honoured `Retry-After`: real IP bans can exceed minutes, but an
/// absurd or overflowing value must not stall an account indefinitely.
pub const MAX_RATE_LIMIT_PAUSE_MS: f64 = 3_600_000.0;

/// The pause a rate-limited response asks for: `Retry-After` seconds on a 418
/// or 429 (capped at an hour), else `default_ms` for those statuses, else none.
pub fn rate_limit_pause_ms(response: &crate::Response<'_>, default_ms: f64) -> f64 {
    if !matches!(response.status, 418 | 429) {
        return 0.0;
    }
    response
        .header("retry-after")
        .and_then(|seconds| seconds.trim().parse::<f64>().ok())
        .filter(|seconds| !seconds.is_nan() && *seconds > 0.0)
        .map_or(default_ms, |seconds| {
            (seconds * 1_000.0).min(MAX_RATE_LIMIT_PAUSE_MS)
        })
}

/// Do not expose raw catalogue rows, which can arrive through authenticated APIs.
/// Debug, as TS: a skipped row is routine catalogue noise (OKX EEA logs dozens),
/// while an incomplete or partial catalogue is still reported by its venue.
pub fn skipped_row<P>(cx: &mut Cx<'_, P>) {
    cx.notice(
        Level::Debug,
        code::ROW_SKIPPED,
        "Skipped malformed public discovery row",
    );
}

/// TS numeric coercion for public fields that allow blank strings.
pub fn number_or_zero(text: &str) -> Option<f64> {
    if text.trim().is_empty() {
        Some(0.0)
    } else {
        text.parse::<f64>().ok().filter(|value| value.is_finite())
    }
}

/// Retains positional identities when one row in a nested public list is invalid.
pub struct Row<T>(pub Option<T>);

/// Parse a JSON list of rows in one pass when every row is well formed. Only
/// a list with a malformed row pays for the tolerant per-row parse, which
/// scans each row twice; the result is the same either way.
pub fn rows<'de, T: serde::Deserialize<'de>>(text: &'de str) -> serde_json::Result<Vec<Row<T>>> {
    match serde_json::from_str::<Vec<T>>(text) {
        Ok(rows) => Ok(rows.into_iter().map(|row| Row(Some(row))).collect()),
        Err(_) => serde_json::from_str(text),
    }
}
impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Row<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: &'de serde_json::value::RawValue = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self(serde_json::from_str(raw.get()).ok()))
    }
}

/// Protects supplied live ticker fields while a slower discovery read is in flight.
/// Masks use `store::TICKER_FIELDS` order, including `MID_PRICE` for midpoint.
/// Inactive recording does no lookup or allocation.
#[derive(Default)]
pub struct LiveTickers {
    active: bool,
    rows: std::collections::HashMap<String, (u16, crate::model::Ticker)>,
}
impl LiveTickers {
    pub const MID_PRICE: u16 = 1 << 11;

    /// Repeated failures keep the same window until a usable seed commits.
    pub fn begin(&mut self) {
        self.active = true;
    }
    pub fn record(&mut self, store: &crate::store::Store, symbol: &str, mask: u16) {
        if !self.active || mask == 0 || store.market(symbol).is_none() {
            return;
        }
        let Some(row) = store.ticker(symbol) else {
            return;
        };
        if let Some((previous, saved)) = self.rows.get_mut(symbol) {
            Self::copy_fields(saved, row, mask);
            *previous |= mask;
        } else {
            self.rows.insert(symbol.into(), (mask, row.clone()));
        }
    }
    fn copy_fields(row: &mut crate::model::Ticker, live: &crate::model::Ticker, mask: u16) {
        let fields = [
            (&mut row.bid, live.bid),
            (&mut row.ask, live.ask),
            (&mut row.last, live.last),
            (&mut row.mark, live.mark),
            (&mut row.index, live.index),
            (&mut row.percentage, live.percentage),
            (&mut row.open_interest, live.open_interest),
            (&mut row.funding_rate, live.funding_rate),
            (&mut row.funding_time, live.funding_time),
            (&mut row.volume, live.volume),
            (&mut row.quote_volume, live.quote_volume),
        ];
        if mask & Self::MID_PRICE != 0 {
            row.mid_price = live.mid_price;
        }
        for (bit, (target, value)) in fields.into_iter().enumerate() {
            if mask & (1 << bit) != 0 {
                *target = value;
            }
        }
    }
    /// Preserve only actual live fields; REST still fills fields absent from live input.
    pub fn overlay(&self, store: &crate::store::Store, rows: &mut Vec<crate::model::Ticker>) {
        for row in rows.iter_mut() {
            if let Some((mask, live)) = self.rows.get(&row.symbol) {
                Self::copy_fields(row, live, *mask);
            }
        }
        // Follow the existing native table order rather than HashMap iteration.
        for previous in store.tickers() {
            if let Some((_, live)) = self.rows.get(&previous.symbol)
                && store.market(&previous.symbol).is_some()
                && !rows.iter().any(|row| row.symbol == previous.symbol)
            {
                rows.push(live.clone());
            }
        }
    }

    pub fn finish(&mut self, store: &crate::store::Store, rows: &mut Vec<crate::model::Ticker>) {
        self.overlay(store, rows);
        self.rows.clear();
        self.active = false;
    }
}

#[cfg(test)]
mod live_ticker_tests {
    use super::LiveTickers;
    use crate::model::{Limits, Market, MinMax, Precision, Ticker};
    use crate::store::Store;
    fn market(symbol: &str) -> Market {
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: symbol.into(),
            quote: "USDC".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.01,
                    max: 1000.0,
                },
                leverage: MinMax {
                    min: 1.0,
                    max: 20.0,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }
    #[test]
    fn late_seed_overlays_only_supplied_live_fields_and_preserves_missing_known_peers() {
        let mut store = Store::default();
        store.set_markets(vec![market("BTC"), market("ETH")]);
        store.set_tickers(vec![
            Ticker {
                id: "BTC".into(),
                symbol: "BTC".into(),
                bid: 110.0,
                ask: 112.0,
                ..Default::default()
            },
            Ticker {
                id: "ETH".into(),
                symbol: "ETH".into(),
                last: 200.0,
                ..Default::default()
            },
        ]);
        let mut live = LiveTickers::default();
        live.begin();
        live.record(&store, "BTC", 3);
        live.record(&store, "ETH", 1 << 2);
        live.record(&store, "UNKNOWN", 1 << 2);
        // A failed source retries within the same window, without erasing live BBO.
        live.begin();
        store.update_ticker("BTC", |ticker| {
            ticker.last = 111.0;
            ticker.bid = 999.0;
        });
        live.record(&store, "BTC", 1 << 2);
        let mut rows = vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 90.0,
            ask: 92.0,
            last: 91.0,
            mark: 93.0,
            funding_time: 123.0,
            ..Default::default()
        }];
        live.finish(&store, &mut rows);
        assert_eq!(
            (
                rows[0].bid,
                rows[0].ask,
                rows[0].last,
                rows[0].mark,
                rows[0].funding_time
            ),
            (110.0, 112.0, 111.0, 93.0, 123.0)
        );
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[1].symbol.as_str(), rows[1].last), ("ETH", 200.0));
        assert!(!live.active && live.rows.is_empty());
        live.record(&store, "BTC", u16::MAX);
        assert!(
            live.rows.is_empty(),
            "steady updates do not allocate startup records"
        );
    }
    #[test]
    fn bbo_only_and_explicit_zero_updates_do_not_erase_unreported_seed_values() {
        let mut store = Store::default();
        store.set_markets(vec![market("BTC")]);
        store.set_tickers(vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 0.0,
            ask: 102.0,
            ..Default::default()
        }]);
        let mut live = LiveTickers::default();
        live.begin();
        live.record(&store, "BTC", 3);
        store.update_ticker("BTC", |ticker| ticker.mid_price = Some(101.0));
        live.record(&store, "BTC", LiveTickers::MID_PRICE);
        let mut rows = vec![Ticker {
            id: "BTC".into(),
            symbol: "BTC".into(),
            bid: 99.0,
            ask: 101.0,
            last: 100.0,
            mark: 103.0,
            ..Default::default()
        }];
        live.overlay(&store, &mut rows);
        assert!(live.active && !live.rows.is_empty());
        assert_eq!(rows[0].mid_price, Some(101.0));
        live.finish(&store, &mut rows);
        assert_eq!(
            (rows[0].bid, rows[0].ask, rows[0].last, rows[0].mark),
            (0.0, 102.0, 100.0, 103.0)
        );
        live.begin();
        store.update_ticker("BTC", |ticker| ticker.last = 200.0);
        live.record(&store, "BTC", 1 << 2);
        live.finish(&store, &mut rows);
        assert_eq!((rows[0].last, rows[0].mark), (200.0, 103.0));
        assert_eq!(crate::store::TICKER_FIELDS[11], "midPrice");
        live.begin();
        store.update_ticker("BTC", |ticker| ticker.mid_price = None);
        live.record(&store, "BTC", LiveTickers::MID_PRICE);
        rows[0].mid_price = Some(999.0);
        live.finish(&store, &mut rows);
        assert_eq!(
            rows[0].mid_price, None,
            "a delayed seed cannot restore a cleared midpoint"
        );
        assert_eq!((rows[0].last, rows[0].mark), (200.0, 103.0));
    }
}

#[cfg(test)]
mod rows_tests {
    use super::rows;

    #[test]
    fn well_formed_and_malformed_lists_keep_every_position() {
        let all: Vec<Option<u32>> = rows::<u32>("[1,2,3]")
            .unwrap()
            .into_iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(all, [Some(1), Some(2), Some(3)]);
        let partial: Vec<Option<u32>> = rows::<u32>(r#"[1,"two",3]"#)
            .unwrap()
            .into_iter()
            .map(|row| row.0)
            .collect();
        assert_eq!(partial, [Some(1), None, Some(3)]);
        assert!(rows::<u32>("[1,").is_err());
    }
}

#[cfg(test)]
mod rate_limit_tests {
    use super::{MAX_RATE_LIMIT_PAUSE_MS, rate_limit_pause_ms};
    use crate::Response;

    fn pause(status: u16, headers: &str) -> f64 {
        rate_limit_pause_ms(
            &Response {
                status,
                headers,
                body: "",
            },
            30_000.0,
        )
    }

    #[test]
    fn retry_after_is_honoured_up_to_an_hour_and_never_unbounded() {
        assert_eq!(pause(429, "Retry-After: 120"), 120_000.0);
        assert_eq!(pause(418, ""), 30_000.0);
        assert_eq!(pause(500, "Retry-After: 120"), 0.0);
        assert_eq!(pause(429, "Retry-After: 86400"), MAX_RATE_LIMIT_PAUSE_MS);
        assert_eq!(pause(429, "Retry-After: 1e308"), MAX_RATE_LIMIT_PAUSE_MS);
        assert_eq!(pause(429, "Retry-After: inf"), MAX_RATE_LIMIT_PAUSE_MS);
        assert_eq!(pause(429, "Retry-After: NaN"), 30_000.0);
        assert_eq!(pause(429, "Retry-After: -5"), 30_000.0);
        assert_eq!(
            pause(429, "Retry-After: Wed, 21 Oct 2026 07:28:00 GMT"),
            30_000.0
        );
    }
}

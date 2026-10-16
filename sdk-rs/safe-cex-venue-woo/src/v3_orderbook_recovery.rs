//! Transport-free incremental-book recovery; never registered by the live socket.
use crate::v3_public::{
    WooV3MappedOrderBookSnapshot, WooV3OrderBookLevel, WooV3OrderBookSnapshot,
    map_woo_v3_order_book,
};
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_store::sequenced_delta_order_book::{
    OrderBookSequence, SequencedBookDelta, SequencedBookFrame, SequencedBookLevel,
    SequencedBookResult, SequencedDeltaOrderBook,
};
use safe_cex_types::{OrderBook, object_map::JsObjectMap};
use std::sync::{Arc, Mutex};

// ts: WooV3RecoveredOrderBook
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3RecoveredOrderBook {
    pub snapshot_timestamp: f64,
    pub timestamp: f64,
    pub book: OrderBook,
}
// ts: WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES
pub const WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES: usize = 500;
// ts: WooV3OrderBookRecoveryResult
#[derive(Debug, Clone, PartialEq)]
pub enum WooV3OrderBookRecoveryResult {
    Buffered {
        buffered_updates: usize,
    },
    AwaitingJoin {
        snapshot_timestamp: f64,
        stale_updates_discarded: usize,
    },
    Joined {
        snapshot_timestamp: f64,
        update_timestamp: f64,
        applied_updates: usize,
        stale_updates_discarded: usize,
        duplicates_ignored: usize,
        publication: WooV3RecoveredOrderBook,
    },
    Applied {
        snapshot_timestamp: f64,
        update_timestamp: f64,
        publication: WooV3RecoveredOrderBook,
    },
    Ignored {
        reason: String,
        update_timestamp: Option<f64>,
    },
    Recover {
        reason: String,
        requires_fresh_snapshot: bool,
    },
    Reset {
        reason: String,
        requires_fresh_snapshot: bool,
    },
}
// Supporting result of requireNumericLevel.
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3NumericLevel {
    pub numeric: [f64; 2],
    pub wire: [String; 2],
}
// Supporting result of normalizeSide.
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3NormalizedSide {
    pub numeric: Vec<SequencedBookLevel>,
    pub wire: Vec<[String; 2]>,
}
// Supporting result of normalizeUpdate.
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3NormalizedUpdate {
    pub symbol: String,
    pub previous_timestamp: f64,
    pub timestamp: f64,
    pub asks: Vec<SequencedBookLevel>,
    pub bids: Vec<SequencedBookLevel>,
    pub fingerprint: String,
}
fn record(value: Option<&JsNumberInput>) -> Option<JsObjectMap<JsNumberInput>> {
    value.and_then(|v| v.clone().decode().ok())
}
fn array(value: Option<&JsNumberInput>) -> Option<Vec<JsNumberInput>> {
    value.and_then(|v| v.clone().decode().ok())
}
fn string(value: Option<&JsNumberInput>) -> Option<String> {
    value.and_then(|v| v.clone().decode().ok())
}
// ts: isRecord
pub fn is_record(value: Option<&JsNumberInput>) -> bool {
    record(value).is_some()
}
// ts: requirePositiveTimestamp
pub fn require_positive_timestamp(
    value: Option<&JsNumberInput>,
    path: &str,
) -> Result<f64, String> {
    let n = value.and_then(|v| v.clone().decode::<f64>().ok());
    n.filter(|n| n.is_finite() && n.trunc() == *n && *n > 0.0 && *n <= 9_007_199_254_740_991.0)
        .ok_or_else(|| format!("{path} must be a positive safe integer millisecond timestamp"))
}
fn decimal_pattern(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let mut parts = s.split('.');
    let left = parts.next().unwrap_or("");
    let right = parts.next();
    if parts.next().is_some() {
        return false;
    }
    let digits = |x: &str| x.bytes().all(|b| b.is_ascii_digit());
    match right {
        Some(r) => digits(left) && digits(r) && (!left.is_empty() || !r.is_empty()),
        None => !left.is_empty() && digits(left),
    }
}
// ts: requireNumericLevel
pub fn require_numeric_level(
    input: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3NumericLevel, String> {
    let values = array(input)
        .filter(|v| v.len() == 2)
        .ok_or_else(|| format!("{path} must be a price/quantity tuple"))?;
    let price = string(values.first())
        .filter(|s| {
            decimal_pattern(s)
                && s.bytes().any(|c| (b'1'..=b'9').contains(&c))
                && !s.starts_with('-')
        })
        .ok_or_else(|| format!("{path}[0] must be a positive decimal string"))?;
    let quantity = string(values.get(1))
        .filter(|s| decimal_pattern(s) && !s.starts_with('-'))
        .ok_or_else(|| format!("{path}[1] must be a non-negative decimal string"))?;
    let p = safe_cex_num::number_from_string(&price);
    let q = safe_cex_num::number_from_string(&quantity);
    if !p.is_finite() || p <= 0.0 {
        return Err(format!("{path}[0] must map to a positive finite number"));
    }
    if !q.is_finite()
        || q < 0.0
        || (q == 0.0 && quantity.bytes().any(|c| (b'1'..=b'9').contains(&c)))
    {
        return Err(format!(
            "{path}[1] must map losslessly to a non-negative finite number"
        ));
    }
    Ok(WooV3NumericLevel {
        numeric: [p, q],
        wire: [price, quantity],
    })
}
// ts: normalizeSide
pub fn normalize_side(
    input: Option<&JsNumberInput>,
    path: &str,
) -> Result<WooV3NormalizedSide, String> {
    let values = array(input).ok_or_else(|| format!("{path} must be an array"))?;
    let decoded = values
        .iter()
        .enumerate()
        .map(|(i, v)| require_numeric_level(Some(v), &format!("{path}[{i}]")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WooV3NormalizedSide {
        numeric: decoded.iter().map(|v| v.numeric.to_vec()).collect(),
        wire: decoded.into_iter().map(|v| v.wire).collect(),
    })
}
// ts: normalizeUpdate
pub fn normalize_update(
    input: Option<&JsNumberInput>,
    expected_symbol: &str,
) -> Result<WooV3NormalizedUpdate, String> {
    let row = record(input).ok_or("WOO V3 orderbook delta must be an object")?;
    if string(row.get("symbol")).as_deref() != Some(expected_symbol) {
        return Err("WOO V3 orderbook delta symbol does not match its coordinator".into());
    }
    let previous_timestamp = require_positive_timestamp(
        row.get("previousTimestamp"),
        "WOO V3 orderbook delta.prevTs",
    )?;
    let timestamp = require_positive_timestamp(row.get("timestamp"), "WOO V3 orderbook delta.ts")?;
    if timestamp <= previous_timestamp {
        return Err("WOO V3 orderbook delta.ts must follow prevTs".into());
    }
    let asks = normalize_side(row.get("asks"), "WOO V3 orderbook delta.asks")?;
    let bids = normalize_side(row.get("bids"), "WOO V3 orderbook delta.bids")?;
    let fingerprint = format!(
        "[{},{},{},{},{}]",
        serde_json::to_string(expected_symbol).expect("string serialization"),
        safe_cex_num::js_number_string(previous_timestamp),
        safe_cex_num::js_number_string(timestamp),
        serde_json::to_string(&asks.wire).expect("string tuples"),
        serde_json::to_string(&bids.wire).expect("string tuples")
    );
    Ok(WooV3NormalizedUpdate {
        symbol: expected_symbol.into(),
        previous_timestamp,
        timestamp,
        asks: asks.numeric,
        bids: bids.numeric,
        fingerprint,
    })
}
// ts: snapshotLevels
pub fn snapshot_levels(
    snapshot: &WooV3MappedOrderBookSnapshot,
    bids: bool,
) -> Vec<SequencedBookLevel> {
    let side = if bids {
        &snapshot.book.bids
    } else {
        &snapshot.book.asks
    };
    side.iter().map(|v| vec![v.price, v.amount]).collect()
}
#[derive(PartialEq)]
enum RecoveryPhase {
    AwaitingSnapshot,
    AwaitingJoin,
    Joined,
}
// ts: WooV3OrderBookRecoveryCoordinator
pub struct WooV3OrderBookRecoveryCoordinator {
    order_book: Arc<Mutex<OrderBook>>,
    sequenced_book: SequencedDeltaOrderBook,
    buffered_updates: Vec<WooV3NormalizedUpdate>,
    phase: RecoveryPhase,
    snapshot_timestamp: Option<f64>,
    current_timestamp: Option<f64>,
    current_fingerprint: Option<String>,
    next_snapshot_request_token: f64,
    active_snapshot_request_token: Option<f64>,
    symbol: String,
    depth: usize,
}
// Capture the REST envelope without passing its numeric input through Value.
// The existing public mapper receives only a validated native snapshot.
fn decode_rest_snapshot(
    input: Option<&JsNumberInput>,
) -> Result<WooV3MappedOrderBookSnapshot, String> {
    let row = record(input).ok_or("invalid envelope")?;
    if row
        .get("success")
        .and_then(|v| v.clone().decode::<bool>().ok())
        != Some(true)
    {
        return Err("invalid success".into());
    }
    let data = record(row.get("data")).ok_or("invalid data")?;
    let timestamp = require_positive_timestamp(row.get("timestamp"), "WOO V3 orderbook.timestamp")?;
    let side = |key: &str| -> Result<Vec<WooV3OrderBookLevel>, String> {
        array(data.get(key))
            .ok_or("invalid side")?
            .iter()
            .map(|v| {
                let row = record(Some(v)).ok_or("invalid level")?;
                let decimal = |key: &str| -> Result<String, String> {
                    let s = string(row.get(key)).ok_or("invalid decimal")?;
                    if safe_cex_num::js_trim(&s).is_empty()
                        || !safe_cex_num::number_from_string(&s).is_finite()
                    {
                        return Err("invalid decimal".into());
                    }
                    Ok(s)
                };
                Ok(WooV3OrderBookLevel {
                    price: decimal("price")?,
                    quantity: decimal("quantity")?,
                })
            })
            .collect()
    };
    let asks = side("asks")?;
    let bids = side("bids")?;
    map_woo_v3_order_book(&WooV3OrderBookSnapshot {
        timestamp,
        asks,
        bids,
    })
}
impl WooV3OrderBookRecoveryCoordinator {
    // ts: constructor
    pub fn new(
        symbol: Option<&JsNumberInput>,
        depth: Option<&JsNumberInput>,
    ) -> Result<Self, String> {
        let symbol = string(symbol)
            .filter(|s| !s.is_empty())
            .ok_or("WOO V3 orderbook coordinator symbol must be a non-empty string")?;
        let depth = depth
            .and_then(|v| v.clone().decode::<f64>().ok())
            .filter(|n| matches!(*n, 50.0 | 200.0 | 500.0))
            .ok_or("WOO V3 orderbook coordinator depth must be 50, 200, or 500")?
            as usize;
        let order_book = Arc::new(Mutex::new(OrderBook {
            bids: vec![],
            asks: vec![],
        }));
        let sequenced_book = SequencedDeltaOrderBook::new(order_book.clone());
        Ok(Self {
            order_book,
            sequenced_book,
            buffered_updates: vec![],
            phase: RecoveryPhase::AwaitingSnapshot,
            snapshot_timestamp: None,
            current_timestamp: None,
            current_fingerprint: None,
            next_snapshot_request_token: 1.0,
            active_snapshot_request_token: None,
            symbol,
            depth,
        })
    }
    // ts: requiresFreshSnapshot
    pub fn requires_fresh_snapshot(&self) -> bool {
        self.phase != RecoveryPhase::Joined
    }
    // ts: beginSnapshotRequest
    pub fn begin_snapshot_request(&mut self) -> Result<f64, String> {
        if !self.next_snapshot_request_token.is_finite()
            || self.next_snapshot_request_token.trunc() != self.next_snapshot_request_token
            || self.next_snapshot_request_token.abs() > 9_007_199_254_740_991.0
        {
            return Err("WOO V3 orderbook snapshot request token space exhausted".into());
        }
        let token = self.next_snapshot_request_token;
        self.next_snapshot_request_token += 1.0;
        self.active_snapshot_request_token = Some(token);
        Ok(token)
    }
    // ts: pushUpdate
    pub fn push_update(&mut self, input: Option<&JsNumberInput>) -> WooV3OrderBookRecoveryResult {
        if let Some(row) = record(input) {
            if array(row.get("asks")).is_some_and(|v| v.len() > self.depth)
                || array(row.get("bids")).is_some_and(|v| v.len() > self.depth)
            {
                return self.recover("frame-level-overflow");
            }
        }
        let update = match normalize_update(input, &self.symbol) {
            Ok(v) => v,
            Err(_) => return self.recover("invalid-delta"),
        };
        match self.phase {
            RecoveryPhase::AwaitingSnapshot => self.buffer_before_snapshot(update),
            RecoveryPhase::AwaitingJoin => self.join_after_snapshot(update),
            RecoveryPhase::Joined => self.apply_joined_update(update),
        }
    }
    // ts: acceptRestSnapshot
    pub fn accept_rest_snapshot(
        &mut self,
        input: Option<&JsNumberInput>,
        token: Option<&JsNumberInput>,
    ) -> WooV3OrderBookRecoveryResult {
        let token = token.and_then(|v| v.clone().decode::<f64>().ok());
        if self.active_snapshot_request_token.is_none()
            || token.is_none_or(|n| {
                !n.is_finite()
                    || n.trunc() != n
                    || n <= 0.0
                    || n > 9_007_199_254_740_991.0
                    || Some(n) != self.active_snapshot_request_token
            })
        {
            return WooV3OrderBookRecoveryResult::Ignored {
                reason: "stale-snapshot-request".into(),
                update_timestamp: None,
            };
        }
        self.active_snapshot_request_token = None;
        let snapshot = match decode_rest_snapshot(input) {
            Ok(v) => v,
            Err(_) => return self.recover("invalid-snapshot"),
        };
        if snapshot.book.bids.len() > self.depth || snapshot.book.asks.len() > self.depth {
            return self.recover("frame-level-overflow");
        }
        self.sequenced_book.reset();
        let result = self.sequenced_book.apply_snapshot(&SequencedBookFrame {
            sequence: OrderBookSequence::Number(snapshot.timestamp),
            bids: snapshot_levels(&snapshot, true),
            asks: snapshot_levels(&snapshot, false),
        });
        if !matches!(result, SequencedBookResult::Applied { .. }) {
            return self.recover("invalid-snapshot");
        }
        self.snapshot_timestamp = Some(snapshot.timestamp);
        self.current_timestamp = Some(snapshot.timestamp);
        self.current_fingerprint = None;
        self.phase = RecoveryPhase::AwaitingJoin;
        let queued = std::mem::take(&mut self.buffered_updates);
        let mut stale_updates_discarded = 0;
        let mut duplicates_ignored = 0;
        let mut applied_updates = 0;
        for update in queued {
            if update.timestamp <= snapshot.timestamp {
                stale_updates_discarded += 1;
                continue;
            }
            if self.phase == RecoveryPhase::AwaitingJoin {
                if update.previous_timestamp != snapshot.timestamp {
                    return self.recover("missing-join");
                }
                if !self.apply_to_sequencer(&update) {
                    return self.recover("sequence-gap");
                }
                self.phase = RecoveryPhase::Joined;
                applied_updates += 1;
                continue;
            }
            if Some(update.timestamp) == self.current_timestamp {
                if self.current_fingerprint.as_ref() != Some(&update.fingerprint) {
                    return self.recover("conflicting-duplicate");
                }
                duplicates_ignored += 1;
                continue;
            }
            if self.current_timestamp.unwrap_or(0.0) > update.timestamp {
                stale_updates_discarded += 1;
                continue;
            }
            if !self.apply_to_sequencer(&update) {
                return self.recover("sequence-gap");
            }
            applied_updates += 1;
        }
        if self.phase == RecoveryPhase::AwaitingJoin {
            WooV3OrderBookRecoveryResult::AwaitingJoin {
                snapshot_timestamp: snapshot.timestamp,
                stale_updates_discarded,
            }
        } else {
            WooV3OrderBookRecoveryResult::Joined {
                snapshot_timestamp: snapshot.timestamp,
                update_timestamp: self.current_timestamp.expect("joined time"),
                applied_updates,
                stale_updates_discarded,
                duplicates_ignored,
                publication: self.create_publication(),
            }
        }
    }
    // ts: reconnect
    pub fn reconnect(&mut self) -> WooV3OrderBookRecoveryResult {
        self.reset();
        WooV3OrderBookRecoveryResult::Reset {
            reason: "reconnect".into(),
            requires_fresh_snapshot: true,
        }
    }
    // ts: bufferBeforeSnapshot
    fn buffer_before_snapshot(
        &mut self,
        update: WooV3NormalizedUpdate,
    ) -> WooV3OrderBookRecoveryResult {
        if let Some(latest) = self.buffered_updates.last() {
            if update.timestamp == latest.timestamp {
                if update.fingerprint != latest.fingerprint {
                    return self.recover("conflicting-duplicate");
                }
                return WooV3OrderBookRecoveryResult::Ignored {
                    reason: "duplicate".into(),
                    update_timestamp: Some(update.timestamp),
                };
            }
            if update.timestamp < latest.timestamp {
                return self.recover("out-of-order-prejoin-update");
            }
        }
        if self.buffered_updates.len() == WOO_V3_ORDERBOOK_MAX_BUFFERED_UPDATES {
            return self.recover("buffer-overflow");
        }
        self.buffered_updates.push(update);
        WooV3OrderBookRecoveryResult::Buffered {
            buffered_updates: self.buffered_updates.len(),
        }
    }
    // ts: joinAfterSnapshot
    fn join_after_snapshot(
        &mut self,
        update: WooV3NormalizedUpdate,
    ) -> WooV3OrderBookRecoveryResult {
        if update.timestamp <= self.snapshot_timestamp.expect("snapshot time") {
            return WooV3OrderBookRecoveryResult::Ignored {
                reason: "stale".into(),
                update_timestamp: Some(update.timestamp),
            };
        }
        if Some(update.previous_timestamp) != self.snapshot_timestamp {
            return self.recover("missing-join");
        }
        if !self.apply_to_sequencer(&update) {
            return self.recover("sequence-gap");
        }
        self.phase = RecoveryPhase::Joined;
        WooV3OrderBookRecoveryResult::Joined {
            snapshot_timestamp: self.snapshot_timestamp.expect("snapshot time"),
            update_timestamp: update.timestamp,
            applied_updates: 1,
            stale_updates_discarded: 0,
            duplicates_ignored: 0,
            publication: self.create_publication(),
        }
    }
    // ts: applyJoinedUpdate
    fn apply_joined_update(
        &mut self,
        update: WooV3NormalizedUpdate,
    ) -> WooV3OrderBookRecoveryResult {
        if Some(update.timestamp) == self.current_timestamp {
            if self.current_fingerprint.as_ref() != Some(&update.fingerprint) {
                return self.recover("conflicting-duplicate");
            }
            return WooV3OrderBookRecoveryResult::Ignored {
                reason: "duplicate".into(),
                update_timestamp: Some(update.timestamp),
            };
        }
        if update.timestamp < self.current_timestamp.expect("joined time") {
            return WooV3OrderBookRecoveryResult::Ignored {
                reason: "stale".into(),
                update_timestamp: Some(update.timestamp),
            };
        }
        if !self.apply_to_sequencer(&update) {
            return self.recover("sequence-gap");
        }
        WooV3OrderBookRecoveryResult::Applied {
            snapshot_timestamp: self.snapshot_timestamp.expect("snapshot time"),
            update_timestamp: update.timestamp,
            publication: self.create_publication(),
        }
    }
    // ts: applyToSequencer
    fn apply_to_sequencer(&mut self, update: &WooV3NormalizedUpdate) -> bool {
        let result = self.sequenced_book.apply_delta(&SequencedBookDelta {
            sequence: OrderBookSequence::Number(update.timestamp),
            previous_sequence: OrderBookSequence::Number(update.previous_timestamp),
            bids: update.bids.clone(),
            asks: update.asks.clone(),
        });
        if !matches!(result, SequencedBookResult::Applied { .. }) {
            return false;
        }
        self.current_timestamp = Some(update.timestamp);
        self.current_fingerprint = Some(update.fingerprint.clone());
        safe_cex_store::orderbook::calc_order_book_total(
            &mut self.order_book.lock().unwrap_or_else(|e| e.into_inner()),
        );
        true
    }
    // ts: createPublication
    fn create_publication(&self) -> WooV3RecoveredOrderBook {
        WooV3RecoveredOrderBook {
            snapshot_timestamp: self.snapshot_timestamp.expect("snapshot time"),
            timestamp: self.current_timestamp.expect("current time"),
            book: self
                .order_book
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }
    // ts: recover
    fn recover(&mut self, reason: &str) -> WooV3OrderBookRecoveryResult {
        self.reset();
        WooV3OrderBookRecoveryResult::Recover {
            reason: reason.into(),
            requires_fresh_snapshot: true,
        }
    }
    // ts: reset
    fn reset(&mut self) {
        self.sequenced_book.reset();
        self.buffered_updates.clear();
        self.phase = RecoveryPhase::AwaitingSnapshot;
        self.snapshot_timestamp = None;
        self.current_timestamp = None;
        self.current_fingerprint = None;
        self.active_snapshot_request_token = None;
    }
}

#[cfg(test)]
mod token_tests {
    use super::*;
    #[test]
    fn native_request_token_exhaustion_matches_source_boundary() {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/orderbook-recovery-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(out.status.success(), "RECOVERY token source oracle");
        let data: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(data["tokenBoundary"].as_array().unwrap().len(), 2);
        let symbol: JsNumberInput =
            safe_cex_num::numeric_ingress::js_from_str("\"PERP_BTC_USDT\"").unwrap();
        let depth: JsNumberInput = safe_cex_num::numeric_ingress::js_from_str("50").unwrap();
        let mut c = WooV3OrderBookRecoveryCoordinator::new(Some(&symbol), Some(&depth)).unwrap();
        c.next_snapshot_request_token = 9_007_199_254_740_991.0;
        let expected = data["tokenBoundary"][0]["ok"]["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert_eq!(
            c.begin_snapshot_request().unwrap().to_bits(),
            expected.to_bits(),
            "RECOVERY native token boundary"
        );
        assert_eq!(
            c.begin_snapshot_request().unwrap_err(),
            data["tokenBoundary"][1]["error"]["message"]
                .as_str()
                .unwrap(),
            "RECOVERY native token exhausted"
        );
        assert_eq!(data["tokenBoundary"][1]["error"]["name"], "Error");
        c.reconnect();
        assert!(
            c.begin_snapshot_request().is_err(),
            "RECOVERY native reset preserves exhausted nonce"
        );
    }
}

//! Port of `utils/sequencedDeltaOrderBook.ts`; callers retain the shared book handle.
use crate::update_order_book::update_order_book_by_price;
use num_bigint::BigInt;
use safe_cex_num::js_trim;
use safe_cex_types::OrderBook;
use serde::Serialize;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub enum OrderBookSequence {
    String(String),
    Number(f64),
    BigInt(BigInt),
}

pub type SequencedBookLevel = Vec<f64>;

#[derive(Clone, Debug)]
pub struct SequencedBookFrame {
    pub sequence: OrderBookSequence,
    pub bids: Vec<SequencedBookLevel>,
    pub asks: Vec<SequencedBookLevel>,
}

#[derive(Clone, Debug)]
pub struct SequencedBookDelta {
    pub sequence: OrderBookSequence,
    pub previous_sequence: OrderBookSequence,
    pub bids: Vec<SequencedBookLevel>,
    pub asks: Vec<SequencedBookLevel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SequencedBookFrameKind {
    Snapshot,
    Delta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SequencedBookIgnoreReason {
    AwaitingSnapshot,
    Duplicate,
    Stale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SequencedBookRecoveryReason {
    InvalidLevel,
    InvalidSequence,
    MissingSnapshot,
    SequenceGap,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum SequencedBookResult {
    Applied {
        frame: SequencedBookFrameKind,
        sequence: String,
    },
    Ignored {
        reason: SequencedBookIgnoreReason,
        #[serde(skip_serializing_if = "Option::is_none")]
        sequence: Option<String>,
    },
    Recover {
        reason: SequencedBookRecoveryReason,
    },
}

#[derive(PartialEq, Eq)]
enum SequenceState {
    Uninitialized,
    Ready,
    Recovering,
}

fn normalize_sequence(value: &OrderBookSequence) -> Option<BigInt> {
    match value {
        OrderBookSequence::BigInt(value) => (value >= &BigInt::from(0)).then(|| value.clone()),
        OrderBookSequence::Number(value) => (value.is_finite()
            && value.fract() == 0.0
            && (0.0..=9_007_199_254_740_991.0).contains(value))
        .then(|| BigInt::from(*value as u64)),
        OrderBookSequence::String(value) => {
            let trimmed = js_trim(value);
            if trimmed.is_empty() || !trimmed.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            BigInt::parse_bytes(trimmed.as_bytes(), 10)
        }
    }
}

fn has_valid_levels(levels: &[SequencedBookLevel]) -> bool {
    levels.iter().all(|level| {
        level.len() >= 2
            && level[0].is_finite()
            && level[0] > 0.0
            && level[1].is_finite()
            && level[1] >= 0.0
    })
}

pub struct SequencedDeltaOrderBook {
    order_book: Arc<Mutex<OrderBook>>,
    current_sequence_value: Option<BigInt>,
    sequence_state: SequenceState,
}

impl SequencedDeltaOrderBook {
    pub fn new(order_book: Arc<Mutex<OrderBook>>) -> Self {
        Self {
            order_book,
            current_sequence_value: None,
            sequence_state: SequenceState::Uninitialized,
        }
    }

    pub fn current_sequence(&self) -> Option<String> {
        self.current_sequence_value
            .as_ref()
            .map(ToString::to_string)
    }

    pub fn is_ready(&self) -> bool {
        self.sequence_state == SequenceState::Ready
    }

    pub fn is_recovering(&self) -> bool {
        self.sequence_state == SequenceState::Recovering
    }

    pub fn apply_snapshot(&mut self, frame: &SequencedBookFrame) -> SequencedBookResult {
        let Some(sequence) = normalize_sequence(&frame.sequence) else {
            return self.require_recovery(SequencedBookRecoveryReason::InvalidSequence);
        };
        if !has_valid_levels(&frame.bids) || !has_valid_levels(&frame.asks) {
            return self.require_recovery(SequencedBookRecoveryReason::InvalidLevel);
        }
        let mut next_book = OrderBook {
            bids: vec![],
            asks: vec![],
        };
        Self::apply_levels(&mut next_book, true, &frame.bids);
        Self::apply_levels(&mut next_book, false, &frame.asks);
        {
            let mut book = self.order_book.lock().unwrap_or_else(|e| e.into_inner());
            book.bids = next_book.bids;
            book.asks = next_book.asks;
        }
        self.current_sequence_value = Some(sequence.clone());
        self.sequence_state = SequenceState::Ready;
        SequencedBookResult::Applied {
            frame: SequencedBookFrameKind::Snapshot,
            sequence: sequence.to_string(),
        }
    }

    pub fn apply_delta(&mut self, frame: &SequencedBookDelta) -> SequencedBookResult {
        if self.sequence_state == SequenceState::Recovering {
            return SequencedBookResult::Ignored {
                reason: SequencedBookIgnoreReason::AwaitingSnapshot,
                sequence: None,
            };
        }
        if self.sequence_state != SequenceState::Ready || self.current_sequence_value.is_none() {
            return self.require_recovery(SequencedBookRecoveryReason::MissingSnapshot);
        }
        let (Some(sequence), Some(previous_sequence)) = (
            normalize_sequence(&frame.sequence),
            normalize_sequence(&frame.previous_sequence),
        ) else {
            return self.require_recovery(SequencedBookRecoveryReason::InvalidSequence);
        };
        if self.current_sequence_value.as_ref() == Some(&sequence) {
            return SequencedBookResult::Ignored {
                reason: SequencedBookIgnoreReason::Duplicate,
                sequence: Some(sequence.to_string()),
            };
        }
        if Some(&sequence) < self.current_sequence_value.as_ref() {
            return SequencedBookResult::Ignored {
                reason: SequencedBookIgnoreReason::Stale,
                sequence: Some(sequence.to_string()),
            };
        }
        if self.current_sequence_value.as_ref() != Some(&previous_sequence)
            || sequence <= previous_sequence
        {
            return self.require_recovery(SequencedBookRecoveryReason::SequenceGap);
        }
        if !has_valid_levels(&frame.bids) || !has_valid_levels(&frame.asks) {
            return self.require_recovery(SequencedBookRecoveryReason::InvalidLevel);
        }
        {
            let mut book = self.order_book.lock().unwrap_or_else(|e| e.into_inner());
            Self::apply_levels(&mut book, true, &frame.bids);
            Self::apply_levels(&mut book, false, &frame.asks);
        }
        self.current_sequence_value = Some(sequence.clone());
        SequencedBookResult::Applied {
            frame: SequencedBookFrameKind::Delta,
            sequence: sequence.to_string(),
        }
    }

    pub fn reset(&mut self) {
        self.clear_book();
        self.current_sequence_value = None;
        self.sequence_state = SequenceState::Uninitialized;
    }

    fn apply_levels(book: &mut OrderBook, bids: bool, levels: &[SequencedBookLevel]) {
        for level in levels {
            update_order_book_by_price(book, bids, level[0], level[1], true);
        }
    }

    fn require_recovery(&mut self, reason: SequencedBookRecoveryReason) -> SequencedBookResult {
        if self.sequence_state == SequenceState::Recovering {
            return SequencedBookResult::Ignored {
                reason: SequencedBookIgnoreReason::AwaitingSnapshot,
                sequence: None,
            };
        }
        self.clear_book();
        self.current_sequence_value = None;
        self.sequence_state = SequenceState::Recovering;
        SequencedBookResult::Recover { reason }
    }

    fn clear_book(&mut self) {
        let mut book = self.order_book.lock().unwrap_or_else(|e| e.into_inner());
        book.bids = vec![];
        book.asks = vec![];
    }
}

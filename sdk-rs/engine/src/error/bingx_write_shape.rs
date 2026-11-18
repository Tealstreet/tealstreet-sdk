//! Closed request shape only; never stores parameter values or native identities.
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::num::NonZeroU64;

pub const PARAMETERS: [&str; 22] = [
    "cancelClientOrderId",
    "cancelOrderId",
    "cancelReplaceMode",
    "cancelRestrictions",
    "clientOrderId",
    "closePosition",
    "orderId",
    "positionId",
    "positionSide",
    "price",
    "priceProtect",
    "quantity",
    "reduceOnly",
    "side",
    "signature",
    "stopGuaranteed",
    "stopPrice",
    "symbol",
    "timeInForce",
    "timestamp",
    "type",
    "workingType",
];
#[derive(Clone, Copy)]
pub enum Endpoint {
    Order,
    Amend,
    CancelReplace,
    CancelAll,
}
#[derive(Clone, Copy)]
pub enum TargetKind {
    Limit,
    TakeProfit,
    StopLoss,
    Trigger,
    Attached,
    Trailing,
    Other,
    Unavailable,
    Multiple,
}
#[derive(Clone, Copy)]
pub enum IdClass {
    Numeric,
    Other,
    Missing,
}
#[derive(Clone, Copy)]
pub enum SymbolFormat {
    Hyphenated,
    Compact,
    Other,
    Missing,
}
#[derive(Clone, Copy)]
pub enum Replacement {
    Observed,
    Unobserved,
    NotApplicable,
}

pub struct Parts {
    pub endpoint: Endpoint,
    pub post: bool,
    pub parameters: u32,
    pub other_parameters: bool,
    pub id_class: IdClass,
    pub id_digits: usize,
    pub target_kind: TargetKind,
    pub symbol_format: SymbolFormat,
    pub replacement: Replacement,
}

/// Compact optional detail keeps raw text and response trees out of error queues.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BingxWriteShape(NonZeroU64);
impl BingxWriteShape {
    pub fn new(parts: Parts) -> Self {
        let bits = (parts.parameters & ((1 << 22) - 1)) as u64
            | u64::from(parts.other_parameters) << 22
            | (parts.endpoint as u64) << 23
            | u64::from(parts.post) << 25
            | (parts.target_kind as u64) << 26
            | (parts.id_class as u64) << 30
            | (parts.id_digits.min(64) as u64) << 32
            | (parts.symbol_format as u64) << 39
            | (parts.replacement as u64) << 41
            | 1 << 63;
        Self(NonZeroU64::new(bits).expect("fixed presence bit"))
    }
}

struct Names(u64);
impl Serialize for Names {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let count = (self.0 as u32 & ((1 << 22) - 1)).count_ones() as usize;
        let mut seq = serializer.serialize_seq(Some(count))?;
        for (index, name) in PARAMETERS.iter().enumerate() {
            if self.0 & (1 << index) != 0 {
                seq.serialize_element(name)?;
            }
        }
        seq.end()
    }
}
impl Serialize for BingxWriteShape {
    #[inline(never)]
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Wire {
            endpoint_family: &'static str,
            method: &'static str,
            parameter_names: Names,
            has_other_parameters: bool,
            id_class: &'static str,
            id_digits: u8,
            target_kind: &'static str,
            symbol_format: &'static str,
            replacement: &'static str,
        }
        let b = self.0.get();
        Wire {
            endpoint_family: ["order", "amend", "cancelReplace", "cancelAll"]
                [(b >> 23 & 3) as usize],
            method: if b >> 25 & 1 == 0 { "DELETE" } else { "POST" },
            parameter_names: Names(b),
            has_other_parameters: b >> 22 & 1 != 0,
            id_class: ["numeric", "other", "missing"][(b >> 30 & 3) as usize],
            id_digits: (b >> 32 & 127) as u8,
            target_kind: [
                "limit",
                "takeProfit",
                "stopLoss",
                "trigger",
                "attached",
                "trailing",
                "other",
                "unavailable",
                "multiple",
            ][(b >> 26 & 15) as usize],
            symbol_format: ["hyphenated", "compact", "other", "missing"][(b >> 39 & 3) as usize],
            replacement: ["observed", "unobserved", "notApplicable"][(b >> 41 & 3) as usize],
        }
        .serialize(serializer)
    }
}

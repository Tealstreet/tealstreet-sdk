//! BloFin's VENUE_STATE section (`Venue::export_venue_state`): the private state
//! a frame-fed engine's writes read that the store frames do not carry. Writes
//! refuse until the account margin mode is loaded; cancels and amends of
//! conditional and attached legs need each order's `Source`; and cancel and
//! amend verify fills against the native fill progress.
//!
//! Version 1, little-endian: a version byte; a flags byte (mode loaded, margin
//! known, margin isolated); the sources, sorted by order id; the fill progress
//! in its eviction order. Strings are a u32 length and UTF-8 bytes; an absent
//! value is NaN.
use super::{Account, Source};
use safe_cex_engine::model::MarginType;
use std::collections::{HashMap, VecDeque};

const VERSION: u8 = 1;
const MODE_LOADED: u8 = 1;
const MARGIN_KNOWN: u8 = 1 << 1;
const MARGIN_ISOLATED: u8 = 1 << 2;

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn put_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes());
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let length = self.u32()? as usize;
        Some(std::str::from_utf8(self.take(length)?).ok()?.to_owned())
    }
}

fn optional(value: f64) -> Option<f64> {
    (!value.is_nan()).then_some(value)
}

impl Account {
    pub(crate) fn export_venue_state(&self) -> Vec<u8> {
        let mut out = vec![VERSION];
        let mut flags = 0;
        if self.mode_loaded {
            flags |= MODE_LOADED;
        }
        if let Some(margin) = self.margin {
            flags |= MARGIN_KNOWN;
            if margin == MarginType::Isolated {
                flags |= MARGIN_ISOLATED;
            }
        }
        out.push(flags);
        // Sorted, so unchanged state encodes to unchanged bytes and is not resent.
        let mut ids: Vec<&String> = self.sources.keys().collect();
        ids.sort();
        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in ids {
            let source = &self.sources[id];
            put_str(&mut out, id);
            for text in [
                &source.inst_id,
                &source.margin_mode,
                &source.position_side,
                &source.order_id,
                &source.tpsl_id,
                &source.algo_id,
                &source.trigger_price_type,
                &source.tp_type,
                &source.sl_type,
            ] {
                put_str(&mut out, text);
            }
            for value in [
                source.order_price,
                source.tp,
                source.sl,
                source.tp_order_price,
                source.sl_order_price,
                source.execution_baseline.0,
                source.execution_baseline.1.unwrap_or(f64::NAN),
            ] {
                put_f64(&mut out, value);
            }
        }
        // Only ids with recorded progress: an evicted id has nothing to verify.
        let fills: Vec<_> = self
            .fill_order
            .iter()
            .filter_map(|id| Some((id, *self.fills.get(id)?)))
            .collect();
        out.extend_from_slice(&(fills.len() as u32).to_le_bytes());
        for (id, (qty, value)) in fills {
            put_str(&mut out, id);
            put_f64(&mut out, qty);
            put_f64(&mut out, value.unwrap_or(f64::NAN));
        }
        out
    }

    /// Replace the exported state with a data source's; `false` leaves it as it
    /// was when the bytes are not a version this build reads.
    pub(crate) fn apply_venue_state(&mut self, bytes: &[u8]) -> bool {
        let Some((mode_loaded, margin, sources, fills)) = decode(bytes) else {
            return false;
        };
        self.mode_loaded = mode_loaded;
        self.margin = margin;
        self.sources = sources;
        self.fills = HashMap::new();
        self.fill_order = VecDeque::new();
        for (id, progress) in fills {
            if self.fills.insert(id.clone(), progress).is_none() {
                self.fill_order.push_back(id);
            }
        }
        true
    }
}

type Decoded = (
    bool,
    Option<MarginType>,
    HashMap<String, Source>,
    Vec<(String, (f64, Option<f64>))>,
);

fn decode(bytes: &[u8]) -> Option<Decoded> {
    let mut reader = Reader(bytes);
    if reader.u8()? != VERSION {
        return None;
    }
    let flags = reader.u8()?;
    let margin = (flags & MARGIN_KNOWN != 0).then_some(if flags & MARGIN_ISOLATED != 0 {
        MarginType::Isolated
    } else {
        MarginType::Cross
    });
    let mut sources = HashMap::new();
    for _ in 0..reader.u32()? {
        let id = reader.string()?;
        let mut text = || reader.string();
        let mut source = Source {
            inst_id: text()?,
            margin_mode: text()?,
            position_side: text()?,
            order_id: text()?,
            tpsl_id: text()?,
            algo_id: text()?,
            trigger_price_type: text()?,
            tp_type: text()?,
            sl_type: text()?,
            ..Source::default()
        };
        source.order_price = reader.f64()?;
        source.tp = reader.f64()?;
        source.sl = reader.f64()?;
        source.tp_order_price = reader.f64()?;
        source.sl_order_price = reader.f64()?;
        source.execution_baseline = (reader.f64()?, optional(reader.f64()?));
        sources.insert(id, source);
    }
    let mut fills = Vec::new();
    for _ in 0..reader.u32()? {
        let id = reader.string()?;
        let qty = reader.f64()?;
        fills.push((id, (qty, optional(reader.f64()?))));
    }
    reader
        .0
        .is_empty()
        .then_some((flags & MODE_LOADED != 0, margin, sources, fills))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_state_round_trips_and_refuses_other_versions_and_trailing_bytes() {
        let mut account = Account::new();
        account.mode_loaded = true;
        account.margin = Some(MarginType::Isolated);
        account.sources.insert(
            "algo-1".into(),
            Source {
                inst_id: "BTC-USDT".into(),
                margin_mode: "isolated".into(),
                position_side: "long".into(),
                algo_id: "algo-1".into(),
                trigger_price_type: "mark".into(),
                order_price: -1.,
                tp: 70_000.,
                sl: f64::NAN,
                execution_baseline: (0.5, Some(30_000.)),
                ..Source::default()
            },
        );
        account.store_execution_prefix("order-1", 2., None);
        let bytes = account.export_venue_state();

        let mut copy = Account::new();
        assert!(copy.apply_venue_state(&bytes));
        assert!(copy.mode_loaded);
        assert_eq!(copy.margin, Some(MarginType::Isolated));
        let source = &copy.sources["algo-1"];
        assert_eq!(
            (source.inst_id.as_str(), source.algo_id.as_str()),
            ("BTC-USDT", "algo-1")
        );
        assert_eq!(
            (source.tp, source.execution_baseline),
            (70_000., (0.5, Some(30_000.)))
        );
        assert!(source.sl.is_nan());
        assert_eq!(copy.fills["order-1"], (2., None));
        assert_eq!(
            copy.export_venue_state(),
            bytes,
            "re-export is byte-identical"
        );

        let mut other = bytes.clone();
        other[0] = 2;
        assert!(!copy.apply_venue_state(&other));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(!copy.apply_venue_state(&trailing));
        assert!(!copy.apply_venue_state(&bytes[..bytes.len() - 1]));
        assert!(copy.mode_loaded, "a refused state leaves the applied one");
    }
}

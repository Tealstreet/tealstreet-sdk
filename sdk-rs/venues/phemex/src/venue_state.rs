//! Phemex's VENUE_STATE for frame-fed engines: what the write path reads that
//! the store does not hold. Per-symbol settings contexts (position mode, margin
//! type, side leverages) build every order and settings request; confirmed
//! modes and order position sides pick `posSide`; the source's lost clOrdIDs and
//! unreconciled fills keep a reused id or an amend from going out too early.
//!
//! Encoding (version 1): u8 version; u32 context count, then per context the
//! symbol, u8 mode, u8 margin, f64 long and f64 short leverage; u32 confirmed
//! count, then symbol and u8 mode; u32 side count, then order id and posSide;
//! u32 lost count, then clOrdID; u8 unreconciled fills. Text is a u16 length
//! and its UTF-8 bytes; numbers are little-endian.
use crate::account_wire::Context;
use safe_cex_engine::model::{MarginType, PositionMode};
use std::collections::{BTreeMap, BTreeSet};

const VERSION: u8 = 1;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct VenueState {
    pub contexts: BTreeMap<String, Context>,
    pub confirmed: BTreeMap<String, PositionMode>,
    pub sides: BTreeMap<String, String>,
    pub lost: BTreeSet<String>,
    pub unreconciled_fills: bool,
}

fn mode_code(mode: PositionMode) -> u8 {
    match mode {
        PositionMode::Oneway => 0,
        PositionMode::Hedged => 1,
    }
}

fn mode(code: u8) -> Option<PositionMode> {
    match code {
        0 => Some(PositionMode::Oneway),
        1 => Some(PositionMode::Hedged),
        _ => None,
    }
}

fn margin(code: u8) -> Option<MarginType> {
    match code {
        0 => Some(MarginType::Cross),
        1 => Some(MarginType::Isolated),
        _ => None,
    }
}

fn text(out: &mut Vec<u8>, value: &str) {
    let bytes = &value.as_bytes()[..value.len().min(u16::MAX as usize)];
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.at..self.at + n)?;
        self.at += n;
        Some(slice)
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
    fn text(&mut self) -> Option<String> {
        let length = u16::from_le_bytes(self.take(2)?.try_into().ok()?) as usize;
        Some(std::str::from_utf8(self.take(length)?).ok()?.to_owned())
    }
}

impl VenueState {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![VERSION];
        out.extend_from_slice(&(self.contexts.len() as u32).to_le_bytes());
        for (symbol, context) in &self.contexts {
            text(&mut out, symbol);
            out.push(mode_code(context.mode));
            out.push(u8::from(context.margin_type == MarginType::Isolated));
            out.extend_from_slice(&context.long_leverage.to_le_bytes());
            out.extend_from_slice(&context.short_leverage.to_le_bytes());
        }
        out.extend_from_slice(&(self.confirmed.len() as u32).to_le_bytes());
        for (symbol, mode) in &self.confirmed {
            text(&mut out, symbol);
            out.push(mode_code(*mode));
        }
        out.extend_from_slice(&(self.sides.len() as u32).to_le_bytes());
        for (id, side) in &self.sides {
            text(&mut out, id);
            text(&mut out, side);
        }
        out.extend_from_slice(&(self.lost.len() as u32).to_le_bytes());
        for id in &self.lost {
            text(&mut out, id);
        }
        out.push(u8::from(self.unreconciled_fills));
        out
    }

    /// `None` for another version or malformed bytes: the caller keeps its state.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader { bytes, at: 0 };
        if r.u8()? != VERSION {
            return None;
        }
        let mut state = Self::default();
        for _ in 0..r.u32()? {
            let symbol = r.text()?;
            let context = Context {
                mode: mode(r.u8()?)?,
                margin_type: margin(r.u8()?)?,
                long_leverage: r.f64()?,
                short_leverage: r.f64()?,
            };
            state.contexts.insert(symbol, context);
        }
        for _ in 0..r.u32()? {
            let symbol = r.text()?;
            state.confirmed.insert(symbol, mode(r.u8()?)?);
        }
        for _ in 0..r.u32()? {
            let id = r.text()?;
            state.sides.insert(id, r.text()?);
        }
        for _ in 0..r.u32()? {
            state.lost.insert(r.text()?);
        }
        state.unreconciled_fills = r.u8()? != 0;
        (r.at == bytes.len()).then_some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_state_round_trips_and_refuses_another_version_or_trailing_bytes() {
        let mut state = VenueState::default();
        state.contexts.insert(
            "BTCUSDT".into(),
            Context {
                mode: PositionMode::Hedged,
                margin_type: MarginType::Isolated,
                long_leverage: 10.,
                short_leverage: -5.,
            },
        );
        state
            .confirmed
            .insert("ETHUSDT".into(), PositionMode::Oneway);
        state.sides.insert("o-1".into(), "Long".into());
        state.lost.insert("c-1".into());
        state.unreconciled_fills = true;
        let bytes = state.encode();
        assert_eq!(VenueState::decode(&bytes), Some(state));
        let mut other = bytes.clone();
        other[0] = 2;
        assert_eq!(VenueState::decode(&other), None);
        assert_eq!(VenueState::decode(&bytes[..bytes.len() - 1]), None);
        let mut longer = bytes.clone();
        longer.push(0);
        assert_eq!(VenueState::decode(&longer), None);
    }
}

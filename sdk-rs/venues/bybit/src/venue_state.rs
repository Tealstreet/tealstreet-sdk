//! Bybit's VENUE_STATE for frame-fed engines: what the write path reads that
//! the store does not hold. Per-symbol hedge mode picks `positionIdx` (a flat
//! hedge-mode symbol has no position row to say so), and position-bound TP/SL
//! ids route a stop amend to the position's trading stop.
//!
//! Encoding (version 1): u8 version; u32 mode count, then per mode u8
//! category, u8 hedged, u16 length, symbol bytes; u32 bound count, then per
//! id u16 length, id bytes.
use crate::paths::Category;
use std::collections::{BTreeMap, BTreeSet};

const VERSION: u8 = 1;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct VenueState {
    pub modes: BTreeMap<(u8, String), bool>,
    pub bound: BTreeSet<String>,
}

pub(crate) fn category_code(category: Category) -> u8 {
    match category {
        Category::Linear => 0,
        Category::Inverse => 1,
        Category::Spot => 2,
        Category::Option => 3,
    }
}

fn text(out: &mut Vec<u8>, value: &str) {
    let bytes = &value.as_bytes()[..value.len().min(u16::MAX as usize)];
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

impl VenueState {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![VERSION];
        out.extend_from_slice(&(self.modes.len() as u32).to_le_bytes());
        for ((category, symbol), hedged) in &self.modes {
            out.push(*category);
            out.push(u8::from(*hedged));
            text(&mut out, symbol);
        }
        out.extend_from_slice(&(self.bound.len() as u32).to_le_bytes());
        for id in &self.bound {
            text(&mut out, id);
        }
        out
    }

    /// `None` for another version or malformed bytes: the caller keeps its state.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut at = 0;
        let mut take = |n: usize| {
            let slice = bytes.get(at..at + n)?;
            at += n;
            Some(slice)
        };
        if take(1)? != [VERSION] {
            return None;
        }
        let mut state = Self::default();
        let count = u32::from_le_bytes(take(4)?.try_into().ok()?);
        for _ in 0..count {
            let head = take(2)?;
            let (category, hedged) = (head[0], head[1] != 0);
            let length = u16::from_le_bytes(take(2)?.try_into().ok()?) as usize;
            let symbol = std::str::from_utf8(take(length)?).ok()?.to_owned();
            state.modes.insert((category, symbol), hedged);
        }
        let count = u32::from_le_bytes(take(4)?.try_into().ok()?);
        for _ in 0..count {
            let length = u16::from_le_bytes(take(2)?.try_into().ok()?) as usize;
            state
                .bound
                .insert(std::str::from_utf8(take(length)?).ok()?.to_owned());
        }
        Some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_state_round_trips_and_refuses_another_version() {
        let mut state = VenueState::default();
        state.modes.insert((0, "BTCUSDT".into()), true);
        state.modes.insert((1, "BTCUSD".into()), false);
        state.bound.insert("stop-1".into());
        let bytes = state.encode();
        assert_eq!(VenueState::decode(&bytes), Some(state));
        let mut other = bytes.clone();
        other[0] = 2;
        assert_eq!(VenueState::decode(&other), None);
        assert_eq!(VenueState::decode(&bytes[..bytes.len() - 1]), None);
    }
}

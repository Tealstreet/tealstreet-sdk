//! Bybit's VENUE_STATE for frame-fed engines: what the write path reads that
//! the store does not hold. Per-symbol hedge mode picks `positionIdx` (a flat
//! hedge-mode symbol has no position row to say so), and position-bound TP/SL
//! ids route a stop amend to the position's trading stop, while trailing
//! take-profit ids refuse an amend.
//!
//! Encoding (version 1): u8 version; u32 mode count, then per mode u8
//! category, u8 hedged, u16 length, symbol bytes; u32 bound count, then per
//! id u16 length, id bytes; then, optionally, u32 trailing take-profit count
//! and per id u16 length, id bytes. The server and the browser may run
//! different builds: an older reader ignores the tail, a newer one reads none.
use crate::paths::Category;
use std::collections::{BTreeMap, BTreeSet};

const VERSION: u8 = 1;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct VenueState {
    pub modes: BTreeMap<(u8, String), bool>,
    pub bound: BTreeSet<String>,
    pub trailing_profit: BTreeSet<String>,
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
        for ids in [&self.bound, &self.trailing_profit] {
            out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
            for id in ids {
                text(&mut out, id);
            }
        }
        out
    }

    /// `None` for another version or malformed bytes: the caller keeps its state.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut at = 0;
        if take(bytes, &mut at, 1)? != [VERSION] {
            return None;
        }
        let mut state = Self::default();
        let count = u32::from_le_bytes(take(bytes, &mut at, 4)?.try_into().ok()?);
        for _ in 0..count {
            let head = take(bytes, &mut at, 2)?;
            let (category, hedged) = (head[0], head[1] != 0);
            let symbol = text_at(bytes, &mut at)?;
            state.modes.insert((category, symbol), hedged);
        }
        state.bound = ids_at(bytes, &mut at)?;
        if at < bytes.len() {
            state.trailing_profit = ids_at(bytes, &mut at)?;
        }
        Some(state)
    }
}

fn take<'a>(bytes: &'a [u8], at: &mut usize, n: usize) -> Option<&'a [u8]> {
    let slice = bytes.get(*at..*at + n)?;
    *at += n;
    Some(slice)
}

fn text_at(bytes: &[u8], at: &mut usize) -> Option<String> {
    let length = u16::from_le_bytes(take(bytes, at, 2)?.try_into().ok()?) as usize;
    Some(
        std::str::from_utf8(take(bytes, at, length)?)
            .ok()?
            .to_owned(),
    )
}

fn ids_at(bytes: &[u8], at: &mut usize) -> Option<BTreeSet<String>> {
    let count = u32::from_le_bytes(take(bytes, at, 4)?.try_into().ok()?);
    (0..count).map(|_| text_at(bytes, at)).collect()
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
        state.bound.insert("trailing-tp".into());
        state.trailing_profit.insert("trailing-tp".into());
        let bytes = state.encode();
        assert_eq!(VenueState::decode(&bytes), Some(state));
        let mut other = bytes.clone();
        other[0] = 2;
        assert_eq!(VenueState::decode(&other), None);
        assert_eq!(VenueState::decode(&bytes[..bytes.len() - 1]), None);
        // An older writer's state, with no trailing tail, still reads.
        let older = VenueState {
            trailing_profit: BTreeSet::new(),
            ..VenueState::decode(&bytes).unwrap()
        };
        let without_tail = &bytes[..bytes.len() - (4 + 2 + "trailing-tp".len())];
        assert_eq!(VenueState::decode(without_tail), Some(older));
    }
}

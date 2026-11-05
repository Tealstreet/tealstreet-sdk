//! VENUE_STATE v1: the write-path facts a frame-fed Bitget engine needs and the
//! store does not hold. `[1, flags]`, then the plan types and the trailing
//! states by order id, sorted so equal state always encodes to equal bytes.
//! Flags: bit 0 the account type is detected, bit 1 it is UTA.
use std::collections::HashMap;

const VERSION: u8 = 1;
const DETECTED: u8 = 1;
const UTA: u8 = 2;

pub(crate) struct VenueState {
    pub detected: bool,
    pub uta: bool,
    pub plans: HashMap<String, String>,
    pub trailing: HashMap<String, (String, f64)>,
}

pub(crate) fn encode(
    detected: bool,
    uta: bool,
    plans: &HashMap<String, String>,
    trailing: &HashMap<String, (String, f64)>,
) -> Vec<u8> {
    let flags = if detected { DETECTED } else { 0 } | if uta { UTA } else { 0 };
    let mut out = vec![VERSION, flags];
    let mut plans: Vec<_> = plans.iter().collect();
    plans.sort_unstable_by_key(|(id, _)| *id);
    out.extend_from_slice(&(plans.len() as u32).to_le_bytes());
    for (id, plan) in plans {
        text(&mut out, id);
        text(&mut out, plan);
    }
    let mut trailing: Vec<_> = trailing.iter().collect();
    trailing.sort_unstable_by_key(|(id, _)| *id);
    out.extend_from_slice(&(trailing.len() as u32).to_le_bytes());
    for (id, (kind, variance)) in trailing {
        text(&mut out, id);
        text(&mut out, kind);
        out.extend_from_slice(&variance.to_le_bytes());
    }
    out
}

/// `None` for another version or malformed bytes: the engine keeps its state.
pub(crate) fn decode(bytes: &[u8]) -> Option<VenueState> {
    let mut reader = Reader(bytes);
    if reader.take(1)? != [VERSION] {
        return None;
    }
    let flags = reader.take(1)?[0];
    let mut plans = HashMap::new();
    for _ in 0..reader.count()? {
        let id = reader.text()?;
        plans.insert(id, reader.text()?);
    }
    let mut trailing = HashMap::new();
    for _ in 0..reader.count()? {
        let id = reader.text()?;
        let kind = reader.text()?;
        let variance = f64::from_le_bytes(reader.take(8)?.try_into().ok()?);
        trailing.insert(id, (kind, variance));
    }
    reader.0.is_empty().then_some(VenueState {
        detected: flags & DETECTED != 0,
        uta: flags & UTA != 0,
        plans,
        trailing,
    })
}

fn text(out: &mut Vec<u8>, value: &str) {
    let bytes = &value.as_bytes()[..value.len().min(u16::MAX as usize)];
    out.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    out.extend_from_slice(bytes);
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        if self.0.len() < length {
            return None;
        }
        let (head, rest) = self.0.split_at(length);
        self.0 = rest;
        Some(head)
    }
    fn count(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn text(&mut self) -> Option<String> {
        let length = u16::from_le_bytes(self.take(2)?.try_into().ok()?) as usize;
        String::from_utf8(self.take(length)?.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_encodes_equal_state_identically() {
        let plans = HashMap::from([
            ("b".into(), "pos_loss".into()),
            ("a".into(), "normal_plan".into()),
        ]);
        let trailing = HashMap::from([("t".into(), ("percentage".into(), 0.5))]);
        let bytes = encode(true, true, &plans, &trailing);
        assert_eq!(bytes, encode(true, true, &plans.clone(), &trailing.clone()));
        let state = decode(&bytes).unwrap();
        assert!(state.detected && state.uta);
        assert_eq!(state.plans, plans);
        assert_eq!(state.trailing, trailing);
    }

    #[test]
    fn refuses_another_version_and_truncated_bytes() {
        let bytes = encode(true, false, &HashMap::new(), &HashMap::new());
        assert!(decode(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]).is_none());
        assert!(decode(&bytes[..bytes.len() - 1]).is_none());
        let state = decode(&bytes).unwrap();
        assert!(state.detected && !state.uta);
    }
}

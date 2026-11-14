//! Lighter's VENUE_STATE for frame-fed engines: what the write path reads that
//! the store does not hold. The account index the data source resolved by its
//! account lookup signs every transaction and seeds the nonce; a known account
//! tier refuses partner-attributed placements a Standard account cannot make.
//! The API key index and signer stay the frame-fed engine's own credential.
//!
//! Encoding (version 1): u8 version; u8 account present; u64 account index
//! (zero when absent); u8 tier (0 unknown, 1 standard, 2 premium, 3 plus).
//! Version 2 appends JSON canonical native-order strings, never MARK receipts.
const VERSION: u8 = 1;
const LENGTH: usize = 11;

#[derive(Debug, Default, PartialEq)]
pub(crate) struct VenueState {
    pub account_index: Option<u64>,
    pub tier: u8,
    pub originals: Vec<String>,
}

impl VenueState {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(LENGTH);
        out.push(if self.originals.is_empty() {
            VERSION
        } else {
            2
        });
        out.push(u8::from(self.account_index.is_some()));
        out.extend_from_slice(&self.account_index.unwrap_or(0).to_le_bytes());
        out.push(self.tier);
        if !self.originals.is_empty() {
            out.extend(serde_json::to_vec(&self.originals).expect("native provenance strings"));
        }
        out
    }

    /// `None` for another version or malformed bytes: the caller keeps its state.
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < LENGTH || !matches!(bytes[0], VERSION | 2) || bytes[1] > 1 || bytes[10] > 3
        {
            return None;
        }
        let originals = if bytes[0] == VERSION {
            if bytes.len() != LENGTH {
                return None;
            }
            Vec::new()
        } else {
            serde_json::from_slice(&bytes[LENGTH..]).ok()?
        };
        let index = u64::from_le_bytes(bytes[2..10].try_into().ok()?);
        Some(Self {
            account_index: (bytes[1] == 1).then_some(index),
            tier: bytes[10],
            originals,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_state_round_trips_and_refuses_another_version() {
        for state in [
            VenueState {
                account_index: Some(718_895),
                tier: 3,
                ..VenueState::default()
            },
            VenueState::default(),
        ] {
            let bytes = state.encode();
            assert_eq!(VenueState::decode(&bytes), Some(state));
        }
        let bytes = VenueState {
            account_index: Some(7),
            tier: 1,
            ..VenueState::default()
        }
        .encode();
        let mut other = bytes.clone();
        other[0] = 2;
        assert_eq!(VenueState::decode(&other), None);
        assert_eq!(VenueState::decode(&bytes[..bytes.len() - 1]), None);
        let mut tier = bytes.clone();
        tier[10] = 4;
        assert_eq!(VenueState::decode(&tier), None);
    }
    #[test]
    fn v2_native_provenance_is_additive_without_a_mark_receipt() {
        let state = VenueState {
            account_index: Some(7),
            tier: 3,
            originals: vec![r#"{"order_index":42,"trigger_price":"80"}"#.into()],
        };
        let bytes = state.encode();
        assert_eq!(bytes[0], 2);
        assert_eq!(VenueState::decode(&bytes), Some(state));
        assert!(VenueState::decode(&bytes[..bytes.len() - 1]).is_none());
        let mut unknown = bytes.clone();
        unknown[0] = 3;
        assert!(VenueState::decode(&unknown).is_none());
        let mut v1 = bytes[..LENGTH].to_vec();
        v1[0] = 1;
        assert!(VenueState::decode(&v1).unwrap().originals.is_empty());
    }
}

//! Bounded host tokens only; the live Plan registry owns group authority.
use super::{Error, HttpRelease, Permit, Preparation};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupPermit {
    members: Vec<Permit>,
}
impl GroupPermit {
    pub(crate) fn new(members: Vec<Permit>) -> Result<Self, Error> {
        if !(2..=256).contains(&members.len()) {
            return Err(Error::Invalid);
        }
        if members.iter().enumerate().any(|(i, p)| {
            members[..i]
                .iter()
                .any(|other| other.attempt().request == p.attempt().request)
        }) {
            return Err(Error::Duplicate);
        }
        Ok(Self { members })
    }
    pub fn members(&self) -> &[Permit] {
        &self.members
    }
    pub fn to_wire(&self) -> String {
        let mut text = String::from("g1");
        for member in &self.members {
            text.push(';');
            text.push_str(&member.to_wire());
        }
        text
    }
    pub fn from_wire(text: &str) -> Result<Self, Error> {
        if text.len() > 2 + 256 * 83 {
            return Err(Error::Invalid);
        }
        let mut parts = text.split(';');
        if parts.next() != Some("g1") {
            return Err(Error::Invalid);
        }
        let mut members = Vec::new();
        for part in parts {
            if members.len() == 256 {
                return Err(Error::Capacity);
            }
            members.try_reserve(1).map_err(|_| Error::Capacity)?;
            members.push(Permit::from_wire(part)?);
        }
        Self::new(members)
    }
}

/// A missing packet explicitly retains a sibling's already-bound resource.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupPacket {
    pub preparation: Preparation,
    pub request: Option<crate::HttpRequest>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroupRelease {
    pub release: HttpRelease,
    pub send_before: f64,
}

pub enum GroupClaim {
    WaitUntil(f64),
    Released(Vec<GroupRelease>),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn widest_member_tokens_round_trip_at_the_maximum_group_size() {
        let members: Vec<_> = (0..256)
            .map(|offset| {
                Permit::from_wire(&format!(
                    "r1:{}:{}:{}:{}",
                    crate::resource_ids::MAX_RESOURCE_ID - offset,
                    u64::MAX,
                    u64::MAX,
                    u64::MAX
                ))
                .unwrap()
            })
            .collect();
        let group = GroupPermit::new(members).unwrap();
        let wire = group.to_wire();
        assert!(wire.len() <= 2 + 256 * 83);
        assert_eq!(GroupPermit::from_wire(&wire), Ok(group));
    }
    #[test]
    fn duplicate_requests_cannot_alias_a_second_member() {
        assert_eq!(
            GroupPermit::from_wire("g1;r1:1:2:3:4;r1:1:5:6:7"),
            Err(Error::Duplicate)
        );
    }
    #[test]
    fn malformed_and_noncanonical_group_tokens_are_refused() {
        for text in [
            "g1",
            "g1;r1:1:2:3:4",
            "g1;r1:1:2:3:4;r1:2:2:3:4;",
            "g1;r1:01:2:3:4;r1:2:2:3:4",
            "g1;r1:1:2:3:4;r1:2:2:3:4 junk",
            "g2;r1:1:2:3:4;r1:2:2:3:4",
        ] {
            assert!(GroupPermit::from_wire(text).is_err());
        }
    }
    #[test]
    fn more_than_256_members_is_capacity_refused() {
        let mut wire = String::from("g1");
        for id in 1..=257 {
            wire.push_str(&format!(";r1:{id}:1:0:1"));
        }
        assert_eq!(GroupPermit::from_wire(&wire), Err(Error::Capacity));
    }
}

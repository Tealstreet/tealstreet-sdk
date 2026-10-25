//! Bounded response custody shared by active writes and retired late ACKs.
//! Contains no outbound request, generic venue continuation or signing recipe.
use super::{Attempt, Error, HttpRelease, Owner};
use crate::RequestId;

pub(crate) const CAPACITY: usize = 256;
pub(crate) const BODY_LIMIT: usize = 256 * 1024;
pub(crate) const HEADER_LIMIT: usize = 16 * 1024;
pub(crate) const DATA_LIMIT: usize = 8 * 1024;
const TOTAL_LIMIT: usize = 4 * 1024 * 1024;
const GRACE_MS: f64 = 30_000.;

#[cfg(all(test, feature = "http-mutation-barriers"))]
#[path = "http_group_storage_tests.rs"]
mod group_storage_tests;

/// Native correlation facts for the late codec, never an outbound request or
/// credentials. This value is neither logged nor serialized onto the host wire.
pub struct ReconciliationData(String);

impl ReconciliationData {
    pub fn new(data: String) -> Result<Self, Error> {
        if data.capacity() > DATA_LIMIT {
            return Err(Error::Capacity);
        }
        Ok(Self(data))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ReconciliationData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReconciliationData(<redacted>)")
    }
}

pub(crate) struct Record {
    pub owner: Owner,
    pub leg: u64,
    pub attempt: Attempt,
    pub release: Option<HttpRelease>,
    pub order_revision: u64,
    pub write_revision: u64,
    pub data: ReconciliationData,
    pub late: bool,
    expires_at: f64,
    pub status: Option<u16>,
    pub headers: String,
    pub body: Vec<u8>,
}

impl Record {
    pub(crate) fn new(
        owner: Owner,
        leg: u64,
        attempt: Attempt,
        deadline: f64,
        data: ReconciliationData,
    ) -> Self {
        Self {
            owner,
            leg,
            attempt,
            release: None,
            order_revision: 0,
            write_revision: 0,
            data,
            late: false,
            expires_at: deadline + GRACE_MS,
            status: None,
            headers: String::new(),
            body: Vec::new(),
        }
    }

    fn bytes(&self) -> usize {
        // Account for fixed metadata and allocated byte capacities, not just
        // used lengths. Body/header growth below uses exact bounded reserves.
        std::mem::size_of::<Self>()
            + self.data.0.capacity()
            + self.headers.capacity()
            + self.body.capacity()
    }
}

enum Slot {
    Stored(Box<Record>),
    /// Moving a complete record into a codec callback must not free capacity
    /// until the callback and its response bytes have actually been dropped.
    Delivering(usize),
}

#[derive(Default)]
pub(crate) struct Responses {
    slots: crate::IdMap<RequestId, Slot>,
    bytes: usize,
}

impl Responses {
    pub(crate) fn prune(&mut self, now: f64) {
        if !now.is_finite() {
            return;
        }
        let slots = &mut self.slots;
        let bytes = &mut self.bytes;
        slots.retain(|_, slot| {
            let Slot::Stored(record) = slot else {
                return true;
            };
            if now >= record.expires_at {
                *bytes -= record.bytes();
                false
            } else {
                true
            }
        });
    }

    /// Called before read-scheduler admission. The caller owns a fully bounded
    /// record already; inserting it reserves all fixed/capture bytes atomically.
    pub(crate) fn insert(&mut self, record: Record, now: f64) -> Result<(), Error> {
        self.prune(now);
        if !now.is_finite() || now >= record.expires_at {
            return Err(Error::TimedOut);
        }
        if self.slots.contains_key(&record.attempt.request) {
            return Err(Error::Duplicate);
        }
        let size = record.bytes();
        if self.slots.len() >= CAPACITY || size > TOTAL_LIMIT - self.bytes {
            return Err(Error::Capacity);
        }
        self.bytes += size;
        self.slots
            .insert(record.attempt.request, Slot::Stored(Box::new(record)));
        Ok(())
    }

    /// Prepare every record and container slot before publishing any member.
    /// The caller completes one synchronous release transaction after success.
    #[cfg(feature = "http-mutation-barriers")]
    pub(crate) fn insert_group(&mut self, records: Vec<Record>, now: f64) -> Result<(), Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        let mut prepared = Vec::new();
        prepared
            .try_reserve_exact(records.len())
            .map_err(|_| Error::Capacity)?;
        let mut bytes = 0usize;
        for record in records {
            if now >= record.expires_at {
                return Err(Error::TimedOut);
            }
            let request = record.attempt.request;
            if self.slots.contains_key(&request) || prepared.iter().any(|(id, _)| *id == request) {
                return Err(Error::Duplicate);
            }
            bytes = bytes.checked_add(record.bytes()).ok_or(Error::Capacity)?;
            prepared.push((request, Slot::Stored(Box::new(record))));
        }
        if self
            .slots
            .len()
            .checked_add(prepared.len())
            .is_none_or(|n| n > CAPACITY)
            || bytes > TOTAL_LIMIT - self.bytes
        {
            return Err(Error::Capacity);
        }
        self.slots
            .try_reserve(prepared.len())
            .map_err(|_| Error::Capacity)?;
        self.bytes += bytes;
        for (request, record) in prepared {
            self.slots.insert(request, record);
        }
        Ok(())
    }

    pub(crate) fn get_mut(&mut self, id: RequestId) -> Option<&mut Record> {
        match self.slots.get_mut(&id)? {
            Slot::Stored(record) => Some(record),
            Slot::Delivering(_) => None,
        }
    }

    pub(crate) fn contains(&self, id: RequestId) -> bool {
        self.slots.contains_key(&id)
    }

    pub(crate) fn remove(&mut self, id: RequestId) {
        if let Some(slot) = self.slots.remove(&id) {
            self.bytes -= match slot {
                Slot::Stored(record) => record.bytes(),
                Slot::Delivering(size) => size,
            };
        }
    }

    fn discard_stored(&mut self, id: RequestId) {
        if matches!(self.slots.get(&id), Some(Slot::Stored(_))) {
            self.remove(id);
        }
    }

    pub(crate) fn retire(&mut self, id: RequestId) {
        if let Some(record) = self.get_mut(id) {
            record.late = true;
        }
    }

    pub(crate) fn start(&mut self, id: RequestId, status: u16, headers: &str) -> Result<(), Error> {
        let result = (|| {
            let Some(Slot::Stored(record)) = self.slots.get_mut(&id) else {
                return Err(Error::Revoked);
            };
            if record.status.is_some() {
                return Err(Error::Duplicate);
            }
            if headers.len() > HEADER_LIMIT || headers.len() > TOTAL_LIMIT - self.bytes {
                return Err(Error::Capacity);
            }
            // Exact allocation avoids doubling the aggregate footprint at a
            // growth boundary. No previous header allocation is retained.
            record.headers = headers.into();
            record.status = Some(status);
            self.bytes += record.headers.capacity();
            Ok(())
        })();
        if result.is_err() {
            self.discard_stored(id);
        }
        result
    }

    pub(crate) fn chunk(&mut self, id: RequestId, data: &[u8]) -> Result<(), Error> {
        let result = (|| {
            let Some(Slot::Stored(record)) = self.slots.get_mut(&id) else {
                return Err(Error::Revoked);
            };
            if record.status.is_none() {
                return Err(Error::State);
            }
            let needed = record.body.len().saturating_add(data.len());
            // Include both the old allocation and its replacement in the
            // temporary peak if growing requires copying the existing buffer.
            let allocation = if needed > record.body.capacity() {
                needed
            } else {
                0
            };
            if needed > BODY_LIMIT || allocation > TOTAL_LIMIT - self.bytes {
                return Err(Error::Capacity);
            }
            let before = record.body.capacity();
            record.body.reserve_exact(data.len());
            let growth = record.body.capacity() - before;
            // The allocator may round a reservation. Refuse before copying
            // bytes if its real capacity would exceed the shared bound.
            if growth > TOTAL_LIMIT - self.bytes {
                // Account for the allocation so the removal below is exact.
                self.bytes += growth;
                return Err(Error::Capacity);
            }
            self.bytes += growth;
            record.body.extend_from_slice(data);
            Ok(())
        })();
        if result.is_err() {
            self.discard_stored(id);
        }
        result
    }

    /// One-use completion; a reservation remains until `remove` after the
    /// callback. Invalid UTF-8 consumes the state just like an oversize body.
    pub(crate) fn complete(&mut self, id: RequestId) -> Result<Box<Record>, Error> {
        let Some(Slot::Stored(record)) = self.slots.get_mut(&id) else {
            return Err(Error::Revoked);
        };
        if record.status.is_none() || std::str::from_utf8(&record.body).is_err() {
            self.remove(id);
            return Err(Error::Invalid);
        }
        let size = record.bytes();
        let Slot::Stored(record) = self
            .slots
            .insert(id, Slot::Delivering(size))
            .expect("stored completion exists")
        else {
            unreachable!()
        };
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u32, data: usize) -> Record {
        let attempt = Attempt {
            request: RequestId(u64::from(id)),
            generation: 1,
        };
        Record::new(
            Owner::Command { id, generation: 1 },
            1,
            attempt,
            100.,
            ReconciliationData::new("x".repeat(data)).unwrap(),
        )
    }

    #[test]
    fn limits_are_inclusive_and_overflow_consumes_the_record() {
        assert!(ReconciliationData::new("x".repeat(DATA_LIMIT)).is_ok());
        assert!(ReconciliationData::new("x".repeat(DATA_LIMIT + 1)).is_err());
        let mut table = Responses::default();
        table.insert(record(1, DATA_LIMIT), 1.).unwrap();
        table
            .start(RequestId(1), 200, &"h".repeat(HEADER_LIMIT))
            .unwrap();
        table.chunk(RequestId(1), &vec![b'x'; BODY_LIMIT]).unwrap();
        assert_eq!(table.chunk(RequestId(1), b"x"), Err(Error::Capacity));
        assert_eq!(table.bytes, 0);
        table.insert(record(2, 0), 1.).unwrap();
        assert_eq!(
            table.start(RequestId(2), 200, &"h".repeat(HEADER_LIMIT + 1)),
            Err(Error::Capacity)
        );
        assert_eq!(table.bytes, 0);
    }

    #[test]
    fn completion_keeps_capacity_until_response_is_dropped_and_is_one_use() {
        let mut table = Responses::default();
        for id in 0..CAPACITY as u32 {
            table.insert(record(id, 0), 1.).unwrap();
        }
        table.start(RequestId(0), 200, "").unwrap();
        table.chunk(RequestId(0), b"{}").unwrap();
        let done = table.complete(RequestId(0)).unwrap();
        assert_eq!(
            table.start(RequestId(0), 200, "duplicate"),
            Err(Error::Revoked)
        );
        assert_eq!(table.chunk(RequestId(0), b"duplicate"), Err(Error::Revoked));
        assert_eq!(table.insert(record(256, 0), 1.), Err(Error::Capacity));
        assert!(table.complete(RequestId(0)).is_err());
        assert_eq!(&done.body, b"{}");
        drop(done);
        table.remove(RequestId(0));
        table.insert(record(256, 0), 1.).unwrap();
    }

    #[test]
    fn mid_body_retirement_keeps_all_bytes_and_frozen_expiry() {
        let mut table = Responses::default();
        table.insert(record(1, 0), 1.).unwrap();
        table.start(RequestId(1), 200, "headers").unwrap();
        table.chunk(RequestId(1), b"{\"id\":").unwrap();
        table.retire(RequestId(1));
        table.chunk(RequestId(1), b"42}").unwrap();
        let done = table.complete(RequestId(1)).unwrap();
        assert!(done.late);
        assert_eq!(&done.body, b"{\"id\":42}");
        drop(done);
        table.remove(RequestId(1));
        table.insert(record(2, 0), 1.).unwrap();
        table.retire(RequestId(2));
        table.prune(30_099.);
        assert!(table.contains(RequestId(2)));
        table.start(RequestId(2), 200, "").unwrap();
        table.chunk(RequestId(2), b"{}").unwrap();
        table.prune(30_100.);
        assert!(!table.contains(RequestId(2)));
        assert_eq!(table.bytes, 0);
    }

    #[test]
    fn aggregate_body_exhaustion_and_invalid_utf8_are_closed() {
        let mut table = Responses::default();
        let body = vec![b'x'; BODY_LIMIT];
        for id in 0..16 {
            table.insert(record(id, DATA_LIMIT), 1.).unwrap();
            table.start(RequestId(u64::from(id)), 200, "").unwrap();
            if id == 15 {
                assert_eq!(
                    table.chunk(RequestId(u64::from(id)), &body),
                    Err(Error::Capacity)
                );
            } else {
                table.chunk(RequestId(u64::from(id)), &body).unwrap();
            }
        }
        assert!(table.bytes <= TOTAL_LIMIT);
        table.insert(record(100, 0), 1.).unwrap();
        table.start(RequestId(100), 200, "").unwrap();
        table.chunk(RequestId(100), &[255]).unwrap();
        assert!(table.complete(RequestId(100)).is_err());
        assert!(!table.contains(RequestId(100)));
    }
}

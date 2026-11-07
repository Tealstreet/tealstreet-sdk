//! Maps keyed by engine-issued integer identities.
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Only for keys the engine itself issues (RequestId, TimerId, reserved
/// context/root IDs): no peer, host or caller chooses them, so SipHash's
/// HashDoS resistance buys nothing. Caller-chosen keys (CommandId, Owner,
/// venue or wire IDs) must keep `RandomState`.
#[derive(Default, Clone, Copy)]
pub(crate) struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }
    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

pub(crate) type IdMap<K, V> = HashMap<K, V, BuildHasherDefault<IdHasher>>;

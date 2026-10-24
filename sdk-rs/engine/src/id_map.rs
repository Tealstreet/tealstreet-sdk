//! Maps keyed by engine-issued integer identities.
use std::collections::HashMap;
#[cfg(feature = "order-locks")]
use std::collections::HashSet;
use std::hash::{BuildHasherDefault, Hasher};

/// Engine-issued IDs never come from a peer, so they need no HashDoS-resistant
/// hashing; one multiply per word is smaller and faster than SipHash.
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
/// A set of engine-issued IDs, hashed like `IdMap`.
#[cfg(feature = "order-locks")]
pub(crate) type IdSet<K> = HashSet<K, BuildHasherDefault<IdHasher>>;

//! Streaming reflected IEEE CRC32. Venues own their checksum input formatting.

static TABLE: [u32; 256] = table();

const fn table() -> [u32; 256] {
    let mut table = [0; 256];
    let mut i = 0;
    while i < table.len() {
        let mut crc = i as u32;
        let mut bit = 0;
        while bit < 8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(crc & 1));
            bit += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

/// A CRC32 byte stream, independent of message chunk boundaries.
pub struct Crc32(u32);

impl Crc32 {
    pub const fn new() -> Self {
        Self(!0)
    }

    pub fn update_byte(&mut self, byte: u8) {
        self.0 = (self.0 >> 8) ^ TABLE[((self.0 as u8) ^ byte) as usize];
    }

    pub fn update(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.update_byte(byte);
        }
    }

    pub const fn finish(self) -> u32 {
        !self.0
    }
}

impl Default for Crc32 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::Crc32;

    fn reference(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 0 {
                    crc >> 1
                } else {
                    (crc >> 1) ^ 0xedb8_8320
                };
            }
        }
        !crc
    }

    #[test]
    fn standard_vectors_and_default_initialization() {
        assert_eq!(Crc32::new().finish(), 0);
        assert_eq!(Crc32::default().finish(), 0);
        for (bytes, expected) in [
            (b"123456789".as_slice(), 0xcbf4_3926),
            (b"The quick brown fox jumps over the lazy dog", 0x414f_a339),
        ] {
            let mut crc = Crc32::new();
            crc.update(bytes);
            assert_eq!(crc.finish(), expected);
        }
    }

    #[test]
    fn binary_vector_all_splits_and_bytewise_streams_match() {
        let bytes: Vec<u8> = (0..=255).collect();
        // Independently computed with Python zlib.crc32(bytes(range(256))).
        assert_eq!(reference(&bytes), 688_229_491);
        for split in 0..=bytes.len() {
            let mut crc = Crc32::new();
            crc.update(&bytes[..split]);
            crc.update(&[]);
            crc.update(&bytes[split..]);
            assert_eq!(crc.finish(), 688_229_491);
        }
        let mut crc = Crc32::default();
        for &byte in &bytes {
            crc.update_byte(byte);
        }
        assert_eq!(crc.finish(), 688_229_491);
    }

    #[test]
    fn table_matches_bitwise_reference_across_all_bytes_and_prefixes() {
        let mut bytes: Vec<u8> = (0..=255).collect();
        bytes.extend((0..=255).rev());
        for prefix in 0..=bytes.len() {
            let mut crc = Crc32::new();
            crc.update(&bytes[..prefix]);
            assert_eq!(crc.finish(), reference(&bytes[..prefix]));
        }
    }
}

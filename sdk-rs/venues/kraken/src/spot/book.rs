//! Kraken v2 CRCs use the original decimal digits, including trailing zeros.
use super::wire::Level;
use safe_cex_engine::book::{Book, BookSide};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Checksum {
    bids: HashMap<u64, (String, String)>,
    asks: HashMap<u64, (String, String)>,
}
impl Checksum {
    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }
    pub fn apply(&mut self, side: BookSide, level: &Level) {
        let rows = match side {
            BookSide::Bids => &mut self.bids,
            BookSide::Asks => &mut self.asks,
        };
        let key = level.price.value.to_bits();
        if level.qty.value == 0. {
            rows.remove(&key);
        } else {
            rows.insert(key, (level.price.digits.clone(), level.qty.digits.clone()));
        }
    }
    pub fn matches(&mut self, book: &Book, expected: Option<u32>) -> bool {
        self.bids.retain(|key, _| {
            book.bids()
                .binary_search_by(|l| f64::from_bits(*key).total_cmp(&l.0))
                .is_ok()
        });
        self.asks.retain(|key, _| {
            book.asks()
                .binary_search_by(|l| l.0.total_cmp(&f64::from_bits(*key)))
                .is_ok()
        });
        let Some(expected) = expected else {
            return true;
        };
        let mut crc = !0u32;
        for (levels, rows) in [(book.asks(), &self.asks), (book.bids(), &self.bids)] {
            for (price, _) in levels.iter().take(10) {
                let Some((price, quantity)) = rows.get(&price.to_bits()) else {
                    return false;
                };
                for byte in price.bytes().chain(quantity.bytes()) {
                    crc ^= byte as u32;
                    for _ in 0..8 {
                        crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
                    }
                }
            }
        }
        !crc == expected
    }
}

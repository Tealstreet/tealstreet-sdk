use crate::wire::Depth;
use safe_cex_engine::book::{Book, BookSide};
use std::borrow::Cow;

#[derive(Default)]
pub struct BookState {
    pub subscribers: usize,
    pub book: Book,
    pub synced: bool,
    sequence: Option<i64>,
    first_update: bool,
    bids: Vec<Level>,
    asks: Vec<Level>,
}

struct Level {
    price: f64,
    price_text: String,
    amount_text: String,
}

impl BookState {
    pub fn reset(&mut self) {
        self.book.clear();
        self.bids.clear();
        self.asks.clear();
        self.sequence = None;
        self.synced = false;
        self.first_update = true;
    }

    pub fn apply(&mut self, action: &str, row: &Depth<'_>) -> Result<bool, &'static str> {
        if row.checksum.is_none()
            && (row.seq.is_none() || (action == "update" && row.pseq.is_none()))
        {
            self.reset();
            return Err("Bitget book has no integrity metadata");
        }
        if action == "snapshot" {
            self.reset();
            if row.bids.is_none() || row.asks.is_none() {
                return Err("incomplete Bitget book snapshot");
            }
        } else if action != "update" || !self.synced {
            return Ok(false);
        } else if let (Some(last), Some(seq)) = (self.sequence, row.seq) {
            if seq == last {
                return Ok(false);
            }
            if seq < last
                || row.pseq.is_some_and(|previous| {
                    if self.first_update {
                        previous > last
                    } else {
                        previous != last
                    }
                })
            {
                self.reset();
                return Err("Bitget book sequence gap");
            }
        }
        for (side, raw, levels) in [
            (
                BookSide::Bids,
                row.bids.as_deref().unwrap_or_default(),
                &mut self.bids,
            ),
            (
                BookSide::Asks,
                row.asks.as_deref().unwrap_or_default(),
                &mut self.asks,
            ),
        ] {
            for pair in raw {
                let (price, amount) = parse_level(pair)?;
                self.book.apply(side, price, amount);
                let found = levels.binary_search_by(|level| match side {
                    BookSide::Bids => price.total_cmp(&level.price),
                    BookSide::Asks => level.price.total_cmp(&price),
                });
                match (found, amount == 0.0) {
                    (Ok(i), true) => {
                        levels.remove(i);
                    }
                    (Ok(i), false) => {
                        levels[i].price_text = pair[0].to_string();
                        levels[i].amount_text = pair[1].to_string();
                    }
                    (Err(_), true) => {}
                    (Err(i), false) => levels.insert(
                        i,
                        Level {
                            price,
                            price_text: pair[0].to_string(),
                            amount_text: pair[1].to_string(),
                        },
                    ),
                }
            }
        }
        if row
            .checksum
            .is_some_and(|expected| expected != checksum(&self.bids, &self.asks))
        {
            self.reset();
            return Err("Bitget book checksum mismatch");
        }
        self.first_update = action == "snapshot";
        self.sequence = row.seq.or(self.sequence);
        self.synced = true;
        Ok(true)
    }
}

fn parse_level(pair: &[Cow<'_, str>; 2]) -> Result<(f64, f64), &'static str> {
    let price = pair[0].parse::<f64>().map_err(|_| "invalid book price")?;
    let amount = pair[1].parse::<f64>().map_err(|_| "invalid book amount")?;
    if !price.is_finite() || !amount.is_finite() || price <= 0.0 || amount < 0.0 {
        return Err("invalid book level");
    }
    Ok((price, amount))
}

fn checksum(bids: &[Level], asks: &[Level]) -> i32 {
    let mut crc = !0u32;
    let mut first = true;
    for i in 0..25 {
        for level in [bids.get(i), asks.get(i)].into_iter().flatten() {
            for text in [&level.price_text, &level.amount_text] {
                if !first {
                    crc_byte(&mut crc, b':');
                }
                first = false;
                for byte in text.bytes() {
                    crc_byte(&mut crc, byte);
                }
            }
        }
    }
    (!crc) as i32
}

fn crc_byte(crc: &mut u32, byte: u8) {
    *crc ^= u32::from(byte);
    for _ in 0..8 {
        *crc = (*crc >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(*crc & 1));
    }
}

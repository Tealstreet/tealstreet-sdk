//! Caller client order ids, so a placement whose answer was lost can be looked
//! up instead of placed again. Only venues whose engine answers a client-id
//! lookup get one, each in the format and attribution the TS bridge mints.
use std::cell::Cell;
use std::hash::{BuildHasher, RandomState};

pub struct ClientIdMinter {
    seed: RandomState,
    sequence: Cell<u64>,
}

impl Default for ClientIdMinter {
    fn default() -> Self {
        Self {
            seed: RandomState::new(),
            sequence: Cell::new(0),
        }
    }
}

impl ClientIdMinter {
    /// 32 lowercase hex characters, unique per process and call.
    fn hex(&self) -> String {
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let high = self.seed.hash_one((sequence, nanos, 0u8));
        let low = self.seed.hash_one((sequence, nanos, 1u8));
        format!("{high:016x}{low:016x}")
    }

    /// The id `venue` honors on a regular limit order, or `None` where the
    /// venue's engine cannot look one up.
    pub fn mint(&self, venue: &str) -> Option<String> {
        let hex = self.hex();
        let id = match venue {
            "hyperliquid" | "hyperliquidspot" => format!("0x{hex}"),
            "binance" | "binancespot" => format!("x-s3vFFRQp{hex}")[..36].to_string(),
            "binancepm" => format!("x-s3vFFRQp{hex}")[..32].to_string(),
            "okx" | "okxdemo" | "okxeu" | "okxeudemo" => {
                format!("e847386590ce4dBC{hex}")[..32].to_string()
            }
            "phemex" => format!("Tealstreet_{hex}"),
            "paradex" => format!("ts_{hex}"),
            "bitrue" => hex[..30].to_string(),
            "grvt" => (u64::from_str_radix(&hex[..16], 16).unwrap_or(1) >> 1)
                .max(1)
                .to_string(),
            _ => return None,
        };
        Some(id)
    }
}

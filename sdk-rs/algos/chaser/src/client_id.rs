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
        // The executor's wall clock: `SystemTime` panics in the wasm worker.
        let nanos = (safe_cex_algos::executor::now_ms() * 1e6) as u128;
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
            // The engine refuses a caller id over 40 characters.
            "phemex" => format!("Tealstreet_{hex}")[..40].to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    // Each venue engine's own limit on a caller client order id.
    #[test]
    fn every_minted_id_fits_its_venue_limit() {
        let minter = ClientIdMinter::default();
        for (venue, max) in [
            ("hyperliquid", 34),
            ("hyperliquidspot", 34),
            ("binance", 36),
            ("binancespot", 36),
            ("binancepm", 32),
            ("okx", 32),
            ("okxdemo", 32),
            ("okxeu", 32),
            ("okxeudemo", 32),
            ("phemex", 40),
            ("paradex", 64),
            ("bitrue", 31),
            ("grvt", 19),
        ] {
            let id = minter.mint(venue).unwrap();
            assert!(id.len() <= max, "{venue}: {id} is {} long", id.len());
        }
    }
}

//! Binance's VENUE_STATE section (`Venue::export_venue_state_for`): what a
//! frame-fed engine's writes read that the store frames do not carry.
//!
//! - Stop-limit order ids, whose native type executes at a limit price once
//!   triggered.
//! - Portfolio margin and spot cancel routing: each client id's owner (regular
//!   or strategy native id, by symbol) and the per-symbol client-to-native maps.
//!
//! Only rows for orders the source's store holds open are exported. The source
//! keeps its full maps, which also guard against reused client ids; the export
//! stays bounded and unchanged state is not resent.
//!
//! Version 2, little-endian: a version byte; the stop ids; then the PM owners,
//! the strategy map and the spot map as rows of (client, symbol, kind, native
//! u64), where an owner's kind is 0 regular or 1 strategy and a map's is 0. Each
//! list is a u32 count and sorted rows; strings are a u32 length and UTF-8
//! bytes. Version 1 carried only the stop ids.
use super::{PmOrderOwner, PmOrderSource};
use crate::BinanceFutures;
use safe_cex_engine::Store;
use safe_cex_engine::model::OrderStatus;

const VERSION: u8 = 2;
const STOPS_ONLY: u8 = 1;

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn put_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&(count as u32).to_le_bytes());
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(length)?;
        self.0 = rest;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let length = self.u32()? as usize;
        Some(std::str::from_utf8(self.take(length)?).ok()?.to_owned())
    }
}

/// One routing row: (key, symbol, kind, native). PM owners key by client with
/// kind 0 regular or 1 strategy; the client-to-native maps key by client, kind 0.
type Row<'a> = (&'a str, &'a str, u8, u64);

fn put_rows(out: &mut Vec<u8>, mut rows: Vec<Row<'_>>) {
    rows.sort_unstable();
    put_count(out, rows.len());
    for (key, symbol, kind, native) in rows {
        put_str(out, key);
        put_str(out, symbol);
        out.push(kind);
        out.extend_from_slice(&native.to_le_bytes());
    }
}

fn read_rows(reader: &mut Reader<'_>) -> Option<Vec<(String, String, u8, u64)>> {
    let mut rows = Vec::new();
    for _ in 0..reader.u32()? {
        let (key, symbol) = (reader.string()?, reader.string()?);
        rows.push((key, symbol, reader.u8()?, reader.u64()?));
    }
    Some(rows)
}

#[derive(Default)]
struct Decoded {
    stops: Vec<String>,
    owners: Vec<(String, String, u8, u64)>,
    strategy: Vec<(String, String, u8, u64)>,
    spot: Vec<(String, String, u8, u64)>,
}

fn decode(bytes: &[u8]) -> Option<Decoded> {
    let mut reader = Reader(bytes);
    let version = reader.u8()?;
    if version != VERSION && version != STOPS_ONLY {
        return None;
    }
    let mut decoded = Decoded::default();
    for _ in 0..reader.u32()? {
        decoded.stops.push(reader.string()?);
    }
    if version == VERSION {
        decoded.owners = read_rows(&mut reader)?;
        if decoded.owners.iter().any(|row| row.2 > 1) {
            return None;
        }
        decoded.strategy = read_rows(&mut reader)?;
        decoded.spot = read_rows(&mut reader)?;
    }
    reader.0.is_empty().then_some(decoded)
}

impl BinanceFutures {
    pub(crate) fn export_venue_state(&self, store: &Store) -> Vec<u8> {
        // Store order ids are the client ids these maps are keyed by.
        let open = |symbol: &str, client: &str| {
            store
                .order(client)
                .is_some_and(|order| order.status == OrderStatus::Open && order.symbol == symbol)
        };
        let account = &self.account;
        let mut out = vec![VERSION];
        let mut stops: Vec<&str> = account
            .limit_stops
            .iter()
            .map(String::as_str)
            .filter(|id| {
                store
                    .order(id)
                    .is_some_and(|order| order.status == OrderStatus::Open)
            })
            .collect();
        stops.sort_unstable();
        put_count(&mut out, stops.len());
        for id in stops {
            put_str(&mut out, id);
        }
        put_rows(
            &mut out,
            account
                .pm_order_owners
                .iter()
                .filter(|(client, owner)| open(&owner.symbol, client))
                .map(|(client, owner)| match owner.source {
                    PmOrderSource::Regular(native) => {
                        (client.as_str(), owner.symbol.as_str(), 0, native)
                    }
                    PmOrderSource::Strategy(native) => {
                        (client.as_str(), owner.symbol.as_str(), 1, native)
                    }
                })
                .collect(),
        );
        for map in [&account.strategy_client_native, &account.spot_client_native] {
            put_rows(
                &mut out,
                map.iter()
                    .filter(|((symbol, client), _)| open(symbol, client))
                    .map(|((symbol, client), native)| {
                        (client.as_str(), symbol.as_str(), 0, *native)
                    })
                    .collect(),
            );
        }
        out
    }

    /// Apply a data source's state: its stop ids replace ours, and its routing
    /// rows overwrite ours while this engine's own fresh placements stay.
    /// `false` leaves everything unchanged for an unreadable payload.
    pub(crate) fn apply_venue_state(&mut self, bytes: &[u8]) -> bool {
        let Some(decoded) = decode(bytes) else {
            return false;
        };
        let account = &mut self.account;
        account.limit_stops = decoded.stops.into_iter().collect();
        for (client, symbol, kind, native) in decoded.owners {
            let source = if kind == 0 {
                PmOrderSource::Regular(native)
            } else {
                PmOrderSource::Strategy(native)
            };
            account
                .pm_order_owners
                .insert(client, PmOrderOwner { symbol, source });
        }
        for (client, symbol, _, native) in decoded.strategy {
            account
                .strategy_client_native
                .insert((symbol, client), native);
        }
        for (client, symbol, _, native) in decoded.spot {
            account.spot_client_native.insert((symbol, client), native);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_one_still_reads_and_bad_payloads_refuse() {
        let mut v1 = vec![STOPS_ONLY];
        put_count(&mut v1, 1);
        put_str(&mut v1, "stop-1");
        let decoded = decode(&v1).unwrap();
        assert!(decoded.stops == ["stop-1"] && decoded.owners.is_empty());
        let mut trailing = v1.clone();
        trailing.push(0);
        assert!(decode(&trailing).is_none());
        assert!(decode(&[3]).is_none());
        let mut bad_kind = vec![VERSION];
        put_count(&mut bad_kind, 0);
        put_count(&mut bad_kind, 1);
        put_str(&mut bad_kind, "c");
        put_str(&mut bad_kind, "BTCUSDT");
        bad_kind.push(7);
        bad_kind.extend_from_slice(&1u64.to_le_bytes());
        put_count(&mut bad_kind, 0);
        put_count(&mut bad_kind, 0);
        assert!(decode(&bad_kind).is_none());
    }
}

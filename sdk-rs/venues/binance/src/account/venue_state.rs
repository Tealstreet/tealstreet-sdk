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
//! Version 2, little-endian: a version byte; the stop ids; the PM owners as
//! (client, symbol, kind 0 regular or 1 strategy, native u64); the strategy
//! and spot maps as (symbol, client, native u64). Each list is a u32 count and
//! sorted rows; strings are a u32 length and UTF-8 bytes. Version 1 carried
//! only the stop ids.
use super::{PmOrderOwner, PmOrderSource};
use crate::BinanceFutures;
use safe_cex_engine::Store;
use safe_cex_engine::model::OrderStatus;
use std::collections::{HashMap, HashSet};

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

type ClientNative = HashMap<(String, String), u64>;

#[derive(Default)]
struct Decoded {
    stops: HashSet<String>,
    owners: HashMap<String, PmOrderOwner>,
    strategy: ClientNative,
    spot: ClientNative,
}

fn client_native(reader: &mut Reader<'_>) -> Option<ClientNative> {
    let mut rows = HashMap::new();
    for _ in 0..reader.u32()? {
        let (symbol, client) = (reader.string()?, reader.string()?);
        rows.insert((symbol, client), reader.u64()?);
    }
    Some(rows)
}

fn decode(bytes: &[u8]) -> Option<Decoded> {
    let mut reader = Reader(bytes);
    let version = reader.u8()?;
    if version != VERSION && version != STOPS_ONLY {
        return None;
    }
    let mut decoded = Decoded::default();
    for _ in 0..reader.u32()? {
        decoded.stops.insert(reader.string()?);
    }
    if version == VERSION {
        for _ in 0..reader.u32()? {
            let (client, symbol) = (reader.string()?, reader.string()?);
            let kind = reader.u8()?;
            let native = reader.u64()?;
            let source = match kind {
                0 => PmOrderSource::Regular(native),
                1 => PmOrderSource::Strategy(native),
                _ => return None,
            };
            decoded
                .owners
                .insert(client, PmOrderOwner { symbol, source });
        }
        decoded.strategy = client_native(&mut reader)?;
        decoded.spot = client_native(&mut reader)?;
    }
    reader.0.is_empty().then_some(decoded)
}

fn put_client_native(out: &mut Vec<u8>, rows: &ClientNative, open: &HashSet<(&str, &str)>) {
    let mut kept: Vec<_> = rows
        .iter()
        .filter(|((symbol, client), _)| open.contains(&(symbol.as_str(), client.as_str())))
        .collect();
    kept.sort();
    put_count(out, kept.len());
    for ((symbol, client), native) in kept {
        put_str(out, symbol);
        put_str(out, client);
        out.extend_from_slice(&native.to_le_bytes());
    }
}

impl BinanceFutures {
    pub(crate) fn export_venue_state(&self, store: &Store) -> Vec<u8> {
        // Store order ids are the client ids these maps are keyed by.
        let open: HashSet<(&str, &str)> = store
            .orders()
            .iter()
            .filter(|order| order.status == OrderStatus::Open)
            .map(|order| (order.symbol.as_str(), order.id.as_str()))
            .collect();
        let open_ids: HashSet<&str> = open.iter().map(|(_, id)| *id).collect();
        let account = &self.account;
        let mut out = vec![VERSION];
        let mut stops: Vec<&String> = account
            .limit_stops
            .iter()
            .filter(|id| open_ids.contains(id.as_str()))
            .collect();
        stops.sort();
        put_count(&mut out, stops.len());
        for id in stops {
            put_str(&mut out, id);
        }
        let mut owners: Vec<_> = account
            .pm_order_owners
            .iter()
            .filter(|(client, owner)| open.contains(&(owner.symbol.as_str(), client.as_str())))
            .collect();
        owners.sort_by(|a, b| a.0.cmp(b.0));
        put_count(&mut out, owners.len());
        for (client, owner) in owners {
            put_str(&mut out, client);
            put_str(&mut out, &owner.symbol);
            let (kind, native) = match owner.source {
                PmOrderSource::Regular(native) => (0u8, native),
                PmOrderSource::Strategy(native) => (1u8, native),
            };
            out.push(kind);
            out.extend_from_slice(&native.to_le_bytes());
        }
        put_client_native(&mut out, &account.strategy_client_native, &open);
        put_client_native(&mut out, &account.spot_client_native, &open);
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
        account.limit_stops = decoded.stops;
        account.pm_order_owners.extend(decoded.owners);
        account.strategy_client_native.extend(decoded.strategy);
        account.spot_client_native.extend(decoded.spot);
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
        assert!(decoded.stops.contains("stop-1") && decoded.owners.is_empty());
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

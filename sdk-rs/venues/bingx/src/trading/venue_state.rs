//! BingX's VENUE_STATE section (`Venue::export_venue_state`): what a frame-fed
//! engine's writes read that the store frames do not carry. Places refuse until
//! the position mode is known, and amends need each open order's native
//! observation (kind, position side, attached, native status).
//!
//! Version 1, little-endian: a version byte; a flags byte (mode known); the
//! observations sorted by order id, each as strings (a u32 length and UTF-8
//! bytes) plus an attached byte. The observed order itself is the store's: a
//! frame-fed engine's ingested ORDERS are its reconcile.
use super::{Observation, Trading};
use safe_cex_engine::model::OrderStatus;

const VERSION: u8 = 1;
const MODE_KNOWN: u8 = 1;

fn put_str(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_le_bytes());
    out.extend_from_slice(value.as_bytes());
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
    fn string(&mut self) -> Option<String> {
        let length = self.u32()? as usize;
        Some(std::str::from_utf8(self.take(length)?).ok()?.to_owned())
    }
}

/// One decoded observation: order id, native kind, position side, attached, native status.
type Row = (String, String, String, bool, String);

fn decode(bytes: &[u8]) -> Option<(bool, Vec<Row>)> {
    let mut reader = Reader(bytes);
    if reader.u8()? != VERSION {
        return None;
    }
    let mode_known = reader.u8()? & MODE_KNOWN != 0;
    let mut rows = Vec::new();
    for _ in 0..reader.u32()? {
        let (id, kind, side) = (reader.string()?, reader.string()?, reader.string()?);
        let attached = reader.u8()? != 0;
        rows.push((id, kind, side, attached, reader.string()?));
    }
    reader.0.is_empty().then_some((mode_known, rows))
}

impl Trading {
    pub(crate) fn export_venue_state(&self, mode_known: bool) -> Vec<u8> {
        let mut out = vec![VERSION, if mode_known { MODE_KNOWN } else { 0 }];
        // Sorted, so unchanged state encodes to unchanged bytes and is not resent.
        let mut ids: Vec<&String> = self.observed.keys().collect();
        ids.sort();
        out.extend_from_slice(&(ids.len() as u32).to_le_bytes());
        for id in ids {
            let seen = &self.observed[id];
            put_str(&mut out, id);
            put_str(&mut out, &seen.native_kind);
            put_str(&mut out, &seen.position_side);
            out.push(u8::from(seen.attached));
            put_str(&mut out, &seen.native_status);
        }
        out
    }

    /// Replace the observations with a data source's; `false` leaves them as
    /// they were when the bytes are not a version this build reads.
    pub(crate) fn apply_venue_state(
        &mut self,
        bytes: &[u8],
        store: &safe_cex_engine::Store,
    ) -> bool {
        let Some((mode_known, rows)) = decode(bytes) else {
            return false;
        };
        self.mode_known = mode_known;
        self.observed.clear();
        for (id, native_kind, position_side, attached, native_status) in rows {
            if let Some(order) = store.order(&id) {
                self.observed.insert(
                    id,
                    Observation {
                        order: order.clone(),
                        native_kind,
                        position_side,
                        attached,
                        native_status,
                    },
                );
            }
        }
        self.refresh_observed(store);
        true
    }

    /// Frame-fed: the ingested orders are the reconcile, so observations follow
    /// them and close with them.
    pub(crate) fn refresh_observed(&mut self, store: &safe_cex_engine::Store) {
        self.observed.retain(|id, seen| match store.order(id) {
            Some(order) if order.status == OrderStatus::Open => {
                seen.order = order.clone();
                true
            }
            _ => false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn venue_state_round_trips_against_the_store_and_refuses_other_versions() {
        let mut store = safe_cex_engine::Store::default();
        use safe_cex_engine::model::{Order, OrderSide, OrderType};
        let order = Order {
            id: "1".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "BTCUSDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 100.,
            amount: 1.,
            filled: 0.,
            remaining: 1.,
            reduce_only: false,
            close: None,
            client_order_id: None,
            time_in_force: None,
            last_update: 0.,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        store.set_orders(vec![order.clone()]);
        let mut source = Trading::default();
        source.observe_order(&order, "LIMIT", "LONG", false, "NEW");
        let closed = Order {
            id: "2".into(),
            ..order.clone()
        };
        source.observe_order(&closed, "STOP_MARKET", "BOTH", true, "NEW");
        let bytes = source.export_venue_state(true);

        let mut copy = Trading::default();
        assert!(copy.apply_venue_state(&bytes, &store));
        assert!(copy.mode_known);
        // Only orders the store holds open are observed.
        assert_eq!(copy.observed.len(), 1);
        let seen = &copy.observed["1"];
        assert_eq!(
            (
                seen.native_kind.as_str(),
                seen.position_side.as_str(),
                seen.attached,
                seen.native_status.as_str()
            ),
            ("LIMIT", "LONG", false, "NEW")
        );
        assert!(seen.order == order);

        let mut other = bytes.clone();
        other[0] = 2;
        assert!(!copy.apply_venue_state(&other, &store));
        assert!(!copy.apply_venue_state(&bytes[..bytes.len() - 1], &store));
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(!copy.apply_venue_state(&trailing, &store));
        assert_eq!(
            copy.observed.len(),
            1,
            "a refused state leaves the applied one"
        );

        // A frame that closes the order drops its observation.
        store.set_orders(vec![]);
        copy.refresh_observed(&store);
        assert!(copy.observed.is_empty());
    }
}

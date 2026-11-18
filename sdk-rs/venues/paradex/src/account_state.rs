//! Frame-fed VENUE_STATE for the account: its native order rows, exported by
//! a data source and applied by the page engine, whose amend reads them.
use super::Account;
use crate::venue_state;
use safe_cex_engine::Store;

impl Account {
    /// The data source's rows, ordered by id so an unchanged state is byte-identical.
    pub(crate) fn export_venue_state(&self) -> Vec<u8> {
        let mut rows = self.native.values().collect::<Vec<_>>();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows.truncate(venue_state::MAX_ROWS);
        venue_state::encode(rows.into_iter())
    }
    /// Replaces the page engine's rows; a refused state leaves them unchanged.
    pub(crate) fn apply_venue_state(&mut self, state: &[u8]) -> Result<(), &'static str> {
        let rows = venue_state::decode(state)?;
        self.native = rows.into_iter().map(|row| (row.id.clone(), row)).collect();
        Ok(())
    }
    /// A row whose order the source's frames no longer list cannot be amended.
    pub(crate) fn prune_native(&mut self, store: &Store) {
        self.native.retain(|id, _| store.order(id).is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::NativeOrder;

    fn row(id: &str) -> NativeOrder {
        NativeOrder {
            id: id.into(),
            market: "BTC-USD-PERP".into(),
            side: "SELL".into(),
            kind: "LIMIT".into(),
            status: "OPEN".into(),
            size: "1".into(),
            remaining_size: "1".into(),
            price: "70000".into(),
            reduce_only: Some(false),
            ..NativeOrder::default()
        }
    }

    #[test]
    fn the_page_engine_amends_from_the_source_rows_and_keeps_its_own_on_a_bad_state() {
        let mut source = Account::new(Default::default());
        for id in ["b", "a"] {
            source.native.insert(id.into(), row(id));
        }
        let state = source.export_venue_state();
        assert_eq!(state, source.export_venue_state());

        let mut page = Account::new(Default::default());
        page.native.insert("stale".into(), row("stale"));
        page.apply_venue_state(&state).unwrap();
        let mut ids = page.native.keys().cloned().collect::<Vec<_>>();
        ids.sort();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(
            format!("{:?}", page.native_order("a")),
            format!("{:?}", Some(&row("a")))
        );

        assert!(page.apply_venue_state(&state[..state.len() - 1]).is_err());
        assert_eq!(page.native.len(), 2);

        page.prune_native(&Store::default());
        assert!(page.native.is_empty());
    }
}

//! The engine store projected into the copy algos read: one [`project`] per
//! engine read, then [`apply`] to the account's [`AlgoStore`]. Every host (the
//! native server, the LOCAL worker's wasm engine) projects the same way.
use std::collections::HashMap;

use safe_cex_engine::engine::Fill;
use safe_cex_engine::model::{
    Balance, CoinBalance, InstrumentClassification, Loaded, Market, Order, Position, Ticker,
};

use crate::{AlgoStore, Change};

/// One read of the engine store, taken between engine turns.
pub struct Snapshot {
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub tickers: Vec<Ticker>,
    pub markets: Vec<Market>,
    pub balance: Balance,
    pub loaded: Loaded,
    pub classifications: Option<Vec<InstrumentClassification>>,
    pub coins: Option<Vec<CoinBalance>>,
}

pub fn project(store: &safe_cex_engine::Store) -> Snapshot {
    Snapshot {
        orders: store.orders().to_vec(),
        positions: store.positions().to_vec(),
        tickers: store.tickers().to_vec(),
        markets: store.markets().to_vec(),
        balance: store.balance().clone(),
        loaded: store.loaded(),
        #[cfg(feature = "product-semantics")]
        classifications: store.instrument_classifications().map(<[_]>::to_vec),
        #[cfg(not(feature = "product-semantics"))]
        classifications: None,
        coins: store.coin_balances().map(<[_]>::to_vec),
    }
}

/// Applies one engine read to the algo store; returns what changed.
/// `rejoined`: fills sent while the feed was dropped are gone, so algos
/// re-read orders and positions and reconcile before acting.
pub fn apply(
    store: &mut AlgoStore,
    snapshot: Snapshot,
    fills: Vec<Fill>,
    rejoined: bool,
) -> Change {
    let orders = store.orders != snapshot.orders;
    let positions = store.positions != snapshot.positions;
    let tickers: HashMap<String, Ticker> = snapshot
        .tickers
        .into_iter()
        .map(|ticker| (ticker.symbol.clone(), ticker))
        .collect();
    let tickers_changed = store.tickers != tickers;
    if orders {
        store.update_ids.orders += 1;
    }
    if positions {
        store.update_ids.positions += 1;
    }
    store.orders = snapshot.orders;
    store.positions = snapshot.positions;
    store.tickers = tickers;
    store.instrument_classifications = snapshot.classifications.map(|labels| {
        labels
            .into_iter()
            .filter_map(|label| {
                let market = snapshot.markets.get(label.market as usize)?;
                Some((market.symbol.clone(), label))
            })
            .collect()
    });
    store.markets = snapshot
        .markets
        .into_iter()
        .map(|market| (market.symbol.clone(), market))
        .collect();
    store.coins = snapshot.coins;
    store.loaded = Some(snapshot.loaded);
    if snapshot.loaded.balance {
        store.balance = Some(snapshot.balance);
    }
    // The engine exposes no private-socket health: report it as unknown
    // (uninitialized), never as healthy.
    store.private_socket = Default::default();
    Change {
        orders: orders || rejoined,
        positions: positions || rejoined,
        tickers: tickers_changed,
        fills,
        // Set by the port on a feed rejoin (fe7lgm).
        resync: rejoined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::model::{Limits, MinMax, Precision};

    fn market(symbol: &str) -> Market {
        Market {
            id: symbol.into(),
            symbol: symbol.into(),
            pretty_symbol: None,
            base: "BTC".into(),
            quote: "USDT".into(),
            active: true,
            precision: Precision {
                amount: 0.001,
                price: 0.1,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.001,
                    max: 1000.,
                },
                leverage: MinMax { min: 1., max: 100. },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }

    fn snapshot(classified: bool, balance_loaded: bool) -> Snapshot {
        Snapshot {
            orders: Vec::new(),
            positions: Vec::new(),
            tickers: Vec::new(),
            markets: vec![market("BTCUSDT"), market("ETHUSDT")],
            balance: Balance::default(),
            loaded: Loaded {
                balance: balance_loaded,
                orders: true,
                markets: true,
                tickers: true,
                positions: true,
            },
            coins: None,
            classifications: classified.then(|| {
                vec![InstrumentClassification {
                    market: 1,
                    symbol_type: Some("stock".into()),
                    market_region: None,
                    underlying_ticker: None,
                }]
            }),
        }
    }

    #[test]
    fn classifications_map_market_index_to_symbol_and_stay_none_until_sent() {
        let mut store = AlgoStore::default();
        apply(&mut store, snapshot(false, false), Vec::new(), false);
        assert!(store.instrument_classifications.is_none());
        assert!(store.coins.is_none(), "absent coins stay absent");
        assert!(store.loaded.is_some_and(|loaded| loaded.orders));
        assert!(
            store.balance.is_none(),
            "no balance before its section loads"
        );
        apply(&mut store, snapshot(true, true), Vec::new(), false);
        let labels = store.instrument_classifications.as_ref().unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels["ETHUSDT"].symbol_type.as_deref(), Some("stock"));
        assert!(store.balance.is_some());
        assert!(
            !store.private_socket.initialized && !store.private_socket.healthy,
            "unknown, never claimed healthy"
        );
        assert_eq!(store.markets.len(), 2);
    }

    #[test]
    fn unchanged_reads_report_no_change() {
        let mut store = AlgoStore::default();
        apply(&mut store, snapshot(true, true), Vec::new(), false);
        let fill = Fill {
            id: None,
            order_id: "o1".into(),
            symbol: "BTCUSDT".into(),
            side: safe_cex_engine::model::OrderSide::Buy,
            price: 1.,
            amount: 2.,
        };
        let change = apply(&mut store, snapshot(true, true), vec![fill.clone()], false);
        assert_eq!(
            change.fills,
            vec![fill],
            "a frame's fills reach subscribers"
        );
        assert!(!change.orders && !change.positions && !change.tickers);
        assert_eq!(store.update_ids.orders, 0);
    }
}

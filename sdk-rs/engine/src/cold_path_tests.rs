//! The venue-independent halves of shared-ticker restoration and the
//! `orders.held` notice, pinned to the behaviour they had inline.
use super::{HashMap, Store, copy_shared_tickers};
use crate::model::{Limits, Market, MinMax, Precision, Ticker};
use crate::public_feed::PublicTicker;

fn market(symbol: &str) -> Market {
    Market {
        id: format!("native-{symbol}"),
        symbol: symbol.into(),
        pretty_symbol: None,
        base: symbol.into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            amount: 0.1,
            price: 0.5,
        },
        limits: Limits {
            amount: MinMax { min: 1., max: 100. },
            leverage: MinMax { min: 1., max: 10. },
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

fn quote(json: &str) -> PublicTicker {
    serde_json::from_str(json).unwrap()
}

#[test]
fn shared_quotes_add_missing_rows_then_overwrite_only_the_fields_they_carry() {
    let mut store = Store::default();
    store.set_markets(vec![market("BTC"), market("ETH")]);
    store.set_tickers(vec![Ticker {
        id: "native-ETH".into(),
        symbol: "ETH".into(),
        bid: 1.,
        last: 7.,
        ..Default::default()
    }]);
    let shared: HashMap<String, PublicTicker> = [
        ("BTC".into(), quote(r#"{"symbol":"BTC","bid":10,"ask":11}"#)),
        ("ETH".into(), quote(r#"{"symbol":"ETH","bid":2}"#)),
        ("GONE".into(), quote(r#"{"symbol":"GONE","bid":3}"#)),
    ]
    .into_iter()
    .collect();
    let before = store.ticker_revision();
    copy_shared_tickers(&mut store, &shared);
    assert_ne!(store.ticker_revision(), before);
    let btc = store.ticker("BTC").unwrap();
    assert_eq!(
        (btc.id.as_str(), btc.bid, btc.ask),
        ("native-BTC", 10., 11.)
    );
    let eth = store.ticker("ETH").unwrap();
    assert_eq!(
        (eth.bid, eth.last),
        (2., 7.),
        "omitted fields keep the venue's"
    );
    assert!(store.ticker("GONE").is_none(), "no row without a market");
    assert_eq!(store.tickers().len(), 2);
}

#[cfg(feature = "order-locks")]
#[test]
fn orders_held_names_each_standing_fence_once_with_the_grace_of_its_clock() {
    use super::orders_held_notice;
    use crate::frame_fed::{FENCE_GRACE_MS, UNSYNCED_FENCE_GRACE_MS};
    let mut locks = crate::order_locks::OrderLocks::default();
    locks.fence(Some(&["ETHUSDT".into(), "BTCUSDT".into()]));
    let epoch = locks
        .uncertain_epochs()
        .map(|(_, epoch)| epoch)
        .max()
        .unwrap();
    let marks = [(epoch, 1_000.)];
    for (synced, grace) in [(true, FENCE_GRACE_MS), (false, UNSYNCED_FENCE_GRACE_MS)] {
        let mut held = vec!["SOLUSDT".to_owned()];
        assert_eq!(
            orders_held_notice(&locks, &mut held, &marks, synced, 1_500.),
            Some((
                "BTCUSDT,ETHUSDT".to_owned(),
                (1_000. + grace - 1_500.).max(0.)
            ))
        );
        assert_eq!(held, ["BTCUSDT", "ETHUSDT"], "names what stands, sorted");
        assert_eq!(
            orders_held_notice(&locks, &mut held, &marks, synced, 1_600.),
            None
        );
    }
    let mut held = Vec::new();
    assert_eq!(
        orders_held_notice(&locks, &mut held, &[], true, 0.),
        None,
        "no mark, no ask"
    );
    assert!(held.is_empty());
}

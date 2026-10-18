use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Balance, Ticker};
use safe_cex_venue_bitrue::account_reads::fetch_balance;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("BITRUE-FALLBACK immediate account result"),
    }
}

fn balance(total: f64, free: f64, used: f64, upnl: f64) -> Balance {
    Balance {
        total,
        free,
        used,
        upnl,
        info: None,
        currencies: None,
        coins: None,
    }
}

#[test]
fn fallback_retains_balance_across_ticker_and_balance_writes() {
    let store = DefaultStore::default();
    Store::update(
        &store,
        StoreUpdate {
            balance: Some(balance(101., 37., 64., -11.)),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let old = ready(fetch_balance(
        &store,
        || Ok(async { Ok(None) }),
        |warning| panic!("unexpected refusal: {warning}"),
    ))
    .unwrap();
    let same_old = ready(fetch_balance(
        &store,
        || Ok(async { Ok(None) }),
        |warning| panic!("unexpected refusal: {warning}"),
    ))
    .unwrap();
    assert_eq!(
        [old.total, old.free, old.used, old.upnl],
        [101., 37., 64., -11.],
        "BITRUE-FALLBACK initial native balance"
    );
    assert!(
        std::ptr::eq(&*old, &*same_old),
        "BITRUE-FALLBACK repeated read aliases current object"
    );
    assert!(
        std::ptr::eq(&*old, &*Store::balance(&store)),
        "BITRUE-FALLBACK current Store identity"
    );

    // This ordinary regression test must panic on the old Store-wide Ref guard.
    // Both old results remain alive and are observed after the unrelated write.
    eprintln!("BITRUE-FALLBACK unrelated ticker write with fallback results alive");
    Store::update(
        &store,
        StoreUpdate {
            tickers: Some(vec![Ticker {
                id: "BTCUSDT".into(),
                symbol: "BTCUSDT".into(),
                bid: 31.,
                ask: 43.,
                last: 37.,
                mark: 41.,
                index: 39.,
                mid_price: None,
                percentage: 3.,
                open_interest: 7.,
                funding_rate: 0.,
                funding_time: 0.,
                volume: 23.,
                quote_volume: 29.,
                timestamp: None,
                info: None,
            }]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        Store::tickers(&store).len(),
        1,
        "BITRUE-FALLBACK native ticker write count"
    );
    assert_eq!(
        Store::tickers(&store)[0].last,
        37.,
        "BITRUE-FALLBACK native ticker write value"
    );
    assert_eq!(
        old.total, 101.,
        "BITRUE-FALLBACK retained after ticker write"
    );
    assert!(
        std::ptr::eq(&*old, &*same_old),
        "BITRUE-FALLBACK aliases survive ticker write"
    );
    assert!(
        std::ptr::eq(&*old, &*Store::balance(&store)),
        "BITRUE-FALLBACK unrelated write preserves object identity"
    );

    Store::update(
        &store,
        StoreUpdate {
            balance: Some(balance(211., 43., 168., -19.)),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        [old.total, old.free, old.used, old.upnl],
        [101., 37., 64., -11.],
        "BITRUE-FALLBACK old object survives replacement"
    );
    assert_eq!(
        same_old.total, 101.,
        "BITRUE-FALLBACK second old alias survives replacement"
    );
    let current = ready(fetch_balance(
        &store,
        || Ok(async { Ok(None) }),
        |warning| panic!("unexpected refusal: {warning}"),
    ))
    .unwrap();
    assert_eq!(
        [current.total, current.free, current.used, current.upnl],
        [211., 43., 168., -19.],
        "BITRUE-FALLBACK current replacement native balance"
    );
    assert!(
        !std::ptr::eq(&*old, &*current),
        "BITRUE-FALLBACK replacement creates a new object"
    );
    assert!(
        std::ptr::eq(&*current, &*Store::balance(&store)),
        "BITRUE-FALLBACK replacement Store identity"
    );
    assert_eq!(
        Store::serialize(&store).balance.total,
        211.,
        "BITRUE-FALLBACK Store snapshot tracks replacement"
    );

    Store::reset(&store).unwrap();
    assert_eq!(
        old.total, 101.,
        "BITRUE-FALLBACK original object survives reset"
    );
    assert_eq!(
        current.total, 211.,
        "BITRUE-FALLBACK replaced object survives reset"
    );
    let reset = ready(fetch_balance(
        &store,
        || Ok(async { Ok(None) }),
        |warning| panic!("unexpected refusal: {warning}"),
    ))
    .unwrap();
    assert_eq!(
        [reset.total, reset.free, reset.used, reset.upnl],
        [0., 0., 0., 0.],
        "BITRUE-FALLBACK reset native balance"
    );
    assert!(
        std::ptr::eq(&*reset, &*Store::balance(&store)),
        "BITRUE-FALLBACK reset Store identity"
    );
}

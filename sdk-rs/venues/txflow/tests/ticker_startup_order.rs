//! Accepted seed publication precedes private account startup.
mod support;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response};
use safe_cex_venue_txflow::{Options, TxFlow};

fn answer(e: &mut Engine<TxFlow>, r: &HttpRequest, now: f64, body: &str) {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn account_read(action: &Action) -> bool {
    matches!(action, Action::Http(r) if r.body.as_deref().is_some_and(|b|
        b.contains("clearinghouseState") || b.contains("frontendOpenOrders")
        || b.contains("userFills") || b.contains("activeAssetData")))
}
#[test]
fn accepted_ticker_seed_is_published_before_any_account_read() {
    let mut e = safe_cex_engine::test_support::seeded_engine(TxFlow::new(Options {
        testnet: true,
        application_id: Some("0x0000000000000000000000000000000000000001".into()),
        secret: Some(format!("0x{:064x}", 1)),
        ..Default::default()
    }));
    e.start(1000.);
    let metadata = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    answer(&mut e, &metadata, 1000., support::META);
    let issued = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.);
    assert!(!issued.iter().any(account_read));
    assert!(!e.store().loaded().tickers);
    let ticker = issued
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r)
                if r.body
                    .as_deref()
                    .is_some_and(|b| b.contains("activeAssetCtx")) =>
            {
                Some(r)
            }
            _ => None,
        })
        .expect("actual prepared and claimed seed");
    answer(&mut e, &ticker, 1001., support::TICKER);
    let accepted: Vec<_> = e.drain_actions().collect();
    assert!(!accepted.iter().any(account_read));
    let cadence = accepted
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1026. => Some(*timer),
            _ => None,
        })
        .expect("native 25ms seed completion cadence");
    e.handle(1026., Event::TimerFired { timer: cadence });
    let started = safe_cex_engine::test_support::prepared_actions(&mut e, 1026.);
    assert!(started.iter().any(account_read));
    assert!(
        e.store().loaded().tickers,
        "accepted seed must publish before account reads"
    );
    assert_eq!(e.store().tickers().len(), 1);
    assert_eq!(e.store().tickers()[0].last, 65000.);
}

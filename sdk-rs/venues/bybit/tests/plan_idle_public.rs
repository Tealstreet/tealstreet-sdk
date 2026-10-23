//! Native public-event regression for the shared Plan scheduler's idle path.
mod support;
use safe_cex_engine::{Command, VenueAction};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::{Harness, Options};

#[test]
fn hyro_public_events_visit_no_plan_cells_with_a_live_context_observer() {
    let mut h = Harness::started(Options {
        variant: Variant::Hyrotrader,
        public_data_only: true,
        endpoints: Some(Endpoints {
            rest: "https://hyro.example".into(),
            public_ws: "wss://hyro.example/v5/public/linear".into(),
            private_ws: "wss://hyro.example/v5/private".into(),
        }),
        ..Options::default()
    });
    h.load_public();
    h.command(Command::Venue {
        command: 71,
        action: VenueAction::RefreshMarkets,
    });
    assert!(h.has_request("/v5/market/instruments-info"));
    assert!(
        h.result(71).is_none(),
        "the genuine Context observer remains live"
    );
    let before = h.engine.plan_scan_visits();
    let started = std::time::Instant::now();
    for _ in 0..10_000 {
        h.message("/public/linear", r#"{"topic":"tickers.BTCUSDT","type":"delta","ts":1735809771618,"data":{"symbol":"BTCUSDT","lastPrice":"66667.00"}}"#);
    }
    let visits = h.engine.plan_scan_visits() - before;
    eprintln!(
        "native Hyro public events=10000 Plan cells visited={visits} elapsed={:?}",
        started.elapsed()
    );
    assert_eq!(
        visits, 0,
        "ordinary Context/execution observers must not be scanned by Plan work"
    );
    assert!(h.result(71).is_none());
    assert_eq!(h.engine.store().ticker("BTCUSDT").unwrap().last, 66667.0);
}

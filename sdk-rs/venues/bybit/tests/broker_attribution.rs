use safe_cex_engine::Action;
use safe_cex_venue_bybit::BybitLinear;

#[test]
fn public_catalogue_preserves_factory_broker_attribution_without_credentials() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(BybitLinear::new(
        safe_cex_venue_bybit::Options {
            public_data_only: true,
            ..safe_cex_venue_bybit::Options::default()
        },
    ));
    engine.start(0.0);
    let requests: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(!requests.is_empty());
    for request in requests {
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| *name == "X-Referer" && value == "Tealstreet"),
            "{:?}",
            request
        );
    }
}

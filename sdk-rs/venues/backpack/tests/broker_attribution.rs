use safe_cex_engine::{Action, Engine};
use safe_cex_venue_backpack::{Backpack, Options};

#[test]
fn public_catalogue_preserves_factory_broker_attribution_without_credentials() {
    let mut engine = Engine::new(Backpack::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
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
                .any(|(name, value)| *name == "X-Broker-Id" && value == "1200"),
            "{:?}",
            request
        );
    }
}

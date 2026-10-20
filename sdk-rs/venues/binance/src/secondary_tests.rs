use super::*;
use safe_cex_engine::{Action, Engine, ReadCredentials};
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),

        ..Options::default()
    }
}

struct Probe(BinanceFutures);
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            self.0.signed(
                cx,
                method,
                "/secondary-test",
                sign::Query::new(),
                Pending::Daily,
            );
        }
    }

    fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
    fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
    fn on_response(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Pending>, _: Pending, _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Pending>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Pending>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    for variant in [
        Variant::UsdM,
        Variant::PortfolioMargin,
        Variant::Spot,
        Variant::CoinM,
    ] {
        let mut opts = options();
        opts.variant = variant;
        let mut engine = Engine::new(Probe(BinanceFutures::new(opts)));
        engine.start(1700000000000.0);
        let requests: Vec<_> = engine
            .drain_actions()
            .filter_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .filter(|r| r.url.contains("secondary-test"))
            .collect();
        assert_eq!(requests.len(), 4);
        for request in requests {
            let expected = if request.method == Method::Get {
                "read-key"
            } else {
                "primary-key"
            };
            let opts = options();
            let secret = if request.method == Method::Get {
                opts.read_credentials.as_ref().unwrap().secret.as_str()
            } else {
                opts.secret.as_deref().unwrap()
            };
            let credentials = sign::Credentials {
                key: expected.into(),
                secret: secret.into(),
            };
            assert!(
                request
                    .url
                    .ends_with(&sign::Query::new().signed(&credentials, 1700000000000.0))
            );
            assert!(
                request
                    .headers
                    .iter()
                    .any(|(name, value)| name.eq_ignore_ascii_case("X-MBX-APIKEY")
                        && value == expected),
                "{:?}",
                request
            );
        }
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = Engine::new(BinanceFutures::new(opts));
    engine.start(1700000000000.0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::WsConnectWithHeaders { .. }
    )));
}

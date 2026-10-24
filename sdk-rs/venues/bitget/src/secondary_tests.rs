use super::*;
use safe_cex_engine::signing::Provider;

pub(crate) fn setup() -> (BitgetFutures, Provider) {
    let (provider, ids) = safe_cex_crypto::signer::LocalSigner::from_json(r#"{"primary":{"key":"primary-key","secret":"AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=","passphrase":"primary-pass"},"secondary":{"key":"read-key","secret":"AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=","passphrase":"read-pass"}}"#.into()).unwrap();
    (
        BitgetFutures::new(Options {
            credentials: ids.primary,
            read_credentials: ids.secondary,
            ..Default::default()
        }),
        std::sync::Arc::new(provider),
    )
}

pub(crate) fn headers(
    venue: BitgetFutures,
    provider: Provider,
    method: Method,
    path: &str,
) -> Vec<(&'static str, String)> {
    struct Probe {
        venue: BitgetFutures,
        method: Method,
        path: String,
    }
    impl Venue for Probe {
        type Pending = Pending;
        fn start(&mut self, cx: &mut Cx<'_, Pending>) {
            let context = cx
                .http_context(
                    safe_cex_engine::request_context::Scope::Background,
                    Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    },
                )
                .unwrap();
            let pending = Pending::Markets {
                context,
                rows: vec![],
                invalid: false,
            };
            self.venue.send(
                cx,
                self.method,
                &self.path,
                Payload::Body(None),
                true,
                pending,
            );
        }
        fn on_open(&mut self, _: &mut Cx<'_, Pending>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, Pending>, _: SocketId, _: u16) {}
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
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue,
        method,
        path: path.into(),
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1_700_000_000_000.);
    engine
        .drain_actions()
        .find_map(|a| match a {
            safe_cex_engine::Action::Http(http) => Some(http.headers),
            _ => None,
        })
        .unwrap()
}

#[test]
fn secondary_get_keeps_primary_mutations() {
    for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
        let (venue, provider) = setup();
        let headers = headers(venue, provider, method, "/secondary-test");
        let (key, pass, material) = if method == Method::Get {
            (
                "read-key",
                "read-pass",
                "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
            )
        } else {
            (
                "primary-key",
                "primary-pass",
                "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
            )
        };
        assert_eq!(
            headers,
            sign::oracle::headers(
                key,
                material,
                pass,
                1_700_000_000_000.,
                method.as_str(),
                "/secondary-test",
                ""
            )
        );
        assert!(headers.contains(&("ACCESS-KEY", key.into())));
        assert!(headers.contains(&("ACCESS-PASSPHRASE", pass.into())));
    }
}

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

struct Probe(Kraken);
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            self.0
                .signed_request(
                    cx,
                    method,
                    "/derivatives/api/v3/secondary-test",
                    String::new(),
                    Pending::Markets {
                        rows: vec![],
                        failed: false,
                    },
                )
                .unwrap();
        }
        self.0
            .signed_streamed(
                cx,
                Method::Get,
                "/api/history/v3/account-log",
                String::new(),
                safe_cex_engine::json::ArrayAt::Root,
                Pending::Markets {
                    rows: vec![],
                    failed: false,
                },
            )
            .unwrap();
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
    let mut engine = Engine::new(Probe(Kraken::new(options())));
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
        let mut signer = sign::Signer::new(secret).unwrap();
        let nonce = request
            .headers
            .iter()
            .find(|(k, _)| *k == "Nonce")
            .unwrap()
            .1
            .as_str();
        assert!(
            nonce.parse::<u64>().unwrap()
                >= signer
                    .nonce(1700000000000.0)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
        );
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(k, _)| *k == "Authent")
                .unwrap()
                .1,
            signer
                .signature("", nonce, "/derivatives/api/v3/secondary-test")
                .unwrap()
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("APIKey") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = Engine::new(Kraken::new(opts));
    engine.start(1700000000000.0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::WsConnectWithHeaders { .. }
    )));
}

#[test]
fn equal_api_keys_share_nonce_clock_but_keep_selected_signing_secrets() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().key = "primary-key".into();
    let mut engine = Engine::new(Probe(Kraken::new(opts)));
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
        .collect();
    assert_eq!(requests.len(), 5);
    let mut previous = 0;
    for request in requests {
        let nonce = request
            .headers
            .iter()
            .find(|(k, _)| *k == "Nonce")
            .unwrap()
            .1
            .as_str();
        let n = nonce.parse::<u64>().unwrap();
        assert!(n > previous, "same API key reused nonce");
        previous = n;
        assert!(request.headers.contains(&("APIKey", "primary-key".into())));
        let opts = options();
        let secret = if request.method == Method::Get && !request.url.contains("/api/history/") {
            &opts.read_credentials.as_ref().unwrap().secret
        } else {
            opts.secret.as_ref().unwrap()
        };
        let signature = sign::Signer::new(secret)
            .unwrap()
            .signature(
                "",
                nonce,
                if request.url.contains("/api/history/") {
                    "/api/history/v3/account-log"
                } else {
                    "/derivatives/api/v3/secondary-test"
                },
            )
            .unwrap();
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(k, _)| *k == "Authent")
                .unwrap()
                .1,
            signature
        );
    }
}

#[test]
fn history_client_keeps_primary_credentials_and_explicit_primary_route() {
    let mut engine = Engine::new(Probe(Kraken::new(options())));
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
        .filter(|r| r.url.contains("/api/history/"))
        .collect();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].route,
        Some(safe_cex_engine::RequestRoute::Primary)
    );
    assert!(
        requests[0]
            .headers
            .contains(&("APIKey", "primary-key".into()))
    );
}

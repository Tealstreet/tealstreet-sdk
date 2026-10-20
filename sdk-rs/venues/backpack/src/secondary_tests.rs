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

struct Probe(Backpack);
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        for method in [Method::Get, Method::Post, Method::Delete] {
            self.0.account_start(cx);
            let path = if method == Method::Get {
                "/api/v1/orders"
            } else {
                "/api/v1/order"
            };
            self.0
                .signed(
                    cx,
                    method,
                    path,
                    Default::default(),
                    None,
                    Pending::Markets {
                        rows: vec![],
                        skipped: vec![],
                    },
                    false,
                )
                .unwrap();
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
    let mut engine = Engine::new(Probe(Backpack::new(options())));
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
        .filter(|r| r.url.ends_with("/api/v1/orders") || r.url.ends_with("/api/v1/order"))
        .collect();
    assert_eq!(requests.len(), 3);
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
        let credentials = sign::Credentials::new(expected.into(), secret).unwrap();
        let instruction = match request.method {
            Method::Get => "orderQueryAll",
            Method::Post => "orderExecute",
            _ => "orderCancel",
        };
        assert_eq!(
            request.headers,
            credentials.headers(instruction, &[Default::default()], 1700000000000.0)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-API-Key") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = Engine::new(Backpack::new(opts));
    engine.start(1700000000000.0);
    assert!(!engine.drain_actions().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::WsConnectWithHeaders { .. }
    )));
}

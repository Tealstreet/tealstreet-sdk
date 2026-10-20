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

struct Probe(Cryptocom);
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.0.signed(
            cx,
            "private/get-trades",
            &sign::Params::default(),
            Pending::Markets {
                rows: vec![],
                indexes: Default::default(),
                skipped: Default::default(),
            },
            false,
        );
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
fn posted_read_keeps_primary_identity_with_secondary_configured() {
    let mut engine = Engine::new(Probe(Cryptocom::new(options())));
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
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, Method::Post);
    let expected = sign::Credentials {
        key: "primary-key".into(),
        secret: options().secret.unwrap(),
    }
    .envelope(
        "private/get-trades",
        &sign::Params::default(),
        1700000000000.0,
        false,
    );
    assert_eq!(requests[0].body.as_deref(), Some(expected.as_str()));
}

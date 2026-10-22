//! Component vectors through the injected provider, not production adoption.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{Identity, Provider};
use safe_cex_engine::{Action, Command, Cx, Engine, Method, Response, SocketId, TimerId, Venue};
use safe_cex_venue_weex::dispatch_signing::{Http, Upgrade};

struct Probe {
    identity: Identity,
    method: Method,
    path: String,
    body: Option<String>,
    upgrade: bool,
    alter: u8,
}
impl Venue for Probe {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        if self.upgrade {
            let step = Upgrade::new(self.identity.clone());
            let mut spec = step.unsigned();
            if self.alter == 1 {
                spec.url.push_str("/wrong");
            }
            cx.connect_signed(spec, Box::new(step));
        } else {
            let step = Http::new(
                self.identity.clone(),
                self.method,
                self.path.clone(),
                self.body.clone(),
            );
            let (mut url, headers, body) = step.unsigned();
            if self.alter == 1 {
                url.push_str("/wrong");
            }
            let id = cx.request(self.method, url, headers, body, ());
            cx.sign_http(id, Box::new(step));
        }
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
fn setup() -> (Identity, Provider) {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("k".into()),
            secret: Secret::new("secret".into()),
            passphrase: Some(Secret::new("p".into())),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    (identities.primary.unwrap(), std::sync::Arc::new(provider))
}
fn run(method: Method, path: &str, body: Option<&str>, upgrade: bool, alter: u8) -> Vec<Action> {
    let (identity, provider) = setup();
    let mut engine: Engine<Probe> = safe_cex_engine::test_support::seeded_engine(Probe {
        identity,
        method,
        path: path.into(),
        body: body.map(str::to_owned),
        upgrade,
        alter,
    });
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1234.9);
    engine.drain_actions().collect()
}
fn headers(expected_mac: &str) -> Vec<(&'static str, String)> {
    vec![
        ("ACCESS-KEY", "k".into()),
        ("ACCESS-PASSPHRASE", "p".into()),
        ("ACCESS-TIMESTAMP", "1234".into()),
        ("ACCESS-SIGN", expected_mac.into()),
        ("Content-Type", "application/json".into()),
        ("User-Agent", "Tealstreet".into()),
    ]
}
#[test]
fn exact_get_and_post_vectors_use_provider_owned_passphrase_and_raw_mac() {
    for (method, path, body, mac) in [
        (
            Method::Get,
            "/capi/v3/openOrders?limit=100&page=0",
            None,
            "bCU4mL/nryWPHh2ijQKaUlRR81jLGrRazQ5ZFQPfR+E=",
        ),
        (
            Method::Post,
            "/capi/v3/order",
            Some(r#"{"symbol":"BTCUSDT"}"#),
            "LGImWmNEu6Q2o2LhDIPtwbUT0PBJLHsWBbY7mB0+0JY=",
        ),
    ] {
        let actions = run(method, path, body, false, 0);
        let requests = actions
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requests.len(),
            1,
            "the provider must release one native request"
        );
        assert_eq!(
            requests[0].url,
            format!("https://api-contract.weex.com{path}")
        );
        assert_eq!(requests[0].body.as_deref(), body);
        assert_eq!(requests[0].headers, headers(mac));
    }
}
#[test]
fn exact_upgrade_vector_has_no_get_prefix_or_login_frame() {
    let actions = run(Method::Get, "", None, true, 0);
    let connects = actions
        .iter()
        .filter_map(|action| match action {
            Action::WsConnect {
                url,
                headers,
                channel,
                ..
            } => Some((url, headers, channel)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(connects.len(), 1, "signed upgrade must release once");
    assert_eq!(connects[0].0, "wss://ws-contract.weex.com/v3/ws/private");
    assert_eq!(
        connects[0].1,
        &headers("8IL8D6LBPFWJeTejIF/swS90xLfKkMPm++NyP0LDezA=")
    );
    assert_eq!(
        *connects[0].2,
        Some(safe_cex_engine::SocketChannel::Private)
    );
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::WsSend { .. }))
    );
}
#[test]
fn altered_url_never_releases_http_or_an_unauthenticated_upgrade() {
    for upgrade in [false, true] {
        assert!(
            !run(Method::Get, "/capi/v3/openOrders", None, upgrade, 1)
                .iter()
                .any(|action| matches!(action, Action::Http(_) | Action::WsConnect { .. }))
        );
    }
}

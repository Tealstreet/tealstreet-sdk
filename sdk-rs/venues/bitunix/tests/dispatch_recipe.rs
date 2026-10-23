//! The exported dispatch recipe reproduces the legacy double-SHA256 signature
//! through the real engine dispatcher and local provider, with a fresh
//! signing-entropy nonce and protocol stamp at each preparation.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::Identity;
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{Action, Command, Cx, Engine, Method, Response, SignStep};
use safe_cex_engine::{SocketId, TimerId, Venue};
use safe_cex_venue_bitunix::sign::{self, Credentials, Dispatch, Params};
use std::sync::Arc;

const REST: &str = "https://fapi.bitunix.com/api/v1/futures/trade/place_order";
const USER: &str = "wss://fapi.bitunix.com/private/";
const NOW: f64 = 1_700_000_000_123.7;

fn provider() -> (Arc<LocalSigner>, Identity) {
    let (signer, ids) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("yourApiKey".into()),
            secret: Secret::new("yourSecretKey".into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

fn legacy() -> Credentials {
    Credentials {
        key: "yourApiKey".into(),
        secret: "yourSecretKey".into(),
    }
}

struct Issue(Vec<(bool, Box<dyn SignStep>)>);
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for (http, step) in self.0.drain(..) {
            if http {
                let id = cx.request(Method::Post, REST.into(), vec![], Some(BODY.into()), ());
                cx.sign_http(id, step);
            } else {
                let socket = cx.connect(USER);
                cx.send_signed(socket, String::new(), step);
            }
        }
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

const BODY: &str = r#"{"symbol":"BTCUSDT","qty":"0.01","side":"BUY"}"#;

fn header<'a>(headers: &'a [(&'static str, String)], name: &str) -> &'a str {
    &headers.iter().find(|(k, _)| *k == name).unwrap().1
}

#[test]
fn rest_and_login_recipes_match_the_legacy_signature_with_fresh_nonces() {
    let (signer, identity) = provider();
    let params = Params::default().str("uid", "200").str("id", "1");
    let steps: Vec<(bool, Box<dyn SignStep>)> = vec![
        (
            true,
            Box::new(Dispatch::http(
                identity.clone(),
                "POST",
                REST,
                &params,
                BODY,
            )),
        ),
        (
            true,
            Box::new(Dispatch::http(
                identity.clone(),
                "POST",
                REST,
                &params,
                BODY,
            )),
        ),
        (false, Box::new(Dispatch::login(identity, USER))),
    ];
    let mut engine = Engine::new(Issue(steps), Some(seeded_entropy(9))).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    let actions = prepared_actions(&mut engine, NOW);
    let https: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect();
    assert_eq!(https.len(), 2);
    for http in &https {
        let nonce = header(&http.headers, "nonce");
        assert_eq!(header(&http.headers, "timestamp"), "1700000000123");
        assert_eq!(header(&http.headers, "api-key"), "yourApiKey");
        assert_eq!(
            header(&http.headers, "sign"),
            sign::signature(&legacy(), nonce, "1700000000123", "id1uid200", BODY)
        );
        assert_eq!(http.body.as_deref(), Some(BODY));
    }
    assert_ne!(
        header(&https[0].headers, "nonce"),
        header(&https[1].headers, "nonce"),
        "each preparation draws a fresh nonce"
    );
    let text = actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("signed login released");
    let login: serde_json::Value = serde_json::from_str(&text).unwrap();
    let args = &login["args"][0];
    let nonce = args["nonce"].as_str().unwrap();
    assert_eq!(login["op"], "login");
    assert_eq!(args["apiKey"], "yourApiKey");
    assert_eq!(args["timestamp"], "1700000000123");
    assert_eq!(
        args["sign"],
        sign::signature(&legacy(), nonce, "1700000000123", "", "")
    );
    assert_eq!(
        text,
        format!(
            r#"{{"op":"login","args":[{{"apiKey":"yourApiKey","timestamp":"1700000000123","nonce":"{nonce}","sign":"{}"}}]}}"#,
            args["sign"].as_str().unwrap()
        )
    );
    assert!(!text.contains("yourSecretKey"));
}

#[test]
fn recipes_are_bound_to_their_transport() {
    let (signer, identity) = provider();
    let params = Params::default();
    // A login recipe on an HTTP request and a REST recipe on a socket both refuse.
    let steps: Vec<(bool, Box<dyn SignStep>)> = vec![
        (true, Box::new(Dispatch::login(identity.clone(), USER))),
        (
            false,
            Box::new(Dispatch::http(identity, "POST", REST, &params, "")),
        ),
    ];
    let mut engine = Engine::new(Issue(steps), Some(seeded_entropy(9))).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    assert!(
        prepared_actions(&mut engine, NOW)
            .iter()
            .all(|action| !matches!(action, Action::Http(_) | Action::WsSend { .. })),
        "mismatched recipes never sign"
    );
}

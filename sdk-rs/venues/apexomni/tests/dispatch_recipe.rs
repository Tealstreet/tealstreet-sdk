//! Exported dispatch recipes reproduce the legacy ApeX bytes through the real
//! engine dispatcher with the declared base64Text key and a provider zkLink seed.
use safe_cex_crypto::signer::{
    LocalExtra, LocalExtras, LocalIdentity, LocalSetup, LocalSigner, Secret,
};
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, Identity,
    IdentitySetupPolicy, KeyEncoding,
};
use safe_cex_engine::test_support::{prepared_actions, seeded_entropy};
use safe_cex_engine::{
    Action, Command, Cx, Engine, HttpRequest, Response, SignStep, SocketId, TimerId, Venue,
};
use safe_cex_venue_apexomni::{Options, sign};
use std::sync::Arc;

const NOW: f64 = 1_700_000_000_123.0;
const REST: &str = "https://omni.apex.exchange/api";
/// The 65-byte packed Ethereum signature used by the npm-vector tests.
fn seed() -> String {
    format!("{}1b", "01".repeat(64))
}

fn legacy_options() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("mysecret".into()),
        passphrase: Some("pass".into()),
        omni_key_seed: Some(seed()),
        ..Options::default()
    }
}

fn provider() -> (Arc<LocalSigner>, Identity) {
    let mut extras = LocalExtras::default();
    extras.0.insert(
        ExtraKey::ZklinkSeed,
        LocalExtra {
            value: Secret::new(seed()),
            encoding: KeyEncoding::Hex,
        },
    );
    let policy = CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Base64Text),
            extra: Some(ExtraKeyPolicy {
                key: ExtraKey::ZklinkSeed,
                field: ExtraFieldPolicy::RequiredForPrivate,
                encoding: Some(KeyEncoding::Hex),
            }),
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, ids) = LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("api-key".into()),
                secret: Secret::new("mysecret".into()),
                passphrase: Some(Secret::new("pass".into())),
                extra: extras,
                ..Default::default()
            }),
            secondary: None,
        },
        policy,
    )
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

/// Issues each recipe from start; every other event is inert.
struct Issue(Vec<(Option<sign::Request>, Box<dyn SignStep>)>);
impl Venue for Issue {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        for (request, step) in self.0.drain(..) {
            match request {
                Some(request) => {
                    let path = request.path.clone();
                    let query = sign::query(&request.params);
                    let url = if query.is_empty() {
                        format!("{REST}{path}")
                    } else {
                        format!("{REST}{path}?{query}")
                    };
                    let id = cx.request(request.method, url, vec![], None, ());
                    cx.sign_http(id, step);
                }
                None => {
                    let socket = cx.connect("wss://quote.omni.apex.exchange/realtime_private");
                    cx.send_signed(socket, String::new(), step);
                }
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

fn run(steps: Vec<(Option<sign::Request>, Box<dyn SignStep>)>) -> Vec<Action> {
    let (signer, _) = provider();
    let mut engine = Engine::new(Issue(steps), Some(seeded_entropy(3))).unwrap();
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    prepared_actions(&mut engine, NOW)
}

fn fields(values: &[(&str, &str)]) -> Vec<(String, String)> {
    values
        .iter()
        .map(|(k, v)| ((*k).into(), (*v).into()))
        .collect()
}

fn https(actions: &[Action]) -> Vec<&HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(http) => Some(http),
            _ => None,
        })
        .collect()
}

#[test]
fn rest_recipes_match_legacy_headers_and_bodies_for_every_body_kind_and_legacy_resend() {
    let (_, identity) = provider();
    let requests = [
        sign::Request::get(
            "/v3/account",
            fields(&[("symbol", "BTC-USDT"), ("limit", "0")]),
        ),
        sign::Request::form("/v3/delete-order", fields(&[("id", "42")])),
        sign::Request::multipart(
            "/v3/set-initial-margin-rate",
            fields(&[("symbol", "BTC-USDT"), ("initialMarginRate", "0.1")]),
        ),
        sign::Request::raw("/v3/order", "clientId=abc&reduceOnly=false&size=1".into()),
    ];
    let mut steps: Vec<(Option<sign::Request>, Box<dyn SignStep>)> = vec![];
    for legacy in [false, true] {
        for request in &requests {
            steps.push((
                Some(request.clone()),
                Box::new(sign::Http::new(identity.clone(), request.clone(), legacy)),
            ));
        }
    }
    let actions = run(steps);
    let sent = https(&actions);
    assert_eq!(sent.len(), requests.len() * 2);
    for (index, http) in sent.iter().enumerate() {
        let legacy = index >= requests.len();
        let expected = sign::request(
            &legacy_options(),
            NOW,
            &requests[index % requests.len()],
            legacy,
        )
        .unwrap();
        assert_eq!(http.headers, expected.headers, "case {index}");
        assert_eq!(http.body, expected.body, "case {index}");
        assert!(http.url.ends_with(&expected.path), "case {index}");
    }
}

#[test]
fn order_recipe_signs_zklink_contracts_in_the_same_batch_as_the_hmac() {
    let (_, identity) = provider();
    let account = "611779989544632629";
    let contract = |id: &str, buy: bool| {
        sign::contract_payload(id, account, 1, "0.001", "60000", "0.0002", "0.0005", buy).unwrap()
    };
    let signature = |id: &str, buy: bool| {
        sign::contract_signature(
            &seed(),
            id,
            account,
            1,
            "0.001",
            "60000",
            "0.0002",
            "0.0005",
            buy,
        )
        .unwrap()
    };
    let body = |main: &str, tp: &str| {
        sign::query(&fields(&[
            ("clientId", "client-1"),
            ("signature", main),
            ("size", "0.001"),
            ("tpSignature", tp),
        ]))
    };
    let template = body(&sign::zk_placeholder(0), &sign::zk_placeholder(1));
    let request = sign::Request::raw("/v3/order", template);
    let actions = run(vec![(
        Some(request.clone()),
        Box::new(sign::Http::with_contracts(
            identity,
            request,
            false,
            vec![contract("client-1", true), contract("leg-1", false)],
        )),
    )]);
    let http = https(&actions)[0];
    let expected = sign::request(
        &legacy_options(),
        NOW,
        &sign::Request::raw(
            "/v3/order",
            body(&signature("client-1", true), &signature("leg-1", false)),
        ),
        false,
    )
    .unwrap();
    assert_eq!(http.body, expected.body);
    assert_eq!(http.headers, expected.headers);
    assert!(!http.body.as_deref().unwrap().contains("PLACEHOLDER"));
}

#[test]
fn ws_login_recipe_matches_the_legacy_frame_and_klines_are_never_signed() {
    let (_, identity) = provider();
    let actions = run(vec![(
        None,
        Box::new(sign::WsLogin::new(
            identity.clone(),
            "wss://quote.omni.apex.exchange/realtime_private",
        )),
    )]);
    let text = actions
        .iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("signed login released");
    assert_eq!(text, sign::ws_login(&legacy_options(), NOW).unwrap());
    assert!(!text.contains("mysecret"));

    let klines = sign::Request::get("/v3/klines", fields(&[("symbol", "BTCUSDT")]));
    let actions = run(vec![(
        Some(klines.clone()),
        Box::new(sign::Http::new(identity, klines, false)),
    )]);
    assert!(
        https(&actions).iter().all(|http| !http
            .headers
            .iter()
            .any(|(name, _)| name.starts_with("APEX-"))),
        "a credential recipe refuses public klines instead of signing them"
    );
}

#[test]
fn missing_seed_or_mismatched_placeholders_refuse_before_any_credential_leaves() {
    let (_, identity) = provider();
    let mut seedless = identity.clone();
    seedless.extra.clear();
    let template = sign::Request::raw(
        "/v3/order",
        format!("signature={}", sign::zk_placeholder(0)),
    );
    let wrong = sign::Request::raw("/v3/order", "signature=missing".into());
    let payload = [0u8; 36];
    for (identity, request) in [(seedless, template.clone()), (identity, wrong)] {
        let actions = run(vec![(
            Some(request.clone()),
            Box::new(sign::Http::with_contracts(
                identity,
                request,
                false,
                vec![payload],
            )),
        )]);
        assert!(
            https(&actions)
                .iter()
                .all(|http| !http.headers.iter().any(|(_, value)| value == "api-key")),
            "no presented credential on refusal"
        );
    }
}

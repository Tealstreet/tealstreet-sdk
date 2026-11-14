//! The exported dispatch recipe reproduces the legacy local envelope bytes
//! through the real engine dispatcher and an injected local provider.
use safe_cex_engine::ControlFrame;
mod support;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Identity;
use safe_cex_engine::signing::{
    Algorithm, Capability, KeyRef, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions};
use safe_cex_engine::{Action, Engine, Method};
use safe_cex_venue_cryptocom::Cryptocom;
use safe_cex_venue_cryptocom::sign::{Envelope, Params};
use std::sync::Arc;
use support::fixture::Options;

const REST: &str = "https://api.crypto.com/exchange/v1/private/create-order";
const USER: &str = "wss://stream.crypto.com/exchange/v1/user";
const NOW: f64 = 1_610_000_000_000.4;

fn provider() -> (Arc<LocalSigner>, Identity) {
    let (signer, ids) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("replay-key".into()),
            secret: Secret::new("replay-secret".into()),
            ..Default::default()
        }),
        secondary: None,
    })
    .unwrap();
    (Arc::new(signer), ids.primary.unwrap())
}

fn params() -> Params {
    Params::default()
        .text("type", "MARKET")
        .text("side", "SELL")
        .text("quantity", "0.01")
        .text("instrument_name", "BTCUSD-PERP")
        .strings("exec_inst", vec!["REDUCE_ONLY".into()])
}

struct MetadataSigner {
    inner: Arc<LocalSigner>,
    ws_headers: bool,
}
impl Signer for MetadataSigner {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.inner.sign_now(request).map(|mut result| {
            if self.ws_headers || request.context.method != "WS" {
                result.headers.push(("X-Trace".into(), "keep".into()));
            }
            result
        }))
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.sign_ready(&request).unwrap()))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.inner.can_sign(key, algorithm)
    }
}

fn signed_actions(metadata: Option<bool>) -> Vec<Action> {
    let (signer, identity) = provider();
    let ws_identity = identity.clone();
    let venue = ProbeVenue::new(
        support::fixture::venue(Options::default()),
        move |_: &mut Cryptocom, cx: &mut safe_cex_engine::Cx<'_, _>| {
            // ProbeVenue ignores this carrier on response; the signed request
            // remains the same legacy recipe control, not a managed Read.
            let context = cx
                .http_context(
                    safe_cex_engine::request_context::Scope::Background,
                    safe_cex_engine::request_context::Policy {
                        operation: safe_cex_engine::request_context::Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: safe_cex_engine::request_context::RetryPolicy::Never,
                    },
                )
                .unwrap();
            let request = cx.request(
                Method::Post,
                REST.into(),
                vec![("Content-Type", "application/json".into())],
                None,
                safe_cex_venue_cryptocom::Pending::Tickers {
                    context,
                    rows: vec![],
                    skipped: Default::default(),
                },
            );
            cx.sign_http(
                request,
                Box::new(Envelope::http(
                    identity.clone(),
                    "private/create-order",
                    params(),
                    REST,
                )),
            );
            let user = cx.connect(USER);
            cx.send_signed(
                user,
                ControlFrame::authentication(String::new()),
                Box::new(Envelope::ws_auth(ws_identity.clone(), USER)),
            );
        },
    );
    let mut engine = Engine::new(
        venue,
        Some(safe_cex_engine::test_support::seeded_entropy(7)),
    )
    .unwrap();
    let signer: safe_cex_engine::signing::Provider = match metadata {
        Some(ws_headers) => Arc::new(MetadataSigner {
            inner: signer,
            ws_headers,
        }),
        None => signer,
    };
    engine.set_signer(Some(signer)).unwrap();
    engine.start(NOW);
    prepared_actions(&mut engine, NOW)
}

#[test]
fn http_metadata_preserves_json_signature_and_ws_local_parity() {
    let control = signed_actions(None);
    let decorated = signed_actions(Some(false));
    let http = |actions: &[Action]| {
        actions
            .iter()
            .find_map(|action| match action {
                Action::Http(http) => Some(http.clone()),
                _ => None,
            })
            .expect("one physical signed HTTP request")
    };
    let with_headers = http(&decorated);
    assert_eq!(with_headers.body, http(&control).body);
    assert!(
        with_headers
            .signer_headers
            .iter()
            .any(|(name, value)| name == "X-Trace" && value == "keep")
    );
    assert!(
        !with_headers
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("sig"))
    );
    let ws = |actions: &[Action]| {
        actions
            .iter()
            .find_map(|action| match action {
                Action::WsSend { text, .. } => Some(text.clone()),
                _ => None,
            })
            .expect("signed WebSocket authentication")
    };
    assert_eq!(ws(&decorated), ws(&control));
}

#[test]
fn ws_metadata_is_refused_without_publishing_a_partial_authentication() {
    let actions = signed_actions(Some(true));
    assert!(actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!actions.iter().any(|a| matches!(a, Action::WsSend { .. })));
}

#[test]
fn rest_and_ws_auth_recipes_match_the_legacy_envelope_bytes() {
    let actions = signed_actions(None);
    let body = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(http) => http.body.clone(),
            _ => None,
        })
        .expect("signed REST body released");
    assert_eq!(
        body,
        r#"{"id":1610000000000,"method":"private/create-order","api_key":"replay-key","params":{"exec_inst":["REDUCE_ONLY"],"instrument_name":"BTCUSD-PERP","quantity":"0.01","side":"SELL","type":"MARKET"},"sig":"e9f8f06c03ce0214c03b5157d0a71fe3407d018fd0d96966a17c250bf93bd8bf","nonce":1610000000000}"#
    );
    assert!(body.contains("e9f8f06c03ce0214c03b5157d0a71fe3407d018fd0d96966a17c250bf93bd8bf"));
    assert!(body.contains(r#""id":1610000000000"#) && body.contains(r#""nonce":1610000000000"#));

    let text = actions
        .into_iter()
        .find_map(|action| match action {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .expect("signed public/auth released");
    assert_eq!(
        text,
        r#"{"id":1610000000000,"method":"public/auth","api_key":"replay-key","sig":"57a5207e795bd876080625ee876fa750cd0134c3916bcdc346d4efe1b6f0218f","nonce":1610000000000}"#
    );
    assert!(!text.contains("params") && !text.contains("replay-secret"));
}

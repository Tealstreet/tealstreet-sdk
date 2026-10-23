#![cfg(all(
    feature = "dispatch-signing",
    feature = "asterdex",
    feature = "coinbase",
    feature = "orderly",
    feature = "paradex"
))]
//! P8 native both-halves proof, one per recipe family. The server engine holds
//! no key: its step binds a recipe, the remote description crosses beside the
//! sign work, a separate browser account signs it through the hosted executor,
//! and the server verifies before release wherever a public key exists.

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::book::Book;
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Identity, KeyRef, Operation, PayloadPart, SignError,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::signing_remote::{RemoteOp, RemoteVerifier};
use safe_cex_engine::{
    Action, Command, Cx, Event, Method, Response, SignStep, SocketId, TimerId, Venue,
};
use safe_cex_venues::hosted_signing::{self, Input};

/// One server step: the payload and its remote description come from the
/// same production canonicalizer, so they cannot diverge.
struct RecipeStep {
    op: RemoteOp,
    present: bool,
}
impl SignStep for RecipeStep {
    fn prepare(
        &mut self,
        _: &mut Action,
        _: &safe_cex_engine::DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        let payload = self.op.recipe.canonicalize(&self.op.inputs)?;
        let mut operations = Vec::new();
        if self.present {
            operations.push(Operation::Present {
                key: KeyRef("server.credential".into()),
            });
        }
        operations.push(Operation::Sign {
            algorithm: self.op.recipe.algorithm(),
            key: KeyRef("server.signing".into()),
            payload: vec![PayloadPart::Bytes { bytes: payload }],
        });
        Ok(SignRequest {
            receipt: Default::default(),
            context: Context::default(),
            operations,
        })
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        let signature = result.values.last().ok_or(SignError::InvalidResult)?;
        http.headers
            .push(("x-signature", safe_cex_crypto::hex(signature)));
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        5000.
    }
    fn remote(&self) -> Option<&RemoteOp> {
        Some(&self.op)
    }
}

struct Proof {
    step: Option<RecipeStep>,
    failed: Vec<String>,
}
impl Venue for Proof {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let request = cx.request(
            Method::Post,
            "https://venue.test/signed".into(),
            vec![],
            None,
            (),
        );
        cx.sign_http(request, Box::new(self.step.take().unwrap()));
    }
    fn on_sign_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        self.failed.push(error.into());
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), error: &str) {
        self.failed.push(error.into());
    }
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// The browser account: its own provider and refs, never the server's.
fn browser(credential: &str, signing: Vec<u8>) -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    signer.insert(
        KeyRef("browser.credential".into()),
        credential.as_bytes().to_vec(),
    );
    signer.insert(KeyRef("browser.signing".into()), signing);
    let identity = Identity {
        credential: KeyRef("browser.credential".into()),
        signing: KeyRef("browser.signing".into()),
        passphrase: None,
        stable_id: None,
        extra: Default::default(),
    };
    (
        signer,
        Identities {
            primary: Some(identity),
            secondary: None,
        },
    )
}

struct Family {
    venue: &'static str,
    recipe: &'static str,
    inputs: serde_json::Value,
    credential: &'static str,
    key: Vec<u8>,
    verify: Option<RemoteVerifier>,
}

/// Runs both halves; `tamper` flips the returned signature before the server sees it.
fn run(family: Family, tamper: bool) -> (Option<String>, Vec<String>, Vec<u8>) {
    let recipe = safe_cex_venues::remote_recipe(family.recipe).unwrap();
    let mut op = RemoteOp::new(recipe, family.inputs.clone());
    op.verify = family.verify;
    let present = recipe.bound_credential_field().is_some();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Proof {
        step: Some(RecipeStep { op, present }),
        failed: vec![],
    });
    engine.start(1000.);
    let (attempt, work, remote) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Sign {
                attempt,
                work,
                remote,
                ..
            } => Some((attempt, work, remote)),
            _ => None,
        })
        .expect("keyless server hands the work to its host");
    let remote = remote.expect("the bound recipe travels with the work");
    assert_eq!(remote.recipe, family.recipe);
    // Browser half: only the recipe id and inputs cross, never bytes or refs.
    let (signer, identities) = browser(family.credential, family.key);
    let input: Input = serde_json::from_value(serde_json::json!({
        "kind": "op", "recipe": remote.recipe, "inputs": remote.inputs
    }))
    .unwrap();
    let (browser_work, kind) = hosted_signing::prepare(family.venue, &identities, input).unwrap();
    let signed = signer.sign_ready(&browser_work).unwrap().unwrap();
    let hex = hosted_signing::finish(kind, &signed)
        .unwrap()
        .signature
        .unwrap();
    let mut signature: Vec<u8> = (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    if tamper {
        signature[0] ^= 1;
    }
    // Remote provider: presentations from its public credentials, then the op.
    let mut values: Vec<Vec<u8>> = work
        .operations
        .iter()
        .filter(|op| matches!(op, Operation::Present { .. }))
        .map(|_| family.credential.as_bytes().to_vec())
        .collect();
    values.push(signature.clone());
    engine.handle(
        1001.,
        Event::Signed {
            attempt,
            result: Ok(SignResult::new(values, work.receipt)),
        },
    );
    let header = engine.drain_actions().find_map(|action| match action {
        Action::Http(http) => http
            .headers
            .iter()
            .find(|(name, _)| *name == "x-signature")
            .map(|(_, value)| value.clone()),
        _ => None,
    });
    (header, engine.venue().failed.clone(), signature)
}

// RFC 8032 test 1: a published seed/public-key pair, not account material.
const ED_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03, 0x1c, 0xae, 0x7f, 0x60,
];
const ED_PUBLIC: [u8; 32] = [
    0xd7, 0x5a, 0x98, 0x01, 0x82, 0xb1, 0x0a, 0xb7, 0xd5, 0x4b, 0xfe, 0xd3, 0xc9, 0x64, 0x07, 0x3a,
    0x0e, 0xe1, 0x72, 0xf3, 0xda, 0xa6, 0x23, 0x25, 0xaf, 0x02, 0x1a, 0x68, 0xf7, 0x07, 0x51, 0x1a,
];
const KID: &str = "organizations/org-id/apiKeys/key-id";

fn orderly() -> Family {
    let key = format!("ed25519:{}", bs58::encode(ED_PUBLIC).into_string());
    Family {
        venue: "orderly",
        recipe: "orderly.rest.1",
        inputs: serde_json::json!({
            "timestamp": "1700000000000", "method": "POST", "path": "/v1/order",
            "body": r#"{"symbol":"PERP_BTC_USDC","order_type":"LIMIT"}"#
        }),
        credential: "ed25519:public",
        key: ED_SEED.to_vec(),
        verify: Some(safe_cex_venue_orderly::remote_verifier(&key).unwrap()),
    }
}

fn paradex() -> Family {
    let mut key = vec![0u8; 31];
    key.push(5);
    let public = safe_cex_crypto::starknet::public_key(&starknet_crypto::Felt::from(5u8));
    Family {
        venue: "paradex",
        recipe: "paradex.order.1",
        inputs: serde_json::json!({
            "chainId": "PRIVATE_SN_POTC_SEPOLIA",
            "account": "0x0129f5ed7e7e3b5a8c9a3e2f1d0c4b6a7e8f9d0c1b2a39485766554433221100",
            "timestamp": "1700000000123", "market": "BTC-USD-PERP", "side": "SELL",
            "orderType": "LIMIT", "size": "0.25", "price": "64000.5"
        }),
        credential: "0xsubkey",
        key,
        verify: Some(
            safe_cex_venue_paradex::sign::remote_verifier(&format!("{public:#x}")).unwrap(),
        ),
    }
}

fn asterdex() -> Family {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        private_key: String,
        user: String,
        signer: String,
        nonce: String,
        payload: String,
    }
    #[derive(serde::Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    let case = serde_json::from_str::<Corpus>(include_str!(
        "../../venues/asterdex/tests/fixtures/signing.json"
    ))
    .unwrap()
    .cases
    .remove(0);
    let hex = case.private_key.trim_start_matches("0x");
    Family {
        venue: "asterdex",
        recipe: "asterdex.v3.1",
        inputs: serde_json::json!({
            "payload": case.payload, "user": case.user, "signer": case.signer, "nonce": case.nonce
        }),
        credential: "aster-api-key",
        key: (0..32)
            .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
            .collect(),
        verify: Some(safe_cex_venue_asterdex::sign::remote_verifier(&case.signer).unwrap()),
    }
}

fn coinbase() -> Family {
    Family {
        venue: "coinbase",
        recipe: "coinbase.restJwtEd25519.1",
        inputs: serde_json::json!({
            "kid": KID, "nonce": "n-1", "nbf": 1_700_000_000u64,
            "uri": "POST api.coinbase.com/api/v3/brokerage/orders"
        }),
        credential: KID,
        key: ED_SEED.to_vec(),
        verify: None,
    }
}

#[test]
fn every_format_signs_remotely_verifies_and_releases_the_exact_bytes() {
    for (family, algorithm) in [
        (orderly(), Algorithm::Ed25519),
        (paradex(), Algorithm::StarkDigest),
        (asterdex(), Algorithm::Secp256k1Digest),
        (coinbase(), Algorithm::Ed25519),
    ] {
        let (venue, recipe, inputs) = (family.venue, family.recipe, family.inputs.clone());
        let (header, failed, signature) = run(family, false);
        assert_eq!(failed, Vec::<String>::new(), "{venue}");
        assert_eq!(
            header.as_deref(),
            Some(safe_cex_crypto::hex(&signature).as_str()),
            "{venue}"
        );
        let recipe = safe_cex_venues::remote_recipe(recipe).unwrap();
        assert_eq!(recipe.algorithm(), algorithm, "{venue}");
        if venue == "coinbase" {
            // No verification key: the venue's own check is the backstop.
            let message = recipe.canonicalize(&inputs).unwrap();
            assert!(safe_cex_crypto::ed25519::verify(&message, &ED_PUBLIC, &signature).unwrap());
        }
    }
}

#[test]
fn a_signature_the_declared_key_does_not_verify_is_never_released() {
    for family in [orderly(), paradex(), asterdex()] {
        let venue = family.venue;
        let (header, failed, _) = run(family, true);
        assert_eq!(header, None, "{venue}");
        assert_eq!(failed.len(), 1, "{venue}");
    }
}

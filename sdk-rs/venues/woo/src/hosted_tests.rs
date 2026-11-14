use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, Capability, Operation, PayloadPart, SignError, SignFuture, SignRequest, SignResult,
    Signer,
};
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions, seeded_engine};
use safe_cex_engine::{Action, HttpRequest};
use std::sync::{Arc, Mutex};
#[derive(Clone, serde::Deserialize)]
struct Vector {
    label: String,
    method: String,
    path: String,
    params: Vec<(String, String)>,
    body: Option<String>,
    payload: String,
    signature: String,
}
struct Delegate {
    local: LocalSigner,
    work: Mutex<Vec<SignRequest>>,
}
impl Signer for Delegate {
    fn sign(&self, request: SignRequest) -> SignFuture {
        let result = self.sign_ready(&request).unwrap();
        Box::pin(async move { result })
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.work.lock().unwrap().push(request.clone());
        Some(self.local.sign_now(request))
    }
    fn can_sign(&self, key: &safe_cex_engine::signing::KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
}
fn method(name: &str) -> Method {
    match name {
        "GET" => Method::Get,
        "POST" => Method::Post,
        "PUT" => Method::Put,
        "DELETE" => Method::Delete,
        _ => panic!("fixture method"),
    }
}
#[test]
fn every_signed_endpoint_dispatches_the_local_wire_and_independent_preimage_through_the_provider() {
    let vectors: Vec<Vector> =
        serde_json::from_str(include_str!("../tests/fixtures/hosted-signing.json")).unwrap();
    let (local, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-public".into()),
            secret: Secret::new("fixture-signing-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let provider = Arc::new(Delegate {
        local,
        work: Mutex::new(Vec::new()),
    });
    let venue = Woo::new(Options {
        credentials: identities.primary.clone(),
        ..Default::default()
    });
    let fixture = vectors.clone();
    let mut engine = seeded_engine(ProbeVenue::new(venue, move |venue, cx| {
        for vector in &fixture {
            venue
                .signed(
                    cx,
                    method(&vector.method),
                    &vector.path,
                    vector.params.clone(),
                    vector.body.clone(),
                    Pending::SigningProbe,
                )
                .unwrap();
        }
    }));
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.set_identities(identities).unwrap();
    engine.start(1_700_000_000_000.);
    let requests: Vec<HttpRequest> = prepared_actions(&mut engine, 1_700_000_000_000.)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), vectors.len());
    let work = provider.work.lock().unwrap();
    assert_eq!(work.len(), vectors.len());
    for ((actual, work), vector) in requests.iter().zip(work.iter()).zip(vectors.iter()) {
        let expected = local_oracle(vector);
        assert_eq!(
            actual.url,
            format!("https://api.woox.io{}", expected.path),
            "{}",
            vector.label
        );
        assert_eq!(actual.body, expected.body, "{}", vector.label);
        assert_eq!(actual.headers, expected.headers, "{}", vector.label);
        assert_eq!(actual.headers[2].1, vector.signature, "{}", vector.label);
        let Operation::Sign { payload, .. } = &work.operations[1] else {
            panic!("HMAC work")
        };
        assert_eq!(
            payload,
            &[PayloadPart::Bytes {
                bytes: vector.payload.as_bytes().to_vec()
            }],
            "{}",
            vector.label
        );
        assert!(
            !serde_json::to_string(work)
                .unwrap()
                .contains("fixture-signing-material")
        );
    }
}

fn local_oracle(vector: &Vector) -> sign::Signed {
    sign::request(
        "fixture-public",
        "fixture-signing-material",
        1_700_000_000_000.,
        method(&vector.method),
        &vector.path,
        &vector.params,
        vector.body.clone(),
    )
}

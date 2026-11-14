//! Existing native fixtures compare every dispatched graph and Ed25519 result
//! with the device capture, including protective/ordinary and Unknown paths.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, PayloadPart, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_engine::signing_capture::Capture;
use safe_cex_venues::hosted_signing::{self, Input};

pub struct Parity {
    pub local: LocalSigner,
    pub ids: Identities,
}
impl Parity {
    fn check(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        let local = self.local.sign_now(request)?;
        let Operation::Sign { key, payload, .. } = &request.operations[1] else {
            panic!("Backpack graph")
        };
        let PayloadPart::Bytes { bytes } = &payload[0] else {
            panic!("Backpack native bytes")
        };
        let slot = if self
            .ids
            .primary
            .as_ref()
            .is_some_and(|id| id.signing_ref().ok() == Some(key))
        {
            "primary"
        } else {
            "secondary"
        };
        let stamp = request.receipt.stamp_ms.unwrap();
        let (work, kind) = hosted_signing::prepare("backpack", &self.ids, Input::Http {
            payload: serde_json::json!({ "slot": slot, "payload": std::str::from_utf8(bytes).unwrap() }).to_string(),
            stamp_ms: stamp, revision: "0".into(),
        }).unwrap();
        assert_eq!(
            serde_json::to_value(&work.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap(),
            "native/hosted payload and selected identity"
        );
        let mut capture = Capture::default();
        let prepared = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
        let hosted = self.local.sign_now(&prepared.work).unwrap();
        let (kind, hosted) = capture
            .finish(prepared.id, 0., stamp, 0, Ok(hosted))
            .unwrap();
        assert_eq!(
            local.values, hosted.values,
            "native/hosted public credential and Ed25519 signature"
        );
        hosted_signing::finish(kind, &hosted).unwrap();
        assert!(capture.is_empty());
        Ok(local)
    }
}
impl Signer for Parity {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(&self, request: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.check(&request)))
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.check(request))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
}

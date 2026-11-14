//! Every existing native fixture also traverses the hosted capture recipe.
//! Comparing operations before crypto proves exact preimages, including recovery
//! reads after Unknown, clock-skew retries and managed protective/ordinary plans.
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
        let sign_key = match &request.operations[1] {
            Operation::Sign { key, .. } => key,
            _ => panic!("KuCoin request graph"),
        };
        let slot = if self
            .ids
            .primary
            .as_ref()
            .is_some_and(|id| id.signing_ref().ok() == Some(sign_key))
        {
            "primary"
        } else {
            "secondary"
        };
        let text = |operation: &Operation| match operation {
            Operation::Sign { payload, .. } => match &payload[0] {
                PayloadPart::Bytes { bytes } => String::from_utf8(bytes.clone()).unwrap(),
                _ => panic!("KuCoin canonical byte preimage"),
            },
            _ => panic!("KuCoin native sign operation"),
        };
        let mut descriptor =
            serde_json::json!({ "slot": slot, "payload": text(&request.operations[1]) });
        if request.operations.len() == 4 {
            descriptor["brokerPrefix"] = text(&request.operations[3]).into();
        }
        let stamp = request.receipt.stamp_ms.unwrap();
        let (work, kind) = hosted_signing::prepare(
            &request.context.venue,
            &self.ids,
            Input::Http {
                payload: descriptor.to_string(),
                stamp_ms: stamp,
                revision: "0".into(),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&work.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap(),
            "hosted native preimages and selected identity must be exact"
        );
        let mut capture = Capture::default();
        let prepared = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
        let hosted = self.local.sign_now(&prepared.work).unwrap();
        let (kind, hosted) = capture
            .finish(prepared.id, 0., stamp, 0, Ok(hosted))
            .unwrap();
        assert_eq!(
            local.values, hosted.values,
            "every request, passphrase and broker MAC must match"
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

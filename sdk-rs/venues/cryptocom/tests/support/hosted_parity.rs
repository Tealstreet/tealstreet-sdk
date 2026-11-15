//! Every native fixture compares dispatch signing with the device capture.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, PayloadPart, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_venues::hosted_signing::{self, Input};
use std::sync::Arc;

pub struct Parity {
    pub local: Arc<LocalSigner>,
    pub ids: Identities,
}
impl Parity {
    fn check(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        let local = self.local.sign_now(request)?;
        let Operation::Present { key: credential } = &request.operations[0] else {
            panic!("Crypto.com present graph")
        };
        let Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key,
            payload,
        } = &request.operations[1]
        else {
            panic!("Crypto.com HMAC graph")
        };
        assert_eq!(request.operations.len(), 2);
        assert_eq!(
            self.ids.primary.as_ref().unwrap().signing_ref().unwrap(),
            key
        );
        let mut bytes = vec![];
        for part in payload {
            match part {
                PayloadPart::Bytes { bytes: part } => bytes.extend_from_slice(part),
                PayloadPart::Credential { key } if key == credential => {
                    bytes.extend_from_slice(&local.values[0])
                }
                _ => panic!("Crypto.com canonical payload"),
            }
        }
        let (work, kind) = hosted_signing::prepare(
            "cryptocom",
            &self.ids,
            Input::Http {
                payload: String::from_utf8(bytes).unwrap(),
                stamp_ms: request.receipt.stamp_ms.unwrap(),
                revision: "0".into(),
            },
        )
        .unwrap();
        let hosted = self.local.sign_now(&work).unwrap();
        assert_eq!(
            local.values, hosted.values,
            "native and hosted credential/signature bytes"
        );
        assert_eq!(local.receipt, hosted.receipt);
        hosted_signing::finish(kind, &hosted).unwrap();
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

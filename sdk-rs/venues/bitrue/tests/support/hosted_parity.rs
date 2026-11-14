//! Existing endpoint fixtures also compare native work against device custody.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, PayloadPart, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_venues::hosted_signing::{self, Input};

pub struct Parity {
    pub local: LocalSigner,
    pub identities: Identities,
}

impl Parity {
    fn check(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        let result = self.local.sign_now(request)?;
        let [
            Operation::Present { key: credential },
            Operation::Sign { key, payload, .. },
        ] = request.operations.as_slice()
        else {
            panic!("Bitrue native HMAC graph")
        };
        let [PayloadPart::Bytes { bytes }] = payload.as_slice() else {
            panic!("Bitrue canonical byte preimage")
        };
        let identity = [
            self.identities.primary.as_ref(),
            self.identities.secondary.as_ref(),
        ]
        .into_iter()
        .flatten()
        .find(|id| &id.credential == credential)
        .unwrap();
        assert_eq!(identity.signing_ref().unwrap(), key);
        // Secondary remains a local/PayloadSigner control; its device slot is not
        // selectable on the raw RemoteSigner wire and SDK setup refuses that mode.
        let device = Identities {
            primary: Some(identity.clone()),
            secondary: None,
        };
        let (work, kind) = hosted_signing::prepare(
            "bitrue",
            &device,
            Input::Http {
                payload: String::from_utf8(bytes.clone()).unwrap(),
                stamp_ms: request.receipt.stamp_ms.unwrap(),
                revision: "0".into(),
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&work.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap()
        );
        assert_eq!(work.receipt, request.receipt);
        let hosted = self.local.sign_now(&work).unwrap();
        assert_eq!(hosted.values, result.values);
        hosted_signing::finish(kind, &hosted).unwrap();
        Ok(result)
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

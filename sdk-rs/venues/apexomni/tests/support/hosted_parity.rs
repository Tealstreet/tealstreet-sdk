//! Every native fixture compares its actual graph and signature bytes with device capture.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, PayloadPart, SignError, SignFuture,
    SignRequest, SignResult, Signer,
};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

pub struct Parity {
    pub local: LocalSigner,
    pub ids: Identities,
}
impl Parity {
    fn check(&self, request: &SignRequest) -> Result<SignResult, SignError> {
        let local = self.local.sign_now(request)?;
        let count = request.operations.len().checked_sub(3).expect("ApeX graph");
        let contracts: Vec<_> = request.operations[..count]
            .iter()
            .map(|op| match op {
                Operation::Sign {
                    algorithm: Algorithm::ZkLinkContract,
                    payload,
                    ..
                } => match payload.as_slice() {
                    [PayloadPart::Bytes { bytes }] => bytes.clone(),
                    _ => panic!("contract bytes"),
                },
                _ => panic!("contract signature"),
            })
            .collect();
        let payload: Vec<_> = match request.operations.last().unwrap() {
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                payload,
                ..
            } => payload
                .iter()
                .map(|part| match part {
                    PayloadPart::Bytes { bytes } => serde_json::json!({"bytes":bytes}),
                    PayloadPart::Result {
                        index,
                        encoding: safe_cex_engine::signing::Encoding::Hex,
                    } => serde_json::json!({"contract":index}),
                    _ => panic!("native HMAC part"),
                })
                .collect(),
            _ => panic!("native HMAC"),
        };
        let (mut captured, kind) = hosted_signing::prepare(
            "apexomni",
            &self.ids,
            Input::WsAuth {
                to_sign: serde_json::json!({"contracts":contracts,"payload":payload}).to_string(),
                output_format: OutputFormat::ApexOmniBatch,
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&captured.operations).unwrap(),
            serde_json::to_value(&request.operations).unwrap(),
            "native and device graphs must be byte-identical"
        );
        captured.receipt = request.receipt;
        let device = self.local.sign_now(&captured).unwrap();
        assert_eq!(
            local.values, device.values,
            "local/device signatures must match"
        );
        hosted_signing::finish(kind, &device).unwrap();
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

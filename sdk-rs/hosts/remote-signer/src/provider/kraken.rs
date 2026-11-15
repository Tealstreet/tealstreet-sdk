//! Existing Kraken Futures primary SHA256/HMAC-SHA512 device batch codec.
use super::*;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    credential: String,
    signature: String,
}

impl Inner {
    pub(super) async fn sign_kraken(
        &self,
        operations: &[Operation],
        receipt: SignReceipt,
    ) -> Result<Vec<Vec<u8>>, SignError> {
        receipt.validate()?;
        let primary = &self.primary;
        if let [Operation::Present { key }] = operations {
            return if *key == primary.credential {
                Ok(vec![primary.public.clone()])
            } else {
                refused()
            };
        }
        let [
            Operation::Present { key: credential },
            Operation::Sha256 { payload },
            Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key,
                payload: dependency,
            },
        ] = operations
        else {
            return refused();
        };
        if *credential != primary.credential
            || !self.owns_signing(key)
            || !matches!(
                dependency.as_slice(),
                [PayloadPart::Result {
                    index: 1,
                    encoding: PartEncoding::Raw
                }]
            )
        {
            return refused();
        }
        let preimage = single_bytes(payload, MAX_PAYLOAD)?;
        if preimage.is_empty() {
            return refused();
        }
        let descriptor = json!({"slot":"primary","payload":utf8(preimage)?}).to_string();
        if descriptor.len() > MAX_PAYLOAD {
            return refused();
        }
        self.device(descriptor, "kraken-sha512", |value| {
            let answer: Answer = serde_json::from_str(value.as_str().ok_or(SignError::Refused)?)
                .map_err(|_| SignError::Refused)?;
            if answer.credential.as_bytes() != primary.public || answer.signature.len() != 88 {
                return refused();
            }
            let signature = base64::engine::general_purpose::STANDARD
                .decode(&answer.signature)
                .map_err(|_| SignError::Refused)?;
            if signature.len() != 64
                || base64::engine::general_purpose::STANDARD.encode(&signature) != answer.signature
            {
                return refused();
            }
            Ok(vec![primary.public.clone(), sha256(preimage), signature])
        })
        .await
    }
}

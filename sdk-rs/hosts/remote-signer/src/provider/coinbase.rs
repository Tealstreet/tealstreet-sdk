//! Coinbase signs only its server-built canonical JWS with the original key ref.
use super::*;
use safe_cex_venue_coinbase::sign::{JwtAlgorithm, inspect_input};

pub(super) fn algorithm(value: JwtAlgorithm) -> Algorithm {
    match value {
        JwtAlgorithm::Es256 => Algorithm::Es256,
        JwtAlgorithm::EdDsa => Algorithm::Ed25519,
    }
}

impl Inner {
    pub(super) async fn sign_coinbase(
        &self,
        request: &SignRequest,
    ) -> Result<SignResult, SignError> {
        request.validate()?;
        let binding = self.coinbase.as_ref().ok_or(SignError::Refused)?;
        let [
            Operation::Present { key: credential },
            Operation::Sign {
                algorithm: requested,
                key,
                payload,
            },
        ] = request.operations.as_slice()
        else {
            return refused();
        };
        if *credential != self.primary.credential
            || !self.owns_signing(key)
            || *requested != algorithm(binding.algorithm)
        {
            return refused();
        }
        let input = utf8(single_bytes(payload, 16_384)?)?;
        let (claims, nbf) = inspect_input(&input).map_err(|_| SignError::Refused)?;
        let receipt = request.receipt;
        if claims.key_name != binding.key_name
            || claims.algorithm != binding.algorithm
            || receipt.nonce.is_some()
            || receipt
                .stamp_ms
                .is_none_or(|stamp| (stamp / 1000.).floor() != nbf as f64)
            || receipt.expiry_ms
                != nbf
                    .checked_add(120)
                    .and_then(|v| v.checked_mul(1000))
                    .map(|v| v as f64)
        {
            return refused();
        }
        let signature = self
            .device(input, "coinbase-jws", |value| {
                let text = value.as_str().ok_or(SignError::Refused)?;
                if text.len() != 86 {
                    return refused();
                }
                let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
                    .decode(text)
                    .map_err(|_| SignError::Refused)?;
                if bytes.len() != 64
                    || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes) != text
                {
                    return refused();
                }
                Ok(bytes)
            })
            .await?;
        Ok(SignResult::new(
            vec![self.primary.public.clone(), signature],
            receipt,
        ))
    }
}

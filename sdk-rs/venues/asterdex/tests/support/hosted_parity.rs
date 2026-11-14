//! Every existing fixture compares device signing with independently dispatched custody.
use safe_cex_engine::signing::{
    Algorithm, Capability, Identities, KeyRef, Operation, OwnedSetupExecution, PayloadPart,
    Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_venues::hosted_wallet::{self, Account};
use std::sync::Arc;

pub fn wrap(local: Provider, ids: Identities, account: Account) -> Provider {
    Arc::new(Parity {
        local,
        ids,
        account,
    })
}
struct Parity {
    local: Provider,
    ids: Identities,
    account: Account,
}
impl Parity {
    fn check(&self, work: &SignRequest) -> Result<SignResult, SignError> {
        let local = self.local.sign_ready(work).unwrap()?;
        let Some(Operation::Present { key }) = work.operations.first() else {
            return Ok(local);
        };
        let slot = if self
            .ids
            .primary
            .as_ref()
            .is_some_and(|id| id.credential == *key)
        {
            "primary"
        } else {
            "secondary"
        };
        let (part, wallet) = match work.operations.as_slice() {
            [
                _,
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    payload,
                    ..
                },
            ] => (&payload[0], false),
            [
                _,
                Operation::Sha256 { payload },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    ..
                },
            ] => (&payload[0], true),
            [_] => return Ok(local),
            _ => panic!("unexpected Aster signing graph"),
        };
        let PayloadPart::Bytes { bytes } = part else {
            panic!("canonical bytes");
        };
        let payload = std::str::from_utf8(bytes).unwrap();
        // The request reducer's synthetic /probe is not an exchange endpoint.
        if wallet
            && serde_json::from_str::<serde_json::Value>(payload).unwrap()["path"]
                == "/fapi/v1/probe"
        {
            return Ok(local);
        }
        let now = if wallet {
            serde_json::from_str::<serde_json::Value>(payload).unwrap()["stampMs"]
                .as_f64()
                .unwrap()
        } else {
            work.receipt.stamp_ms.unwrap_or(0.)
        };
        let text = serde_json::json!({ "slot": slot, "payload": payload }).to_string();
        let output = hosted_wallet::sign(
            "asterdex",
            &self.ids,
            &self.account,
            &self.local,
            &text,
            if wallet {
                "asterdex-wallet"
            } else {
                "asterdex-hmac"
            },
            now,
        )
        .unwrap();
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            output["credential"].as_str().unwrap().as_bytes(),
            local.values[0]
        );
        assert_eq!(
            output["signature"],
            safe_cex_crypto::hex(local.values.last().unwrap()),
            "device/local signature bytes"
        );
        Ok(local)
    }
}
impl Signer for Parity {
    fn immediate(&self) -> bool {
        true
    }
    fn sign_ready(&self, work: &SignRequest) -> Option<Result<SignResult, SignError>> {
        Some(self.check(work))
    }
    fn sign(&self, work: SignRequest) -> SignFuture {
        Box::pin(std::future::ready(self.check(&work)))
    }
    fn can_sign(&self, key: &KeyRef, algorithm: Algorithm) -> Capability {
        self.local.can_sign(key, algorithm)
    }
    fn owned_public_address_capability(&self) -> Capability {
        self.local.owned_public_address_capability()
    }
    fn create_public_address_setup(
        &self,
        work: SignRequest,
    ) -> Option<Box<dyn OwnedSetupExecution>> {
        self.local.create_public_address_setup(work)
    }
}

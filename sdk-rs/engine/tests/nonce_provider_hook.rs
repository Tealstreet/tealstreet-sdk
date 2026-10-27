//! The provider override must compile under independently unified feature sets.
use safe_cex_engine::nonce::handoff::{LegacyNonceProof, LegacyNonceRequest};
use safe_cex_engine::signing::{SignError, SignFuture, SignRequest, Signer};
struct Downstream;
impl Signer for Downstream {
    fn sign(&self, _: SignRequest) -> SignFuture {
        Box::pin(async { Err(SignError::Unsupported) })
    }
    fn legacy_nonce_association(
        &self,
        request: LegacyNonceRequest,
    ) -> Result<LegacyNonceProof, SignError> {
        request.bind_key_bytes(b"fixture decoded installed key")
    }
}
#[test]
fn downstream_override_is_unconditional() {
    let _: &dyn Signer = &Downstream;
}

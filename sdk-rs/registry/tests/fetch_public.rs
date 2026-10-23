//! P4 setup-time public fetch: public-only, bounded, inline providers only,
//! and results held to the engine's exact widths.
#![cfg(feature = "dispatch-signing")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_venues::fetch_public;

/// Answers every operation with `width` bytes and counts the calls it sees.
struct Fixed {
    immediate: bool,
    width: usize,
    calls: AtomicUsize,
}
impl Signer for Fixed {
    fn immediate(&self) -> bool {
        self.immediate
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.immediate.then(|| {
            Ok(SignResult::new(
                vec![vec![1; self.width]; request.operations.len()],
                request.receipt,
            ))
        })
    }
    fn sign(&self, _request: SignRequest) -> SignFuture {
        panic!("setup fetch never schedules provider work")
    }
}
fn provider(immediate: bool, width: usize) -> (Arc<Fixed>, Provider) {
    let fixed = Arc::new(Fixed {
        immediate,
        width,
        calls: AtomicUsize::new(0),
    });
    (fixed.clone(), fixed)
}
fn address() -> Operation {
    Operation::PublicAddress {
        key: KeyRef("primary.signing".into()),
    }
}

#[test]
fn inline_provider_answers_exact_width_public_values() {
    let (_, signer) = provider(true, 20);
    assert_eq!(
        fetch_public(&signer, vec![address()]),
        Ok(vec![vec![1; 20]])
    );
    let (_, signer) = provider(true, 32);
    let stark = Operation::PublicKey {
        algorithm: Algorithm::StarkDigest,
        key: KeyRef("primary.signing".into()),
    };
    assert_eq!(fetch_public(&signer, vec![stark]), Ok(vec![vec![1; 32]]));
}

#[test]
fn wrong_width_results_are_invalid() {
    let (_, signer) = provider(true, 32);
    assert_eq!(
        fetch_public(&signer, vec![address()]),
        Err(SignError::InvalidResult)
    );
}

#[test]
fn host_or_remote_provider_is_unavailable_without_any_provider_call() {
    let (fixed, signer) = provider(false, 20);
    assert_eq!(
        fetch_public(&signer, vec![address()]),
        Err(SignError::Unavailable)
    );
    assert_eq!(fixed.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn only_bounded_public_only_requests_reach_the_provider() {
    let (fixed, signer) = provider(true, 20);
    let present = Operation::Present {
        key: KeyRef("primary.credential".into()),
    };
    for operations in [
        vec![],
        vec![address(); 5],
        vec![address(), present],
        vec![Operation::PublicKey {
            algorithm: Algorithm::Es256,
            key: KeyRef("primary.signing".into()),
        }],
    ] {
        assert_eq!(
            fetch_public(&signer, operations),
            Err(SignError::InvalidRequest)
        );
    }
    // Es256 has no public form yet; request validation refuses it first.
    assert_eq!(fixed.calls.load(Ordering::SeqCst), 0);
}

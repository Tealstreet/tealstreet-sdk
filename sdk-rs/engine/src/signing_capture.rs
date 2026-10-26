//! Bounded signing-only captures for trusted host adapters.
//!
//! Callers construct canonical work from owned identities. This service neither
//! chooses keys nor executes a provider, and it never emits transport actions.
//! Metadata must describe formatting, not contain provider material. Results are
//! moved to the caller once, or dropped (and wiped by `SignResult`) on failure.

use crate::signing::{SignError, SignRequest, SignResult};
use std::collections::HashMap;

pub const MAX_PENDING: usize = 64;
pub const MAX_TIMEOUT_MS: u32 = 5_000;
const MAX_CLOCK_MS: f64 = 9_007_199_254_740_991.;

/// Account-local, never-reused correlation identity. It carries no key reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CaptureId(u32);

impl CaptureId {
    pub fn get(self) -> u32 {
        self.0
    }

    /// Decode a host's correlation value; existence is checked by the service.
    pub fn from_raw(value: u32) -> Option<Self> {
        (value != 0).then_some(Self(value))
    }
}

/// Canonical work for either immediate local or asynchronous provider execution.
/// The deadline is in the uncorrected host clock domain.
#[derive(Debug)]
pub struct Prepared {
    pub id: CaptureId,
    pub work: SignRequest,
    pub deadline_ms: f64,
}

struct Pending<T> {
    work: SignRequest,
    header_policy: crate::signing_headers::ResultHeaderPolicy,
    metadata: T,
    issued_at: f64,
    protocol_at: f64,
    revision: u64,
    deadline_ms: f64,
}

/// No timers or side maps: the stored work, metadata and fences share one life.
pub struct Capture<T> {
    pending: HashMap<CaptureId, Pending<T>>,
    last_id: u32,
}

impl<T> Default for Capture<T> {
    fn default() -> Self {
        Self {
            pending: HashMap::new(),
            last_id: 0,
        }
    }
}

fn valid_clock(now: f64) -> bool {
    now.is_finite() && (0. ..=MAX_CLOCK_MS).contains(&now)
}

impl<T> Capture<T> {
    /// Admit trusted canonical work without running it. Rejections do not consume
    /// identities; accepted identities stay retired after completion or clearing.
    pub fn prepare(
        &mut self,
        work: SignRequest,
        metadata: T,
        now: f64,
        protocol_now: f64,
        revision: u64,
        timeout_ms: u32,
    ) -> Result<Prepared, SignError> {
        self.prepare_with_policy(
            (work, crate::signing_headers::ResultHeaderPolicy::Deny),
            metadata,
            now,
            protocol_now,
            revision,
            timeout_ms,
        )
    }

    /// Trusted HTTP captures opt into the same declared policy as dispatch.
    /// This policy is retained with work and cannot come from SignRequest.context.
    pub fn prepare_with_policy(
        &mut self,
        work: (SignRequest, crate::signing_headers::ResultHeaderPolicy),
        metadata: T,
        now: f64,
        protocol_now: f64,
        revision: u64,
        timeout_ms: u32,
    ) -> Result<Prepared, SignError> {
        let (work, header_policy) = work;
        work.validate()?;
        if !valid_clock(now)
            || !valid_clock(protocol_now)
            || !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms)
        {
            return Err(SignError::InvalidRequest);
        }
        let deadline_ms = now + f64::from(timeout_ms);
        if !valid_clock(deadline_ms)
            || deadline_ms <= now
            || work
                .receipt
                .expiry_ms
                .is_some_and(|expiry| expiry <= protocol_now)
        {
            return Err(SignError::InvalidRequest);
        }
        if self.pending.len() >= MAX_PENDING {
            return Err(SignError::Unavailable);
        }
        let next = self.last_id.checked_add(1).ok_or(SignError::Unavailable)?;
        let id = CaptureId(next);
        // Only canonical bytes and opaque references are copied. Provider results
        // and formatting metadata are never cloned by the service.
        let outbound = work.clone();
        self.pending.insert(
            id,
            Pending {
                work,
                header_policy,
                metadata,
                issued_at: now,
                protocol_at: protocol_now,
                revision,
                deadline_ms,
            },
        );
        self.last_id = next;
        Ok(Prepared {
            id,
            work: outbound,
            deadline_ms,
        })
    }

    /// Inspect the same immutable work for immediate provider execution.
    pub fn work(&self, id: CaptureId) -> Option<&SignRequest> {
        self.pending.get(&id).map(|pending| &pending.work)
    }

    /// Every matching completion retires its token, even a malformed/late error.
    /// Timeout and clock fences apply before interpreting an old provider result.
    pub fn finish(
        &mut self,
        id: CaptureId,
        now: f64,
        protocol_now: f64,
        revision: u64,
        result: Result<SignResult, SignError>,
    ) -> Result<(T, SignResult), SignError> {
        let pending = self.take(id, now, protocol_now, revision)?;
        let result = result?;
        pending
            .work
            .validate_result_with_policy(&result, pending.header_policy)?;
        Ok((pending.metadata, result))
    }

    /// Synchronous local execution uses the same retirement and clock fences as
    /// asynchronous completion, before invoking the provider even once.
    pub fn execute(
        &mut self,
        id: CaptureId,
        now: f64,
        protocol_now: f64,
        revision: u64,
        run: impl FnOnce(&SignRequest) -> Result<SignResult, SignError>,
    ) -> Result<(T, SignResult), SignError> {
        let pending = self.take(id, now, protocol_now, revision)?;
        let result = run(&pending.work)?;
        pending
            .work
            .validate_result_with_policy(&result, pending.header_policy)?;
        Ok((pending.metadata, result))
    }

    fn take(
        &mut self,
        id: CaptureId,
        now: f64,
        protocol_now: f64,
        revision: u64,
    ) -> Result<Pending<T>, SignError> {
        let pending = self.pending.remove(&id).ok_or(SignError::Unavailable)?;
        if !valid_clock(now) {
            return Err(SignError::InvalidRequest);
        }
        if now >= pending.deadline_ms {
            return Err(SignError::TimedOut);
        }
        if !valid_clock(protocol_now)
            || now < pending.issued_at
            || revision != pending.revision
            || protocol_now < pending.protocol_at
        {
            return Err(SignError::InvalidRequest);
        }
        if pending
            .work
            .receipt
            .expiry_ms
            .is_some_and(|expiry| protocol_now >= expiry)
        {
            return Err(SignError::TimedOut);
        }
        Ok(pending)
    }

    pub fn cancel(&mut self, id: CaptureId) -> bool {
        self.pending.remove(&id).is_some()
    }

    /// Retire everything without resetting the identity allocator.
    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::{Algorithm, Context, KeyRef, Operation, PayloadPart, SignReceipt};

    fn work(expiry: Option<f64>) -> SignRequest {
        SignRequest {
            receipt: SignReceipt {
                nonce: None,
                stamp_ms: Some(1_000.),
                expiry_ms: expiry,
            },
            context: Context {
                venue: "capture-fixture".into(),
                method: "WS-AUTH".into(),
                host: "fixture.invalid".into(),
                path: "/auth".into(),
            },
            operations: vec![Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: KeyRef("owned.primary.signing".into()),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"exact original payload\0\xc3\xa9".to_vec(),
                }],
            }],
        }
    }

    fn result(receipt: SignReceipt) -> SignResult {
        // Independent fixed bytes: this service must not calculate crypto.
        SignResult::new(vec![vec![0x37; 32]], receipt)
    }

    fn prepare<T>(captures: &mut Capture<T>, metadata: T) -> Prepared {
        captures
            .prepare(work(None), metadata, 100., 1_000., 7, 500)
            .unwrap()
    }

    #[test]
    fn exact_work_and_nonclone_metadata_move_once_without_provider_execution() {
        struct Formatting(&'static str);
        let mut captures = Capture::default();
        let prepared = prepare(&mut captures, Formatting("hex"));
        assert_eq!(prepared.id.get(), 1);
        assert_eq!(prepared.deadline_ms, 600.);
        assert_eq!(captures.work(prepared.id), Some(&prepared.work));
        let (metadata, signed) = captures
            .finish(
                prepared.id,
                101.,
                1_001.,
                7,
                Ok(result(prepared.work.receipt)),
            )
            .unwrap();
        assert_eq!(metadata.0, "hex");
        assert_eq!(signed.values, vec![vec![0x37; 32]]);
        assert!(captures.is_empty());
        assert!(captures.work(prepared.id).is_none());
        assert!(matches!(
            captures.finish(prepared.id, 102., 1_002., 7, Err(SignError::Refused)),
            Err(SignError::Unavailable)
        ));
    }

    #[test]
    fn bounded_pending_slots_free_without_reusing_any_identity() {
        let mut captures = Capture::default();
        let ids: Vec<_> = (0..MAX_PENDING)
            .map(|n| prepare(&mut captures, n).id)
            .collect();
        assert_eq!(captures.len(), 64);
        assert!(matches!(
            captures.prepare(work(None), 100, 100., 1_000., 7, 500),
            Err(SignError::Unavailable)
        ));
        assert!(captures.cancel(ids[0]));
        assert!(!captures.cancel(ids[0]));
        assert_eq!(prepare(&mut captures, 100).id.get(), 65);
        captures.clear();
        assert!(captures.is_empty());
        assert_eq!(prepare(&mut captures, 101).id.get(), 66);
        assert!(captures.work(ids[1]).is_none());
        assert!(CaptureId::from_raw(0).is_none());
        assert_eq!(CaptureId::from_raw(66).unwrap().get(), 66);
    }

    #[test]
    fn invalid_work_clocks_deadlines_and_durations_do_not_allocate_ids() {
        let mut captures = Capture::default();
        for invalid in [f64::NAN, f64::INFINITY, -1., MAX_CLOCK_MS + 1.] {
            for (raw, protocol) in [(invalid, 1_000.), (100., invalid)] {
                assert!(matches!(
                    captures.prepare(work(None), (), raw, protocol, 7, 500),
                    Err(SignError::InvalidRequest)
                ));
            }
        }
        for timeout in [0, MAX_TIMEOUT_MS + 1, u32::MAX] {
            assert!(matches!(
                captures.prepare(work(None), (), 100., 1_000., 7, timeout),
                Err(SignError::InvalidRequest)
            ));
        }
        assert!(matches!(
            captures.prepare(work(None), (), MAX_CLOCK_MS, 1_000., 7, 1),
            Err(SignError::InvalidRequest)
        ));
        for invalid in [
            SignRequest {
                operations: vec![],
                ..work(None)
            },
            SignRequest {
                receipt: SignReceipt {
                    nonce: None,
                    stamp_ms: Some(1_000.5),
                    expiry_ms: None,
                },
                ..work(None)
            },
            SignRequest {
                operations: vec![Operation::Present {
                    key: KeyRef(String::new()),
                }],
                ..work(None)
            },
            SignRequest {
                context: Context {
                    venue: "x".repeat(4_097),
                    ..Default::default()
                },
                ..work(None)
            },
        ] {
            assert!(matches!(
                captures.prepare(invalid, (), 100., 1_000., 7, 500),
                Err(SignError::InvalidRequest)
            ));
        }
        assert!(captures.is_empty());
        assert_eq!(prepare(&mut captures, ()).id.get(), 1);
    }

    #[test]
    fn fractional_host_clock_is_preserved_and_maximum_timeout_is_fixed() {
        let mut captures = Capture::default();
        let prepared = captures
            .prepare(work(None), (), 100.25, 1_000.25, 7, 5_000)
            .unwrap();
        assert_eq!(prepared.deadline_ms, 5_100.25);
        assert!(
            captures
                .finish(
                    prepared.id,
                    5_100.24,
                    5_999.,
                    7,
                    Ok(result(prepared.work.receipt))
                )
                .is_ok()
        );
    }

    #[test]
    fn raw_timeout_precedes_provider_success_error_and_revision() {
        for protocol in [1_001., f64::NAN] {
            for supplied in [Ok(result(work(None).receipt)), Err(SignError::Refused)] {
                let mut captures = Capture::default();
                let id = prepare(&mut captures, ()).id;
                assert!(matches!(
                    captures.finish(id, 600., protocol, 8, supplied),
                    Err(SignError::TimedOut)
                ));
                assert!(captures.is_empty());
            }
        }
    }

    #[test]
    fn changed_revision_retires_success_and_error_at_the_same_raw_time() {
        for supplied in [Ok(result(work(None).receipt)), Err(SignError::Refused)] {
            let mut captures = Capture::default();
            let id = prepare(&mut captures, ()).id;
            assert!(matches!(
                captures.finish(id, 100., 1_000., 8, supplied),
                Err(SignError::InvalidRequest)
            ));
            assert!(captures.is_empty());
            assert!(matches!(
                captures.finish(id, 100., 1_000., 7, Ok(result(work(None).receipt))),
                Err(SignError::Unavailable)
            ));
        }
    }

    #[test]
    fn expiry_uses_protocol_time_and_rejects_expired_preparation() {
        let mut captures = Capture::default();
        assert!(matches!(
            captures.prepare(work(Some(1_100.)), (), 100., 1_100., 7, 500),
            Err(SignError::InvalidRequest)
        ));
        let prepared = captures
            .prepare(work(Some(1_100.)), (), 100., 1_000., 7, 500)
            .unwrap();
        assert!(matches!(
            captures.finish(prepared.id, 101., 1_100., 7, Err(SignError::Refused)),
            Err(SignError::TimedOut)
        ));
        let prepared = captures
            .prepare(work(Some(1_100.)), (), 100., 1_000., 7, 500)
            .unwrap();
        assert!(
            captures
                .finish(
                    prepared.id,
                    199.,
                    1_099.,
                    7,
                    Ok(result(prepared.work.receipt))
                )
                .is_ok()
        );
    }

    #[test]
    fn invalid_or_reversed_completion_clocks_fail_closed_and_consume_only_matching_token() {
        for (raw, protocol) in [
            (f64::NAN, 1_001.),
            (101., f64::INFINITY),
            (-1., 1_001.),
            (MAX_CLOCK_MS + 1., 1_001.),
            (99., 1_001.),
            (101., 999.),
        ] {
            let mut captures = Capture::default();
            let first = prepare(&mut captures, ());
            let peer = prepare(&mut captures, ());
            assert!(matches!(
                captures.finish(first.id, raw, protocol, 7, Ok(result(first.work.receipt))),
                Err(SignError::InvalidRequest)
            ));
            assert_eq!(captures.len(), 1);
            assert!(
                captures
                    .finish(peer.id, 101., 1_001., 7, Ok(result(peer.work.receipt)))
                    .is_ok()
            );
        }
    }

    #[test]
    fn malformed_results_and_receipts_retire_the_capture_atomically() {
        let expected = work(None).receipt;
        for supplied in [
            SignResult::new(vec![], expected),
            SignResult::new(vec![vec![0x37; 31]], expected),
            SignResult::new(vec![vec![0x37; 33]], expected),
            SignResult::new(vec![vec![0x37; 32], vec![0x37; 32]], expected),
            SignResult::new(
                vec![vec![0x37; 32]],
                SignReceipt {
                    nonce: None,
                    stamp_ms: Some(1_001.),
                    expiry_ms: None,
                },
            ),
            SignResult::new(
                vec![vec![0x37; 32]],
                SignReceipt {
                    nonce: None,
                    stamp_ms: Some(f64::NAN),
                    expiry_ms: None,
                },
            ),
        ] {
            let mut captures = Capture::default();
            let prepared = prepare(&mut captures, ());
            assert!(matches!(
                captures.finish(prepared.id, 101., 1_001., 7, Ok(supplied)),
                Err(SignError::InvalidResult)
            ));
            assert!(captures.is_empty());
        }
    }

    #[test]
    fn shared_validator_covers_credential_presentation_and_all_primitive_lengths() {
        let mut captures = Capture::default();
        let mut request = work(None);
        request.operations = vec![
            Operation::Present {
                key: KeyRef("owned.credential".into()),
            },
            Operation::Sha256 { payload: vec![] },
            Operation::Sign {
                algorithm: Algorithm::HmacSha512,
                key: KeyRef("owned.signing".into()),
                payload: vec![],
            },
            Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: KeyRef("owned.signing".into()),
                payload: vec![],
            },
        ];
        let prepared = captures
            .prepare(request.clone(), (), 100., 1_000., 7, 500)
            .unwrap();
        let signed = SignResult::new(
            vec![
                b"fixture-credential".to_vec(),
                vec![1; 32],
                vec![2; 64],
                vec![3; 65],
            ],
            request.receipt,
        );
        assert!(
            captures
                .finish(prepared.id, 101., 1_001., 7, Ok(signed))
                .is_ok()
        );
        let prepared = captures
            .prepare(request.clone(), (), 100., 1_000., 7, 500)
            .unwrap();
        let oversized = SignResult::new(
            vec![vec![1; 1_048_577], vec![1; 32], vec![2; 64], vec![3; 65]],
            request.receipt,
        );
        assert!(matches!(
            captures.finish(prepared.id, 101., 1_001., 7, Ok(oversized)),
            Err(SignError::InvalidResult)
        ));
    }

    #[test]
    fn provider_failure_cancel_clear_and_unknown_ids_do_not_affect_peers() {
        let mut captures = Capture::default();
        let first = prepare(&mut captures, ());
        let peer = prepare(&mut captures, ());
        assert!(matches!(
            captures.finish(
                CaptureId::from_raw(99).unwrap(),
                101.,
                1_001.,
                7,
                Err(SignError::Refused)
            ),
            Err(SignError::Unavailable)
        ));
        assert_eq!(captures.len(), 2);
        assert!(matches!(
            captures.finish(first.id, 101., 1_001., 7, Err(SignError::Refused)),
            Err(SignError::Refused)
        ));
        assert_eq!(captures.len(), 1);
        assert!(captures.work(first.id).is_none());
        assert!(captures.work(peer.id).is_some());
        captures.clear();
        assert!(matches!(
            captures.finish(peer.id, 101., 1_001., 7, Ok(result(peer.work.receipt))),
            Err(SignError::Unavailable)
        ));
    }

    #[test]
    fn identifier_exhaustion_never_wraps_or_reuses_a_cleared_token() {
        let mut captures = Capture {
            pending: HashMap::new(),
            last_id: u32::MAX - 1,
        };
        let prepared = prepare(&mut captures, ());
        assert_eq!(prepared.id.get(), u32::MAX);
        captures.clear();
        assert!(matches!(
            captures.prepare(work(None), (), 100., 1_000., 7, 500),
            Err(SignError::Unavailable)
        ));
        assert!(captures.is_empty());
    }

    #[test]
    fn synchronous_execution_checks_all_fences_before_invoking_the_provider() {
        use std::cell::Cell;
        for (raw, protocol, revision, expected) in [
            (600., 1_001., 7, SignError::TimedOut),
            (101., 1_001., 8, SignError::InvalidRequest),
            (f64::NAN, 1_001., 7, SignError::InvalidRequest),
            (101., f64::INFINITY, 7, SignError::InvalidRequest),
            (99., 1_001., 7, SignError::InvalidRequest),
            (101., 999., 7, SignError::InvalidRequest),
            (101., 1_100., 7, SignError::TimedOut),
        ] {
            let mut captures = Capture::default();
            let prepared = captures
                .prepare(work(Some(1_100.)), (), 100., 1_000., 7, 500)
                .unwrap();
            let calls = Cell::new(0);
            let outcome = captures.execute(prepared.id, raw, protocol, revision, |work| {
                calls.set(calls.get() + 1);
                Ok(result(work.receipt))
            });
            assert!(matches!(outcome, Err(error) if error == expected));
            assert_eq!(calls.get(), 0);
            assert!(captures.is_empty());
        }
    }

    #[test]
    fn synchronous_execution_is_single_use_for_success_refusal_and_malformed_results() {
        use std::cell::Cell;
        let mut captures = Capture::default();
        let prepared = prepare(&mut captures, "hex");
        let calls = Cell::new(0);
        let (format, signed) = captures
            .execute(prepared.id, 101., 1_001., 7, |work| {
                assert_eq!(work, &prepared.work);
                calls.set(calls.get() + 1);
                Ok(result(work.receipt))
            })
            .unwrap();
        assert_eq!(format, "hex");
        assert_eq!(signed.values, vec![vec![0x37; 32]]);
        assert_eq!(calls.get(), 1);
        assert!(matches!(
            captures.execute(prepared.id, 101., 1_001., 7, |_| {
                calls.set(calls.get() + 1);
                Err(SignError::Refused)
            }),
            Err(SignError::Unavailable)
        ));
        assert_eq!(calls.get(), 1);
        for (supplied, expected) in [
            (Err(SignError::Refused), SignError::Refused),
            (
                Ok(SignResult::new(vec![vec![1; 31]], work(None).receipt)),
                SignError::InvalidResult,
            ),
            (
                Ok(SignResult::new(vec![vec![1; 32]], SignReceipt::default())),
                SignError::InvalidResult,
            ),
        ] {
            let prepared = prepare(&mut captures, "hex");
            assert!(matches!(
                captures.execute(prepared.id, 101., 1_001., 7, |_| supplied),
                Err(error) if error == expected
            ));
            assert!(captures.is_empty());
        }
    }
    #[test]
    fn trusted_http_headers_are_attempt_owned_without_weakening_default_capture_fences() {
        use crate::signing_headers::ResultHeaderPolicy;
        let mut captures = Capture::default();
        for policy in [
            ResultHeaderPolicy::Deny,
            ResultHeaderPolicy::Http { protected: &[] },
        ] {
            let prepared = captures
                .prepare_with_policy((work(None), policy), (), 100., 1000., 7, 500)
                .unwrap();
            let mut signed = result(prepared.work.receipt);
            signed.headers = vec![("X-Proof".into(), "attempt".into())];
            let completion = captures.finish(prepared.id, 101., 1001., 7, Ok(signed));
            assert_eq!(
                completion.is_ok(),
                matches!(policy, ResultHeaderPolicy::Http { .. })
            );
            assert!(captures.is_empty());
        }
        for (now, revision, header) in [
            (101., 8, "valid"),
            (600., 7, "valid"),
            (101., 7, "bad\r\nvalue"),
        ] {
            let prepared = captures
                .prepare_with_policy(
                    (work(None), ResultHeaderPolicy::Http { protected: &[] }),
                    (),
                    100.,
                    1000.,
                    7,
                    500,
                )
                .unwrap();
            let mut signed = result(prepared.work.receipt);
            signed.headers = vec![("X-Proof".into(), header.into())];
            assert!(
                captures
                    .finish(prepared.id, now, 1001., revision, Ok(signed))
                    .is_err()
            );
            assert!(captures.is_empty());
        }
        let prepared = prepare(&mut captures, ());
        let mut signed = result(prepared.work.receipt);
        signed.headers = vec![("X-Proof".into(), "default-deny".into())];
        assert!(
            captures
                .finish(prepared.id, 101., 1001., 7, Ok(signed))
                .is_err()
        );
    }
}

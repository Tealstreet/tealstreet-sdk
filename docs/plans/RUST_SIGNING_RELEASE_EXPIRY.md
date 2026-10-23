# Engine-owned raw signature expiry at final release

Owner: codex-uufub4. Assigned by Sam on 2026-10-24. Reviewers:
claude-dazzy3 (classifier/lifecycle) and codex-zs5e83 (Phemex consumer).
Branch `feat/rust-signing-release-expiry`, actual landing
`60d4b8cf9ebf74aa8064d1c16a5831a6707d50ab`. This is a separate small engine
prerequisite; no frozen Plan, signer or socket PR grows with it.

## Problem and limits

Phemex `trading::Request.signed_until` is a RAW deadline used to defer repair
after an uncertain write. Dispatch recipes instead receive calibrated protocol
UTC, and `SignReceipt.expiry_ms` remains protocol UTC. The existing
`Venue::on_signing_receipt` has neither raw release time nor an engine projection.
A venue must not store UTC as raw time or reconstruct an offset.

`HttpRelease.signature_valid_until` is not the missing value. It is explicitly
`min(lifecycle_deadline, projected_signature_expiry)`, for managed repair/lease
scheduling. A 20s HTTP lifecycle can end while its signature remains valid for
60s. Using the capped bound as an absence/reconciliation fence is too early.
Its current meaning and tests remain unchanged.

This slice supplies immutable release METADATA, not receipt evidence that a
venue accepted a write. No classifier grant, resend, reconciliation proof,
command completion, root renewal, queue, timer or owner is added. Phemex
adoption follows in its own PR; no venue or legacy inventory prune is claimed.
Missing expiry stays `None`, never an invented bound. A consumer unable to
obtain its required expiry must fail closed under its own reviewed contract.

## Proposed API for review

```rust
pub struct SigningRelease {
    // Private, engine-constructed; no public constructor or deserializer.
    receipt: SignReceipt,
    time: ReleaseTime,
    raw_expiry_ms: Option<f64>,
}
impl SigningRelease {
    pub fn receipt(&self) -> SignReceipt;
    pub fn time(&self) -> ReleaseTime;
    pub fn raw_expiry_ms(&self) -> Option<f64>;
}
trait Venue {
    fn on_signing_release(&mut self, target: SigningTarget<'_, Self::Pending>,
        release: SigningRelease) {
        self.on_signing_receipt(target, release.receipt());
    }
}
```

Placement proposal: new `engine/src/signing_release.rs`, one public export.
It uses the existing `ReleaseTime` (raw, protocol, clock revision). Values are
nonsecret and Copy metadata; copying one cannot authorize another release.
`SignReceipt` and all provider/wire formats remain unchanged. The provider
cannot choose or echo an engine raw release time, revision or projected expiry.

The additive hook is a deliberate compatibility decision for review: changing
the current public trait parameter directly would require edits to numerous
held venue/signing branches and external providers. The engine invokes ONLY
the new hook at each successful release; its default delegates the old receipt
hook once. A consumer overriding the new hook updates its same exact borrowed
Pending and does not also receive an independent old-hook invocation. Reviewers
must accept this compatibility boundary before production changes.

The hook has no Cx or Store. It cannot queue, complete, retry, adjust clocks or
select a Pending by numeric id. HTTP receives the actual pending mutable borrow
for its released request; socket targets preserve today's exact target shape.
Keep socket/HTTP ordering and the old default observer behavior unchanged.

## One shared projection, two distinct meanings

At final controllable release, after existing time/revision/result/route/socket
checks, capture the engine's raw time, calibrated protocol time and revision.
Validate the canonical receipt before exposing anything. If expiry is present:

```
remaining = receipt.expiry_ms - release_time.protocol_ms
raw_expiry = release_time.raw_ms + remaining
```

All operands/results must be finite, remaining positive, and raw expiry must
advance beyond raw release. There is NO lifecycle/freshness/deadline cap on this
metadata. A failed or stale projection refuses/reprepares through existing typed
paths before hook or physical action; it must not produce an invented expiry.
The current lifecycle and signature-validity send fences remain authoritative.

There must be one private engine projector used by this metadata and by
`HttpRelease::new`; managed `signature_valid_until` then applies its existing
lifecycle min to the common uncapped result. Preserve existing Invalid/Stale
classification and empty-expiry behavior. Do not duplicate clock arithmetic in
dispatch, venue recipes, `SignStep::apply`, hosts or consumers.

Legacy `Action::SignedOutbound`: only after Dispatch::release passes its exact
pending revision, expiry/freshness and original deadline checks, immediately
before the outbound action enters final action order. There is no callback on
provider completion alone, intermediate reprepare, refusal, duplicate/late
result, retired socket or replaced signing attempt. The signed bytes, target,
route and timeout remain exact. No new id namespace or release authority.

Managed HTTP: only on the exact successful `claim_http` Released path, using
that claim's checked ReleaseTime/receipt. Bind/preparation/provider completion
do not publish release metadata. The uncapped metadata may exceed the request
deadline; the existing HttpRelease field remains capped. Projection validation
must finish before any fallible final release admission or effect is committed;
reuse the claim's preflight, never add a new fallible operation after release.

## RED-first evidence and controls

New isolated `engine/tests/signing_release_expiry.rs` uses actual Engine
dispatch and existing public APIs, no fake Context/permit, test-local clock
projection or new-method compilation failures. A probe's existing receipt hook
records precisely what the API supplies. Its raw observation is `None` because
the API has no raw metadata. This is explicitly a NEW-CONTRACT assertion RED
for absent engine metadata, not a claim that current receipt UTC is malformed
or that existing capped managed semantics are wrong. At cutover, only that
observation adapter switches to the reviewed hook; transport assertions remain.

Original working controls retain protocol receipt bytes, one final callback,
provider refusal/no callback, no-expiry `None`, and the capped managed repair
bound. Intended assertion REDs cover positive/negative calibration with delayed
provider completion, uncapped managed expiry beyond a 20s lifecycle, and the
final re-prepared receipt after a clock revision (old result stays inert).
Use nonzero offsets so a coincident raw/UTC clock cannot make the fixture pass.

Executed on the unchanged `60d4b8cf9eb` production source with
`cargo test -p safe-cex-engine --features test-support --test signing_release_expiry --locked`:
8 tests compiled and ran, 4 GREEN / 4 intended assertion RED / 0 ignored.
The four failures are precisely absent raw metadata (`None`) versus expected
raw expiry: 61000 for both calibration offsets, 61001 for the managed claim,
and 61101 for the final re-prepared receipt. The four controls pass: exact
protocol receipt/borrowed Pending, absent expiry, provider refusal without
release, and the unchanged capped managed repair deadline. No production or
inventory source changed. These are new-contract metadata assertions; runtime
and reviewer acceptance remain pending.

Classifier/lifecycle plan review at `6fa528a7319` is CLEAR, including the
additive hook and exact final-release custody. The requested additional
`managed_denied_admission_observes_no_signing_release` control passes on the
unchanged production source: a real Prepare/Bind/Claim reaches admission once,
Denied returns Revoked with one typed failure, no observation or HTTP packet,
and a duplicate claim remains inert. Scoped strict Clippy passes. Consumer
review is still required before runtime.

Additional source/new-contract controls before runtime is READY:

- No metadata or mutation of another Pending before successful physical release;
  two requests have independent target identity and expiry.
- Managed Prepare/Bind/Claim is driven genuinely. Claim denial, duplicate claim,
  preclaim response and provider/preparation failure publish no release metadata.
- Clock revision change before final dispatch forces fresh preparation, no old
  callback; expired/equality signature refuses/reprepares without renewing root.
- Invalid/nonfinite/overflow/non-advancing projection has no hook/physical send.
- Missing receipt expiry remains None; stamp-only and expiry-only receipts keep
  their canonical semantics. Unchanged provider receipt echo validation.
- New-hook override and old-hook default each receive exactly one notification.
  Socket retirement and delayed signed upgrade/message gates remain unchanged.
- `raw_expiry_ms` stays beyond the lifecycle when appropriate; no consumer or
  classifier treats it as proof of acceptance, absence or permission to resend.

## Scope, collisions and gates

Expected runtime: new signing_release module; small final-release call sites in
engine.rs/http_lifecycle.rs; the existing managed projector call in
request_context.rs; export; isolated tests. No venue/runtime adoption, provider
schema, decoder/host, Plan policy, repair timer or inventory changes.

Current claims include #1926 signing_dispatch/engine/export, #1939 and #1919
request_context/engine, #1943 sockets, #1945/#1949 entry/engine/export, #1963
signing schema and #1962 export. Documents and new tests can proceed; obtain
exact hunk agreement before touching those runtime files. Do not integrate
unreviewed dependencies or resolve a shared source conflict inside a combo.

1. Commit this plan, then compiling original assertion REDs with unchanged
   production and working controls; retain the exact output/pin. Open a draft
   engine PR documenting intentional REDs; it is not READY.
2. Request dazzy3 and zs5e83 plan/evidence/API review. Send the exact immutable
   pin. Runtime follows accepted signatures and source-hunk coordination only.
3. Implement common projection and final-release observation, preserve all old
   transport/receipt assertions, turn the REDs green, then focused engine
   feature-off/on and signing/release checks, strict Clippy and formatting.
4. Independent source/consumer review and normal exact-head CI, including WASM
   size/replay/inventory gates. No local WASM/replay builds, threshold edits,
   ignored tests or fake native migration. One watcher at >=120s, no merge
   without the overseer's explicit window and head/tree guard.

Shared implementation and actual consumer adoption are separate readiness
claims. A legacy signed release cannot acquire managed Context or write facts
from this observation; all classifier/journal custody stays unchanged.

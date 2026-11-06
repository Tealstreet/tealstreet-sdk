# KuCoin Futures guarded protective edits

Standalone SL/TP edits, virtual attached legs, and entry edits
whose cancellation also removes attached protection use one managed serial Plan.
Unsupported conditional types retain their local refusal before any cancel.
The Classic Futures API documents cancel/add, but no verified native stop-amend
route. The implementation therefore preserves the native cancel/read/create
sequence instead of guessing a native modify operation.

## Ownership and release

The genuine command entry creates a WriteAfterReads preflight with captured read
and mutation identities. Native account metadata reads keep their current owner;
the edit parks without a physical write and waits at most 10 seconds. The exact
preflight upgrades to a serial Plan. Every cancel, family fallback, signature
re-send, confirmation read, replacement and put-back uses managed HTTP
Prepare/Bind/Claim under that Plan, with a 20-second physical bound. Engine APIs
own failure classification, resend grants, delayed continuations, terminal
projection and retirement. No engine source changes or numeric terminal fallback.

The pure final-Claim veto checks the original order and complete protection group,
the original parent, a positive position on each protected side, and each actual
trigger source. A quote must have a native field receipt no more than 5,000 ms old;
5,000 is live, and equality of trigger and reference is crossed. The receipt's
value must still equal the Store field. Public socket observations refresh only
fields actually present, including unchanged values. Discovery stages native
field receipts with their row-arrival time and commits only after a valid complete
envelope. Catalogue mark/index fallbacks and arbitrary Store updates grant no
receipt. References are restricted to listed markets and pruned on catalogue
refresh. Frame-fed edits fail NotSent rather than fabricate local provenance.

Attached st-orders use one shared stopPriceType, so every affected leg is checked
against that actual source. A changed group or parent fails before cancellation.
Native type, execution price, trigger, lots, time in force and closeOrder semantics
come from the preceding stop-limit repair. New placements and ordinary edits keep
the legacy wire path; a newly protective order discovered during ordinary edit
planning is refused rather than sent through that path.

## Confirmation and restoration

An accepted cancel does not authorize create. An exact-id terminal order read must
show cancellation (or a completely filled terminal row), explicit finite,
nonnegative fills, mutually consistent fill fields and no stale fill watermark.
Malformed/refused/lost proof gets at most two read retries; a live target gets at
most 30 polls. Missing or uncertain proof never creates or restores. New fills
abort the edit and report lost protection; they never resize a guessed put-back.

The replacement payload is validated and frozen before cancellation. A definitive
replacement refusal permits one exact original standalone put-back, at the original
native lots and native type, with a fresh client identity. Attached/group/parented
shapes never get a guessed put-back; a definitive refusal reports Unprotected and
asks the user to re-place protection. A successful original put-back reports
Restored. Its Claim still checks terminal proof, newer fills and the protected
position. A put-back itself has one attempt, including signature refusals.

Documented signature refusal may re-sign cancel or replacement once, using the
engine's opaque grant; fallback cancellation is limited to a definite 404 family
refusal. Every resulting cancel Claim rechecks the guard. HTTP 429/408/5xx, native
rate codes, unlisted codes, transport loss, malformed replies and ambiguous success
IDs never restore or resend writes. Stage-specific present_plan_failure words
cover cancel, proof, replacement and put-back expiry and uncertainty.

Whole-action outcomes are engine journal projections. An initial veto is NotSent.
A veto after a previously released, definitively rejected family cancel remains
Rejected; a withheld re-sign can conservatively project Unknown. Neither case
claims whole-command NotSent after an earlier physical release. The original
protection remains untouched in those controls.

## Evidence

The trading and private fixtures drive real signing and Prepare/Bind/Claim.
Controls hold a real Prepare before the final Claim, cover the 5-second boundary,
trigger equality, changed fills/group, flatness, per-field freshness, exact proof,
ambiguous failures, bounded re-signing, one-shot restore and generation reuse.
Nine original assertions fail against the unchanged PR 1 source; the successful
family-fallback/re-sign control is already green there. The explicit serial
refusals-complete policy permits success only after the exact replacement ACK;
fail_plan still projects earlier facts. Existing new-placement and ordinary-limit
request-shape controls remain. The
private full-position fixture supplies a genuine public mark update and drives
the managed pacing step; its financial and payload assertions are retained. Existing controls change their
expectations where the standing contract is stricter: a partly filled standalone
stop refuses before cancel, a newly filled cancelled stop sends no replacement,
and already released cancel attempts retain the engine's whole-action outcome.
Virtual-leg positives now supply the protected position; both siblings use the
native opposite side rather than the old TP fixture's impossible buy leg.

Sources:

- [Classic Futures order endpoints](https://www.kucoin.com/docs-new/rest/futures-trading/orders/add-order)
- [Classic Futures error codes](https://www.kucoin.com/docs-new/error-code/futures)
- [Official pinned request schema](https://github.com/Kucoin/kucoin-universal-sdk/blob/c2f39bdb8b7d5f31cfb3fd532f9baf2f09fd8f12/spec/rest/api/openapi-futures-order.json)

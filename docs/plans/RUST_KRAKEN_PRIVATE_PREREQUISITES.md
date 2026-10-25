# Kraken native adoption: local prerequisite cut

Local, unpushed on actual 9977ccf1aa6, authorized by the overseer while #2156
qualifies. Companion to landed RUST_KRAKEN_PRIVATE_DISPATCH_ADOPTION.md and its
26 original controls. No bridge activation, hosted adapter, inventory prune or
native signing cutover is authorized by a prototype result. Owner codex-9czdjh;
uufub4 reviews nonce/dispatch and fe7lgm the shared engine seam. Numeric rows
below are explicit proposals for owner disposition, not native parity claims.

## A. Enforced legacy fence, drain and atomic takeover

Actual legacy reserve_kraken_legacy uses SHA256(decoded secret), returning a bare
u64. Dispatch hashes a tagged trusted stable subject. Both happen to use NONCES,
but neither the key nor outstanding authority is shared. Legacy has no released
floor or physical-drain lease. No existing API can prove that copying a maximum
is safe. A fence followed by a caller's boolean drained is not a solution.

The proposed shared transition has three phases with one-way, generation-bound
custody, no public raw nonce seed and no callback under the nonce-store lock:

1. Before taking any live issuer over, its owning hosts must register every
   legacy issuer and retain a physical-work lease for every prepared/reserved,
   queued, signer-pending and sent request. Registration and reservation share
   the nonce-store fence. Existing untracked issuers are a hard refusal, never
   an assumed deployment drain. Native and browser/WASM host coverage are separate.
2. A trusted construction/host transition coordinator closes issuance for the
   complete legacy namespace set. Old signer instances and new signer creation
   both refuse after this fence. Cancel prepared/queued work and drain all sent
   transport work at the host boundary; only actual terminal/cancellation and
   transport resource retirement consumes a physical lease. A request's engine
   timeout, dropping a signer, a user boolean or a signature callback cannot
   independently certify the host drain. The fence remains closed on failure.
3. With the exact fenced epoch and zero outstanding leases, one store transaction
   validates the full source set and target subject, imports max of all reserved
   maxima AND max of all released maxima (including existing target state),
   permanently retires legacy issuance and opens dispatch takeover. Missing or
   stale evidence, unknown issuers, overflow, storage failure or target mismatch
   leaves every floor/phase unchanged. No partial source import. Existing raw
   reservations conservatively count toward the release floor if exact release
   tracking did not exist, but only after real host drain; they cannot be ignored.

Source shape to pin with shared owners: non-cloneable fence/drain capabilities
minted by tracked host lifetime, not a serialized import API, arbitrary caller
number or namespace alias. Trusted setup binds decoded-secret identities to the
trusted account subject before the transition; no secret-derived dispatch subject.
Equal API keys with different secrets require a single account transition over
ALL legacy digests for that subject, never one digest at a time. A source key
cannot be assigned to two targets. Separate processes/WASM memories require an
explicit coordinator and full issuer census; an in-process unit test proves no
cross-process rollout. Production remains closed until that host proof exists.

Controls first: reservation racing fence; existing signer after fence; stale epoch;
held provider/Prepare/Bind and live sent request block takeover; cancel without
resource retirement still blocks; unknown issuer refuses; two source digests and
an existing higher target merge both maxima atomically; reverse release cannot
move the floor down; UINT64_MAX stays exhausted; unrelated subjects are unchanged;
failed takeover changes no maxima; no new dispatch reservation before takeover.

## B. Owner/window and terminal policy proposal

Native Futures sends have timeout_ms=None; Spot explicitly sets 20,000ms per
physical request. Do not claim a native Futures 20s contract. Shared command entry
already has a default 120,000ms caller wait, overridden only by the accepted
caller timeout. All source changes disclose their new bounds in CALLERS.

| Native work | Genuine owner | Logical/caller window proposal | Physical window / retry |
| --- | --- | --- | --- |
| Futures accounts/openorders/openpositions/leveragepreferences refresh | Independent Background Read | 20,000ms, NEW | 20,000ms NEW, Never; native poll/backoff starts a fresh owner |
| Futures leverage command Read | Command Read/selected admitted read | Original caller wait cap, never renewed | min(20,000ms NEW, owner remaining), Never |
| Futures fills/account-log/orders history | Command Read history root + one current page | Original caller wait cap; no renewal; native maximum 50 pages retained | min(20,000ms NEW, root remaining), Never; no retry grants from pagination |
| Futures serial trade/settings command | Preflight to genuine serial Mutation Plan | Queue/preflight retains caller wait; no root wall-clock after serial admission, preserving uncapped native serial length | 20,000ms NEW per call, one attempt; reads in the plan retain Read role |
| Futures protective cancel then replacement | Same Mutation command and original order provenance | Serial ACK/provenance barrier, not simultaneous cancel/place | Each physical call 20,000ms NEW; no automatic write resend |
| Spot Balance/OpenOrders | Independent Background Read chain | 100,000ms NEW total ceiling for maximum 5 physical sends | Native 20,000ms per send; existing three transport retries plus one independent invalid-signature resend ledger |
| Spot token | Actual socket-epoch Session | Same 100,000ms NEW retry-chain cap, additionally epoch retirement | Native 20,000ms per send; same nontrade ledgers; no late token subscription |
| Spot serial trade command | Genuine serial Mutation owner | Caller-wait queue then no root wall-clock; no new order-count cap | Native 20,000ms; one invalid-signature resend only; no write transport retry |

Signing remains bounded by the actual dispatcher attempt window and existing
5,000ms signature freshness, never a renewed logical owner. Spot signature retry
is classified definitive refusal and is distinct from nonce-floor repair; three
transport retries do not reset on an invalid-signature resend and vice versa.
All equality/nonfinite failures occur before IDs, provider work and I/O as the
shared contract requires. Native Futures reconnect/backoff and Spot zero-delay
retry timer/poll cadence remain literal; nonfinite events cannot schedule NaN.

Before send, Mutation refusal is typed NotSent; released unanswered writes are
Unknown and must not be resent. Native genuine response classification, message
prefixes and accepted IDs are preserved. Shared journal owns terminal truth and
partial-progress receipts: native trading_finish currently strips Rejected or
NotSent to None after an accepted ID/effective write/cancel. Its shared-journal
projection must be explicitly ruled and disclosed before native source adoption;
no venue callback erases released write evidence. This is an OPEN behavior gate.

## C. Nonce-compatible protective barriers

Actual native Futures protective amendment is Step::Cancel then Step::Place:
recheck original filled/price/amount/kind/trigger provenance before cancel, wait
for real cancel response, suppress replacement after execution, and retain
amend_canceled/accepted IDs in failure messages. Spot uses its native AmendOrder.
Do not convert the Futures serial pair into simultaneous group HTTP egress.

The shared plan barrier API currently refuses all nonempty barriers under
NonceUnscheduled, even with genuine nonce capability. Group Claim also commits
ordinary Lifecycles without an atomic group nonce-floor check. Removing only
the admission guard would release nonce work unsafely. The shared owner must
pin whether the required Kraken barrier is a serial provenance/ACK barrier,
a prepare-all custody barrier with staged sends, or an existing parallel
MutationBarrierBound used only for genuinely parallel native work. No native
cancellation/replacement ordering is changed to fit an API.

For genuine parallel groups, proposed private atomic commit: prepare all fallible
admission, reconciliation storage and pure venue validation outside the store
lock; then validate every nonce against an evolving per-subject released floor
in declared send order and commit all Lifecycles/floors under ONE lock using an
engine-private infallible closure. No Mutable scheduler callback or public
arbitrary commit closure. Same-subject group nonces must strictly increase in
native dispatch order; a stale/invalid sibling releases no member and advances
no floor or journal. Pure admission precedes this transaction; observations
and native callbacks follow the completed transaction with the lock released.

A stale nonce sibling requires bounded group reprepare/cancel custody, retaining
managed Attempts and caller windows, replacing the precise affected preparation
and receipt/MAC, without spending credits for Bearer/route repairs. The existing
eight nonce-repair budget is not raised or silently reset. No no-op release hook,
barrier deletion, partial release or journal early publication counts as proof.

First real Engine original: genuine nonce-capable NonceUnscheduled Plan with a
nonempty Mutation barrier must admit and retain Prepare/Bind/Claim custody;
legacy default NonceUnscheduled without permission must still refuse before
IDs. Further controls: reverse provider completion, held sibling, stale final
Claim race, equal/reversed same-subject nonces, independent subjects, partial
provider failure, immutable protective provenance, cancel ACK before replacement,
execution during cancel, zero partial observations/reconciliation/floors, exact
capacity recovery and disposed resources. Keep all old 26 native originals.

## Local delivery gates

Commit plan/policy checkpoint, compiling assertion REDs, then bounded core/nonce
review before widening shared release authority. Implement only agreed seams;
unknown host evidence and unresolved policy remain closed, never activate via
an option. All work stays local/unpushed per the overseer. Scoped engine/Kraken
checks at JOBS=4, no workspace-wide build unless needed to freeze. Any future
publication of generic engine changes measures Kraken, Bybit, Bitget and combined
on the composed landing tree, same profiles/budgets, and deletes own scratch
targets after every measurement/review build. No hosted or namespace activation
is inferred from unit tests, native proof or a passing size check.

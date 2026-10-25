# Kraken native adoption: local prerequisite cut

Started locally on actual 9977ccf1aa6 while #2156 qualified. The overseer now
authorizes publication of the inert policy and shared-fixture checkpoint only. Companion to landed RUST_KRAKEN_PRIVATE_DISPATCH_ADOPTION.md and its
26 original controls. No bridge activation, hosted adapter, inventory prune or
native signing cutover is authorized by a prototype result. Owner codex-9czdjh;
uufub4 reviews nonce/dispatch and fe7lgm the shared engine seam. Numeric table B was approved by claude-ddi2il on 2026-10-26. New bounds
are disclosed policy, not native parity claims.

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

### Concrete host coverage gap before the takeover source cut

At incoming 8fcedfbbdfc, Kraken Signer::new retains only the decoded secret and
digest. Signer::nonce calls crypto::nonce::kraken, which reaches the exported
engine reserve_kraken_legacy returning a bare u64. None retains an issuer or
physical-work lease. All three boundaries must participate in tracking; a new
tracked entry cannot silently certify callers of the old bare entry.

The native Transport::request returns unit. Its NetworkTransport implementation
spawns preparation and drops the handle returned by bound.dispatch, so the host
currently has no joined retirement receipt for that legacy request. In the JS
host, dispose aborts controllers then clears requests; it does not wait for every
transport promise to settle. Neither operation proves a generation drained.
Custom transports and old hosts without the tracking contract remain unsupported
for takeover, while their existing unfenced legacy requests continue unchanged.

The next shared source review must bind issuer registration and every reservation
to a host generation, with an exact request-to-lease handoff through preparation,
signing and transport retirement. Lost or merely dropped leases must keep the
fence closed. Old raw entry use without that handoff marks the generation
untracked; it never mints drain evidence. Host completion proves resource drain
only, retaining uncertain venue outcomes. Only a complete reviewed generation
can produce the opaque fenced/drained capability consumed by the atomic import;
the import reads both maxima from the same locked store, never caller numbers.
This audit is not implementation or host-drain proof. Do not expose a takeover
switch until the native and WASM host adapters satisfy these conditions.

## B. Approved owner/window and terminal policy

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
prefixes and accepted IDs are preserved. The shared journal retains truthful per-item Rejected/NotSent outcomes and
partial-progress receipts. The caller-visible terminal stays byte-compatible:
native trading_finish strips Rejected/NotSent to None after effective progress,
accepted IDs or a protective cancellation. Apply that projection only to the
caller clone, never to the journal evidence. The overseer approved this explicit
divergence; CALLERS names a follow-up for richer caller projection. Pin both
truthful journal evidence and the exact native caller None in source controls.

## C. Nonce-compatible protective barriers

Actual native Futures protective amendment is Step::Cancel then Step::Place:
recheck original filled/price/amount/kind/trigger provenance before cancel, wait
for real cancel response, suppress replacement after execution, and retain
amend_canceled/accepted IDs in failure messages. Spot uses its native AmendOrder.
Do not convert the Futures serial pair into simultaneous group HTTP egress.

The required Kraken barrier is SERIAL provenance and real-ACK custody, per
uufub4's b4a1689 review. It does not authorize parallel MutationBarrierBound
admission or prepare-all as an assumed prerequisite. Keep nonempty parallel
barrier refusal unchanged. A first generic original demanding its admission
was a mistaken geometry probe; its log is preserved as superseded evidence,
not an accepted Kraken RED. The corrected baseline tests preserve that refusal.

fe7lgm's engine disposition uses the existing streamed serial Plan, with no new
release hook, Store-view API or prepare-all seam. From the command's genuine
WriteAfterReads Preflight, admit_serial_plan admits Cancel with no declared
barriers. Leave NonceUnscheduled's nonempty-barrier refusal in admit_plan_root
unchanged. A venue snapshot never substitutes for the current Store.

In Cancel's own validated DATA delivery, record its native write fact first;
validate_serial_continuation requires that leg's recorded answer. Recheck the
original filled/price/amount/kind/trigger provenance against the current Store,
preserving the native account removal/tombstone ordering. Only an Accepted
cancel with unchanged provenance and no observed execution admits Place through
continue_serial_plan under the same command and journal. Rejected/Unknown,
changed provenance or execution suppresses Place: finish_plan_call then
fail_plan, retaining the cancel fact and any accepted cancel ID. Never rewrite
an accepted Cancel to NotSent. The engine API intentionally supports serial
continuation after some definitive rejections; Kraken's protective consumer
must therefore enforce its stricter Accepted-only rule explicitly.

Each call obtains its own reservation from the dispatcher after the previous
call's DATA. Place's nonce must exceed Cancel's released nonce. Each call keeps
its independent eight-credit repair budget; no write resend is added. The
uncommitted release-view declaration was removed without being wired or run.

The separate future parallel nonce-group seam stays out of this cut. It needs
complete member/subject/evolving-floor validation before any commit. Existing
http_group_claim::prepare_group_commit publishes insert_group records before
release: those records are NOT merely reserved capacity and cannot precede a
failing nonce transaction. A later design must reserve storage outside lock,
then commit private Lifecycle/journal/floor changes infallibly together; observe
and call native code only after unlock. No allocation, fallible step or arbitrary
callback after the first change. A stale member retains exact Attempts, windows
and each eight-credit nonce ledger, cancels its exact preparation, and causes
whole-group revalidation. This requires a separate shared-owner-reviewed plan.

Required serial controls: replacement admission outside Cancel's exact DATA
refuses, including a delivery without a recorded fact; Rejected/Unknown cancel
seals with its truthful fact and no replacement; provenance change or execution
during cancellation suppresses replacement; only after the real Accepted cancel
ACK does Place reserve its higher nonce; no write resend. Post-cancel replacement
refusal keeps truthful journal and native caller projection. Shared-fixture
controls prove engine custody only, not native adoption. Keep all 26 landed
adoption originals and old wire controls; native consumer REDs/source remain
separate gates.

## Local delivery gates

Commit plan/policy checkpoint, compiling assertion REDs, then bounded core/nonce
review before widening shared release authority. Implement only agreed seams;
unknown host evidence and unresolved policy remain closed, never activate via
an option. This PR publishes only the policy and shared-engine controls; further source
implementation and activation retain their separate owner gates. Scoped
engine/Kraken checks use JOBS=4; workspace gates run when needed to freeze. Any future
publication of generic engine changes measures Kraken, Bybit, Bitget and combined
on the composed landing tree, same profiles/budgets, and deletes own scratch
targets after every measurement/review build. No hosted or namespace activation
is inferred from unit tests, native proof or a passing size check.

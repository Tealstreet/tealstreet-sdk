# Hyperliquid trading ownership and EIP-712 dispatch

## Beta execution after the managed-info slice

Current owner: codex-zs5e83. Current base: master `2f2b62a02e8` (#2303).
This section supersedes the historical base, phase holds and ownership notes
below. One combined venue PR consumes existing shared APIs; no hosted activation
or new engine API is proposed. #2303 removed two request fingerprints. The next
slice targets the remaining 15 request fingerprints and 17 signing fingerprints;
these are lexical inventory facts, not counts of physical requests. The later
order_read_epoch adoption remains a separate, subsequent slice.

### Trusted construction and selected authority

Both native setup and the SDK's trusted setup mapping declare Primary
`source: scalarOnly`, `signingAlgorithm: secp256k1Digest`, required signing and
hex scalar decoding. The primary LocalIdentity starts from Default (absent key)
and assigns only the consumed scalar. Public options.key, applicationId and vault
remain public account configuration and never become credential material. No
secondary is installed. Existing caller metadata is validated normally; stableId
is not synthesized and is not required by the address-keyed nonce domain.
A native/TS golden binds identical setup JSON, including prefixed/unprefixed
hex, absent key and an ordinary nonempty-key negative. Invalid setup refuses.
The measured legacy scalar parser does not trim: ASCII space, NEL and BOM
around the scalar all refuse. Goldens retain that byte-exact policy. Native
accepts 0X as well as 0x; the old SDK Wallet rejects 0X. The provider cutover
uses the native hex decoder consistently on both trusted setup paths, preserving
the same proven scalar rather than changing its bytes. Invalid hex, length,
zero or out-of-range scalars yield fixed errors, never echoed input. Options,
Debug and notices expose no scalar. Failed engine-owned address setup grants no
private Mutation authority; no venue or TS derivation substitutes for its proof.

The venue Options replaces its raw secret with an installed Identity reference.
Trusted registry helpers install the provider and identities before host-owned
address setup. Hyperliquid declares Primary public-address setup when private,
HYPERLIQUID_AGENT address nonce domain, and dispatch_nonce. At start, the venue
rebuilds its Primary Slots from cx.address_nonce_identities; it never expects an
old clone to gain a binding. A selected Mutation uses Selection::authority and
select_with_nonce(HYPERLIQUID_AGENT), not identity.credential. The actual sealed
agent alone keys raw-ms, scale-one, MAX_SAFE reserved/released nonce custody.
Wallet/user/vault/network/ref/stableId do not create a new agent counter.

Perps retain the configured-agent mismatch warning. Spot retains its existing
configured key versus actual signer check, using the engine-sealed public address
instead of deriving it from a scalar. User/account and vault selection remain
separate from that proof. Hosted remoteSigner and payloadSigner remain refused;
this cutover enables local-key accounts only. Injected empty credential identities
do not acquire the trusted ScalarOnly marker. Existing legacy engines must be
retired before a new engine starts; sharing the reserved counter alone does not
fence an old unfenced release.

### Recipe, queue and journal API

Exchange::new(identity, nonce_claim, action, vault, testnet) freezes ordered JSON
and MessagePack before provider work. Its unsigned intent is exactly POST
/exchange, Content-Type application/json, no body. Prepare reserves through the
claim and computes the native action hash and Agent digest, then emits exactly
same-ref PublicAddress plus Secp256k1Digest Sign. The receipt includes the genuine
prepared stamp and decimal nonce. Apply validates the complete request/result/
receipt/header graph, exact 20-byte address, low-s signature and recovery over
the origin-computed digest to the sealed address before publishing the body.
Stale preparation repeats real work; no eager signature/body cache survives.
Every distinct physical successor mints a fresh selected Claim; stale repair
remains engine-owned. Legacy handoff must cancel/fence late host release and
join the actual old work before starting the new engine; dispose/map clearing
or a shared reserved maximum alone does not prove that boundary.

Each real command owns a WriteAfterReads Preflight with no read credential and
the selected write authority. Early refusals use the real command-entry API,
never a deferred numeric command id. A queued command retains a parked Preflight;
only its exact on_http_preflight_ready callback admits the first serial Mutation.
The serial root is unbounded, each physical call is 20 seconds, attempts one,
CallAuth::Preflight, and refusals_complete false. Empty work uses genuine empty
completion, without a dummy HTTP/write fact. Builder reads retain their actual
background read owner and eight-second readiness timeout; they do not invent
Session or Read-plan authority. Every successor call uses its exact Plan/Slot.

The registered release domain has one 334ms group and Mutations membership.
The engine stamps member Contexts, queues successful physical submissions FIFO,
and advances the gap at final Claim. Info/builder reads are not members. Native
job order and dependent amend cancel-before-create remain. No reservation is
rolled back, and stale repairs keep the engine's cumulative budget. The old
queue-time write_at timer is removed in the same cutover; there is no dual pacing.

Delivery validation precedes classification, reduction or Store publication.
The shared journal records accepted, refused, partial or Unknown write facts,
retains structured IDs and owns the single command terminal and order locks.
Transport failure, deadline, malformed data and unproven HTTP never resend.
Unknown retains the existing placement fence and starts real reconciliation.
The pure reply classifier retains exact row cardinality/kind, per-row cancel
proofs and late-fill behavior; lexical macro fragments are not hidden by renames.

### Required production-bound regressions

- Actual native and SDK/WASM local setup retain no scalar in venue/options;
  ScalarOnly has credential_ref None, genuine managed signed release, and Present
  or Credential interpolation of the signing ref refuses before provider work.
- Native signing goldens cover main/test, vault, Place/brackets, cancel,
  leverage and scheduleCancel, with byte-equal body/digest for fixed nonce.
- Real venue commands pin queued ownership/capacity, delayed provider/no early
  HTTP, prepare/bind/claim, deadline equality, stale repair and 334ms final-release
  pacing across two commands. Missing/invalid key and unsupported signer refuse
  before financial side effects. Public reads remain unsigned and unpaced.
- Malformed receipt/address/signature/high-s or changed outbound intent publishes
  nothing; same-agent engines/vaults/networks share nonce highwater, with no
  stableId requirement. The same packet cannot claim twice.
- All existing financial assertions remain, including mixed Place IDs/Unknown,
  cancel accepted/terminal versus refused rows, prior-call partials, amend source
  fill, builder timeout and scheduleCancel uncertainty. Genuine failure and
  no-I/O controls replace every migrated legacy terminal.

CARGO_BUILD_JOBS=2, one cargo at a time. Scoped native/registry/SDK tests, strict,
fmt, bans, exact removal-only inventory and CI family/replay/required coverage
qualify the source. JS/WASM-host gates are the beta record, not native-only tests.
Any cap raise is explicit and needs auditor disposition; no local family matrix.
Credential review: m26ps4; money/recipe/API review: 9czdjh and/or fe7lgm/dazzy3;
every landing requires 8x5tm1's exact-tree GO. This API direction grants no source,
size, hosted or live-credential clearance.

## Local source qualification checkpoint

The local cutover removes every HL runtime raw exchange request, numeric command
terminal, agent-scalar holder and eager signature call. It uses the declared
ScalarOnly local provider and the sealed installed identity for every physical
successor. The engine owns each selected nonce claim, serial leg and journal.
The original native sender/hash helpers remain test oracles and the disabled
TxFlow consumer's dependency; HL runtime has no fallback to them.

The current full native receipt is 359 PASS across 27 targets, including the
38 production custody controls. The later scoped source qualification has
38 custody, 37 trading and 3 security controls green. Crypto capability ON/OFF
is pinned; unchanged AsterDEX and GRVT suites total 628 PASS. Strict clippy for
HL/registry/crypto all targets passes. Final source reviews, composed reruns,
final composed JS/WASM qualification, CI bytes and auditor GO remain required.
Fresh unoptimized browser bindings already passed the actual EngineHost path for
both names and both native/private-capsule setup forms. The CI family and combined
conformance paths run that same production-binding control.

KnownPartial is the engine's deliberate accepted-plus-cited-refusal projection
(outcome None with structured IDs). Unlisted errors and unmatched placement rows without an oid remain Unknown,
without write replay. Confirmed waitingForFill attachments fold into their
accepted parent's real oid; a standalone zero-size reduce-only positionTpsl is
predeclared Setting and may acknowledge waitingForTrigger with no oid. The changed legacy outcome
oracles are disclosed in the caller fragment; prices, sizes, grouping, actual IDs
and the no-third-send/no-retry predicates stay asserted. Native packet fixtures
now drive real readiness and release-domain timers without firing call deadlines.
The read-only test-support clock accessor proves no host resource settlement.

A settings write is marked at issue, so an engine-owned 503 seal cannot skip its
native uncertainty guard. The existing reconcile hook joins the targeted read;
its revision must follow the write before it can clear the mark. The connected
stale-read/recovery control goes assertion RED when the issue mark is removed.
A real joined failed address-setup execution leaves public reads available but
refuses a private write before Prepare or provider signing.

The source supports a fresh JS/WASM host, not warm legacy-owner replacement.
Synchronous dispose cannot prove the original transport/provider promises joined.
Replacement activation remains held until that external join and callback fence
is genuinely demonstrated; a cold page teardown alone is not such evidence.
Hosted signing and order_read_epoch adoption stay outside this change.

The current request inventory has zero HL entries (15 removed in this slice;
#2303 removed the first two). Pure enum predicates and explicit queued-read
ownership remove the conservative non-call fragments without renaming receivers
or weakening the checker. All 17 original signing fingerprints are removed;
two new test-only startup JSON/policy fingerprints remain explicitly recorded
as fixture inputs, not runtime signing debt. Their addition needs auditor GO.

## Historical original plan and controls

Owner: codex-zs5e83. Native/recipe reviewer: codex-9czdjh (or codex-uufub4).
Base: b0ee0a9a4d5654980dea00d67d2066c7e524ed87, landing branch
origin/feat/rust-rewrite. Assignment: trading.rs, 15 frozen request-inventory
occurrences. PLAN FIRST, then original assertion controls; no production edits.
#2146 integration and the GRVT G3/G6 cutover take priority when unblocked.

## Exact slice and count

The Hyperliquid crate serves perps, standalone spot and merged spot. The family
has 22 request-inventory occurrences; trading.rs owns 15. Its local-settings
exits already use done_local_setting/refuse_command and stay unchanged.

The 15 frozen entries are eleven actual calls plus four lexical macro fragments:

| Native responsibility | Calls / inventory hashes |
| --- | --- |
| Place validation, cancellation lookup, amended-order validation | ae320910 x3; 50ff9b0e, 49dcc4f8, 84bc0552, e0159b37: seven fails |
| Missing/private-disabled signing material | 15cc6440: one fail |
| Shared /exchange sender | ca0d3809: one request |
| Job terminal | 06eea309 and 4310c56c: one fail and one done |
| Macros mentioning request.effect in a Cx-tainted callback | dc655ef7, 48fcd933 x2, 16c49568: four counted fragments |

The scoped checker independently reproduces all 15 and their exact counts.
The four fragments are not four HTTP operations. Extracting native response
classification into a pure codec, with no Cx parameter or escaping callback,
can remove their legacy provenance when effect decoding is adopted. A rename
of request, changed whitespace or a scanner exception is not a migration.
If the source leaves any such provenance, retain its manifest entries and
report the actual smaller prune. Target 15->0 gives family22->7 only after the
exact source scanner proves it. Original-control and plan commits prune zero.

Owned production intent: trading.rs, a provider recipe/setup module, the small
Pending::Trade/callback union in lib.rs, and explicit feature opt-ins. Installed
construction may need registry integration, but its public-address/credential
shape is an OPEN shared-fit gate below. Existing sign.rs math may become a pure
recipe helper; preserve native oracle controls. Do not migrate account, builder
read producers, history, catalog, sockets, or schedule.rs's remaining legacy
credential refusal by pruning a nearby line. The valid ScheduleCancel job does
flow through the shared trading sender, so that path is a composition control.
No competing Hyperliquid source reservation is assumed from old plan ownership.

## Native behavior to preserve

trade_command validates PlaceOrders before queueing. Cancels resolve every id
and market before any send. AmendOrder refuses another pending amendment,
missing or non-open/non-creating source, then builds a new client id and
preflights replacement before queueing [cancel, place]. Native TP-to-SL inference
uses agreeing positive mark/last/index prices. Guard both queued and in-flight
source fills, and preserve the confirmed replacement id if a late source fill
arrives after replacement dispatch.

A single active Job and FIFO queue own all placements, bracket legs, cancels,
settings, and valid scheduleCancel controls. One Step::Place can issue several
native order rows in one HTTP action; this is not a managed mutation group.
The response classifies per-row outcomes. There is no order-count cap and no
write retry, including a lost response. Cancel ACK/status evidence must be
sufficient before an amendment can issue its replacement. A terminal-cancel
message is idempotent only for a non-amend cancellation, including the exact
optional asset suffix. A bare cancellation loss or malformed ACK never permits
replacement. Empty cancellation scopes advance without signing or a request.

334 ms pacing currently starts when a raw request is queued (write_at=cx.now),
not when a delayed host claims it. Builder discovery runs at most once per
native placement stage and holds the active job for eight seconds; reply or
expiry resumes it, expiry means builder_approved=false and native unsigned
builder queries keep their current owner. Preserve optional builder inclusion,
configured fee/address, all price/size rounding, zero-bracket omission,
positionTpsl/normalTpsl/na, cloid generation and the pre-request progress receipt
for every actual bracket row. Private-side provisional records and Canceling
must never precede successful shared ownership/reservations.

Native signing hashes MessagePack with declaration order, then appends the
u64 big-endian nonce, vault-presence byte and optional 20-byte vault address.
The Agent EIP-712 domain is Exchange/version1/chain1337/zero contract; source
is a for mainnet, b for testnet. POST /exchange has JSON Content-Type and exact
action, nonce, signature r/s/v, optional vaultAddress order. It is not GRVT's
order digest, chain id, login Session or cookie bundle.

Nonce domain is HYPERLIQUID_AGENT: raw milliseconds, safe-integer upper bound,
max(floor, previous+1), keyed by the normalized actual agent address. It is
shared across engines, vaults, key hex spellings and TxFlow in one process/WASM
memory. Vault, configured account/application_id, opaque ref and stable-id text
must not create independent counters for the same agent. Separate processes
retain the existing deployment coordination obligation.

Native DATA distinguishes complete explicit top-level refusal, definitive
per-row rejections, mixed accepted/rejected rows, malformed/unknown statuses,
network loss and HTTP status. Keep exact message/kind/native metadata controls.
An Unknown schedules native reconciliation and blocks dependent settings until
fresh account proof. Successful ACKs preserve native core/spot/HIP3 publication,
pending/expected ids, follow-up timers and revision fences. This slice neither
changes their native read producers nor claims reconciliation proves a
scheduleCancel arm/disarm. schedule.rs's strict control codec remains separate.

finish_trade currently strips Rejected/NotSent to None after effective work or
returned ids. The shared Plan journal owns the adopted command terminal. Record
any operation-versus-command outcome difference in CALLERS; never preserve it
by hiding a write fact or resetting partial ids. Definitive mixed batches may
have accepted effects even when the native reducer returns an error early.
The classifier and reducer must agree on exact row correlation before source.

## Intended shared custody

1. Genuine entry acquires a WriteAfterReads Preflight with no read credential
   and the trusted selected Primary write credential. Preserve the native
   validation/diagnostic precedence as far as the shared entry contract allows.
   Every early local refusal seals via fail_preflight/refuse_command, never a
   deferred numeric command id. Admission/capacity/clock/duplicate refusal owns
   the single terminal and runs before cloid/nonce allocation, queue and Store
   changes. Require declared signing support before a replacement's cancel.
2. Keep the FIFO policy, but queue a retained Preflight. park_preflight_for(None)
   covers the queue, existing builder readiness and re-parks from ready custody.
   Release the actual front owner through request_preflight_ready; no borrowed
   venue timer may issue a later owner's HTTP context. Builder query/8 s timers
   are native prerequisites, not fake serial Read calls or session waiters.
3. On native eligibility, admit_serial_plan with one Mutation call, attempts=1,
   and SerialPolicy refusals_complete=false. Do not predeclare finite capacity
   for an unbounded input batch. Empty jobs use done_empty_plan without dummy
   HTTP or a dummy Mutation fact. Each later actual call uses the same Plan
   and a fresh Slot from its exact DATA continuation. Native delays use the
   serial timer/condition machinery only after API fit is reviewed.
4. Queue request_signed_managed with a real nonce-bearing SignStep. Its unsigned
   intent has only native static headers and no signed body. Prepare creates
   PublicAddress + Secp256k1Digest work for the same selected ref, validates the
   exact receipt/result/header shape and recovered low-s signer, and apply builds
   the signed packet. No raw scalar in venue state, no eager signed-byte cache,
   no fake provider, invented identity/credential/nonce authority, or fallback.
5. validate_plan_delivery precedes every codec, reducer, Store mutation,
   builder/queue wake, ACK and terminal. classifying via http_write_fact/batch
   records Accepted/Rejected/KnownPartial/Unknown, then the native reducer
   applies only proven data. A transport/physical timeout, preparation failure,
   integrity failure, uncertain status or malformed row set is terminal: no
   auth resend, generic retry, session resend, or replacement after Unknown.
6. finish/continue/done/fail use the Plan's exact call custody. The command owns
   all held order locks and ids until its journal terminal. Capture bounded
   native reconciliation metadata before release and keep late callbacks
   isolated from queue/command completion. Retire precisely once.

The proposed call bound is 20,000 ms from call eligibility/admission through
Prepare, signing, Bind and physical I/O, with attempts=1. Native trading supplies
no explicit request timeout. This is a NEW contract, not native timeout parity;
the overseer approved that value and one-attempt/unbounded-root shape; CALLERS
must disclose it before source.
The serial root has no wall-clock and WriteAfterReads binds no caller receipt.
A caller receipt expiry does not kill a queued/active multi-step job. Do not
invent a 20 s whole-command cap. Delayed serial calls retain the existing API's
eligibility/window semantics, not a guessed timer.

## OPEN shared-fit and behavior gates

| Gate | Exact question and proposed owner |
| --- | --- |
| Actual-agent nonce authority | Current NonceClaim::mint and matches_identity use NonceKey::stable_id. HYPERLIQUID_AGENT legacy reserve uses ethereum_address, so selecting that domain with today's stable-id claim would split the sequence. Need an engine-issued, selected-identity-bound actual-agent subject attested by provider PublicAddress; preserve TxFlow/native sharing and release freshness without a caller-forged address. Assigned by the overseer to m26ps4 for design/source after phase3, reviewers9czdjh/uufub4; still OPEN until reviewed/landed. |
| Installed minimum material | The only secret is the agent secp256k1 scalar, not an API key or GRVT Session token. Wallet, account/application_id and vault are public and may differ from agent. Perps trade checks only nonempty scalar; standalone spot additionally requires its configured wallet to match the scalar. Preserve scalar-only/perps and missing/empty/public-only precedence without inventing a credential key/ref. The overseer APPROVED trusted scalar-only setup and this native distinction. Exact shared installation/public-address resolution remains to be reviewed with m26ps4; do not force a new secret or stable_id input. |
| Actual-release pacing | RESOLVED-PENDING: the overseer ruled a new non-default serial-pacing engine feature, owned by claude-dazzy3, reviewed by uufub4. Preserve the 334 ms gap at actual release. Moving the native queue-time anchor to release is an approved behavior change, recorded in CALLERS at adoption. Today SerialPlan still refuses nonzero policy pacing; do not adopt until the reviewed feature lands. No venue-local parallel authority. |
| Per-call 20 s | APPROVED new20s per-call bound, attempts1 and unbounded serial root (same ruling as Kraken). CALLERS disclosure remains required; preserve FIFO and no write replay. |
| Classifier/late evidence | Correlate full native status arrays before projection; native mixed/partial and journal outcome differences disclosed. ScheduleCancel uses its control-specific codec and cannot inherit order-reconciliation proof. Existing engine APIs first; any absent fact/evidence shape returns to its owner. |

Feature intent at adoption: serial-plans, condition-waits, dispatch-signing,
dispatch-nonce, crypto/secp256k1, with existing order-locks/sockets retained.
Add the reviewed serial-pacing opt-in once dazzy3's shared feature lands; its
name here is the approved prerequisite, not an existing Cargo declaration.
No session-bearer, Session or mutation-group feature is needed by this funnel.
These are opt-ins, not assumed shipped headroom. No generic core source or
feature flag is added by this plan/originals stage.

## Original assertion controls, before source

Add one new integration target delegating to the actual Hyperliquid Venue; its
boot seeds the same native catalog/account/socket fixtures and forwards every
relevant hook, guard and capability. Do not manufacture a Session, permit,
Selection, provider result or command owner. Capacity peers are real admitted
Command Read owners. Native wire/reply-ingress probes may explicitly take raw
HTTP, or Prepare.request without Bind/Claim; custody probes require a genuine
Prepare and never auto-claim it. Compile/setup failures are not REDs.

- Prepare-before-/exchange, unclaimed early DATA, delayed Bind/Claim, preparation
  cancellation/duplicate, expiry and stale-old-request callbacks: the first
  failure must be an actual assertion. Disclose later gated suffixes.
- Full 256-root capacity refuses place/cancel/amend/settings before queue,
  cloid/nonce/provisional effect. Retire exactly the first peer, recover the
  native result and validate all255 survivors. Zero-wait, duplicate owner and
  external NaN controls keep native state and existing owner intact.
- FIFO queued ownership, 334 ms timing, builder reply and exact8s timeout,
  native empty cancellation, unbounded serial input, two-step amendment proof,
  source-fill invalidation before cancel/replacement and partial replacement ids.
- Both networks/vault absence/presence, MessagePack declaration order and
  official EIP-712 oracles; cloid/bracket receipts and complete native action
  bytes. Same-agent engines/vaults/hex spellings/TxFlow share one nonce domain.
  A different agent is isolated; fixed/backwards clocks and safe-integer limits
  retain native behavior. Shared-API-only cases stay named future source gates.
- Local deterministic full-wire parity and asynchronous external signer over
  the exact digest/agent, valid low-s r/s/v variation and wrong signer/digest
  refusal. Fix the action/cloids/vault/network for one call; a legitimate Stale
  reprepare may need a fresh monotonic nonce and recomputed digest, unlike GRVT.
  It is never a second released write. Pin identical action fields and exactly
  one final release; no cached bytes or nonce reset.
- No network/timeout/auth resend. 401/403/429/408/5xx, malformed JSON/status
  arrays, mixed status outcomes, idempotent cancel exception versus amend,
  no OID trigger acknowledgments, native restore_cancel/settings uncertainty,
  and an untouched scheduleCancel success/refusal matrix.

The first target must execute against unchanged native production with a
per-test receipt listing intended assertion REDs and GREEN controls. Keep all
existing fixture assertions; any prepared_actions conversion is collector
plumbing only, with actual ingress ownership controls remaining explicit. API
and constructor tests needing unlanded seams are deferred by name, not no-op
stubs or counted fake REDs. Plan CLEAR plus originals CLEAR is required before
production, and every OPEN gate above remains a source hold.

## Validation and size gates

After source, run the scoped Hyperliquid suite, strict clippy/fmt, full bans and
all credential/request/signing/socket ratchets on the actual landing composition.
Preserve foreign inventory bytes/order, native financial/wire fixtures and the
spot/merged/socket/reconciliation controls. Source review is bounded to the
exact immutable pin. Push only a frozen reviewed candidate and retain one PR
Rust run, never a duplicate explicit dispatch or job rerun.

Measure optimized Hyperliquid and combined WASM as paired actual-tip/source
snapshots, both web and node, with the checked-in recipe and unchanged budgets.
Report enabled features, raw/gzip bytes, hashes and any budget shortfall before
claiming READY. If a prerequisite needs generic engine source, its own composed
Bybit/Bitget gates apply; do not attribute that to this venue-only plan. Remove
each own scratch target after measurement/review builds and retain sources,
logs and tables. No build, size, CI, source or installed-constructor clearance
is claimed by this plan commit.


## Overseer disposition after plan submission

Actual-agent nonce authority is assigned to m26ps4 after phase3, reviewed by
9czdjh/uufub4, and remains OPEN. Trusted scalar-only setup is approved, with
public wallet/user/vault separate from secrets and the native spot pair check
retained; actual setup API fit is still a source gate. NEW20s per-call, one
attempt and an unbounded serial root are approved. The overseer resolved pacing to a new
serial-pacing engine feature owned by claude-dazzy3, reviewed by uufub4. Its
334 ms actual-release anchor is a CALLERS-recorded behavior change from native
queue-time pacing. This is RESOLVED-PENDING the reviewed feature, not a landed
API. No venue-local authority is authorized. Plan/native originals may proceed
while nonce and pacing source are unavailable; neither is bypassed by a green
native control.

## Original execution checkpoint

The frozen target runs on actual #2146 landing f895d0d7018. Exactly three owned
paths differ: this plan, its README row and the new integration target. Runtime,
Cargo, maps and every existing fixture remain byte-exact to incoming.

request_context_trading executes29 controls: thirteen intended behavior
assertion REDs and sixteen GREENs, zero ignored. Four capacity writes reach raw
exchange before shared admission; empty-cancel capacity returns native []
instead of a shared refusal. NaN yields native kind None versus InvalidInput.
Unclaimed DATA completes the native command. Six Prepare controls fail at raw
exchange before Prepare. These are runtime predicates, not compile/setup
failures. The unchanged separate native trading target passes32/32. Strict
clippy with warnings denied, formatting, full bans/ratchets and diff checks pass.
The review build target is removed; logs and hashes are retained outside Git.

Exact first-peer retirement, recovered native completion and validation of all
255 survivors are gated behind each capacity RED. Preparation refusal/duplicate,
twenty-second expiry, late preparation, released physical timeout, delayed
Claim and stale-reply/live-FIFO suffixes are gated behind Prepare. They are
present but unexecuted on native. Signed Bind/Claim needs the actual installed
provider fixture at source; no raw compatibility is allowed past custody
assertions, and no provider result, nonce subject or owner is fabricated.

The sixteen GREENs pin real startup/account/socket delegation, zero-wait,
duplicate ownership, native wire and no-OID status, five HTTP refusal classes,
no network resend, native FIFO334ms, full mixed bracket correlation, uncertain
cancel stopping replacement, scheduleCancel proof and builder reply/8s expiry.
Independent fixtures use distinct real scalars/addresses. Existing native
same-agent/vault/TxFlow sharing controls stay unchanged.

Installed scalar-only setup, attested actual-agent nonce controls, asynchronous
provider/Stale signing and delayed release pacing remain named source gates.
Preflight/Plan/condition-ready delegation is added with the relevant feature
opt-ins during adoption, since those hooks are feature-gated today. The native
receipt proves no installed constructor, new nonce API, shared release pacing,
size, feature activation or production cutover. Both shared prerequisite source
holds remain, despite the approved numeric policy and plan direction.

Native/original review at5126b80ab56 accepts the boundaries and receipt, with
one gated capacity strengthening before checkpoint CLEAR: after each recovered
operation's terminal (including EmptyCancel), preserve validation of255 peers,
admit a fresh real command1256 as the256th retained Read, then validate all256.
This proves the recovered operation released its last registry credit. It is an
unexecuted source suffix on native, not a replacement for earlier assertions.

The fresh256 suffix passes strict clippy. Removing only that appended block
reconstructs the entire5126 target byte-for-byte, so every earlier assertion and
native GREEN body is unchanged. The13RED/16GREEN runtime receipt remains bound
to5126; the new suffix is typed but unexecuted. Its own target is removed.

The checkpoint is published separately from adoption, as for Kraken #2154.
The non-default private-adoption-originals feature gates this test target while
the thirteen custody assertions remain RED. Run it explicitly with
cargo test -p safe-cex-venue-hyperliquid --features private-adoption-originals
--test request_context_trading -- --test-threads=1. Normal workspace CI retains
all existing native tests. The original target is unchanged from 17baf7b6cbf;
the gate is not a production feature or a claim that the custody gaps are fixed.


## Additional native originals after the checkpoint landing

Local preparation extends the target by appending eight controls; the complete
landed 29-control target stays an exact unchanged prefix. These require no new
nonce subject, scalar-only constructor, provider fixture or release-pacing API.
They use distinct actual agents and the same real engine/native ingress helpers.

The additions pin accepted rows after an earlier rejection, ordered diagnostics
when every row is rejected, short and long status-array cardinality, wrong
response kind, both waiting statuses without invented order IDs, the terminal
cancel exception versus amendment proof, and a source fill after replacement
dispatch preserving the confirmed replacement order/client IDs.

A separate native control records an accepted prefix followed by an unknown
status: the current reducer has already published the accepted order and the
command terminal preserves Unknown despite retaining its ID. This is native evidence,
not authorization to skip full-array classification in the adopted journal or
to claim Store/outcome parity. Any changed publication or terminal projection
needs an explicit source disposition and CALLERS disclosure. The existing
mixed-refusal control remains unchanged. No shared prerequisite is implemented,
no inventory is pruned, and native adoption remains held.

The extended target executes 37 tests against unchanged native production:
13 intended assertion REDs and 24 GREENs, zero ignored. All eight added native
controls are GREEN, and every previous first assertion and GREEN body remains
unchanged. Strict scoped clippy with -D warnings, target formatting and
patch whitespace checks pass. The first extension run caught an incorrect new
expectation that Unknown was stripped; the corrected assertion matches the
native finish_trade rule, which strips only Rejected/NotSent after progress.
That mistaken expectation is not counted as a custody RED. Own review target
removed; no WASM build or additional activation proof is claimed.
